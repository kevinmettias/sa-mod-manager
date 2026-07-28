use crate::prelude::*;
use eframe::egui;

use super::san_andreas_mod_ui::{
    ModInfoTab, ModStatusFilter, ModTelemetry, PendingRunStatus, ROW_HEIGHT, ReadmeProposal,
    ReadmeProposalState, SanAndreasModUi, TelemetryEvent,
};
use super::state::{ModConfigItem, UiTab};
use super::widgets::{
    infrastructure_grid, selected_profile_entries, should_add_mod_to_profile,
};

impl SanAndreasModUi {
    /// The top toolbar: profile + run target on the first row (the things you
    /// reach for constantly), game folder + maintenance on the second.
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.strong("SA Mod Manager"); // literal: allow external interface text or file-format spelling
            ui.separator();

            ui.label("Profile"); // literal: allow external interface text or file-format spelling
            let mut changed_profile = false;
            egui::ComboBox::from_id_salt("toolbar_profile")
                .selected_text(&self.selected_profile)
                .width(160.0)
                .show_ui(ui, |ui| {
                    for profile in &self.state.profiles {
                        let choice = profile.clone();
                        if ui
                            .selectable_value(&mut self.selected_profile, choice, profile)
                            .changed()
                        {
                            changed_profile = true;
                        }
                    }
                });
            if changed_profile {
                self.refresh();
            }
            ui.add_sized(
                [110.0, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.new_profile_input).hint_text("new profile"),
            );
            if ui.button("New").clicked() {
                self.create_profile();
            }

            ui.separator();

            // Run target (MO2's run dropdown), always at hand in the toolbar.
            let selected_label = if self.selected_run_target == 0 {
                "▶ Play current profile".to_string()
            } else {
                self.executables
                    .get(self.selected_run_target - 1)
                    .map(|tool| tool.name.clone())
                    .unwrap_or_else(|| "▶ Play current profile".to_string())
            };
            let tool_names: Vec<String> =
                self.executables.iter().map(|tool| tool.name.clone()).collect();
            egui::ComboBox::from_id_salt("toolbar_run_target")
                .selected_text(selected_label)
                .width(200.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.selected_run_target, 0, "▶ Play current profile");
                    for (index, name) in tool_names.iter().enumerate() {
                        ui.selectable_value(&mut self.selected_run_target, index + 1, name);
                    }
                });
            let idle = !self.is_busy();
            let run_label = if self.selected_run_target == 0 {
                "Run ▶"
            } else {
                "Run tool ▶"
            };
            if ui
                .add_enabled(idle, egui::Button::new(run_label))
                .on_hover_text("Launch the selected run target")
                .on_disabled_hover_text("A background task is running")
                .clicked()
            {
                self.run_selected_target(ui.ctx());
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Game"); // literal: allow external interface text or file-format spelling
            ui.add_sized(
                [340.0, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.game_root_input),
            );
            if ui
                .button("Browse…")
                .on_hover_text("Pick the GTA San Andreas folder")
                .clicked()
            {
                self.browse_game_folder();
            }
            if ui
                .button("Reload")
                .on_hover_text("Re-read manager state from disk (Ctrl+R)")
                .clicked()
            {
                self.refresh();
            }
            if ui
                .button("Initialize")
                .on_hover_text("Create the .sa-mod-manager state folders in the game directory")
                .clicked()
            {
                self.initialize_state();
            }
            let theme_label = if self.dark_mode { "Light" } else { "Dark" };
            if ui
                .button(theme_label)
                .on_hover_text("Switch between light and dark appearance")
                .clicked()
            {
                self.dark_mode = !self.dark_mode;
            }
            if let Some(summary) = self.pending_cleanup_summary() {
                ui.separator();
                let warn = ui.visuals().warn_fg_color;
                ui.colored_label(warn, summary);
                if ui.button("Clean finished").clicked() {
                    self.request_cleanup_finished();
                }
            }
        });
    }
}

impl SanAndreasModUi {
    /// The left filters column (MO2's Categories/filters pane): narrows the mod
    /// list by status, text, category, and conflicts, and hosts the content scan.
    pub(super) fn filters_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.strong("Game Setup"); // literal: allow external interface text or file-format spelling
        game_setup_summary(ui, &self.state.infrastructure);
        ui.separator();
        ui.strong("Filters"); // literal: allow external interface text or file-format spelling
        ui.separator();

        ui.label("Status"); // literal: allow external interface text or file-format spelling
        ui.selectable_value(&mut self.mod_filter_status, ModStatusFilter::All, "All");
        ui.selectable_value(&mut self.mod_filter_status, ModStatusFilter::Enabled, "Enabled");
        ui.selectable_value(&mut self.mod_filter_status, ModStatusFilter::Disabled, "Disabled");
        ui.separator();

        ui.label("Search"); // literal: allow external interface text or file-format spelling
        ui.add(
            egui::TextEdit::singleline(&mut self.mod_filter_text)
                .hint_text("mod id")
                .desired_width(f32::INFINITY),
        );
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Category"); // literal: allow external interface text or file-format spelling
            if ui.small_button("clear").clicked() {
                self.mod_filter_category = None;
            }
        });
        if ui
            .selectable_label(self.mod_filter_category.is_none(), "All categories")
            .clicked()
        {
            self.mod_filter_category = None;
        }
        for category in ContentCategory::all() {
            let selected = self.mod_filter_category == Some(category);
            if ui.selectable_label(selected, category.label()).clicked() {
                self.mod_filter_category = Some(category);
            }
        }
        ui.separator();

        // User-assigned categories (from mod annotations), if any exist.
        let user_categories: std::collections::BTreeSet<String> = self
            .mod_meta
            .values()
            .flat_map(|meta| meta.categories.iter().cloned())
            .collect();
        if !user_categories.is_empty() {
            ui.horizontal(|ui| {
                ui.label("My categories"); // literal: allow external interface text or file-format spelling
                if ui.small_button("clear").clicked() {
                    self.mod_filter_user_category = None;
                }
            });
            if ui
                .selectable_label(self.mod_filter_user_category.is_none(), "Any")
                .clicked()
            {
                self.mod_filter_user_category = None;
            }
            for category in &user_categories {
                let selected =
                    self.mod_filter_user_category.as_deref() == Some(category.as_str());
                if ui.selectable_label(selected, category).clicked() {
                    self.mod_filter_user_category = Some(category.clone());
                }
            }
            ui.separator();
        }

        let scanned = self.content_index.is_some();
        ui.add_enabled_ui(scanned, |ui| {
            ui.checkbox(&mut self.mod_filter_conflicts, "Conflicts only")
                .on_hover_text("Mods that overwrite or are overwritten (needs a content scan)");
        });
        if ui
            .button("Analyze content")
            .on_hover_text("Scan enabled mods for content flags and conflicts")
            .clicked()
        {
            self.rescan_content();
        }
        match &self.content_index {
            Some(index) => {
                let conflicts = index.conflict_count();
                if conflicts == 0 {
                    ui.weak("no conflicts");
                } else {
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, format!("{conflicts} conflicts"));
                }
            }
            None => {
                ui.weak("not analyzed");
            }
        }
        if !scanned && (self.mod_filter_category.is_some() || self.mod_filter_conflicts) {
            ui.weak("category/conflict filters need Analyze");
        }

        ui.separator();
        ui.strong("Diagnostics"); // literal: allow external interface text or file-format spelling
        self.diagnostics_summary(ui);
    }

    /// ModLoader last-run log health and CLEO health, from the last content scan.
    /// Each issue line is clickable and jumps to its viewer in the Content tab.
    fn diagnostics_summary(&mut self, ui: &mut egui::Ui) {
        // Snapshot counts first so the borrow on `self.*` is released before a
        // click mutates `self.tab`/`self.content_category`.
        let modloader = self
            .modloader_log
            .as_ref()
            .map(|log| (log.is_clean(), log.errors.len(), log.warnings.len()));
        let cleo = self.cleo_diagnostics.as_ref().map(|diag| {
            (
                diag.is_empty(),
                diag.blacklisted_plugins.len() + diag.fxt_conflicts.len() + diag.script_issues.len(),
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
                    egui::RichText::new(format!("ModLoader: {errors} err · {warnings} warn"))
                        .color(warn),
                )
                .on_hover_text("Open the ModLoader viewer")
                .clicked(),
        };
        if modloader_jump {
            self.content_category = Some(ContentCategory::ModLoader);
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
        if cleo_jump {
            self.content_category = Some(ContentCategory::Cleo);
            self.tab = UiTab::Content;
        }

        // Overwrite (unmanaged game-folder files) — only meaningful after a scan.
        if self.content_index.is_some() {
            let unmanaged = self.overwrite_files.len();
            if unmanaged == 0 {
                ui.weak("Unmanaged: none");
            } else if ui
                .selectable_label(
                    false,
                    egui::RichText::new(format!("Unmanaged: {unmanaged} files")).color(warn),
                )
                .on_hover_text("Game-folder files no enabled mod owns — see the Play tab")
                .clicked()
            {
                self.tab = UiTab::Run;
            }
        }
    }

    /// The centre pane — the always-visible mod list (load order) with its profile
    /// settings and per-profile root overrides. This is the heart of the window.
    pub(super) fn mods_center_panel(&mut self, ui: &mut egui::Ui) {
        let entries = selected_profile_entries(&self.state, &self.selected_profile);
        let enabled = entries.iter().filter(|entry| entry.enabled).count();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.heading(format!("Mods — {}", self.selected_profile));
            ui.label(format!("{enabled}/{} enabled", entries.len()));
        });
        egui::CollapsingHeader::new("Profile settings")
            .id_salt("profile_settings")
            .show(ui, |ui| {
                self.profile_management_panel(ui);
                self.profile_launch_args_panel(ui);
            });
        ui.separator();
        if entries.is_empty() {
            ui.label("No mods in this profile."); // literal: allow external interface text or file-format spelling
            ui.horizontal(|ui| {
                if ui.button("Import a mod").clicked() {
                    self.tab = UiTab::Import;
                }
            });
            return;
        }
        ui.horizontal(|ui| {
            if ui
                .button("＋ Separator")
                .on_hover_text("Add a labeled group divider at the top; move it with ▲/▼")
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
    pub(super) fn detail_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
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
        match self.tab {
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

impl SanAndreasModUi {
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let busy_label = self.task_label().map(str::to_string);
        ui.horizontal(|ui| {
            if let Some(label) = busy_label {
                ui.add(egui::Spinner::new());
                ui.strong(format!("{label}…"));
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

            if self.game_child.is_some() {
                ui.separator();
                ui.colored_label(egui::Color32::from_rgb(120, 190, 120), "▶ running");
            }
            if let Some(index) = &self.content_index {
                ui.separator();
                let conflicts = index.conflict_count();
                if conflicts > 0 {
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, format!("{conflicts} conflicts"));
                } else {
                    ui.weak("0 conflicts");
                }
            }
            let pending = self.pending_runs.len();
            if pending > 0 {
                ui.separator();
                let warn = ui.visuals().warn_fg_color;
                ui.colored_label(warn, format!("⚠ {pending} to clean"));
            }
            // Post-scan reality checks: what ModLoader logged last run and CLEO
            // health. Only shown when there is something to flag.
            if let Some(log) = &self.modloader_log {
                if !log.is_clean() {
                    ui.separator();
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, format!("ML {} err", log.errors.len()))
                        .on_hover_text("ModLoader reported errors last run — see the ModLoader viewer");
                }
            }
            if let Some(diag) = &self.cleo_diagnostics {
                let issues = diag.blacklisted_plugins.len()
                    + diag.fxt_conflicts.len()
                    + diag.script_issues.len();
                if issues > 0 {
                    ui.separator();
                    let warn = ui.visuals().warn_fg_color;
                    ui.colored_label(warn, format!("CLEO {issues}"))
                        .on_hover_text("CLEO health issues — see the CLEO viewer");
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
    pub(super) fn error_banner(&mut self, ctx: &egui::Context) {
        let Some(message) = self.last_error.clone() else {
            return;
        };
        let color = egui::Color32::from_rgb(200, 64, 64);
        egui::TopBottomPanel::top("error_banner").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Dismiss").clicked() {
                    self.last_error = None;
                }
                ui.colored_label(color, "⚠");
                ui.colored_label(color, message);
            });
        });
    }

    /// Modal confirmation for destructive actions. Runs the queued action on
    /// confirm, discards it on cancel.
    pub(super) fn confirm_modal(&mut self, ctx: &egui::Context) {
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
            ui.set_max_width(360.0);
            ui.heading(title);
            ui.label(message);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(confirm_label).clicked() {
                    confirmed = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    confirmed = Some(false);
                }
            });
        });
        // Clicking the dimmed backdrop or pressing Escape cancels the action.
        if modal.should_close() {
            confirmed = Some(false);
        }

        match confirmed {
            Some(true) => {
                if let Some(confirm) = self.pending_confirm.take() {
                    self.run_confirmed_action(confirm.action);
                }
            }
            Some(false) => self.pending_confirm = None,
            None => {}
        }
    }
}

impl SanAndreasModUi {
    /// Show a hint while files hover and consume any dropped onto the window.
    pub(super) fn handle_file_drops(&mut self, ctx: &egui::Context) {
        let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
        if hovering {
            egui::Area::new(egui::Id::new("drop_hint"))
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.heading("Drop a mod package or GTA folder");
                    });
                });
        }
        let dropped =
            ctx.input(|input| input.raw.dropped_files.iter().find_map(|file| file.path.clone()));
        if let Some(path) = dropped {
            self.handle_dropped_path(path);
        }
    }

    pub(super) fn home_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Overview");
        ui.label("Your recommended next step and where this profile sits in the workflow.");
        ui.separator();
        // Profile / enabled / cleanup live in the toolbar + status bar now, and
        // the current profile is the always-visible centre list, so Overview is
        // just guidance: next action, workflow stage, and recent signals. Game
        // Setup moved to the left panel so it is visible on every screen.
        self.next_action_panel(ui);
        ui.separator();
        self.workflow_panel(ui);
        ui.separator();
        self.recent_signal_panel(ui);
    }
}

fn summary_tile(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.group(|ui| {
        ui.set_min_width(136.0);
        ui.label(label);
        ui.strong(value);
    });
}

/// A dense inline stat (`label value`) with a trailing separator, for packing
/// many counts onto one wrapping line instead of a wall of boxed tiles.
fn stat_inline(ui: &mut egui::Ui, label: &str, value: impl std::fmt::Display) {
    ui.label(label);
    ui.strong(value.to_string());
    ui.separator();
}

/// Shorten `text` to at most `max` characters with an ellipsis, so long free-text
/// cells cannot blow out a grid's width (full text stays available on hover).
fn clip_text(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let mut clipped: String = text.chars().take(max.saturating_sub(1)).collect();
        clipped.push('…');
        clipped
    } else {
        text.to_string()
    }
}

/// A compact install-status list for the left panel: one ✓/✗ line per detected
/// component (executable, ModLoader, CLEO, ASI), hovering shows the full path.
/// Fits the narrow column, unlike the wide `infrastructure_grid`.
fn game_setup_summary(ui: &mut egui::Ui, infrastructure: &[super::state::InfrastructureItem]) {
    if infrastructure.is_empty() {
        ui.weak("Set the game folder to detect components.");
        return;
    }
    let present_color = egui::Color32::from_rgb(120, 190, 120);
    let missing_color = ui.visuals().warn_fg_color;
    for item in infrastructure {
        let (mark, color) = if item.present {
            ("✓", present_color)
        } else {
            ("✗", missing_color)
        };
        ui.horizontal(|ui| {
            ui.colored_label(color, mark);
            ui.label(&item.label);
        })
        .response
        .on_hover_text(item.path.display().to_string());
    }
}

fn enabled_profile_count(ui_state: &SanAndreasModUi) -> usize {
    selected_profile_entries(&ui_state.state, &ui_state.selected_profile)
        .iter()
        .filter(|entry| entry.enabled)
        .count()
}

impl SanAndreasModUi {
    fn next_action_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.heading("Recommended Next Step");
            let action = recommended_action(self);
            ui.label(action.detail);
            ui.horizontal(|ui| {
                if ui.button(action.primary_label).clicked() {
                    match action.primary_tab {
                        Some(tab) => self.tab = tab,
                        None => self.cleanup_stale_pending_runs(),
                    }
                }
                if let Some((label, tab)) = action.secondary {
                    if ui.button(label).clicked() {
                        self.tab = tab;
                    }
                }
            });
        });
    }

    fn workflow_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Workflow");
        // Game setup lives in the always-visible left panel, so it is not repeated
        // here — the workflow starts from importing mods.
        egui::Grid::new("home_workflow")
            .striped(true)
            .min_col_width(120.0)
            .show(ui, |ui| {
                workflow_row(
                    ui,
                    "1. Import",
                    import_status(self),
                    "Review archive contents and readme-driven install proposals.",
                );
                workflow_row(
                    ui,
                    "2. Profile",
                    profile_status(self),
                    "Enable mods and set load order in the centre list.",
                );
                workflow_row(
                    ui,
                    "3. Play",
                    run_status(self),
                    "Launch temporarily, then clean the game folder back to vanilla.",
                );
            });
    }

    fn recent_signal_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Recent Signals");
        let issues = self.telemetry.missing_sources + self.telemetry.blocked_bootstrap;
        ui.horizontal_wrapped(|ui| {
            summary_tile(
                ui,
                "Readme proposals",
                &self.readme_proposals.len().to_string(),
            );
            summary_tile(ui, "Pending cleanup", &self.pending_runs.len().to_string());
            summary_tile(
                ui,
                "Overwrites tracked",
                &self.telemetry.overwritten_files.to_string(),
            );
            summary_tile(ui, "Issues tracked", &issues.to_string());
        });
        ui.horizontal(|ui| {
            if ui.button("Review telemetry").clicked() {
                self.tab = UiTab::Telemetry;
            }
            if ui.button("Check cleanup").clicked() {
                self.tab = UiTab::Run;
            }
        });
    }
}

struct RecommendedAction {
    detail: &'static str,
    primary_label: &'static str,
    primary_tab: Option<UiTab>,
    secondary: Option<(&'static str, UiTab)>,
}

fn recommended_action(ui_state: &SanAndreasModUi) -> RecommendedAction {
    if has_cleanable_pending_run(ui_state) {
        return RecommendedAction {
            detail: "Finished temporary files are still materialized. Clean them before launching another profile.",
            primary_label: "Clean finished runs",
            primary_tab: None,
            secondary: Some(("Review cleanup", UiTab::Run)),
        };
    }
    if !game_executable_present(ui_state) {
        return RecommendedAction {
            detail: "The game executable was not detected in the configured folder.",
            primary_label: "Review setup",
            primary_tab: Some(UiTab::Home),
            secondary: None,
        };
    }
    if ui_state.state.mods.is_empty() {
        return RecommendedAction {
            detail: "Import a package, review its readme hints, then add it to a profile.",
            primary_label: "Import a mod",
            primary_tab: Some(UiTab::Import),
            secondary: None,
        };
    }
    if enabled_profile_count(ui_state) == 0 {
        return RecommendedAction {
            detail: "The selected profile has no enabled mods yet — tick them in the centre list.",
            primary_label: "Open library",
            primary_tab: Some(UiTab::Mods),
            secondary: None,
        };
    }
    RecommendedAction {
        detail: "This profile is ready for an ephemeral launch.",
        primary_label: "Play profile",
        primary_tab: Some(UiTab::Run),
        secondary: Some(("View telemetry", UiTab::Telemetry)),
    }
}

fn workflow_row(ui: &mut egui::Ui, stage: &str, status: WorkflowStatus, detail: &str) {
    ui.strong(stage);
    ui.label(status.label());
    ui.label(detail);
    ui.end_row();
}

#[derive(Clone, Copy)]
enum WorkflowStatus {
    Ready,
    Review,
    Waiting,
}

impl WorkflowStatus {
    fn label(self) -> &'static str {
        match self {
            WorkflowStatus::Ready => "ready",
            WorkflowStatus::Review => "needs review",
            WorkflowStatus::Waiting => "waiting",
        }
    }
}

fn import_status(ui_state: &SanAndreasModUi) -> WorkflowStatus {
    if !ui_state.readme_proposals.is_empty() {
        WorkflowStatus::Review
    } else if ui_state.state.mods.is_empty() {
        WorkflowStatus::Waiting
    } else {
        WorkflowStatus::Ready
    }
}

fn profile_status(ui_state: &SanAndreasModUi) -> WorkflowStatus {
    if enabled_profile_count(ui_state) > 0 {
        WorkflowStatus::Ready
    } else if ui_state.state.mods.is_empty() {
        WorkflowStatus::Waiting
    } else {
        WorkflowStatus::Review
    }
}

fn run_status(ui_state: &SanAndreasModUi) -> WorkflowStatus {
    if !ui_state.pending_runs.is_empty() {
        WorkflowStatus::Review
    } else if enabled_profile_count(ui_state) > 0 {
        WorkflowStatus::Ready
    } else {
        WorkflowStatus::Waiting
    }
}

fn readiness_label(ui_state: &SanAndreasModUi) -> &'static str {
    match run_status(ui_state) {
        WorkflowStatus::Ready => "Ready",
        WorkflowStatus::Review => "Needs review",
        WorkflowStatus::Waiting => "Setup incomplete",
    }
}

fn has_cleanable_pending_run(ui_state: &SanAndreasModUi) -> bool {
    ui_state
        .pending_runs
        .iter()
        .any(|record| record.status != PendingRunStatus::Running)
}

fn count_pending_status(ui_state: &SanAndreasModUi, status: PendingRunStatus) -> usize {
    ui_state
        .pending_runs
        .iter()
        .filter(|record| record.status == status)
        .count()
}

fn game_executable_present(ui_state: &SanAndreasModUi) -> bool {
    ui_state.state.infrastructure.iter().any(|item| {
        item.present && (item.label == "Steam executable" || item.label == "Classic executable")
    })
}

impl SanAndreasModUi {
    /// Profile lifecycle actions that were previously CLI-only: set-active,
    /// copy, rename, and delete.
    fn profile_management_panel(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                if ui
                    .button("Set active")
                    .on_hover_text("Use this profile by default (like `profile-use`)")
                    .clicked()
                {
                    self.set_active_selected_profile();
                }
                if ui
                    .button("All off (vanilla)")
                    .on_hover_text("Disable every mod so the next run is vanilla")
                    .clicked()
                {
                    self.set_all_selected_profile_mods(ProfileModActivation::Disabled);
                }
                if ui
                    .button("All on")
                    .on_hover_text("Enable every mod in this profile")
                    .clicked()
                {
                    self.set_all_selected_profile_mods(ProfileModActivation::Enabled);
                }
                if ui
                    .button("Delete")
                    .on_hover_text("Delete this profile (asks first)")
                    .clicked()
                {
                    self.request_delete_profile();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Copy to");
                ui.add_sized(
                    [140.0, ROW_HEIGHT],
                    egui::TextEdit::singleline(&mut self.copy_profile_input),
                );
                if ui.button("Copy").clicked() {
                    self.copy_selected_profile();
                }
                ui.label("Rename to");
                ui.add_sized(
                    [140.0, ROW_HEIGHT],
                    egui::TextEdit::singleline(&mut self.rename_profile_input),
                );
                if ui.button("Rename").clicked() {
                    self.rename_selected_profile();
                }
            });
        });
    }

    /// Editor for the profile's launch arguments (space-separated), mirroring the
    /// CLI `profile-args` verb.
    fn profile_launch_args_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Launch args");
            ui.add_sized(
                [360.0, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.launch_args_input)
                    .hint_text("-nointro -windowed"),
            );
            if ui
                .button("Save args")
                .on_hover_text("Set this profile's launch arguments")
                .clicked()
            {
                self.save_launch_args();
            }
        });
    }
}

impl SanAndreasModUi {
    /// A Mod-Organizer-style ordered mod list: a drag handle for seamless
    /// drag-to-reorder, ▲/▼ nudges, a directly editable priority index, and a
    /// per-mod enable checkbox. All mutations are collected during the immutable
    /// pass over `entries` and applied afterwards, so `self` is never borrowed
    /// mutably while the rows are drawn.
    /// Resolve which mods the filter bar leaves visible. Returns `None` when no
    /// filter is active, which keeps the list fully reorderable; `Some(set)` of
    /// mod ids otherwise. Category/conflict filters consult the last content scan.
    fn compute_visible_mods(
        &self,
        entries: &[ProfileModEntry],
    ) -> Option<std::collections::BTreeSet<String>> {
        let text = self.mod_filter_text.trim().to_ascii_lowercase();
        let filtering = !text.is_empty()
            || self.mod_filter_status != ModStatusFilter::All
            || self.mod_filter_category.is_some()
            || self.mod_filter_conflicts
            || self.mod_filter_user_category.is_some();
        if !filtering {
            return None;
        }
        let flags = self.content_index.as_ref().map(per_mod_flags);
        let set = entries
            .iter()
            .filter(|entry| {
                let text_ok = text.is_empty() || entry.id.to_ascii_lowercase().contains(&text);
                let status_ok = match self.mod_filter_status {
                    ModStatusFilter::All => true,
                    ModStatusFilter::Enabled => entry.enabled,
                    ModStatusFilter::Disabled => !entry.enabled,
                };
                let mod_flags = flags.as_ref().and_then(|map| map.get(&entry.id));
                let category_ok = self
                    .mod_filter_category
                    .is_none_or(|category| mod_flags.is_some_and(|f| f.categories.contains(&category)));
                let conflict_ok = !self.mod_filter_conflicts
                    || mod_flags.is_some_and(|f| f.overwrites_others || f.overwritten);
                let user_category_ok = self.mod_filter_user_category.as_ref().is_none_or(|wanted| {
                    self.mod_meta.get(&entry.id).is_some_and(|meta| {
                        meta.categories
                            .iter()
                            .any(|category| category.eq_ignore_ascii_case(wanted))
                    })
                });
                text_ok && status_ok && category_ok && conflict_ok && user_category_ok
            })
            .map(|entry| entry.id.clone())
            .collect();
        Some(set)
    }

    pub(super) fn profile_mod_order_list(
        &mut self,
        ui: &mut egui::Ui,
        entries: &[ProfileModEntry],
        visible: Option<&std::collections::BTreeSet<String>>,
    ) {
        let count = entries.len();
        let order_ids: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        // Per-mod content flags come from the last content scan (if any). Cheap
        // to derive from the cached index; absent until the user analyzes.
        let flags_map = self.content_index.as_ref().map(per_mod_flags);
        let overwrite_color = egui::Color32::from_rgb(120, 190, 120);
        let overwritten_color = ui.visuals().warn_fg_color;
        // Subsystem badges (ModLoader / CLEO / ASI) derived from each mod's
        // install-root kinds — always shown, no content scan required.
        let subsystems: std::collections::BTreeMap<String, (bool, bool, bool)> = self
            .state
            .mods
            .iter()
            .map(|item| {
                let mut modloader = false;
                let mut cleo = false;
                let mut asi = false;
                for root in &item.config.install_roots {
                    match root.kind.to_ascii_lowercase().as_str() {
                        "modloader" => modloader = true,
                        "cleo" | "cleo_text" | "cleo_plugin" => cleo = true,
                        "asi" | "plugin" => asi = true,
                        _ => {}
                    }
                }
                (item.config.id.clone(), (modloader, cleo, asi))
            })
            .collect();
        // Per-mod color label + note presence, snapshotted so rows read them
        // without holding a borrow on `self.mod_meta`.
        let mod_marks: std::collections::BTreeMap<String, (Option<egui::Color32>, bool)> = self
            .mod_meta
            .iter()
            .map(|(id, meta)| {
                (
                    id.clone(),
                    (
                        meta.color.as_deref().and_then(meta_color),
                        !meta.note.trim().is_empty(),
                    ),
                )
            })
            .collect();
        // A filter hides rows, which would make drag targets and priority indices
        // point at the wrong slots, so ordering is disabled while one is active.
        let reorderable = visible.is_none();

        // Deferred side effects (at most one fires per frame in practice).
        let mut activation: Option<(String, bool)> = None;
        let mut remove: Option<String> = None;
        let mut reorder: Option<Vec<String>> = None;
        // (mod id, conflicts_only) for a "Show in Content" cross-link click.
        let mut focus: Option<(String, bool)> = None;
        // Mod id to open the per-mod info window for.
        let mut details: Option<String> = None;

        // Separators (labeled dividers) render only in the unfiltered, reorderable
        // view — they are organizational and reordering is off while filtering.
        let entries_len = entries.len();
        let separators = if reorderable {
            self.separators.clone()
        } else {
            Vec::new()
        };
        let collapsed = self.collapsed_separators.clone();
        let mut sep_edit = self.separator_edit.clone();
        let hidden = collapsed_hidden_indices(&separators, &collapsed, entries_len);
        // Deferred separator effects.
        let mut sep_remove: Option<String> = None;
        let mut sep_move: Option<(String, usize)> = None;
        let mut sep_collapse: Option<String> = None;
        let mut sep_rename: Option<(String, String)> = None;
        let mut sep_start_edit: Option<String> = None;

        // Fixed column widths so every row lines up as a real table; the name
        // column flexes to fill whatever the centre pane leaves.
        const GRIP_W: f32 = 18.0;
        const ON_W: f32 = 26.0;
        const PRIO_W: f32 = 52.0;
        const MOVE_W: f32 = 54.0;
        const SUBSYS_W: f32 = 78.0;
        const FLAGS_W: f32 = 170.0;
        const MAX_CHIPS: usize = 4;
        // Reserve for the trailing Remove/⋯ actions, inter-cell spacing, and the
        // scrollbar; underfill (a small gap) rather than overflow the row.
        let name_w = (ui.available_width()
            - GRIP_W
            - ON_W
            - PRIO_W
            - MOVE_W
            - SUBSYS_W
            - FLAGS_W
            - 160.0)
            .max(140.0);

        ui.horizontal(|ui| {
            table_cell(ui, GRIP_W, |_ui| {});
            table_cell(ui, ON_W, |ui| {
                ui.strong("On");
            });
            table_cell(ui, PRIO_W, |ui| {
                ui.strong("#").on_hover_text("Load-order priority");
            });
            table_cell(ui, MOVE_W, |_ui| {});
            table_cell(ui, name_w, |ui| {
                ui.strong("Mod");
            });
            table_cell(ui, SUBSYS_W, |ui| {
                ui.strong("Subsys").on_hover_text("ModLoader / CLEO / ASI");
            });
            table_cell(ui, FLAGS_W, |ui| {
                ui.strong("Flags");
            });
            ui.strong("Actions");
        });
        ui.separator();
        if !reorderable {
            ui.weak("Reordering is disabled while a filter is active — clear filters to drag or renumber.");
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Reorder resolved from a completed drag (source → insertion slot).
                let mut drag_from: Option<usize> = None;
                let mut drop_to: Option<usize> = None;

                for (idx, entry) in entries.iter().enumerate() {
                    // Any separators anchored to this display slot render above it.
                    for sep in separators.iter().filter(|sep| sep.position == idx) {
                        let editing = sep_edit.as_ref().is_some_and(|(id, _)| *id == sep.id);
                        let mut buf =
                            sep_edit.as_ref().map(|(_, name)| name.clone()).unwrap_or_default();
                        let action =
                            separator_header(ui, sep, collapsed.contains(&sep.id), editing, &mut buf);
                        if editing {
                            if let Some((_, name)) = sep_edit.as_mut() {
                                *name = buf.clone();
                            }
                        }
                        match action {
                            Some(SepAction::ToggleCollapse) => sep_collapse = Some(sep.id.clone()),
                            Some(SepAction::StartEdit) => sep_start_edit = Some(sep.id.clone()),
                            Some(SepAction::Commit) => sep_rename = Some((sep.id.clone(), buf)),
                            Some(SepAction::Remove) => sep_remove = Some(sep.id.clone()),
                            Some(SepAction::MoveUp) => {
                                sep_move = Some((sep.id.clone(), sep.position.saturating_sub(1)));
                            }
                            Some(SepAction::MoveDown) => {
                                sep_move =
                                    Some((sep.id.clone(), (sep.position + 1).min(entries_len)));
                            }
                            None => {}
                        }
                    }
                    // Skip rows hidden by the active filter, but keep `idx` tied to
                    // the full list so priority ranks stay truthful.
                    if let Some(set) = visible {
                        if !set.contains(&entry.id) {
                            continue;
                        }
                    }
                    // Hidden inside a collapsed separator section.
                    if hidden.contains(&idx) {
                        continue;
                    }
                    // A move requested by this row via a button or the index field.
                    let mut move_to: Option<usize> = None;
                    let row = ui
                        .horizontal(|ui| {
                            // Drag handle — the drag source carries this row index.
                            table_cell(ui, GRIP_W, |ui| {
                                if reorderable {
                                    ui.dnd_drag_source(
                                        egui::Id::new(("mod_grip", &entry.id)),
                                        idx,
                                        |ui| {
                                            ui.label("⣿").on_hover_text("Drag to reorder");
                                        },
                                    );
                                } else {
                                    ui.weak("⣿");
                                }
                            });

                            table_cell(ui, ON_W, |ui| {
                                let mut enabled = entry.enabled;
                                if ui
                                    .checkbox(&mut enabled, "")
                                    .on_hover_text("Enable or disable this mod in the current profile")
                                    .changed()
                                {
                                    activation = Some((entry.id.clone(), enabled));
                                }
                            });

                            table_cell(ui, PRIO_W, |ui| {
                                if reorderable {
                                    // 1-based priority; typing a new number moves it.
                                    let mut position = idx + 1;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut position)
                                                .range(1..=count.max(1))
                                                .speed(0.1),
                                        )
                                        .on_hover_text(
                                            "Priority — type a number to move this mod there",
                                        )
                                        .changed()
                                    {
                                        let target = position.clamp(1, count) - 1;
                                        if target != idx {
                                            move_to = Some(target);
                                        }
                                    }
                                } else {
                                    // Read-only true load-order rank while filtered.
                                    ui.monospace(format!("{:>3}", idx + 1)).on_hover_text(
                                        "Load-order position — clear filters to change",
                                    );
                                }
                            });

                            table_cell(ui, MOVE_W, |ui| {
                                if reorderable {
                                    if ui
                                        .add_enabled(idx > 0, egui::Button::new("▲").small())
                                        .on_hover_text("Move up (loads earlier)")
                                        .clicked()
                                    {
                                        move_to = Some(idx - 1);
                                    }
                                    if ui
                                        .add_enabled(idx + 1 < count, egui::Button::new("▼").small())
                                        .on_hover_text("Move down (loads later, overwrites)")
                                        .clicked()
                                    {
                                        move_to = Some(idx + 1);
                                    }
                                }
                            });

                            // The mod name is itself a drag source (when ordering
                            // is allowed), so the row body — not just the ⣿ handle
                            // — can be grabbed to reorder. Truncated to the column
                            // and greyed when disabled.
                            table_cell(ui, name_w, |ui| {
                                let marks =
                                    mod_marks.get(&entry.id).copied().unwrap_or((None, false));
                                if let Some(color) = marks.0 {
                                    ui.colored_label(color, "●").on_hover_text("Color label");
                                }
                                let text = if entry.enabled {
                                    egui::RichText::new(&entry.id)
                                } else {
                                    egui::RichText::new(&entry.id).weak()
                                };
                                // Double-click opens the Mod Info dialog. When the
                                // list is reorderable the name is also a drag
                                // source (click_and_drag), so dragging it still
                                // reorders — we feed the drag payload manually.
                                let sense = if reorderable {
                                    egui::Sense::click_and_drag()
                                } else {
                                    egui::Sense::click()
                                };
                                let name =
                                    ui.add(egui::Label::new(text).truncate().sense(sense));
                                if reorderable && name.dragged() {
                                    egui::DragAndDrop::set_payload(ui.ctx(), idx);
                                }
                                if name.double_clicked() {
                                    details = Some(entry.id.clone());
                                }
                                name.on_hover_text(entry.config.display().to_string());
                                if marks.1 {
                                    ui.small("📝").on_hover_text("Has a note (see Details)");
                                }
                            });

                            // Subsystem badges (always on): which loader systems
                            // this mod installs into.
                            table_cell(ui, SUBSYS_W, |ui| {
                                if let Some((modloader, cleo, asi)) =
                                    subsystems.get(&entry.id).copied()
                                {
                                    if modloader {
                                        ui.colored_label(egui::Color32::from_rgb(90, 150, 220), "M")
                                            .on_hover_text("ModLoader mod");
                                    }
                                    if cleo {
                                        ui.colored_label(egui::Color32::from_rgb(120, 190, 120), "C")
                                            .on_hover_text("CLEO script");
                                    }
                                    if asi {
                                        ui.colored_label(egui::Color32::from_rgb(220, 140, 60), "A")
                                            .on_hover_text("ASI plugin");
                                    }
                                }
                            });

                            // Content flags (MO2-style): conflict arrows first,
                            // then compact category chips (capped). Only present
                            // once the profile's content has been analyzed.
                            table_cell(ui, FLAGS_W, |ui| {
                                if let Some(flags) =
                                    flags_map.as_ref().and_then(|map| map.get(&entry.id))
                                {
                                    if flags.overwrites_others {
                                        ui.colored_label(overwrite_color, "⬆").on_hover_text(
                                            "Overwrites files from lower-priority mods",
                                        );
                                    }
                                    if flags.overwritten {
                                        ui.colored_label(overwritten_color, "⬇").on_hover_text(
                                            "Some files are overwritten by higher-priority mods",
                                        );
                                    }
                                    for category in flags.categories.iter().take(MAX_CHIPS) {
                                        ui.small(category.short_label())
                                            .on_hover_text(category.label());
                                    }
                                    let extra = flags.categories.len().saturating_sub(MAX_CHIPS);
                                    if extra > 0 {
                                        ui.small(format!("+{extra}"));
                                    }
                                }
                            });

                            if ui
                                .button("Remove")
                                .on_hover_text("Remove this mod from the profile (asks first)")
                                .clicked()
                            {
                                remove = Some(entry.id.clone());
                            }
                            ui.menu_button("⋯", |ui| {
                                if ui.button("Details…").clicked() {
                                    details = Some(entry.id.clone());
                                    ui.close_menu();
                                }
                                if ui.button("Show files in Content").clicked() {
                                    focus = Some((entry.id.clone(), false));
                                    ui.close_menu();
                                }
                                if ui.button("Show conflicts in Content").clicked() {
                                    focus = Some((entry.id.clone(), true));
                                    ui.close_menu();
                                }
                            })
                            .response
                            .on_hover_text("Open this mod's details, files, or conflicts");
                        })
                        .response;

                    if let Some(target) = move_to {
                        reorder = Some(move_in_list(&order_ids, idx, target));
                    }

                    // Drag feedback + drop resolution over the whole row rect —
                    // only when ordering is allowed (no filter active).
                    if reorderable {
                        let row_zone = ui.interact(
                            row.rect,
                            egui::Id::new(("mod_row", &entry.id)),
                            egui::Sense::hover(),
                        );
                        if row_zone.dnd_hover_payload::<usize>().is_some() {
                            let center_y = row.rect.center().y;
                            let pointer_y = ui
                                .input(|input| input.pointer.interact_pos().map(|pos| pos.y))
                                .unwrap_or(center_y);
                            let line_y = if pointer_y < center_y {
                                row.rect.top()
                            } else {
                                row.rect.bottom()
                            };
                            ui.painter().hline(
                                row.rect.x_range(),
                                line_y,
                                egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                            );
                        }
                        if let Some(payload) = row_zone.dnd_release_payload::<usize>() {
                            let center_y = row.rect.center().y;
                            let pointer_y = ui
                                .input(|input| input.pointer.interact_pos().map(|pos| pos.y))
                                .unwrap_or(center_y);
                            drag_from = Some(*payload);
                            drop_to = Some(if pointer_y < center_y { idx } else { idx + 1 });
                        }
                    }
                }

                // Separators positioned at or past the end sit below every mod.
                for sep in separators.iter().filter(|sep| sep.position >= entries_len) {
                    let editing = sep_edit.as_ref().is_some_and(|(id, _)| *id == sep.id);
                    let mut buf = sep_edit.as_ref().map(|(_, name)| name.clone()).unwrap_or_default();
                    let action =
                        separator_header(ui, sep, collapsed.contains(&sep.id), editing, &mut buf);
                    if editing {
                        if let Some((_, name)) = sep_edit.as_mut() {
                            *name = buf.clone();
                        }
                    }
                    match action {
                        Some(SepAction::ToggleCollapse) => sep_collapse = Some(sep.id.clone()),
                        Some(SepAction::StartEdit) => sep_start_edit = Some(sep.id.clone()),
                        Some(SepAction::Commit) => sep_rename = Some((sep.id.clone(), buf)),
                        Some(SepAction::Remove) => sep_remove = Some(sep.id.clone()),
                        Some(SepAction::MoveUp) => {
                            sep_move = Some((sep.id.clone(), sep.position.saturating_sub(1)));
                        }
                        Some(SepAction::MoveDown) => {
                            sep_move = Some((sep.id.clone(), (sep.position + 1).min(entries_len)));
                        }
                        None => {}
                    }
                }

                if let (Some(from), Some(to)) = (drag_from, drop_to) {
                    // A drop onto the source's own slot (or its lower edge) is a no-op.
                    if to != from && to != from + 1 {
                        let adjusted = if from < to { to - 1 } else { to };
                        reorder = Some(move_in_list(&order_ids, from, adjusted));
                    }
                }
            });

        if let Some((mod_id, enabled)) = activation {
            let activation = if enabled {
                ProfileModActivation::Enabled
            } else {
                ProfileModActivation::Disabled
            };
            self.set_mod_activation(&mod_id, activation);
        }
        if let Some(ordered_ids) = reorder {
            self.reorder_profile_mods(ordered_ids);
        }
        if let Some(mod_id) = remove {
            self.request_remove_mod(&mod_id);
        }
        if let Some((mod_id, conflicts_only)) = focus {
            self.focus_mod_in_content(&mod_id, conflicts_only);
        }
        if let Some(mod_id) = details {
            self.open_mod_info(&mod_id);
        }

        // Persist the in-progress rename buffer across frames, then apply any
        // separator action (start-edit overrides the buffer; commit/others write).
        self.separator_edit = sep_edit;
        if let Some(id) = sep_start_edit {
            let name = self
                .separators
                .iter()
                .find(|sep| sep.id == id)
                .map(|sep| sep.name.clone())
                .unwrap_or_default();
            self.separator_edit = Some((id, name));
        }
        if let Some(id) = sep_collapse {
            self.toggle_separator_collapsed(&id);
        }
        if let Some((id, name)) = sep_rename {
            self.rename_separator(&id, name);
        }
        if let Some((id, position)) = sep_move {
            self.move_separator(&id, position);
        }
        if let Some(id) = sep_remove {
            self.remove_separator(&id);
        }
    }

    /// Per-profile install-root overrides (disable or retarget a single root),
    /// previously reachable only via `profile-root`/`profile-root-target`.
    fn profile_root_overrides_panel(&mut self, ui: &mut egui::Ui, entries: &[ProfileModEntry]) {
        let mods = self.state.mods.clone();
        let has_any = entries.iter().any(|entry| {
            mods.iter()
                .any(|item| item.config.id == entry.id && !item.config.install_roots.is_empty())
        });
        if !has_any {
            return;
        }
        ui.separator();
        ui.heading("Per-Profile Install Root Overrides");
        ui.label("Disable or retarget individual install roots for this profile without editing the mod.");
        for entry in entries {
            let Some(item) = mods.iter().find(|item| item.config.id == entry.id) else {
                continue;
            };
            if item.config.install_roots.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(format!(
                "{} — {} roots",
                entry.id,
                item.config.install_roots.len()
            ))
            .id_salt(format!("root_overrides_{}", entry.id))
            .show(ui, |ui| {
                for root in &item.config.install_roots {
                    self.profile_root_override_row(ui, &entry.id, entry, root);
                }
            });
        }
    }

    fn profile_root_override_row(
        &mut self,
        ui: &mut egui::Ui,
        mod_id: &str,
        entry: &ProfileModEntry,
        root: &ModInstallRootJson,
    ) {
        let key = format!("{mod_id}#{}", root.source);
        let override_entry = entry.root_overrides.get(&root.source);
        let mut enabled = override_entry.and_then(|o| o.enabled).unwrap_or(root.enabled);
        let effective_target = override_entry
            .and_then(|o| o.target.clone())
            .unwrap_or_else(|| root.target.clone());
        let mut enabled_changed = false;
        let mut save_target = None;
        // Borrow the edit buffer up front so the closure never touches `self`.
        let target_buf = self
            .profile_root_target_edits
            .entry(key.clone())
            .or_insert(effective_target);
        ui.horizontal(|ui| {
            enabled_changed = ui
                .checkbox(&mut enabled, "")
                .on_hover_text("Enable this install root for this profile")
                .changed();
            ui.label(&root.source);
            ui.label("→");
            ui.add_sized([180.0, ROW_HEIGHT], egui::TextEdit::singleline(target_buf));
            if ui
                .button("Retarget")
                .on_hover_text("Point this root at a different game target for this profile")
                .clicked()
            {
                save_target = Some(target_buf.clone());
            }
        });
        if enabled_changed {
            self.set_profile_root_enabled(mod_id, &root.source, enabled);
        }
        if let Some(target) = save_target {
            self.save_profile_root_target(mod_id, &root.source, &target);
            self.profile_root_target_edits.remove(&key);
        }
    }
}

impl SanAndreasModUi {
    pub(super) fn mods_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Library"); // literal: allow external interface text or file-format spelling
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, "Imported mods", &self.state.mods.len().to_string());
            summary_tile(
                ui,
                "Selected",
                &self
                    .state
                    .mods
                    .iter()
                    .filter(|item| item.in_selected_profile)
                    .count()
                    .to_string(),
            );
            summary_tile(
                ui,
                "Install roots",
                &self
                    .state
                    .mods
                    .iter()
                    .map(|item| item.config.install_roots.len())
                    .sum::<usize>()
                    .to_string(),
            );
        });
        ui.label(
            "Review imported packages, edit install roots, and select mods for the active profile.",
        );
        ui.separator();
        if self.state.mods.is_empty() {
            ui.label("No imported mods."); // literal: allow external interface text or file-format spelling
            if ui.button("Import a mod").clicked() {
                self.tab = UiTab::Import;
            }
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for item in self.state.mods.clone() {
                self.mod_config_card(ui, &item);
                ui.add_space(6.0);
            }
        });
    }
}

impl SanAndreasModUi {
    pub(super) fn mod_config_card(&mut self, ui: &mut egui::Ui, item: &ModConfigItem) {
        ui.group(|ui| {
            self.mod_config_card_header(ui, item);
            let path_display = item.path.display().to_string();
            ui.monospace(format!("config: {path_display}"));
            ui.monospace(format!("package: {}", item.config.package.display()));
            if let Some(source_root) = &item.config.source_root {
                ui.monospace(format!("library files: {}", source_root.display()));
            }
            ui.separator();
            ui.strong("Install locations");
            if item.config.install_roots.is_empty() {
                ui.label("No install roots were detected. Review this package before running it.");
            }
            for (idx, root) in item.config.install_roots.iter().enumerate() {
                self.mod_install_root_editor(ui, item, idx, root);
            }
        });
    }
}

impl SanAndreasModUi {
    fn mod_install_root_editor(
        &mut self,
        ui: &mut egui::Ui,
        item: &ModConfigItem,
        root_index: usize,
        root: &ModInstallRootJson,
    ) {
        let key = mod_root_edit_key(&item.path, root_index);
        let mut save_root = None;
        let edit = self
            .mod_root_edits
            .entry(key.clone())
            .or_insert_with(|| root.clone());
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut edit.enabled, "enabled");
                ui.checkbox(&mut edit.optional, "optional");
                ui.label(format!("root {root_index}"));
            });
            ui.horizontal(|ui| {
                ui.label("source");
                ui.add_sized(
                    [260.0, ROW_HEIGHT],
                    egui::TextEdit::singleline(&mut edit.source),
                );
                ui.label("target");
                ui.add_sized(
                    [260.0, ROW_HEIGHT],
                    egui::TextEdit::singleline(&mut edit.target),
                );
            });
            ui.horizontal(|ui| {
                ui.label("kind");
                install_kind_combo(ui, &key, &mut edit.kind);
                if ui.button("Save root").clicked() {
                    save_root = Some(edit.clone());
                }
                if ui.button("Reset").clicked() {
                    *edit = root.clone();
                }
            });
        });
        if let Some(updated) = save_root {
            // Keep the in-progress edit on failure so a rejected source/target can
            // be corrected instead of silently reverting.
            if self.save_mod_install_root(&item.path, root_index, updated) {
                self.mod_root_edits.remove(&key);
            }
        }
    }
}

fn mod_root_edit_key(path: &Path, root_index: usize) -> String {
    format!("{}#{root_index}", path.display())
}

/// The preset color-label palette for mod annotations (name → swatch), mirroring
/// MO2's color labels. Names are what get persisted.
const MOD_COLORS: [(&str, egui::Color32); 7] = [
    ("red", egui::Color32::from_rgb(210, 80, 80)),
    ("orange", egui::Color32::from_rgb(220, 140, 60)),
    ("yellow", egui::Color32::from_rgb(210, 190, 80)),
    ("green", egui::Color32::from_rgb(120, 190, 120)),
    ("blue", egui::Color32::from_rgb(90, 150, 220)),
    ("purple", egui::Color32::from_rgb(170, 120, 210)),
    ("gray", egui::Color32::from_rgb(150, 150, 150)),
];

/// Resolve a stored color-label name to its swatch color.
fn meta_color(name: &str) -> Option<egui::Color32> {
    MOD_COLORS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, color)| *color)
}

/// An action a separator header row can request.
#[derive(Clone, Copy)]
enum SepAction {
    ToggleCollapse,
    StartEdit,
    Commit,
    Remove,
    MoveUp,
    MoveDown,
}

/// Render one separator (group divider) row: collapse toggle, name or rename
/// field, and move/remove controls. Returns the action the user requested.
fn separator_header(
    ui: &mut egui::Ui,
    separator: &Separator,
    collapsed: bool,
    editing: bool,
    buf: &mut String,
) -> Option<SepAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(if collapsed { "▶" } else { "▼" })
            .on_hover_text("Collapse or expand this section")
            .clicked()
        {
            action = Some(SepAction::ToggleCollapse);
        }
        if editing {
            let response = ui.add(egui::TextEdit::singleline(buf).desired_width(220.0));
            let committed_with_enter =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.small_button("✔").on_hover_text("Save name").clicked() || committed_with_enter {
                action = Some(SepAction::Commit);
            }
        } else {
            ui.strong(format!("═══  {}  ═══", separator.name));
            if ui.small_button("✎").on_hover_text("Rename").clicked() {
                action = Some(SepAction::StartEdit);
            }
        }
        if ui.small_button("▲").on_hover_text("Move divider up").clicked() {
            action = Some(SepAction::MoveUp);
        }
        if ui.small_button("▼").on_hover_text("Move divider down").clicked() {
            action = Some(SepAction::MoveDown);
        }
        if ui.small_button("✕").on_hover_text("Remove divider").clicked() {
            action = Some(SepAction::Remove);
        }
    });
    action
}

/// The set of mod display indices hidden inside a collapsed separator section
/// (from the separator's slot up to the next separator, or the end).
fn collapsed_hidden_indices(
    separators: &[Separator],
    collapsed: &std::collections::BTreeSet<String>,
    count: usize,
) -> std::collections::BTreeSet<usize> {
    let mut positions: Vec<usize> = separators.iter().map(|sep| sep.position).collect();
    positions.sort_unstable();
    let mut hidden = std::collections::BTreeSet::new();
    for separator in separators {
        if collapsed.contains(&separator.id) {
            let start = separator.position.min(count);
            let end = positions
                .iter()
                .copied()
                .find(|&position| position > separator.position)
                .unwrap_or(count)
                .min(count);
            for index in start..end {
                hidden.insert(index);
            }
        }
    }
    hidden
}

/// A fixed-width, vertically-centred table cell, so the mod list's columns line
/// up row-to-row instead of drifting with each row's content.
fn table_cell(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| add(ui),
    );
}

/// Return a copy of `ids` with the entry at `from` moved so it lands at index
/// `to`. Out-of-range indices are clamped, so callers can pass a raw drop slot
/// or a typed priority without extra bounds checks.
fn move_in_list(ids: &[String], from: usize, to: usize) -> Vec<String> {
    let mut ids = ids.to_vec();
    if from >= ids.len() {
        return ids;
    }
    let item = ids.remove(from);
    let to = to.min(ids.len());
    ids.insert(to, item);
    ids
}

/// The install-root kinds the planner understands. Selecting from this list
/// replaces free-text entry, so a root cannot be saved with a kind that later
/// fails manifest validation. An unrecognized existing value (e.g. a legacy
/// alias) still displays and is preserved until the user picks a new one.
const INSTALL_KIND_OPTIONS: [&str; 12] = [
    "modloader",
    "cleo",
    "cleo_text",
    "cleo_plugins",
    "cleo_modules",
    "cleo_saves",
    "asi",
    "plugin",
    "bootstrap",
    "runtime",
    "direct",
    "direct_managed",
];

fn install_kind_combo(ui: &mut egui::Ui, id_source: &str, kind: &mut String) {
    egui::ComboBox::from_id_salt(format!("kind_{id_source}"))
        .selected_text(kind.clone())
        .width(160.0)
        .show_ui(ui, |ui| {
            for option in INSTALL_KIND_OPTIONS {
                ui.selectable_value(kind, option.to_string(), option);
            }
        });
}

impl SanAndreasModUi {
    pub(super) fn mod_config_card_header(&mut self, ui: &mut egui::Ui, item: &ModConfigItem) {
        ui.horizontal(|ui| {
            ui.heading(&item.config.id);
            ui.separator();
            ui.label(format!(
                "{} install locations",
                item.config.install_roots.len()
            ));
            ui.label(if item.in_selected_profile {
                "selected" // literal: allow external interface text or file-format spelling
            } else {
                "not selected" // literal: allow external interface text or file-format spelling
            });
            if should_add_mod_to_profile(ui, item) {
                self.add_mod_to_profile(&item.config.id);
            }
        });
    }
}

impl SanAndreasModUi {
    /// The SA-native "Data tab": what the selected profile actually materializes,
    /// grouped by category (ModLoader / CLEO / ASI / direct resources) with the
    /// load-order winner for every file and the mods it overwrites.
    pub(super) fn content_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Content"); // literal: allow external interface text or file-format spelling
        ui.label(
            "Everything the selected profile materializes, by San Andreas category, with the \
             load-order winner for each file. Later mods (lower in the load order) overwrite earlier ones.",
        );
        ui.separator();

        let mut rescan = false;
        ui.horizontal(|ui| {
            rescan = ui
                .button("Scan / refresh")
                .on_hover_text("Read every enabled mod's files and rebuild the conflict index")
                .clicked();
            ui.separator();
            ui.label(format!("Profile: {}", self.selected_profile));
        });
        if rescan {
            self.rescan_content();
        }

        // Take the cached index out so the filter controls can mutate `self`
        // freely while the (borrowed) index is rendered; it is restored after.
        let Some(index) = self.content_index.take() else {
            ui.add_space(8.0);
            ui.weak(
                "No content scanned yet. Click Scan / refresh to analyze the profile's enabled mods.",
            );
            return;
        };
        self.content_body(ui, &index);
        self.content_index = Some(index);
    }

    fn content_body(&mut self, ui: &mut egui::Ui, index: &ContentIndex) {
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, "Files", &index.entries.len().to_string());
            summary_tile(ui, "Conflicts", &index.conflict_count().to_string());
            summary_tile(ui, "Not indexed", &index.not_indexed.len().to_string());
        });
        if !index.not_indexed.is_empty() {
            ui.weak(format!(
                "Not indexed (import to the library to include): {}",
                index.not_indexed.join(", ")
            ));
        }
        ui.separator();

        // Category chips (only those with content) plus an "All" reset.
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(
                    self.content_category.is_none(),
                    format!("All ({})", index.entries.len()),
                )
                .clicked()
            {
                self.content_category = None;
            }
            for category in ContentCategory::all() {
                let count = index.category_count(category);
                if count == 0 {
                    continue;
                }
                let selected = self.content_category == Some(category);
                if ui
                    .selectable_label(selected, format!("{} ({count})", category.label()))
                    .clicked()
                {
                    self.content_category = Some(category);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.content_conflicts_only, "Conflicts only")
                .on_hover_text("Show only files written by more than one enabled mod");
            ui.separator();
            ui.label("Filter");
            ui.add_sized(
                [260.0, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.content_search)
                    .hint_text("target path or mod id"),
            );
        });
        ui.separator();

        let needle = self.content_search.trim().to_ascii_lowercase();
        let selected_category = self.content_category;
        let conflicts_only = self.content_conflicts_only;
        let rows: Vec<&ContentEntry> = index
            .entries
            .iter()
            .filter(|entry| selected_category.is_none_or(|cat| entry.category == cat))
            .filter(|entry| !conflicts_only || entry.is_conflict())
            .filter(|entry| {
                needle.is_empty()
                    || entry.target.to_ascii_lowercase().contains(&needle)
                    || entry
                        .providers
                        .iter()
                        .any(|id| id.to_ascii_lowercase().contains(&needle))
            })
            .collect();

        if rows.is_empty() {
            ui.label("No files match the current filter.");
            return;
        }
        ui.label(format!("{} files shown", rows.len()));
        if let Some(category) = selected_category {
            ui.weak(category.description());
        }
        ui.add_space(4.0);

        let conflict_color = ui.visuals().warn_fg_color;
        // Cloned out so the tailored viewers can borrow them inside the closure.
        let priorities = self.modloader_priorities.clone();
        let modloader_log = self.modloader_log.clone();
        let cleo_diagnostics = self.cleo_diagnostics.clone();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match selected_category {
                // A specific category becomes its own tailored viewer: ModLoader
                // grouped by its sandboxed mod folders, others by winning mod.
                Some(category) => content_group_view(
                    ui,
                    category,
                    &rows,
                    conflict_color,
                    priorities.as_ref(),
                    modloader_log.as_ref(),
                    cleo_diagnostics.as_ref(),
                ),
                // "All" stays a single flat table across every category.
                None => content_table(ui, "content_grid_all", &rows, conflict_color),
            });
    }
}

/// The flat "Category / Target / Winner / Overwrites" table, shared by the All
/// view and each grouped section. `id` must be unique per instance so nested
/// grids in grouped sections do not collide.
fn content_table(
    ui: &mut egui::Ui,
    id: &str,
    rows: &[&ContentEntry],
    conflict_color: egui::Color32,
) {
    const ROW_CAP: usize = 3000;
    egui::Grid::new(id.to_string())
        .striped(true)
        .num_columns(4)
        .min_col_width(90.0)
        .show(ui, |ui| {
            ui.strong("Category");
            ui.strong("Target file");
            ui.strong("Winner");
            ui.strong("Overwrites");
            ui.end_row();
            for entry in rows.iter().take(ROW_CAP) {
                ui.label(entry.category.label());
                ui.monospace(&entry.target);
                if entry.is_conflict() {
                    ui.colored_label(conflict_color, entry.winner())
                        .on_hover_text("Wins this file after load order is applied");
                } else {
                    ui.label(entry.winner());
                }
                let overwritten = &entry.providers[..entry.providers.len() - 1];
                if overwritten.is_empty() {
                    ui.weak("—");
                } else {
                    ui.label(overwritten.join(", ")).on_hover_text(
                        "These mods' copies of this file are overwritten by the winner",
                    );
                }
                ui.end_row();
            }
        });
    if rows.len() > ROW_CAP {
        ui.weak(format!("{} more files not shown", rows.len() - ROW_CAP));
    }
}

/// Route a category to its tailored viewer. CLEO, ASI, and ModLoader get bespoke
/// layouts (scripts+companions; plugins vs loaders; folders by ModLoader
/// priority); everything else groups into collapsing sections by winning mod.
fn content_group_view(
    ui: &mut egui::Ui,
    category: ContentCategory,
    rows: &[&ContentEntry],
    conflict_color: egui::Color32,
    priorities: Option<&ModLoaderPriorities>,
    modloader_log: Option<&ModLoaderLogSummary>,
    cleo_diagnostics: Option<&CleoDiagnostics>,
) {
    match category {
        ContentCategory::Cleo => content_cleo_view(ui, rows, conflict_color, cleo_diagnostics),
        ContentCategory::Asi => content_asi_view(ui, rows, conflict_color),
        ContentCategory::ModLoader => {
            content_modloader_view(ui, rows, conflict_color, priorities, modloader_log)
        }
        _ => content_grouped(ui, category, rows, conflict_color),
    }
}

/// A compact "what ModLoader actually did last run" panel from `modloader.log`:
/// the version banner, and any lines that read as errors/warnings. Best-effort —
/// the log is free text — so it is framed as a reality check, not authoritative.
fn content_modloader_log_summary(
    ui: &mut egui::Ui,
    log: &ModLoaderLogSummary,
    conflict_color: egui::Color32,
) {
    let version = log
        .version
        .as_deref()
        .map(|v| format!("Mod Loader {v}"))
        .unwrap_or_else(|| "Mod Loader".to_string());
    if log.is_clean() {
        ui.weak(format!("Last run: {version} — loaded cleanly (no warnings or errors)."));
        ui.add_space(4.0);
        return;
    }
    let header = format!(
        "Last run: {version} — {} errors, {} warnings{}",
        log.errors.len(),
        log.warnings.len(),
        if log.truncated { " (truncated)" } else { "" }
    );
    egui::CollapsingHeader::new(header)
        .id_salt("modloader_log_summary")
        .default_open(!log.errors.is_empty())
        .show(ui, |ui| {
            ui.weak(
                "Scraped from modloader/modloader.log — what ModLoader reported on the last \
                 launch. Free-text, so treat as a hint, not a guarantee.",
            );
            for line in &log.errors {
                ui.colored_label(conflict_color, line);
            }
            for line in &log.warnings {
                ui.label(line);
            }
        });
    ui.add_space(4.0);
}

/// The ModLoader viewer: one section per sandboxed folder, ordered and labeled by
/// ModLoader's own priority (from modloader.ini) — the order ModLoader itself
/// applies them in, which is independent of this manager's profile load order.
fn content_modloader_view(
    ui: &mut egui::Ui,
    rows: &[&ContentEntry],
    conflict_color: egui::Color32,
    priorities: Option<&ModLoaderPriorities>,
    modloader_log: Option<&ModLoaderLogSummary>,
) {
    if let Some(log) = modloader_log {
        content_modloader_log_summary(ui, log, conflict_color);
    }
    match priorities {
        Some(_) => {
            ui.weak(
                "ModLoader applies these folders by its own priority (higher = later = wins), \
                 from modloader/modloader.ini — separate from the profile load order.",
            );
        }
        None => {
            ui.weak(
                "modloader.ini not found, so ModLoader's per-folder priority is unknown — it will \
                 use its default. Run ModLoader once to generate it.",
            );
        }
    }

    // Cross-folder conflicts: two sandbox folders that both provide the same
    // underlying asset never collide on disk, so they are invisible in the plain
    // file tables below — ModLoader resolves them by priority at runtime. Surface
    // them explicitly, since this is the conflict class the viewer exists to show.
    let effective_priorities = priorities.cloned().unwrap_or_default();
    let conflicts = modloader_conflicts(rows, &effective_priorities);
    if !conflicts.is_empty() {
        // Mergeable data files (handling.cfg, *.ide…) are soft: ModLoader combines
        // them entry-by-entry, so they collide only on overlapping entries. The
        // rest are hard: the highest-priority folder wins the whole file.
        let override_count = conflicts.iter().filter(|c| !c.mergeable).count();
        let merge_count = conflicts.len() - override_count;
        let ambiguous = conflicts.iter().filter(|c| c.ambiguous && !c.mergeable).count();
        let mut parts = Vec::new();
        if override_count > 0 {
            parts.push(format!("{override_count} override"));
        }
        if merge_count > 0 {
            parts.push(format!("{merge_count} mergeable"));
        }
        if ambiguous > 0 {
            parts.push(format!("{ambiguous} tied"));
        }
        let header = format!("Cross-folder conflicts — {}", parts.join(", "));
        egui::CollapsingHeader::new(header)
            .id_salt("modloader_virtual_conflicts")
            .default_open(override_count > 0)
            .show(ui, |ui| {
                ui.weak(
                    "Same file provided by more than one folder. Override: the highest-priority \
                     folder wins the whole file. Mergeable: ModLoader combines entries, so folders \
                     clash only where they edit the same entry. Tied priorities are \
                     non-deterministic — give them distinct priorities to pin the winner.",
                );
                egui::Grid::new("modloader_conflict_grid")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        ui.strong("Asset");
                        ui.strong("Kind");
                        ui.strong("Result");
                        ui.strong("Other folders");
                        ui.end_row();
                        for conflict in &conflicts {
                            ui.label(&conflict.asset);
                            let winner = conflict.winner();
                            let winner_priority = conflict
                                .contenders
                                .first()
                                .map(|c| c.priority)
                                .unwrap_or_default();
                            if conflict.mergeable {
                                ui.weak("merge").on_hover_text(
                                    "ModLoader merges this file entry-by-entry; only entries edited \
                                     by more than one folder actually conflict.",
                                );
                                ui.label("combined").on_hover_text(
                                    "All folders' entries are kept; overlapping entries resolve by \
                                     priority.",
                                );
                            } else if conflict.ambiguous {
                                ui.colored_label(conflict_color, "override");
                                ui.colored_label(
                                    conflict_color,
                                    format!("{winner} (priority {winner_priority}, tied)"),
                                )
                                .on_hover_text(
                                    "Two folders share the top priority; ModLoader's winner here \
                                     is not guaranteed.",
                                );
                            } else {
                                ui.label("override");
                                ui.label(format!("{winner} wins (priority {winner_priority})"));
                            }
                            let others: Vec<String> = conflict.contenders[1..]
                                .iter()
                                .map(|c| format!("{} ({})", c.folder, c.priority))
                                .collect();
                            if others.is_empty() {
                                ui.weak("—");
                            } else {
                                ui.label(others.join(", "));
                            }
                            ui.end_row();
                        }
                    });
            });
    }

    // Attach each folder its effective priority (when known) and order by it,
    // so the list reads top-to-bottom the way ModLoader will apply them.
    let mut groups: Vec<(Option<i32>, String, Vec<&ContentEntry>)> =
        group_entries(ContentCategory::ModLoader, rows)
            .into_iter()
            .map(|(folder, folder_rows)| {
                let priority = priorities.map(|table| table.for_folder(&folder));
                (priority, folder, folder_rows)
            })
            .collect();
    groups.sort_by(|a, b| {
        a.0.unwrap_or(50)
            .cmp(&b.0.unwrap_or(50))
            .then_with(|| a.1.cmp(&b.1))
    });

    let default_open = groups.len() <= 8;
    for (priority, folder, folder_rows) in groups {
        let conflicts = folder_rows.iter().filter(|entry| entry.is_conflict()).count();
        let priority_label = match priority {
            Some(value) => format!("priority {value}"),
            None => "priority —".to_string(),
        };
        let header = if conflicts > 0 {
            format!(
                "{folder} — {priority_label} — {} files, {conflicts} conflicts",
                folder_rows.len()
            )
        } else {
            format!("{folder} — {priority_label} — {} files", folder_rows.len())
        };
        egui::CollapsingHeader::new(header)
            .id_salt(format!("modloader_folder_{folder}"))
            .default_open(default_open)
            .show(ui, |ui| {
                content_table(
                    ui,
                    &format!("modloader_grid_{folder}"),
                    &folder_rows,
                    conflict_color,
                );
            });
    }
}

/// One collapsing section per group (ModLoader mod folder, or winning mod for
/// other categories), each carrying its own file table and a conflict count.
fn content_grouped(
    ui: &mut egui::Ui,
    category: ContentCategory,
    rows: &[&ContentEntry],
    conflict_color: egui::Color32,
) {
    let groups = group_entries(category, rows);
    // Open every section when there are only a handful, so small profiles read at
    // a glance; collapse by default once the list would get long.
    let default_open = groups.len() <= 8;
    for (label, group_rows) in groups {
        let conflicts = group_rows.iter().filter(|entry| entry.is_conflict()).count();
        let header = if conflicts > 0 {
            format!("{label} — {} files, {conflicts} conflicts", group_rows.len())
        } else {
            format!("{label} — {} files", group_rows.len())
        };
        egui::CollapsingHeader::new(header)
            .id_salt(format!("content_group_{}_{label}", category.short_label()))
            .default_open(default_open)
            .show(ui, |ui| {
                content_table(
                    ui,
                    &format!("content_grid_{}_{label}", category.short_label()),
                    &group_rows,
                    conflict_color,
                );
            });
    }
}

/// The installed-CLEO health panel shown atop the CLEO viewer — what the actual
/// game folder's CLEO setup will do (from the last content scan), as opposed to
/// the profile plan the rest of the viewer shows. Hidden when there is nothing to
/// report.
fn content_cleo_diagnostics(
    ui: &mut egui::Ui,
    diagnostics: &CleoDiagnostics,
    conflict_color: egui::Color32,
) {
    if diagnostics.is_empty() {
        return;
    }
    egui::CollapsingHeader::new("CLEO health (installed game folder)")
        .id_salt("cleo_health")
        .default_open(true)
        .show(ui, |ui| {
            if !diagnostics.blacklisted_plugins.is_empty() {
                ui.colored_label(
                    conflict_color,
                    format!(
                        "{} blacklisted plugin(s) — legacy, superseded by SA.*, will not load:",
                        diagnostics.blacklisted_plugins.len()
                    ),
                );
                for name in &diagnostics.blacklisted_plugins {
                    ui.monospace(format!("    {name}"));
                }
            }
            if !diagnostics.fxt_conflicts.is_empty() {
                ui.colored_label(
                    conflict_color,
                    format!(
                        "{} text key conflict(s) — same GXT key in multiple .fxt (last wins):",
                        diagnostics.fxt_conflicts.len()
                    ),
                );
                for conflict in &diagnostics.fxt_conflicts {
                    ui.label(format!("    {}: {}", conflict.key, conflict.files.join(", ")));
                }
            }
            if !diagnostics.script_issues.is_empty() {
                ui.strong(format!(
                    "{} script(s) with dependencies / elevated access:",
                    diagnostics.script_issues.len()
                ));
                for issue in &diagnostics.script_issues {
                    let mut parts = Vec::new();
                    if !issue.missing_plugins.is_empty() {
                        parts.push(format!("MISSING {}", issue.missing_plugins.join(", ")));
                    }
                    if !issue.capabilities.is_empty() {
                        parts.push(format!("elevated: {}", issue.capabilities.join(", ")));
                    }
                    let text = format!("    {}: {}", issue.script, parts.join("; "));
                    if issue.missing_plugins.is_empty() {
                        ui.label(text);
                    } else {
                        ui.colored_label(conflict_color, text);
                    }
                }
            }
        });
    ui.add_space(6.0);
}

/// The CLEO viewer: an installed-folder health panel (blacklisted plugins, text
/// key conflicts, per-script missing-plugin/capability issues), then each script
/// listed with its `.ini`/`.fxt`/data companions, then loose files.
fn content_cleo_view(
    ui: &mut egui::Ui,
    rows: &[&ContentEntry],
    conflict_color: egui::Color32,
    diagnostics: Option<&CleoDiagnostics>,
) {
    if let Some(diagnostics) = diagnostics {
        content_cleo_diagnostics(ui, diagnostics, conflict_color);
    }
    let view = cleo_view(rows);
    if !view.plugins.is_empty() {
        ui.strong(format!("{} plugin modules (.cleo)", view.plugins.len()));
        ui.weak("CLEO5 loads these from CLEO/cleo_plugins. Scripts may require a matching plugin (e.g. SA.IniFiles).");
        content_table(ui, "cleo_plugins_grid", &view.plugins, conflict_color);
        ui.add_space(6.0);
    }
    ui.strong(format!("{} scripts", view.scripts.len()));
    egui::Grid::new("cleo_scripts_grid")
        .striped(true)
        .num_columns(4)
        .min_col_width(90.0)
        .show(ui, |ui| {
            ui.strong("Script");
            ui.strong("Winner");
            ui.strong("Companions");
            ui.strong("Overwrites");
            ui.end_row();
            for script in &view.scripts {
                ui.monospace(&script.script.target);
                if script.script.is_conflict() {
                    ui.colored_label(conflict_color, script.script.winner())
                        .on_hover_text("Another mod ships a script with the same file name");
                } else {
                    ui.label(script.script.winner());
                }
                if script.companions.is_empty() {
                    ui.weak("—");
                } else {
                    let names: Vec<&str> = script
                        .companions
                        .iter()
                        .map(|companion| base_name(&companion.target))
                        .collect();
                    let full: Vec<&str> = script
                        .companions
                        .iter()
                        .map(|companion| companion.target.as_str())
                        .collect();
                    ui.label(names.join(", ")).on_hover_text(full.join("\n"));
                }
                let overwritten = &script.script.providers[..script.script.providers.len() - 1];
                if overwritten.is_empty() {
                    ui.weak("—");
                } else {
                    ui.label(overwritten.join(", "));
                }
                ui.end_row();
            }
        });
    if !view.loose.is_empty() {
        ui.add_space(6.0);
        ui.strong(format!(
            "{} loose files (no matching script)",
            view.loose.len()
        ));
        content_table(ui, "cleo_loose_grid", &view.loose, conflict_color);
    }
}

/// The ASI viewer: plugins, then the loader/proxy DLLs that boot them, then any
/// other files — so a clashing loader is obvious versus a duplicate plugin.
fn content_asi_view(ui: &mut egui::Ui, rows: &[&ContentEntry], conflict_color: egui::Color32) {
    let view = asi_view(rows);
    ui.strong(format!("{} ASI plugins", view.plugins.len()));
    if view.plugins.is_empty() {
        ui.weak("No .asi plugins in this profile.");
    } else {
        content_table(ui, "asi_plugins_grid", &view.plugins, conflict_color);
    }
    if !view.loaders.is_empty() {
        ui.add_space(6.0);
        ui.strong(format!("{} loader / proxy DLLs", view.loaders.len()));
        ui.weak("These hook the game to load ASI plugins (e.g. Ultimate ASI Loader). Normally only one should win.");
        content_table(ui, "asi_loaders_grid", &view.loaders, conflict_color);
    }
    if !view.other.is_empty() {
        ui.add_space(6.0);
        ui.strong(format!("{} other files", view.other.len()));
        content_table(ui, "asi_other_grid", &view.other, conflict_color);
    }
}

/// The last path segment of a forward-slash target (its file name).
fn base_name(target: &str) -> &str {
    target.rsplit('/').next().unwrap_or(target)
}

/// The ModLoader mod folder a materialized target belongs to: the segment right
/// after a `modloader` segment, when there is a path component beneath it (so a
/// loose file at `modloader/` isn't mistaken for a mod folder). Reserved
/// dot-folders (`.data`, …) yield `None`.
fn modloader_folder_of(target: &str) -> Option<String> {
    let mut segments = target.split('/');
    while let Some(segment) = segments.next() {
        if segment.eq_ignore_ascii_case("modloader") {
            let folder = segments.next()?;
            if folder.starts_with('.') {
                return None;
            }
            segments.next()?; // require a component beneath the folder
            return Some(folder.to_string());
        }
    }
    None
}

impl SanAndreasModUi {
    /// The per-mod info window (MO2's Mod Info dialog): Files / Conflicts / Install
    /// roots / Readme for one mod. Shown as a floating, closable window.
    pub(super) fn mod_info_window(&mut self, ctx: &egui::Context) {
        let Some(info) = &self.mod_info else {
            return;
        };
        let id = info.id.clone();
        let mut tab = info.tab;
        let mut open = true;
        egui::Window::new(format!("Mod: {id}"))
            .id(egui::Id::new("mod_info_window"))
            .open(&mut open)
            .resizable(true)
            .default_size([560.0, 440.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut tab, ModInfoTab::Files, "Files");
                    ui.selectable_value(&mut tab, ModInfoTab::Conflicts, "Conflicts");
                    ui.selectable_value(&mut tab, ModInfoTab::Roots, "Install roots");
                    ui.selectable_value(&mut tab, ModInfoTab::Readme, "Readme");
                    ui.selectable_value(&mut tab, ModInfoTab::Notes, "Notes");
                });
                ui.separator();
                match tab {
                    ModInfoTab::Files => self.mod_info_files_tab(ui),
                    ModInfoTab::Conflicts => self.mod_info_conflicts_tab(ui, &id),
                    ModInfoTab::Roots => self.mod_info_roots_tab(ui, &id),
                    ModInfoTab::Readme => self.mod_info_readme_tab(ui),
                    ModInfoTab::Notes => self.mod_info_notes_tab(ui, &id),
                }
            });
        if let Some(info) = self.mod_info.as_mut() {
            info.tab = tab;
        }
        if !open {
            self.mod_info = None;
        }
    }

    fn mod_info_files_tab(&mut self, ui: &mut egui::Ui) {
        let Some(info) = &self.mod_info else {
            return;
        };
        match &info.source_root {
            Some(root) => {
                ui.weak(format!("Library: {}", root.display()));
            }
            None => {
                ui.weak("Not extracted to the library — file list unavailable.");
            }
        }
        ui.label(format!("{} files", info.files.len()));
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for file in info.files.iter().take(5000) {
                    ui.monospace(file);
                }
                if info.files.len() > 5000 {
                    ui.weak(format!("{} more not shown", info.files.len() - 5000));
                }
            });
    }

    fn mod_info_conflicts_tab(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(index) = &self.content_index else {
            ui.label("Run Analyze content (left panel) to compute conflicts.");
            return;
        };
        let conflicts: Vec<&ContentEntry> = index
            .entries
            .iter()
            .filter(|entry| entry.is_conflict() && entry.providers.iter().any(|p| p == id))
            .collect();
        if conflicts.is_empty() {
            ui.label("None of this mod's files conflict with another enabled mod.");
            return;
        }
        ui.label(format!("{} conflicting files", conflicts.len()));
        let win_color = egui::Color32::from_rgb(120, 190, 120);
        let lose_color = ui.visuals().warn_fg_color;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("mod_info_conflicts")
                    .striped(true)
                    .num_columns(3)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        ui.strong("File");
                        ui.strong("This mod");
                        ui.strong("Winner");
                        ui.end_row();
                        for entry in &conflicts {
                            ui.monospace(&entry.target);
                            if entry.winner() == id {
                                ui.colored_label(win_color, "wins");
                            } else {
                                ui.colored_label(lose_color, "overwritten");
                            }
                            ui.label(entry.winner());
                            ui.end_row();
                        }
                    });
            });
    }

    fn mod_info_roots_tab(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(item) = self.state.mods.iter().find(|item| item.config.id == id) else {
            ui.label("Mod not found in the library.");
            return;
        };
        ui.monospace(format!("package: {}", item.config.package.display()));
        if let Some(source_root) = &item.config.source_root {
            ui.monospace(format!("library: {}", source_root.display()));
        }
        ui.separator();
        if item.config.install_roots.is_empty() {
            ui.label("No install roots detected for this mod.");
            return;
        }
        egui::Grid::new("mod_info_roots")
            .striped(true)
            .num_columns(4)
            .min_col_width(60.0)
            .show(ui, |ui| {
                ui.strong("Source");
                ui.strong("Target");
                ui.strong("Kind");
                ui.strong("On");
                ui.end_row();
                for root in &item.config.install_roots {
                    ui.monospace(&root.source);
                    ui.monospace(&root.target);
                    ui.label(&root.kind);
                    ui.label(if root.enabled { "yes" } else { "no" });
                    ui.end_row();
                }
            });
    }

    fn mod_info_readme_tab(&mut self, ui: &mut egui::Ui) {
        let Some(info) = &self.mod_info else {
            return;
        };
        if info.readmes.is_empty() {
            ui.label("No readme files found in this mod.");
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (name, text) in &info.readmes {
                    egui::CollapsingHeader::new(name)
                        .id_salt(name)
                        .default_open(info.readmes.len() == 1)
                        .show(ui, |ui| {
                            ui.monospace(text);
                        });
                }
            });
    }

    /// Editor for a mod's user annotations: color label, categories, and note.
    /// All mutations are deferred until after the widgets render, so `self.mod_meta`
    /// / `self.mod_info` are never borrowed while a setter (which takes `&mut self`)
    /// runs.
    fn mod_info_notes_tab(&mut self, ui: &mut egui::Ui, id: &str) {
        let meta = self.mod_meta.get(id).cloned().unwrap_or_default();
        let mut set_color: Option<Option<String>> = None;
        let mut toggle_category: Option<String> = None;
        let mut add_category = false;
        let mut save_note = false;

        ui.horizontal(|ui| {
            ui.label("Color");
            for (name, color) in MOD_COLORS {
                let selected = meta.color.as_deref() == Some(name);
                let label = if selected { "●" } else { "  " };
                if ui
                    .add(egui::Button::new(label).fill(color).min_size(egui::vec2(22.0, 18.0)))
                    .on_hover_text(name)
                    .clicked()
                {
                    set_color = Some(Some(name.to_string()));
                }
            }
            if ui.button("none").clicked() {
                set_color = Some(None);
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Categories");
            for category in &meta.categories {
                if ui
                    .button(format!("{category} ✕"))
                    .on_hover_text("Remove this category")
                    .clicked()
                {
                    toggle_category = Some(category.clone());
                }
            }
            if meta.categories.is_empty() {
                ui.weak("none yet");
            }
        });
        ui.horizontal(|ui| {
            if let Some(info) = self.mod_info.as_mut() {
                ui.add(
                    egui::TextEdit::singleline(&mut info.new_category)
                        .hint_text("new category")
                        .desired_width(160.0),
                );
            }
            if ui.button("Add").clicked() {
                add_category = true;
            }
        });

        ui.separator();
        ui.label("Note");
        if let Some(info) = self.mod_info.as_mut() {
            ui.add(
                egui::TextEdit::multiline(&mut info.note_edit)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
        }
        if ui.button("Save note").clicked() {
            save_note = true;
        }

        if let Some(color) = set_color {
            self.set_mod_color(id, color);
        }
        if let Some(category) = toggle_category {
            self.toggle_mod_category(id, &category);
        }
        if add_category {
            let category = self
                .mod_info
                .as_ref()
                .map(|info| info.new_category.trim().to_string())
                .unwrap_or_default();
            if !category.is_empty() {
                self.toggle_mod_category(id, &category);
                if let Some(info) = self.mod_info.as_mut() {
                    info.new_category.clear();
                }
            }
        }
        if save_note {
            let note = self
                .mod_info
                .as_ref()
                .map(|info| info.note_edit.clone())
                .unwrap_or_default();
            self.set_mod_note(id, note);
        }
    }

    pub(super) fn import_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Import Mod"); // literal: allow external interface text or file-format spelling
        ui.label("Review first, then import. Medium-confidence readme matches stay visible for human judgment.");
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Package or folder"); // literal: allow external interface text or file-format spelling
            let package_editor = egui::TextEdit::singleline(&mut self.import_path_input);
            let package_response = ui.add_sized([470.0, ROW_HEIGHT], package_editor);
            if package_response.changed() {
                // Editing the path invalidates proposals from the last review, so
                // "Add to config" can never target a different package.
                self.clear_readme_review();
            }
            if ui
                .button("File…")
                .on_hover_text("Pick a .zip / .wrap / .7z / .rar package")
                .clicked()
            {
                self.browse_package_file();
            }
            if ui
                .button("Folder…")
                .on_hover_text("Pick an already-extracted mod folder")
                .clicked()
            {
                self.browse_package_folder();
            }
        });
        ui.label("Tip: you can also drag a package or your GTA folder onto this window.");
        let idle = !self.is_busy();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(idle, egui::Button::new("Review package"))
                .on_hover_text("List entries and readmes without writing anything")
                .on_disabled_hover_text("A background task is running")
                .clicked()
            {
                self.analyze_package(ui.ctx());
            }
            if ui
                .add_enabled(idle, egui::Button::new("Import to library"))
                .on_hover_text("Extract the package into the managed library")
                .clicked()
            {
                self.import_package(ui.ctx());
            }
        });
        ui.separator();
        ui.label("Supported: folders, .zip, .wrap, .7z, .rar."); // literal: allow external interface text or file-format spelling
        if self.analysis_summary.is_none() {
            ui.label("Use Review package to inspect files, readmes, and proposed install roots before committing it to the library.");
        }
        self.readme_proposals_panel(ui);
    }
}

impl SanAndreasModUi {
    fn readme_proposals_panel(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Readme Review");
        if let Some(summary) = &self.analysis_summary {
            ui.label(summary);
        }
        if self.readme_proposals.is_empty() {
            ui.label("No readme proposals yet.");
            return;
        }
        let auto_selected = self
            .readme_proposals
            .iter()
            .filter(|proposal| proposal.review_state == ReadmeProposalState::AutoSelected)
            .count();
        let needs_review = self
            .readme_proposals
            .iter()
            .filter(|proposal| proposal.review_state == ReadmeProposalState::NeedsReview)
            .count();
        let warnings = self
            .readme_proposals
            .iter()
            .filter(|proposal| proposal.review_state == ReadmeProposalState::WarningOnly)
            .count();
        ui.horizontal_wrapped(|ui| {
            stat_inline(ui, "Auto-selected", auto_selected);
            stat_inline(ui, "Needs review", needs_review);
            stat_inline(ui, "Warnings", warnings);
        });
        // A Copy proposal can only be written into a config that already exists,
        // i.e. after the package has been imported. Surface that state once so
        // each row's button can explain itself instead of failing on click.
        let imported = self
            .reviewed_mod_config_path()
            .map(|path| path.exists())
            .unwrap_or(false);
        if !imported {
            ui.label("Import this package to the library to enable applying a proposal to its config.");
        }
        let proposals = self.readme_proposals.clone();
        let mut accepted = None;
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(ui, |ui| {
                for proposal in &proposals {
                    if readme_proposal_panel(ui, proposal, imported) {
                        accepted = Some(proposal.clone());
                    }
                    ui.add_space(6.0);
                }
            });
        if let Some(proposal) = accepted {
            self.accept_readme_proposal(&proposal);
        }
    }
}

/// Returns `true` when the user clicked "Add to config" for this proposal.
fn readme_proposal_panel(ui: &mut egui::Ui, proposal: &ReadmeProposal, imported: bool) -> bool {
    let mut accepted = false;
    let evidence = format!(
        "{} line {}: {}",
        proposal.source_readme, proposal.line_number, proposal.evidence
    );
    ui.group(|ui| {
        // Header: title, confidence, state — and the action button, so a proposal
        // is one compact row plus its evidence, not a five-line stack.
        ui.horizontal(|ui| {
            ui.strong(readme_proposal_title(proposal));
            ui.weak(format!("{:.0}%", proposal.confidence * 100.0));
            ui.weak(proposal.review_state.to_string());
            if readme_proposal_is_actionable(proposal) {
                accepted = ui
                    .add_enabled(imported, egui::Button::new("Add to config"))
                    .on_hover_text("Append this copy as an install root on the imported mod's config")
                    .on_disabled_hover_text("Import this package to the library first")
                    .clicked();
            }
        });
        ui.label(format!("→ {}", proposal.proposed_install));
        ui.small(clip_text(&evidence, 100))
            .on_hover_text(format!("{evidence}\nNormalized: {}", proposal.normalized_text));
        if !proposal.reasons.is_empty() {
            ui.small(format!("Why: {}", proposal.reasons.join("; ")));
        }
    });
    accepted
}

/// Only a `copy` proposal with a concrete source and target maps to an install
/// root; relationship hints (requires/conflict/…) are informational only.
fn readme_proposal_is_actionable(proposal: &ReadmeProposal) -> bool {
    proposal.action == "copy" && proposal.source.is_some() && proposal.target.is_some()
}

fn readme_proposal_title(proposal: &ReadmeProposal) -> String {
    match proposal.review_state {
        ReadmeProposalState::AutoSelected => format!("Ready {}", proposal.action),
        ReadmeProposalState::NeedsReview => format!("Review {}", proposal.action),
        ReadmeProposalState::WarningOnly => format!("Warning {}", proposal.action),
    }
}

impl SanAndreasModUi {
    /// Manage external run targets (MO2's executables): list the configured
    /// tools with a Remove button, and a form to add a new one.
    fn executables_editor(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Run targets / tools")
            .id_salt("executables_editor")
            .show(ui, |ui| {
                ui.label(
                    "Configure external tools (map editors, IMG tools, a CLEO debugger, or the \
                     game with custom args) to launch from the dropdown above.",
                );
                if self.executables.is_empty() {
                    ui.weak("No tools configured yet.");
                } else {
                    let mut remove_index = None;
                    for (index, tool) in self.executables.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.strong(&tool.name);
                            ui.monospace(&tool.path);
                            if !tool.args.is_empty() {
                                ui.weak(format!("args: {}", tool.args));
                            }
                            if ui.button("Remove").clicked() {
                                remove_index = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove_index {
                        self.remove_executable(index);
                    }
                }
                ui.separator();
                ui.label("Add a tool");
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add_sized(
                        [140.0, ROW_HEIGHT],
                        egui::TextEdit::singleline(&mut self.new_tool_name),
                    );
                    ui.label("Args");
                    ui.add_sized(
                        [160.0, ROW_HEIGHT],
                        egui::TextEdit::singleline(&mut self.new_tool_args)
                            .hint_text("-optional -flags"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Path");
                    ui.add_sized(
                        [320.0, ROW_HEIGHT],
                        egui::TextEdit::singleline(&mut self.new_tool_path)
                            .hint_text("full path to a .exe"),
                    );
                    if ui.button("Browse…").clicked() {
                        self.browse_tool_path();
                    }
                    if ui.button("Add").clicked() {
                        self.add_executable();
                    }
                });
            });
    }

    /// View, compare and edit every ModLoader mod's priority for this profile,
    /// and enable/disable them within ModLoader (priority 0). Edits persist per
    /// profile and are written to modloader.ini via Apply. Requires a content
    /// scan to know which mods are ModLoader mods.
    pub(super) fn modloader_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("ModLoader Priority");
        ui.label(
            "Order and compare ModLoader mods for this profile. Higher priority wins file \
             conflicts; priority 0 disables the mod in ModLoader — its files stay installed and it \
             still shows in the in-game Mod Configuration menu. These override the load-order \
             defaults and are applied automatically when you Play through the manager. Use Apply to \
             also write them into your active ModLoader profile for playing outside the manager.",
        );
        ui.separator();

        let Some(index) = &self.content_index else {
            ui.weak("Run Analyze content (left panel) to list ModLoader mods.");
            return;
        };
        let mut folders: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in &index.entries {
            if entry.category == ContentCategory::ModLoader {
                if let Some(folder) = modloader_folder_of(&entry.target) {
                    folders.insert(folder);
                }
            }
        }
        if folders.is_empty() {
            ui.label("No ModLoader mods found in this profile's content.");
            return;
        }

        let priorities = self.modloader_priorities.as_ref();
        let mut rows: Vec<(String, i32)> = folders
            .iter()
            .map(|folder| {
                let priority = self
                    .modloader_overrides
                    .get(folder)
                    .copied()
                    .or_else(|| priorities.map(|table| table.for_folder(folder)))
                    .unwrap_or(50);
                (folder.clone(), priority)
            })
            .collect();
        // Highest priority (runtime winner) first, then by name.
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut set_priority: Option<(String, i32)> = None;
        let mut apply = false;
        ui.horizontal(|ui| {
            if ui
                .button("Apply to modloader.ini")
                .on_hover_text("Write these priorities into modloader.ini's active profile")
                .clicked()
            {
                apply = true;
            }
            let overrides = self.modloader_overrides.len();
            if overrides > 0 {
                ui.weak(format!("{overrides} override(s) set for this profile"));
            }
        });
        ui.separator();

        egui::Grid::new("modloader_priority_grid")
            .striped(true)
            .num_columns(3)
            .min_col_width(64.0)
            .show(ui, |ui| {
                ui.strong("On");
                ui.strong("Priority");
                ui.strong("ModLoader folder");
                ui.end_row();
                for (folder, priority) in &rows {
                    let mut enabled = *priority > 0;
                    if ui
                        .checkbox(&mut enabled, "")
                        .on_hover_text(
                            "Enabled in ModLoader — uncheck to set priority 0 (disabled, but still \
                             installed and shown in the in-game menu)",
                        )
                        .changed()
                    {
                        set_priority = Some((folder.clone(), if enabled { 50 } else { 0 }));
                    }
                    let mut value = *priority;
                    let response = ui.add_enabled(
                        *priority > 0,
                        egui::DragValue::new(&mut value).range(0..=100).speed(0.25),
                    );
                    if response.changed() {
                        set_priority = Some((folder.clone(), value.clamp(0, 100)));
                    }
                    ui.label(folder);
                    ui.end_row();
                }
            });

        if let Some((folder, priority)) = set_priority {
            self.set_modloader_priority(&folder, priority);
        }
        if apply {
            self.apply_modloader_priorities();
        }
    }

    /// MO2's "overwrite": game-folder files under mod areas that no enabled mod
    /// provides — installed by hand or left by a tool. Computed on the last
    /// content scan; the manager never touches these, so they persist across runs.
    fn overwrite_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Unmanaged Files (Overwrite)");
        if self.content_index.is_none() {
            ui.weak("Run Analyze content (left panel) to detect unmanaged files.");
            return;
        }
        if self.overwrite_files.is_empty() {
            ui.label(
                "No unmanaged files under modloader/ or cleo/ — everything there is provided by an \
                 enabled mod or is loader infrastructure.",
            );
            return;
        }
        ui.label(format!(
            "{} file(s) under modloader/ or cleo/ that no enabled mod in this profile provides. \
             Installed manually or left by a tool — the manager does not touch them.",
            self.overwrite_files.len()
        ));
        egui::ScrollArea::vertical()
            .id_salt("overwrite_files")
            .auto_shrink([false, true])
            .max_height(200.0)
            .show(ui, |ui| {
                for file in self.overwrite_files.iter().take(2000) {
                    ui.monospace(file);
                }
                if self.overwrite_files.len() > 2000 {
                    ui.weak(format!("{} more not shown", self.overwrite_files.len() - 2000));
                }
            });
    }

    pub(super) fn run_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Play Selected Profile"); // literal: allow external interface text or file-format spelling
        ui.separator();
        let active_count = selected_profile_entries(&self.state, &self.selected_profile)
            .iter()
            .filter(|entry| entry.enabled)
            .count();
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, "Profile", &self.selected_profile);
            summary_tile(ui, "Enabled mods", &active_count.to_string());
            summary_tile(
                ui,
                "Cleanup",
                if self.pending_runs.is_empty() {
                    "clear"
                } else {
                    "needed"
                },
            );
            summary_tile(ui, "Readiness", readiness_label(self));
        });
        ui.label(
            "The run target (play the profile or a tool) lives in the top toolbar. This tab \
             configures tools and cleans up the temporary files left after a run.",
        );
        ui.horizontal(|ui| {
            if ui
                .button("Clean selected")
                .on_hover_text("Delete the selected run's materialized files (asks first)")
                .clicked()
            {
                self.request_cleanup_selected();
            }
            if ui.button("Check status").clicked() {
                self.refresh_pending_runs();
            }
            if ui
                .button("Clean finished")
                .on_hover_text("Delete all finished runs' materialized files (asks first)")
                .clicked()
            {
                self.request_cleanup_finished();
            }
        });
        self.executables_editor(ui);
        ui.separator();
        self.overwrite_panel(ui);
        ui.separator();
        ui.heading("Temporary Files Cleanup");
        if self.pending_runs.is_empty() {
            ui.label("No temporary mod files are waiting for cleanup.");
        } else {
            let stale = count_pending_status(self, PendingRunStatus::Stale);
            let running = count_pending_status(self, PendingRunStatus::Running);
            let unknown = count_pending_status(self, PendingRunStatus::Unknown);
            let invalid = count_pending_status(self, PendingRunStatus::Invalid);
            ui.horizontal_wrapped(|ui| {
                summary_tile(ui, "Finished", &stale.to_string());
                summary_tile(ui, "Running", &running.to_string());
                summary_tile(ui, "Unknown", &unknown.to_string());
                summary_tile(ui, "Invalid", &invalid.to_string());
            });
            ui.label("Running games block cleanup. Finished runs can be cleaned safely from here.");
            let pending_runs = self.pending_runs.clone();
            egui::Grid::new("pending_run_grid")
                .striped(true)
                .min_col_width(88.0)
                .show(ui, |ui| {
                    ui.label("Status");
                    ui.label("PID");
                    ui.label("Detail");
                    ui.label("Cleanup record");
                    ui.label("");
                    ui.end_row();
                    for record in pending_runs {
                        ui.monospace(record.status.to_string());
                        let pid = record
                            .pid
                            .map(|pid| pid.to_string())
                            .unwrap_or_else(|| "unknown".to_string());
                        ui.monospace(pid);
                        ui.label(&record.detail);
                        ui.monospace(record.journal.display().to_string());
                        if ui
                            .button("Clean")
                            .on_hover_text("Delete this run's materialized files (asks first)")
                            .clicked()
                        {
                            self.request_cleanup_record(record);
                        }
                        ui.end_row();
                    }
                });
        }
        ui.separator();
        ui.heading("Game Infrastructure"); // literal: allow external interface text or file-format spelling
        infrastructure_grid(ui, &self.state.infrastructure);
    }
}

impl SanAndreasModUi {
    pub(super) fn telemetry_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Telemetry");
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            stat_inline(ui, "Imports", self.telemetry.imports);
            stat_inline(ui, "Runs", self.telemetry.run_journals);
            stat_inline(ui, "Failed", self.telemetry.failed_launches);
            stat_inline(ui, "Installs", self.telemetry.install_journals);
            stat_inline(ui, "Copied", self.telemetry.copied_files);
            stat_inline(ui, "Overwrites", self.telemetry.overwritten_files);
            stat_inline(ui, "Cleanup", self.telemetry.pending_cleanup);
            stat_inline(ui, "New files", self.telemetry.new_files);
            stat_inline(ui, "Blocked bootstrap", self.telemetry.blocked_bootstrap);
            stat_inline(ui, "Missing sources", self.telemetry.missing_sources);
            stat_inline(ui, "Journals", self.telemetry.journals);
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.add_sized(
                [240.0, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.telemetry_search),
            );
            egui::ComboBox::from_id_salt("telemetry_kind_filter")
                .selected_text(&self.telemetry_kind_filter)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.telemetry_kind_filter, "all".to_string(), "all");
                    ui.selectable_value(
                        &mut self.telemetry_kind_filter,
                        "import".to_string(),
                        "imports",
                    );
                    ui.selectable_value(&mut self.telemetry_kind_filter, "run".to_string(), "runs");
                    ui.selectable_value(
                        &mut self.telemetry_kind_filter,
                        "install".to_string(),
                        "installs",
                    );
                });
            if ui.button("Export JSON").clicked() {
                self.export_telemetry();
            }
        });
        ui.separator();
        ui.heading("Recent Activity");
        let recent_events = filtered_telemetry_events(
            &self.telemetry.recent_events,
            &self.telemetry_search,
            &self.telemetry_kind_filter,
        );
        if recent_events.is_empty() {
            ui.label("No telemetry records yet.");
        } else {
            egui::Grid::new("telemetry_recent_events")
                .striped(true)
                .min_col_width(48.0)
                .show(ui, |ui| {
                    ui.strong("Type");
                    ui.strong("When");
                    ui.strong("Name");
                    ui.strong("Details");
                    ui.end_row();
                    for event in recent_events.into_iter().take(50) {
                        ui.label(&event.kind);
                        ui.monospace(human_datetime(event.created_unix))
                            .on_hover_text(format!("unix {}", event.created_unix));
                        ui.label(clip_text(&event.title, 40)).on_hover_text(&event.title);
                        ui.label(clip_text(&event.detail, 72))
                            .on_hover_text(&event.detail);
                        ui.end_row();
                    }
                });
        }
        ui.separator();
        ui.heading("Per-Mod History");
        let mod_rows = filtered_mod_history(&self.telemetry.mod_history, &self.telemetry_search);
        if mod_rows.is_empty() {
            ui.label("No per-mod telemetry yet.");
            return;
        }
        egui::Grid::new("telemetry_mod_history")
            .striped(true)
            .min_col_width(48.0)
            .show(ui, |ui| {
                ui.strong("Mod");
                ui.strong("Imports");
                ui.strong("Runs");
                ui.strong("Installs");
                ui.strong("Copied");
                ui.strong("Overwrites");
                ui.strong("Issues");
                ui.strong("Last seen");
                ui.end_row();
                for row in mod_rows.into_iter().take(50) {
                    ui.label(&row.id);
                    ui.monospace(row.imports.to_string());
                    ui.monospace(row.runs.to_string());
                    ui.monospace(row.installs.to_string());
                    ui.monospace(row.copied_files.to_string());
                    ui.monospace(row.overwritten_files.to_string());
                    ui.monospace((row.missing_sources + row.blocked_bootstrap).to_string());
                    ui.monospace(human_datetime(row.last_seen_unix))
                        .on_hover_text(format!("unix {}", row.last_seen_unix));
                    ui.end_row();
                }
            });
    }
}

fn filtered_telemetry_events(
    events: &[TelemetryEvent],
    search: &str,
    kind_filter: &str,
) -> Vec<TelemetryEvent> {
    let needle = search.trim().to_ascii_lowercase();
    events
        .iter()
        .filter(|event| kind_filter == "all" || event.kind == kind_filter)
        .filter(|event| {
            needle.is_empty()
                || event.title.to_ascii_lowercase().contains(&needle)
                || event.detail.to_ascii_lowercase().contains(&needle)
                || event.kind.to_ascii_lowercase().contains(&needle)
        })
        .cloned()
        .collect()
}

fn filtered_mod_history(rows: &[ModTelemetry], search: &str) -> Vec<ModTelemetry> {
    let needle = search.trim().to_ascii_lowercase();
    rows.iter()
        .filter(|row| needle.is_empty() || row.id.to_ascii_lowercase().contains(&needle))
        .cloned()
        .collect()
}
