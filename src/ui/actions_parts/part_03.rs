use crate::workspace::ProfileRootEnabledState;
pub(super) struct ProfileRootToggle<'a>
{
    pub(super) mod_id: &'a str,
    pub(super) source: &'a str,
    pub(super) state: ProfileRootEnabledState,
}

pub(super) struct ProfileRootTargetEdit<'a>
{
    pub(super) mod_id: &'a str,
    pub(super) source: &'a str,
    pub(super) target: &'a str,
}

impl SanAndreasModUi
{
    pub(super) fn refresh_pending_runs(&mut self)
    {
        self.pending_runs = load_pending_run_records(&self.game_root()).unwrap_or_default();
        if self.pending_journal.is_none()
        {
            self.pending_journal = self
                .pending_runs
                .first()
                .map(|record_log_message_from_arguments| record_log_message_from_arguments.journal.clone());
        }
        if !self.pending_runs.is_empty() && !self.recovery_focus_applied
        {
            self.tab = crate::ui::state::UiTab::Run;
            self.recovery_focus_applied = true;
        }
    }

    pub(super) fn tick_pending_run_watcher(&mut self)
    {
        self.poll_game_child();
        if self.last_pending_watch.elapsed() < crate::settings::pending_run_watch_interval()
        {
            return;
        }
        self.last_pending_watch = Instant::now();
        self.cleanup_finished_pending_runs();
    }

    pub(super) fn poll_game_child(&mut self)
    {
        let Some(child) = &mut self.game_child else {
            return;
        };
        match child.try_wait()
        {
            Ok(Some(status)) => self.handle_game_exit(status),
            Ok(None) => {}
            Err(err) => {
                self.game_child = None;
                self.active_run = None;
                self.status = format!("could not check game process: {err}");
            }
        }
    }
    fn handle_game_exit(&mut self, status: ExitStatus)
    {
        let journal = self.pending_journal.to_owned();
        self.game_child = None;
        self.record_run_exit_outcome(status);
        if let Some(journal) = journal
        {
            match self.cleanup_journal(&journal)
            {
                Ok(()) => {
                    self.pending_journal = None;
                    self.refresh_pending_runs();
                    self.status = "game exited; cleaned temporary files".to_string();
                }
                Err(err) => {
                    self.refresh_pending_runs();
                    self.status = format!("game exited; automatic cleanup failed: {err}");
                }
            }
        }
        else
        {
            self.cleanup_finished_pending_runs();
        }
    }
    /// Persist the full outcome (result, exit code, duration, launch args) of the
    /// run this session launched, once its game process has exited. Only possible
    /// while we still hold the `ActiveRun` context; a no-op otherwise.
    fn record_run_exit_outcome(&mut self, status: ExitStatus)
    {
        let Some(active) = self.active_run.take() else {
            return;
        };
        let finished_unix = unix_now();
        let (result, exit_code) = if status.success() {
            (RUN_RESULT_SUCCESS, status.code())
        } else {
            (RUN_RESULT_GAME_ERROR, status.code())
        };
        let outcome = RunOutcome {
            version: 1,
            txid: active.txid,
            profile: active.profile,
            result: result.to_string(),
            exit_code,
            duration_ms: Some(
                finished_unix
                    .saturating_sub(active.started_unix)
                    .saturating_mul(MILLISECONDS_PER_SECOND),
            ),
            launch_args: active.launch_args,
            started_unix: active.started_unix,
            finished_unix,
        };
        if let Err(err) = write_run_outcome(&state_directory(&self.game_root()), &outcome)
        {
            log_warn!("could not record_log_message_from_arguments run outcome: {err}");
        }
    }

    fn cleanup_journal(&self, journal: &Path) -> Result<(), AppError>
    {
        return cleanup_journal_for_game_root(&self.game_root(), journal);
    }

    fn selected_pending_run(&self) -> Option<PendingRunRecord>
    {
        if let Some(journal) = &self.pending_journal
        {
            if let Some(record_log_message_from_arguments) = self
                .pending_runs
                .iter()
                .find(|record_log_message_from_arguments| &record_log_message_from_arguments.journal == journal)
            {
                return Some(record_log_message_from_arguments.clone());
            }
            return Some(PendingRunRecord {
                journal: journal.clone(),
                pid: None,
                status: PendingRunStatus::Unknown,
                detail: "selected cleanup record_log_message_from_arguments is not tracked".to_string(),
            });
        }
        return self.pending_runs.first().cloned();
    }

    fn is_pending_run_running(&mut self, record_log_message_from_arguments: &PendingRunRecord) -> bool
    {
        self.poll_game_child();
        if self.game_child.is_some()
        {
            return true;
        }
        return record_log_message_from_arguments.status == PendingRunStatus::Running
            || record_log_message_from_arguments.pid.map(is_child_program_running).unwrap_or(false);
    }

    fn has_unsafe_pending_runs_for_launch(&self) -> bool
    {
        return self.pending_runs
            .iter()
            .any(|record_log_message_from_arguments| !self.is_pending_run_current_session_running(record_log_message_from_arguments));
    }

    fn is_pending_run_current_session_running(&self, record_log_message_from_arguments: &PendingRunRecord) -> bool
    {
        return is_pending_run_match_for_current_session(
            record_log_message_from_arguments,
            self.game_child.as_ref().map(|child| child.id()),
        );
    }

    pub(super) fn pending_cleanup_summary(&self) -> Option<String>
    {
        if self.pending_runs.is_empty()
        {
            return None;
        }
        let stale = self.count_pending_status(PendingRunStatus::Stale);
        let running = self.count_pending_status(PendingRunStatus::Running);
        let unknown = self.count_pending_status(PendingRunStatus::Unknown);
        let invalid = self.count_pending_status(PendingRunStatus::Invalid);
        return Some(format!(
            "{} temporary runs need cleanup: {stale} finished, {running} running, {unknown} unknown, {invalid} invalid",
            self.pending_runs.len()
        ));
    }

    fn count_pending_status(&self, status: PendingRunStatus) -> usize
    {
        return self.pending_runs
            .iter()
            .filter(|record_log_message_from_arguments| record_log_message_from_arguments.status == status)
            .count();
    }

    fn cleanup_finished_pending_runs(&mut self)
    {
        self.refresh_pending_runs();
        let stale_runs = self
            .pending_runs
            .iter()
            .filter(|record_log_message_from_arguments| record_log_message_from_arguments.status == PendingRunStatus::Stale)
            .cloned()
            .collect::<Vec<_>>();
        if stale_runs.is_empty()
        {
            return;
        }

        let game_root = self.game_root();
        let mut cleaned = 0;
        let mut first_error = None;
        for record_log_message_from_arguments in stale_runs
        {
            match cleanup_journal_for_game_root(&game_root, &record_log_message_from_arguments.journal)
            {
                Ok(()) => cleaned += 1,
                Err(err) => {
                    first_error = Some(err);
                    break;
                }
            }
        }
        self.refresh_pending_runs();
        self.status = match first_error {
            Some(err) => format!("auto-cleaned {cleaned} finished runs; then failed: {err}"),
            None => format!("auto-cleaned {cleaned} finished runs"),
        };
    }
}

impl SanAndreasModUi
{
    pub(super) fn create_profile(&mut self)
    {
        let name = self.inputs.new_profile.trim().to_string();
        if name.is_empty()
        {
            self.status = "profile name is required".to_string();
            return;
        }
        let result = safe_profile_name(&name);
        let profile_name = result.name;
        let note = result.note;
        self.run_action("created profile", |game_root| {
            create_profile(game_root, &profile_name)
        });
        // Only follow the picker to the new profile when it was actually created.
        if self.last_error.is_none()
        {
            self.selected_profile = profile_name;
        }
        self.inputs.new_profile.clear();
        self.note_name_normalization(note);
    }

    pub(super) fn set_active_selected_profile(&mut self)
    {
        let profile = self.selected_profile.to_owned();
        self.run_action("set active profile", |game_root| {
            set_active_profile(game_root, &profile)
        });
    }

    pub(super) fn copy_selected_profile(&mut self)
    {
        if self.inputs.copy_profile.trim().is_empty()
        {
            self.status = "destination profile name is required".to_string();
            return;
        }
        let result = safe_profile_name(&self.inputs.copy_profile);
        let dest = result.name;
        let note = result.note;
        let source = self.selected_profile.to_owned();
        let target = dest.clone();
        self.run_action("copied profile", |game_root| {
            copy_profile(game_root, ProfileCopyRequest { source_name: &source, dest_name: &target })
        });
        if self.last_error.is_none()
        {
            self.selected_profile = dest;
        }
        self.inputs.copy_profile.clear();
        self.note_name_normalization(note);
    }

    pub(super) fn rename_selected_profile(&mut self)
    {
        if self.inputs.rename_profile.trim().is_empty()
        {
            self.status = "new profile name is required".to_string();
            return;
        }
        let result = safe_profile_name(&self.inputs.rename_profile);
        let new_name = result.name;
        let note = result.note;
        let old = self.selected_profile.to_owned();
        let target = new_name.clone();
        self.run_action("renamed profile", |game_root| {
            rename_profile(game_root, ProfileRenameRequest { old_name: &old, new_name: &target })
        });
        if self.last_error.is_none()
        {
            self.selected_profile = new_name;
        }
        self.inputs.rename_profile.clear();
        self.note_name_normalization(note);
    }

    pub(super) fn set_all_selected_profile_mods(&mut self, activation: ProfileModActivation)
    {
        let profile = self.selected_profile.to_owned();
        let label = activation.label();
        self.run_action(&format!("{label} all mods"), |game_root| {
            set_all_profile_mods(game_root, &profile, activation)
        });
    }

    pub(super) fn request_delete_profile(&mut self)
    {
        let profile = self.selected_profile.to_owned();
        self.request_confirm(PendingConfirm { title: "Delete profile".to_string(), message: format!("Delete profile `{profile}`? This is not undoable."), confirm_label: "Delete".to_string(), action: ConfirmAction::DeleteProfile(profile) });
    }

    pub(super) fn delete_selected_profile(&mut self, name: &str)
    {
        let name = name.to_string();
        self.run_action("deleted profile", |game_root| {
            delete_profile(game_root, &name)
        });
        // Fall back to `default` only if the delete actually happened.
        if self.last_error.is_none()
        {
            self.selected_profile = "default".to_string();
        }
    }

    pub(super) fn save_launch_args(&mut self)
    {
        // Space-separated tokens become launch arguments, matching `profile-args`.
        let args: Vec<String> = self
            .inputs.launch_args
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let profile = self.selected_profile.to_owned();
        self.run_action("saved launch args", |game_root| {
            set_profile_launch_args(game_root, &profile, args)
        });
    }

    pub(super) fn set_profile_root_enabled(&mut self, toggle: ProfileRootToggle<'_>)
    {
        let mod_id = toggle.mod_id;
        let source = toggle.source;
        let state = toggle.state;
        let profile = self.selected_profile.to_owned();
        let mod_id = mod_id.to_string();
        let source = source.to_string();
        self.run_action("updated root override", |game_root| {
            let selector = ProfileRootSelector {
                profile_name: &profile,
                mod_id: &mod_id,
                source: &source,
            };
            set_profile_root_override(game_root, selector, state)
        });
    }

    pub(super) fn save_profile_root_target(&mut self, edit: ProfileRootTargetEdit<'_>)
    {
        let mod_id = edit.mod_id;
        let source = edit.source;
        let target = edit.target;
        let profile = self.selected_profile.to_owned();
        let mod_id = mod_id.to_string();
        let source = source.to_string();
        let target = target.to_string();
        self.run_action("retargeted install root", |game_root| {
            let selector = ProfileRootSelector {
                profile_name: &profile,
                mod_id: &mod_id,
                source: &source,
            };
            set_profile_root_target(game_root, selector, &target)
        });
    }

    /// Surface a profile-name normalization note in the status bar, but only on a
    /// successful action so it never masks an error banner.
    fn note_name_normalization(&mut self, note: Option<String>)
    {
        if let (Some(note), None) = (note, &self.last_error)
        {
            self.status = format!("{}; {note}", self.status);
        }
    }
}

impl SanAndreasModUi
{
    pub(super) fn import_package(&mut self, ctx: &egui::Context)
    {
        let trimmed_package = self.inputs.import_path.trim();
        let package = PathBuf::from(trimmed_package);
        if package.as_os_str().is_empty()
        {
            self.status = "package path is required".to_string();
            return;
        }
        let profile = self.selected_profile.to_owned();
        let game_root = self.game_root();
        // Extraction + copy into the library can be slow; run it off the UI thread.
        self.spawn_task(ctx, "importing package", move || {
            let options = CommandOptions {
                game_root: game_root.clone(),
                profile,
                includes: BTreeSet::new(),
                excludes: BTreeSet::new(),
                write_manifest: false,
            };
            let import_result = import_package(&package, &options);
            TaskResult::Import(import_result)
        });
    }
}

impl SanAndreasModUi
{
    pub(super) fn request_remove_mod(&mut self, mod_id: &str)
    {
        self.request_confirm(PendingConfirm { title: "Remove mod".to_string(), message: format!("Remove `{mod_id}` from profile `{}`? This is not undoable.", self.selected_profile), confirm_label: "Remove".to_string(), action: ConfirmAction::RemoveMod(mod_id.to_string()) });
    }

    pub(super) fn request_cleanup_selected(&mut self)
    {
        self.request_confirm(PendingConfirm { title: "Clean temporary files".to_string(), message: "Roll back the selected run and delete its materialized files from the game folder?".to_string(), confirm_label: "Clean".to_string(), action: ConfirmAction::CleanSelectedRun });
    }

    pub(super) fn request_cleanup_record(&mut self, record_log_message_from_arguments: PendingRunRecord)
    {
        self.request_confirm(PendingConfirm { title: "Clean temporary files".to_string(), message: "Roll back this run and delete its materialized files from the game folder?".to_string(), confirm_label: "Clean".to_string(), action: ConfirmAction::CleanRunRecord(record_log_message_from_arguments) });
    }

    pub(super) fn request_cleanup_finished(&mut self)
    {
        self.request_confirm(PendingConfirm { title: "Clean finished runs".to_string(), message: "Roll back every finished run and delete its materialized files from the game folder?".to_string(), confirm_label: "Clean finished".to_string(), action: ConfirmAction::CleanFinishedRuns });
    }
}
