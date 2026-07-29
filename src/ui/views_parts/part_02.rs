
impl SanAndreasModUi
{
    /// The left filters column (MO2's Categories/filters pane): narrows the mod
    /// list by status, text, category, and conflicts, and hosts the content scan.
    pub(super) fn filters_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.add_space(UI_SMALL_GAP);
        ui.strong("Game Setup");
        game_setup_summary(ui, &self.state.infrastructure);
        ui.separator();
        ui.strong("Filters");
        ui.separator();

        ui.label("Status");
        ui.selectable_value(&mut self.filters.mod_status, ModStatusFilter::All, "All");
        ui.selectable_value(
            &mut self.filters.mod_status,
            ModStatusFilter::Enabled,
            "Enabled",
        );
        ui.selectable_value(
            &mut self.filters.mod_status,
            ModStatusFilter::Disabled,
            "Disabled",
        );
        ui.separator();

        ui.label("Search");
        ui.add(
            egui::TextEdit::singleline(&mut self.filters.mod_text)
                .hint_text("mod id")
                .desired_width(f32::INFINITY),
        );
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Category");
            if ui.small_button("clear").clicked()
            {
                self.filters.mod_category = None;
            }
        });
        if ui
            .selectable_label(self.filters.mod_category.is_none(), "All categories")
            .clicked()
        {
            self.filters.mod_category = None;
        }
        for category in ContentCategory::all()
        {
            let selected = self.filters.mod_category == Some(category);
            if ui.selectable_label(selected, category.label()).clicked()
            {
                self.filters.mod_category = Some(category);
            }
        }
        ui.separator();

        // User-assigned categories (from mod annotations), if any exist.
        let user_categories: std::collections::BTreeSet<String> = self
            .mod_meta
            .values()
            .flat_map(|meta| meta.categories.iter().cloned())
            .collect();
        if !user_categories.is_empty()
        {
            ui.horizontal(|ui| {
                ui.label("My categories");
                if ui.small_button("clear").clicked()
                {
                    self.filters.mod_user_category = None;
                }
            });
            if ui
                .selectable_label(self.filters.mod_user_category.is_none(), "Any")
                .clicked()
            {
                self.filters.mod_user_category = None;
            }
            for category in &user_categories
            {
                let selected = self.filters.mod_user_category.as_deref() == Some(category.as_str());
                if ui.selectable_label(selected, category).clicked()
                {
                    self.filters.mod_user_category = Some(category.clone());
                }
            }
            ui.separator();
        }

        let scanned = self.content.index.is_some();
        ui.add_enabled_ui(scanned, |ui| {
            ui.checkbox(&mut self.filters.mod_conflicts, "Conflicts only")
                .on_hover_text("Mods that overwrite or are overwritten (needs a content scan)");
        });
        if ui
            .button("Analyze content")
            .on_hover_text("Scan enabled mods for content flags and conflicts")
            .clicked()
        {
            self.rescan_content();
        }
        match &self.content.index
        {
            Some(index) => {
                let conflicts = index.conflict_count();
                if conflicts == 0
                {
                    ui.weak("no conflicts");
                }
                else
                {
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, format!("{conflicts} conflicts"));
                }
            }
            None => {
                ui.weak("not analyzed");
            }
        }
        if !scanned && (self.filters.mod_category.is_some() || self.filters.mod_conflicts)
        {
            ui.weak("category/conflict filters need Analyze");
        }

        ui.separator();
        ui.strong("Diagnostics");
        self.diagnostics_summary(ui);
    }

    /// ModLoader last-run log health and CLEO health, from the last content scan.
    /// Each issue line is clickable and jumps to its viewer in the Content tab.
    fn diagnostics_summary(&mut self, ui: &mut egui::Ui)
    {
        // Snapshot counts first so the borrow on `self.*` is released before a
        // click mutates `self.tab`/`self.content.category`.
        let modloader = self
            .modloader_log
            .as_ref()
            .map(|log| (log.is_clean(), log.errors.len(), log.warnings.len()));
        let cleo = self.cleo_diagnostics.as_ref().map(|diag| {
            (
                diag.is_empty(),
                diag.blacklisted_plugins.len()
                    + diag.fxt_conflicts.len()
                    + diag.script_issues.len(),
            )
        });
        let warn = ui.visuals().warn_fg_color;

        let modloader_jump = match modloader {
            None => {
                ui.weak("ModLoader: run Analyze");
                false
            }
            Some((true, _, _)) => {
                ui.weak("ModLoader: clean last run");
                false
            }
            Some((false, errors, warnings)) => ui
                .selectable_label(
                    false,
                    egui::RichText::new(format!("ModLoader: {errors} err Â· {warnings} warn"))
                        .color(warn),
                )
                .on_hover_text("Open the ModLoader viewer")
                .clicked(),
        };
        if modloader_jump
        {
            self.content.category = Some(ContentCategory::ModLoader);
            self.tab = UiTab::Content;
        }

        let cleo_jump = match cleo {
            None => {
                ui.weak("CLEO: run Analyze");
                false
            }
            Some((true, _)) => {
                ui.weak("CLEO: healthy");
                false
            }
            Some((false, count)) => ui
                .selectable_label(
                    false,
                    egui::RichText::new(format!(
                        "CLEO: {count} issue{}",
                        if count == 1 { "" } else { "s" }
                    ))
                    .color(warn),
                )
                .on_hover_text("Open the CLEO viewer")
                .clicked(),
        };
        if cleo_jump
        {
            self.content.category = Some(ContentCategory::Cleo);
            self.tab = UiTab::Content;
        }

        // Overwrite (unmanaged game-folder files) â€” only meaningful after a scan.
        if self.content.index.is_some()
        {
            let unmanaged = self.overwrite_files.len();
            if unmanaged == 0
            {
                ui.weak("Unmanaged: none");
            } else if ui
                .selectable_label(
                    false,
                    egui::RichText::new(format!("Unmanaged: {unmanaged} files")).color(warn),
                )
                .on_hover_text("Game-folder files no enabled mod owns â€” see the Play tab")
                .clicked()
            {
                self.tab = UiTab::Run;
            }
        }
    }

    /// The centre pane â€” the always-visible mod list (load order) with its profile
    /// settings and per-profile root overrides. This is the heart of the window.
    pub(super) fn mods_center_panel(&mut self, ui: &mut egui::Ui)
    {
        let entries = selected_profile_entries(&self.state, &self.selected_profile);
        let enabled = entries.iter().filter(|entry| entry.enabled).count();
        ui.add_space(UI_SMALL_GAP);
        ui.horizontal(|ui| {
            ui.heading(format!("Mods â€” {}", self.selected_profile));
            ui.label(format!("{enabled}/{} enabled", entries.len()));
        });
        egui::CollapsingHeader::new("Profile settings")
            .id_salt("profile_settings")
            .show(ui, |ui| {
                self.profile_management_panel(ui);
                self.profile_launch_args_panel(ui);
            });
        ui.separator();
        if entries.is_empty()
        {
            ui.label("No mods in this profile.");
            ui.horizontal(|ui| {
                if ui.button("Import a mod").clicked()
                {
                    self.tab = UiTab::Import;
                }
            });
            return;
        }
        ui.horizontal(|ui| {
            if ui
                .button("ï¼‹ Separator")
                .on_hover_text("Add a labeled group divider at the top; move it with â–²/â–¼")
                .clicked()
            {
                self.add_separator(0);
            }
        });
        let visible = self.compute_visible_mods(&entries);
        self.profile_mod_order_list(ui, &entries, visible.as_ref());
        self.profile_root_overrides_panel(ui, &entries);
    }

    /// The right detail pane: a top tab strip over the secondary views. The mod
    /// list stays put in the centre; only this pane changes with the tab.
    pub(super) fn detail_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.add_space(UI_TINY_GAP);
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.tab, UiTab::Content, "Content");
            ui.selectable_value(&mut self.tab, UiTab::ModLoader, "ModLoader");
            ui.selectable_value(&mut self.tab, UiTab::Import, "Import");
            ui.selectable_value(&mut self.tab, UiTab::Run, "Play");
            ui.selectable_value(&mut self.tab, UiTab::Mods, "Library");
            ui.selectable_value(&mut self.tab, UiTab::Telemetry, "Telemetry");
            ui.selectable_value(&mut self.tab, UiTab::Home, "Overview");
        });
        ui.separator();
        // Content and Library manage their own scrolling; wrap the rest so they
        // never overflow the pane.
        match self.tab
        {
            UiTab::Content => self.content_panel(ui),
            UiTab::Mods => self.mods_panel(ui),
            other => {
                egui::ScrollArea::vertical()
                    .id_salt("detail_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| match other {
                        UiTab::ModLoader => self.modloader_panel(ui),
                        UiTab::Import => self.import_panel(ui),
                        UiTab::Run => self.run_panel(ui),
                        UiTab::Telemetry => self.telemetry_panel(ui),
                        // Overview, plus the retired Profiles tab, land here.
                        _ => self.home_panel(ui),
                    });
            }
        }
    }
}

impl SanAndreasModUi
{
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui)
    {
        let busy_label = self.task_label().map(str::to_string);
        ui.horizontal(|ui| {
            if let Some(label) = busy_label
            {
                ui.add(egui::Spinner::new());
                ui.strong(format!("{label}â€¦"));
                ui.separator();
            }
            ui.strong(readiness_label(self));
            ui.separator();

            // At-a-glance state chips, so the status bar is more than a log line.
            let enabled = selected_profile_entries(&self.state, &self.selected_profile)
                .iter()
                .filter(|entry| entry.enabled)
                .count();
            ui.label(format!("{enabled} enabled"));

            if self.game_child.is_some()
            {
                ui.separator();
                let running_color = egui::Color32::from_rgb(
                    RUNNING_STATUS_RED,
                    RUNNING_STATUS_GREEN,
                    RUNNING_STATUS_BLUE,
                );
                ui.colored_label(running_color, "? running");
            }
            if let Some(index) = &self.content.index
            {
                ui.separator();
                let conflicts = index.conflict_count();
                if conflicts > 0
                {
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, format!("{conflicts} conflicts"));
                }
                else
                {
                    ui.weak("0 conflicts");
                }
            }
            let pending = self.pending_runs.len();
            if pending > 0
            {
                ui.separator();
                let warn = ui.visuals().warn_fg_color;
                ui.colored_label(warn, format!("âš  {pending} to clean"));
            }
            // Post-scan reality checks: what ModLoader logged last run and CLEO
            // health. Only shown when there is something to flag.
            if let Some(log) = &self.modloader_log
            {
                if !log.is_clean()
                {
                    ui.separator();
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, format!("ML {} err", log.errors.len()))
                        .on_hover_text(
                            "ModLoader reported errors last run â€” see the ModLoader viewer",
                        );
                }
            }
            if let Some(diag) = &self.cleo_diagnostics
            {
                let issues = diag.blacklisted_plugins.len()
                    + diag.fxt_conflicts.len()
                    + diag.script_issues.len();
                if issues > 0
                {
                    ui.separator();
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, format!("CLEO {issues}"))
                        .on_hover_text("CLEO health issues â€” see the CLEO viewer");
                }
            }

            ui.separator();
            // Status message last, truncated so a long line can't push the chips
            // off-screen.
            ui.add(egui::Label::new(&self.status).truncate());
        });
    }

    /// A dismissible banner for the most recent error, so it persists instead of
    /// being overwritten by the next transient status line.
    pub(super) fn error_banner(&mut self, ctx: &egui::Context)
    {
        let Some(message) = self.last_error.to_owned() else {
            return;
        };
        let color =
            egui::Color32::from_rgb(ERROR_BANNER_RED, ERROR_BANNER_GREEN, ERROR_BANNER_BLUE);
        egui::TopBottomPanel::top("error_banner").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Dismiss").clicked()
                {
                    self.last_error = None;
                }
                ui.colored_label(color, "âš ");
                ui.colored_label(color, message);
            });
        });
    }

    /// Modal confirmation for destructive actions. Runs the queued action on
    /// confirm, discards it on cancel.
    pub(super) fn confirm_modal(&mut self, ctx: &egui::Context)
    {
        let Some(confirm) = self.pending_confirm.as_ref() else {
            return;
        };
        let title = confirm.title.clone();
        let message = confirm.message.clone();
        let confirm_label = confirm.confirm_label.clone();

        let mut confirmed = None;
        // A real modal dims and blocks the background, so destructive-action
        // buttons behind it cannot be clicked while the prompt is open.
        let modal = egui::Modal::new(egui::Id::new("confirm_modal")).show(ctx, |ui| {
            ui.set_max_width(CONFIRM_MODAL_MAX_WIDTH);
            ui.heading(title);
            ui.label(message);
            ui.add_space(DETAIL_ROW_GAP);
            ui.horizontal(|ui| {
                if ui.button(confirm_label).clicked()
                {
                    confirmed = Some(true);
                }
                if ui.button("Cancel").clicked()
                {
                    confirmed = Some(false);
                }
            });
        });
        // Clicking the dimmed backdrop or pressing Escape cancels the action.
        if modal.should_close()
        {
            confirmed = Some(false);
        }

        match confirmed
        {
            Some(true) => {
                if let Some(confirm) = self.pending_confirm.take()
                {
                    self.run_confirmed_action(confirm.action);
                }
            }
            Some(false) => self.pending_confirm = None,
            None => {}
        }
    }
}
