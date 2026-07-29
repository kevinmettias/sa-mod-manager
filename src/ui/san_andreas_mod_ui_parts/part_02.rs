
impl SanAndreasModUi
{
    pub(super) fn new(game_root: PathBuf, preferences: UiPreferences) -> Self
    {
        let selected_profile = if preferences.profile.trim().is_empty() {
            "default".to_string()
        } else {
            preferences.profile.trim().to_string()
        };
        let mut ui = Self {
            inputs: UiInputs {
                game_root: game_root.display().to_string(),
                import_path: String::new(),
                new_profile: String::new(),
                copy_profile: String::new(),
                rename_profile: String::new(),
                launch_args: String::new(),
            },
            selected_profile,
            status: String::new(),
            state: UiState::default(),
            tab: preferences.tab(),
            pending_journal: None,
            pending_runs: Vec::new(),
            game_child: None,
            active_run: None,
            recovery_focus_applied: false,
            last_pending_watch: Instant::now(),
            readme_proposals: Vec::new(),
            analysis_summary: None,
            mod_root_edits: BTreeMap::new(),
            telemetry: TelemetrySummary::default(),
            telemetry_search: String::new(),
            filters: UiFilters {
                telemetry_kind: "all".to_string(),
                mod_text: String::new(),
                mod_status: ModStatusFilter::default(),
                mod_category: None,
                mod_conflicts: false,
                mod_user_category: None,
            },
            last_error: None,
            pending_confirm: None,
            task: None,
            dark_mode: preferences.dark_mode,
            profile_root_target_edits: BTreeMap::new(),
            content: UiContentView {
                index: None,
                category: None,
                search: String::new(),
                conflicts_only: false,
            },
            modloader_priorities: None,
            overwrite_files: Vec::new(),
            modloader_log: None,
            cleo_diagnostics: None,
            mod_meta: BTreeMap::new(),
            modloader_overrides: BTreeMap::new(),
            separators: Vec::new(),
            collapsed_separators: BTreeSet::new(),
            separator_edit: None,
            executables: Vec::new(),
            selected_run_target: 0,
            new_tool_name: String::new(),
            new_tool_path: String::new(),
            new_tool_args: String::new(),
            mod_info: None,
            window_size: preferences.window_size(),
            last_pref_save: Instant::now(),
            prefs_signature: String::new(),
        };
        ui.refresh();
        // Baseline the signature against the state that survived `refresh` (which
        // may have replaced a stale saved profile), so the first frame does not
        // rewrite an unchanged file.
        ui.prefs_signature = ui.current_preferences().signature();
        return ui;
    }

    pub(super) fn game_root(&self) -> PathBuf
    {
        let trimmed_root = self.inputs.game_root.trim();
        return PathBuf::from(trimmed_root);
    }

    pub(super) fn refresh(&mut self)
    {
        match self.reload_state()
        {
            Ok(()) => {
                self.status = self
                    .pending_cleanup_summary()
                    .unwrap_or_else(|| "ready".to_string());
            }
            Err(err) => self.status = err.to_string(),
        }
    }

    pub(super) fn reload_state(&mut self) -> Result<(), AppError>
    {
        let state = load_ui_state(&self.game_root(), &self.selected_profile)?;
        if !state
            .profiles
            .iter()
            .any(|name| name == &self.selected_profile)
        {
            self.selected_profile = state
                .profiles
                .first()
                .cloned()
                .unwrap_or_else(|| "default".to_string());
            self.state = load_ui_state(&self.game_root(), &self.selected_profile)?;
        }
        else
        {
            self.state = state;
        }
        self.refresh_pending_runs();
        self.telemetry =
            load_telemetry_summary(&self.game_root(), self.pending_runs.len()).unwrap_or_default();
        // Any edit that reloads state (toggle, reorder, import, profile switch)
        // can change what materializes, so drop the cached content index; the
        // viewer re-scans on demand.
        self.content.index = None;
        self.modloader_priorities = None;
        self.overwrite_files = Vec::new();
        self.modloader_log = None;
        self.cleo_diagnostics = None;
        self.executables = read_executables(&state_directory(&self.game_root()));
        self.mod_meta = read_mod_meta(&state_directory(&self.game_root()));
        self.separators =
            read_separators(&state_directory(&self.game_root()), &self.selected_profile);
        self.modloader_overrides =
            read_modloader_overrides(&state_directory(&self.game_root()), &self.selected_profile);
        // Keep the run-target selection in range if a tool was removed elsewhere.
        if self.selected_run_target > self.executables.len()
        {
            self.selected_run_target = 0;
        }
        return Ok(());
    }

    pub(super) fn run_action(
        &mut self,
        success_message: &str,
        action: impl FnOnce(&Path) -> Result<(), AppError>,
    )
    {
        let result = action(&self.game_root());
        let status_result = result.map(|_| success_message.to_string());
        self.set_action_result(success_message, status_result);
    }

    pub(super) fn set_action_result(
        &mut self,
        success_message: &str,
        result: Result<String, AppError>,
    )
    {
        match result
        {
            Ok(detail) => {
                self.last_error = None;
                self.status = if detail == success_message {
                    success_message.to_string()
                } else {
                    format!("{success_message}: {detail}")
                };
                if let Err(err) = self.reload_state()
                {
                    self.record_error(err);
                }
            }
            Err(err) => self.record_error(err),
        }
    }

    /// Surface an error both transiently (status bar) and persistently (a
    /// dismissible banner), so it is not lost the moment the next status arrives.
    pub(super) fn record_error(&mut self, err: AppError)
    {
        let text = err.to_string();
        self.status = text.clone();
        self.last_error = Some(text);
    }

    /// Start a long-running operation off the UI thread. Only one runs at a time;
    /// the worker wakes the UI via `request_repaint` when it finishes.
    pub(super) fn spawn_task(
        &mut self,
        ctx: &egui::Context,
        label: &str,
        // rust-closure: allow: the worker closure crosses the thread::spawn boundary
        // rust-lifetime: allow: thread::spawn owns the closure after this stack frame returns
        work: impl FnOnce() -> TaskResult + Send + 'static,
    )
    {
        if self.task.is_some()
        {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = work();
            if sender.send(result).is_err()
            {
                log_warn!("background task finished after its receiver was dropped");
            }
            ctx.request_repaint();
        });
        self.last_error = None;
        self.status = format!("working: {label}â€¦");
        self.task = Some(BackgroundTask {
            label: label.to_string(),
            receiver,
        });
    }

    pub(super) fn poll_task(&mut self)
    {
        let Some(task) = self.task.as_ref() else {
            return;
        };
        match task.receiver.try_recv()
        {
            Ok(result) => {
                self.task = None;
                self.apply_task_result(result);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.task = None;
                self.record_error(AppError::Tool(
                    "background task ended unexpectedly".to_string(),
                ));
            }
        }
    }

    fn apply_task_result(&mut self, result: TaskResult)
    {
        match result
        {
            TaskResult::Import(result) => self.set_action_result(
                "imported package",
                result.map(|_| "imported package".into()),
            ),
            TaskResult::Analyze(result) => self.apply_analysis(result),
            TaskResult::Play(result) => self.apply_play(result),
        }
    }

    pub(super) fn is_busy(&self) -> bool
    {
        return self.task.is_some();
    }

    pub(super) fn task_label(&self) -> Option<&str>
    {
        return self.task.as_ref().map(|task| task.label.as_str());
    }

    /// Queue a destructive action for modal confirmation instead of running it.
    pub(super) fn request_confirm(&mut self, pending_confirm: PendingConfirm)
    {
        self.pending_confirm = Some(pending_confirm);
    }

    pub(super) fn run_confirmed_action(&mut self, action: ConfirmAction)
    {
        match action
        {
            ConfirmAction::RemoveMod(mod_id) => self.remove_mod_from_profile(&mod_id),
            ConfirmAction::CleanSelectedRun => self.cleanup_pending_run(),
            ConfirmAction::CleanRunRecord(record_log_message_from_arguments) => self.cleanup_pending_run_record(record_log_message_from_arguments),
            ConfirmAction::CleanFinishedRuns => self.cleanup_stale_pending_runs(),
            ConfirmAction::DeleteProfile(name) => self.delete_selected_profile(&name),
        }
    }
}

impl SanAndreasModUi
{
    /// Apply the chosen light/dark palette. Cheap to call each frame, and doing so
    /// keeps the window in sync the instant the toggle flips.
    fn apply_theme(&self, ctx: &egui::Context)
    {
        let visuals = if self.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        ctx.set_visuals(visuals);
    }

    /// Keyboard access to the core navigation: Ctrl/Cmd+1..6 jump to a tab,
    /// Ctrl/Cmd+R reloads, and Escape dismisses the error banner. The confirm
    /// modal owns Escape/backdrop while it is open (see `confirm_modal`).
    fn handle_shortcuts(&mut self, ctx: &egui::Context)
    {
        use egui::{Key, Modifiers};
        let editing = ctx.memory(|memory| memory.focused().is_some());
        // When nothing is focused and no modal is up, Escape clears the banner.
        // While a field is focused, leave Escape to egui so it defocuses instead.
        if !editing
            && self.pending_confirm.is_none()
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.last_error = None;
        }
        // Do not steal navigation/reload chords while the user is typing (Ctrl+R
        // would reload mid-edit) or while the modal should hold focus.
        if editing || self.pending_confirm.is_some()
        {
            return;
        }
        // Detail-pane tabs, matching the strip order in `detail_panel`.
        const TAB_KEYS: [(Key, UiTab); 6] = [
            (Key::Num1, UiTab::Content),
            (Key::Num2, UiTab::Import),
            (Key::Num3, UiTab::Run),
            (Key::Num4, UiTab::Mods),
            (Key::Num5, UiTab::Telemetry),
            (Key::Num6, UiTab::Home),
        ];
        for (key, tab) in TAB_KEYS
        {
            if ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, key))
            {
                self.tab = tab;
            }
        }
        if ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::R))
        {
            self.refresh();
        }
    }

    /// Track the live window size and persist preferences at most once per
    /// interval, and only when something actually changed.
    fn persist_preferences_if_changed(&mut self, ctx: &egui::Context)
    {
        let size = ctx.input(|input| input.screen_rect().size());
        self.window_size = [size.x, size.y];
        if self.last_pref_save.elapsed() < PREF_SAVE_INTERVAL
        {
            return;
        }
        let preferences = self.current_preferences();
        let signature = preferences.signature();
        self.last_pref_save = Instant::now();
        if signature == self.prefs_signature
        {
            return;
        }
        preferences.save();
        self.prefs_signature = signature;
    }

    /// Snapshot the session state that is worth remembering between runs.
    fn current_preferences(&self) -> UiPreferences
    {
        return UiPreferences {
            game_root: self.inputs.game_root.clone(),
            profile: self.selected_profile.to_owned(),
            tab: self.tab.as_key().to_string(),
            dark_mode: self.dark_mode,
            width: self.window_size[0],
            height: self.window_size[1],
        };
    }
}

impl eframe::App for SanAndreasModUi
{
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame)
    {
        self.apply_theme(context);
        self.handle_shortcuts(context);
        self.poll_task();
        self.handle_file_drops(context);
        self.tick_pending_run_watcher();
        context.request_repaint_after(crate::settings::pending_run_watch_interval());
        // MO2-style single-page shell: a top toolbar, a left filters column, the
        // mod list as the always-visible centre, a right tabbed detail pane, and a
        // bottom status bar â€” no full-page tab switching.
        egui::TopBottomPanel::top("toolbar").show(context, |ui| self.toolbar(ui));
        self.error_banner(context);
        egui::TopBottomPanel::bottom("status")
            .exact_height(34.0) // literal: allow UI tuning threshold is local to this control
            .show(context, |ui| self.status_bar(ui));
        egui::SidePanel::left("filters")
            .resizable(true)
            .default_width(190.0) // literal: allow UI tuning threshold is local to this control
            .show(context, |ui| self.filters_panel(ui));
        egui::SidePanel::right("detail")
            .resizable(true)
            .default_width(560.0) // literal: allow UI tuning threshold is local to this control
            .show(context, |ui| self.detail_panel(ui));
        egui::CentralPanel::default().show(context, |ui| self.mods_center_panel(ui));
        self.mod_details_window(context);
        self.confirm_modal(context);
        self.persist_preferences_if_changed(context);
    }

    /// A final flush on close captures a resize made in the last throttle window.
    fn on_exit(&mut self)
    {
        self.current_preferences().save();
    }
}

fn load_telemetry_summary(
    game_root: &Path,
    pending_cleanup: usize,
) -> Result<TelemetrySummary, AppError>
{
    let state_root = state_directory(game_root);
    let mut summary = TelemetrySummary {
        pending_cleanup,
        ..TelemetrySummary::default()
    };
    load_import_telemetry(&state_root, &mut summary)?;
    load_journal_telemetry(&state_root, &mut summary)?;
    summary.recent_events.sort_by(|left, right| {
        right.created_unix
            .cmp(&left.created_unix)
            .then_with(|| left.title.cmp(&right.title))
    });
    summary.recent_events.truncate(200); // literal: allow UI tuning threshold is local to this control
    summary.mod_history.sort_by(|left, right| {
        right.last_seen_unix
            .cmp(&left.last_seen_unix)
            .then_with(|| left.id.cmp(&right.id))
    });
    return Ok(summary);
}
