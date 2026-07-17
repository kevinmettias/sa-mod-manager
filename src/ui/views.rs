use crate::prelude::*;
use eframe::egui;

use super::san_andreas_mod_ui::{ROW_HEIGHT, ReadmeProposal, ReadmeProposalState, SanAndreasModUi};
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
        });
        if let Some(summary) = self.pending_cleanup_summary() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong(summary);
                if ui.button("Review").clicked() {
                    self.tab = UiTab::Run;
                }
                if ui.button("Clean finished").clicked() {
                    self.cleanup_stale_pending_runs();
                }
            });
        }
    }
}

impl SanAndreasModUi {
    pub(super) fn navigation(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.selectable_value(&mut self.tab, UiTab::Home, "Start"); // literal: allow external interface text or file-format spelling
        ui.selectable_value(&mut self.tab, UiTab::Profiles, "Profiles"); // literal: allow external interface text or file-format spelling
        ui.selectable_value(&mut self.tab, UiTab::Mods, "Library"); // literal: allow external interface text or file-format spelling
        ui.selectable_value(&mut self.tab, UiTab::Import, "Import Mod"); // literal: allow external interface text or file-format spelling
        ui.selectable_value(&mut self.tab, UiTab::Run, "Play & Cleanup"); // literal: allow external interface text or file-format spelling
        ui.selectable_value(&mut self.tab, UiTab::Telemetry, "Telemetry"); // literal: allow external interface text or file-format spelling
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
        ui.horizontal(|ui| {
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
}

impl SanAndreasModUi {
    pub(super) fn home_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Start");
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
        ui.horizontal(|ui| {
            if ui.button("Import a mod").clicked() {
                self.tab = UiTab::Import;
            }
            if ui.button("Review profile").clicked() {
                self.tab = UiTab::Profiles;
            }
            if ui.button("Play profile").clicked() {
                self.tab = UiTab::Run;
            }
            if !self.pending_runs.is_empty() && ui.button("Clean finished runs").clicked() {
                self.cleanup_stale_pending_runs();
            }
            if ui.button("View telemetry").clicked() {
                self.tab = UiTab::Telemetry;
            }
        });
        ui.separator();
        ui.heading("Current Profile");
        let entries = selected_profile_entries(&self.state, &self.selected_profile);
        if entries.is_empty() {
            ui.label("No mods selected.");
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
        }
        ui.separator();
        ui.heading("Game Setup");
        infrastructure_grid(ui, &self.state.infrastructure);
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
    pub(super) fn profiles_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Profile"); // literal: allow external interface text or file-format spelling
        ui.separator();
        let entries = selected_profile_entries(&self.state, &self.selected_profile);
        if entries.is_empty() {
            ui.label("No mods in this profile."); // literal: allow external interface text or file-format spelling
            if ui.button("Add imported mods").clicked() {
                self.tab = UiTab::Mods;
            }
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
        if ui.checkbox(&mut enabled, "").changed() {
            // literal: allow external interface text or file-format spelling
            let activation = if enabled {
                ProfileModActivation::Enabled
            } else {
                ProfileModActivation::Disabled
            };
            self.set_mod_activation(&entry.id, activation);
        }
        let mut order = entry.load_order;
        let order_drag = egui::DragValue::new(&mut order).speed(10);
        if ui.add(order_drag).changed() {
            self.set_mod_order(&entry.id, order);
        }
        ui.label(&entry.id);
        let config_display = entry.config.display().to_string();
        ui.monospace(config_display);
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        if ui.button("Remove").clicked() {
            // literal: allow external interface text or file-format spelling
            self.remove_mod_from_profile(&entry.id);
        }
        ui.end_row();
    }
}

impl SanAndreasModUi {
    pub(super) fn mods_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Library"); // literal: allow external interface text or file-format spelling
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
            ui.label(format!("Config: {path_display}"));
            ui.label(format!("package: {}", item.config.package.display()));
            if let Some(source_root) = &item.config.source_root {
                ui.label(format!("Library files: {}", source_root.display()));
            }
            ui.separator();
            ui.strong("Install locations");
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
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Package or folder"); // literal: allow external interface text or file-format spelling
            let package_editor = egui::TextEdit::singleline(&mut self.import_path_input);
            ui.add_sized([650.0, ROW_HEIGHT], package_editor);
        });
        ui.horizontal(|ui| {
            /* literal: allow external interface text or file-format spelling */
            /* literal: allow external interface text or file-format spelling */
            if ui.button("Review package").clicked() {
                // literal: allow external interface text or file-format spelling
                self.analyze_package();
            }
            /* literal: allow external interface text or file-format spelling */
            /* literal: allow external interface text or file-format spelling */
            if ui.button("Import to library").clicked() {
                // literal: allow external interface text or file-format spelling
                self.import_package();
            }
        });
        ui.separator();
        ui.label("Supported: folders, .zip, .wrap, .7z, .rar."); // literal: allow external interface text or file-format spelling
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
        ui.label(format!("Install location: {}", proposal.proposed_install));
        ui.label(format!(
            "Evidence: {} line {}: {}",
            proposal.source_readme, proposal.line_number, proposal.evidence
        ));
        ui.label(format!("Matched text: {}", proposal.normalized_text));
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
        ui.label(format!(
            "Profile `{}` has {} enabled mods.",
            self.selected_profile, active_count
        ));
        ui.horizontal(|ui| {
            /* literal: allow external interface text or file-format spelling */
            /* literal: allow external interface text or file-format spelling */
            if ui.button("Play").clicked() {
                // literal: allow external interface text or file-format spelling
                self.launch_selected_profile();
            }
            /* literal: allow external interface text or file-format spelling */
            /* literal: allow external interface text or file-format spelling */
            if ui.button("Clean selected").clicked() {
                // literal: allow external interface text or file-format spelling
                self.cleanup_pending_run();
            }
            if ui.button("Check status").clicked() {
                self.refresh_pending_runs();
            }
            if ui.button("Clean finished").clicked() {
                self.cleanup_stale_pending_runs();
            }
        });
        ui.separator();
        ui.heading("Temporary Files Cleanup");
        if self.pending_runs.is_empty() {
            ui.label("No temporary mod files are waiting for cleanup.");
        } else {
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
                        if ui.button("Clean").clicked() {
                            self.cleanup_pending_run_record(record);
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
        ui.heading("Recent Activity");
        if self.telemetry.recent_events.is_empty() {
            ui.label("No telemetry records yet.");
            return;
        }
        egui::Grid::new("telemetry_recent_events")
            .striped(true)
            .min_col_width(96.0)
            .show(ui, |ui| {
                ui.strong("Type");
                ui.strong("When");
                ui.strong("Name");
                ui.strong("Details");
                ui.end_row();
                for event in &self.telemetry.recent_events {
                    ui.label(&event.kind);
                    ui.monospace(event.created_unix.to_string());
                    ui.label(&event.title);
                    ui.label(&event.detail);
                    ui.end_row();
                }
            });
    }
}
