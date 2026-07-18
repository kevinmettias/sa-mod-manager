use crate::prelude::*;
use eframe::egui;

use super::san_andreas_mod_ui::{
    ModTelemetry, PendingRunStatus, ROW_HEIGHT, ReadmeProposal, ReadmeProposalState,
    SanAndreasModUi, TelemetryEvent,
};
use super::state::{ModConfigItem, UiTab};
use super::widgets::{
    infrastructure_grid, profile_grid_header, selected_profile_entries, should_add_mod_to_profile,
};

impl SanAndreasModUi {
    pub(super) fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("SA Mod Manager"); // literal: allow external interface text or file-format spelling
            ui.separator();
            ui.label("Game folder"); // literal: allow external interface text or file-format spelling
            let game_root_editor = egui::TextEdit::singleline(&mut self.game_root_input);
            ui.add_sized([520.0, ROW_HEIGHT], game_root_editor);
            /* literal: allow external interface text or file-format spelling */
            /* literal: allow external interface text or file-format spelling */
            if ui.button("Reload").clicked() {
                // literal: allow external interface text or file-format spelling
                self.refresh();
            }
            /* literal: allow external interface text or file-format spelling */
            /* literal: allow external interface text or file-format spelling */
            if ui.button("Initialize").clicked() {
                // literal: allow external interface text or file-format spelling
                self.initialize_state();
            }
            if ui.button("Start").clicked() {
                self.tab = UiTab::Home;
            }
        });
        if let Some(summary) = self.pending_cleanup_summary() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong(summary);
                if ui.button("Review").clicked() {
                    self.tab = UiTab::Run;
                }
                if ui.button("Clean finished").clicked() {
                    self.request_cleanup_finished();
                }
            });
        }
    }
}

impl SanAndreasModUi {
    pub(super) fn navigation(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.selectable_value(&mut self.tab, UiTab::Home, "Start"); // literal: allow external interface text or file-format spelling
        ui.small("overview and next action");
        ui.selectable_value(&mut self.tab, UiTab::Profiles, "Profiles"); // literal: allow external interface text or file-format spelling
        ui.small("load order and toggles");
        ui.selectable_value(&mut self.tab, UiTab::Mods, "Library"); // literal: allow external interface text or file-format spelling
        ui.small("installed packages");
        ui.selectable_value(&mut self.tab, UiTab::Import, "Import Mod"); // literal: allow external interface text or file-format spelling
        ui.small("readme review");
        ui.selectable_value(&mut self.tab, UiTab::Run, "Play & Cleanup"); // literal: allow external interface text or file-format spelling
        ui.small("temporary run safety");
        ui.selectable_value(&mut self.tab, UiTab::Telemetry, "Telemetry"); // literal: allow external interface text or file-format spelling
        ui.small("history and export");
        ui.separator();
        ui.label("Profile"); // literal: allow external interface text or file-format spelling
        let mut changed_profile = false;
        egui::ComboBox::from_id_salt("profile_picker") // literal: allow external interface text or file-format spelling
            .selected_text(&self.selected_profile)
            .show_ui(ui, |ui| {
                for profile in &self.state.profiles {
                    let profile_choice = profile.clone();
                    if ui
                        .selectable_value(&mut self.selected_profile, profile_choice, profile)
                        .changed()
                    {
                        changed_profile = true;
                    }
                }
            });
        if changed_profile {
            self.refresh();
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let new_profile_editor = egui::TextEdit::singleline(&mut self.new_profile_input);
            ui.add_sized([112.0, ROW_HEIGHT], new_profile_editor);
            /* literal: allow external interface text or file-format spelling */
            /* literal: allow external interface text or file-format spelling */
            if ui.button("New").clicked() {
                // literal: allow external interface text or file-format spelling
                self.create_profile();
            }
        });
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
            ui.label(&self.status);
            if let Some(journal) = &self.pending_journal {
                ui.separator();
                ui.label(format!("cleanup record: {}", journal.display()));
            }
            if !self.pending_runs.is_empty() {
                ui.separator();
                ui.label("temporary files need cleanup before another launch");
            }
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
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
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
    pub(super) fn home_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Start");
        ui.label("Current setup, next action, and cleanup state in one place.");
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, "Profile", &self.selected_profile);
            summary_tile(ui, "Enabled mods", &enabled_profile_count(self).to_string());
            summary_tile(
                ui,
                "Library",
                &format!("{} imported", self.state.mods.len()),
            );
            summary_tile(
                ui,
                "Cleanup",
                if self.pending_runs.is_empty() {
                    "clear"
                } else {
                    "needed"
                },
            );
            summary_tile(ui, "Runs", &self.telemetry.run_journals.to_string());
        });
        ui.add_space(8.0);
        self.next_action_panel(ui);
        ui.separator();
        self.workflow_panel(ui);
        ui.separator();
        ui.heading("Current Profile");
        let entries = selected_profile_entries(&self.state, &self.selected_profile);
        if entries.is_empty() {
            ui.label("No mods selected for this profile.");
            ui.horizontal(|ui| {
                if ui.button("Open library").clicked() {
                    self.tab = UiTab::Mods;
                }
                if ui.button("Import first mod").clicked() {
                    self.tab = UiTab::Import;
                }
            });
        } else {
            egui::Grid::new("home_profile_summary")
                .striped(true)
                .min_col_width(100.0)
                .show(ui, |ui| {
                    ui.strong("Order");
                    ui.strong("Status");
                    ui.strong("Mod");
                    ui.end_row();
                    for entry in entries.iter().take(8) {
                        ui.monospace(entry.load_order.to_string());
                        ui.label(if entry.enabled { "enabled" } else { "off" });
                        ui.label(&entry.id);
                        ui.end_row();
                    }
                });
            if entries.len() > 8 {
                ui.label(format!(
                    "{} more profile entries hidden.",
                    entries.len() - 8
                ));
            }
            if ui.button("Edit profile load order").clicked() {
                self.tab = UiTab::Profiles;
            }
        }
        ui.separator();
        ui.heading("Game Setup");
        infrastructure_grid(ui, &self.state.infrastructure);
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
        egui::Grid::new("home_workflow")
            .striped(true)
            .min_col_width(120.0)
            .show(ui, |ui| {
                workflow_row(
                    ui,
                    "1. Game setup",
                    setup_status(self),
                    "Verify executable, Mod Loader, CLEO, and ASI loader paths.",
                );
                workflow_row(
                    ui,
                    "2. Import",
                    import_status(self),
                    "Review archive contents and readme-driven install proposals.",
                );
                workflow_row(
                    ui,
                    "3. Profile",
                    profile_status(self),
                    "Choose enabled mods and load order for this profile.",
                );
                workflow_row(
                    ui,
                    "4. Play",
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
            detail: "The selected profile has no enabled mods yet.",
            primary_label: "Build profile",
            primary_tab: Some(UiTab::Profiles),
            secondary: Some(("Open library", UiTab::Mods)),
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

fn setup_status(ui_state: &SanAndreasModUi) -> WorkflowStatus {
    if game_executable_present(ui_state) {
        WorkflowStatus::Ready
    } else {
        WorkflowStatus::Review
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
    pub(super) fn profiles_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Profile"); // literal: allow external interface text or file-format spelling
        ui.separator();
        let entries = selected_profile_entries(&self.state, &self.selected_profile);
        let enabled = entries.iter().filter(|entry| entry.enabled).count();
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, "Selected", &self.selected_profile);
            summary_tile(ui, "Enabled", &enabled.to_string());
            summary_tile(ui, "Total", &entries.len().to_string());
            summary_tile(
                ui,
                "Load order",
                if entries
                    .windows(2)
                    .all(|pair| pair[0].load_order <= pair[1].load_order)
                {
                    "sorted"
                } else {
                    "review"
                },
            );
        });
        ui.label("Toggle mods for this profile and set lower load-order numbers first.");
        ui.separator();
        if entries.is_empty() {
            ui.label("No mods in this profile."); // literal: allow external interface text or file-format spelling
            ui.horizontal(|ui| {
                if ui.button("Add imported mods").clicked() {
                    self.tab = UiTab::Mods;
                }
                if ui.button("Import a mod").clicked() {
                    self.tab = UiTab::Import;
                }
            });
            return;
        }
        egui::Grid::new("profile_mod_grid") // literal: allow external interface text or file-format spelling
            .striped(true)
            .min_col_width(92.0)
            .show(ui, |ui| {
                profile_grid_header(ui);
                for entry in entries {
                    self.profile_mod_row(ui, &entry);
                }
            });
    }
}

impl SanAndreasModUi {
    pub(super) fn profile_mod_row(&mut self, ui: &mut egui::Ui, entry: &ProfileModEntry) {
        let mut enabled = entry.enabled;
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        if ui
            .checkbox(&mut enabled, "")
            .on_hover_text("Enable or disable this mod in the current profile")
            .changed()
        {
            let activation = if enabled {
                ProfileModActivation::Enabled
            } else {
                ProfileModActivation::Disabled
            };
            self.set_mod_activation(&entry.id, activation);
        }
        let mut order = entry.load_order;
        let order_drag = egui::DragValue::new(&mut order).speed(10);
        if ui
            .add(order_drag)
            .on_hover_text("Load order: lower loads first; later mods overwrite earlier files")
            .changed()
        {
            self.set_mod_order(&entry.id, order);
        }
        ui.label(&entry.id);
        let config_display = entry.config.display().to_string();
        ui.monospace(config_display);
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        if ui
            .button("Remove")
            .on_hover_text("Remove this mod from the profile (asks first)")
            .clicked()
        {
            self.request_remove_mod(&entry.id);
        }
        ui.end_row();
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
                ui.add_sized(
                    [160.0, ROW_HEIGHT],
                    egui::TextEdit::singleline(&mut edit.kind),
                );
                if ui.button("Save root").clicked() {
                    save_root = Some(edit.clone());
                }
                if ui.button("Reset").clicked() {
                    *edit = root.clone();
                }
            });
        });
        if let Some(updated) = save_root {
            self.save_mod_install_root(&item.path, root_index, updated);
            self.mod_root_edits.remove(&key);
        }
    }
}

fn mod_root_edit_key(path: &Path, root_index: usize) -> String {
    format!("{}#{root_index}", path.display())
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
    pub(super) fn import_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Import Mod"); // literal: allow external interface text or file-format spelling
        ui.label("Review first, then import. Medium-confidence readme matches stay visible for human judgment.");
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Package or folder"); // literal: allow external interface text or file-format spelling
            let package_editor = egui::TextEdit::singleline(&mut self.import_path_input);
            ui.add_sized([650.0, ROW_HEIGHT], package_editor);
        });
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
            summary_tile(ui, "Auto-selected", &auto_selected.to_string());
            summary_tile(ui, "Needs review", &needs_review.to_string());
            summary_tile(ui, "Warnings", &warnings.to_string());
        });
        let proposals = self.readme_proposals.clone();
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .show(ui, |ui| {
                for proposal in proposals {
                    readme_proposal_panel(ui, &proposal);
                    ui.add_space(6.0);
                }
            });
    }
}

fn readme_proposal_panel(ui: &mut egui::Ui, proposal: &ReadmeProposal) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.strong(readme_proposal_title(proposal));
            ui.separator();
            ui.label(format!("{:.0}% confidence", proposal.confidence * 100.0));
            ui.separator();
            ui.label(proposal.review_state.to_string());
        });
        ui.label(format!("Proposed install: {}", proposal.proposed_install));
        ui.label(format!(
            "Evidence: {} line {}: {}",
            proposal.source_readme, proposal.line_number, proposal.evidence
        ));
        ui.label(format!("Normalized match: {}", proposal.normalized_text));
        if !proposal.reasons.is_empty() {
            ui.label(format!("Why: {}", proposal.reasons.join("; ")));
        }
    });
}

fn readme_proposal_title(proposal: &ReadmeProposal) -> String {
    match proposal.review_state {
        ReadmeProposalState::AutoSelected => format!("Ready {}", proposal.action),
        ReadmeProposalState::NeedsReview => format!("Review {}", proposal.action),
        ReadmeProposalState::WarningOnly => format!("Warning {}", proposal.action),
    }
}

impl SanAndreasModUi {
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
        ui.label("Play materializes this profile temporarily, launches the game, then watches for cleanup.");
        let idle = !self.is_busy();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(idle, egui::Button::new("Play"))
                .on_hover_text("Materialize this profile, launch the game, then auto-clean on exit")
                .on_disabled_hover_text("A background task is running")
                .clicked()
            {
                self.launch_selected_profile(ui.ctx());
            }
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
            summary_tile(ui, "Imports", &self.telemetry.imports.to_string());
            summary_tile(ui, "Runs", &self.telemetry.run_journals.to_string());
            summary_tile(ui, "Installs", &self.telemetry.install_journals.to_string());
            summary_tile(ui, "Copied files", &self.telemetry.copied_files.to_string());
            summary_tile(
                ui,
                "Overwrites",
                &self.telemetry.overwritten_files.to_string(),
            );
            summary_tile(
                ui,
                "Cleanup needed",
                &self.telemetry.pending_cleanup.to_string(),
            );
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, "New files", &self.telemetry.new_files.to_string());
            summary_tile(
                ui,
                "Blocked bootstrap",
                &self.telemetry.blocked_bootstrap.to_string(),
            );
            summary_tile(
                ui,
                "Missing sources",
                &self.telemetry.missing_sources.to_string(),
            );
            summary_tile(ui, "Journals", &self.telemetry.journals.to_string());
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
                .min_col_width(96.0)
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
                        ui.label(&event.title);
                        ui.label(&event.detail);
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
            .min_col_width(96.0)
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
