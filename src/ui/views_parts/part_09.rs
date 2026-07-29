
/// Returns `true` when the user clicked "Add to config" for this proposal.
enum ReadmeImportState
{
    Imported,
    NotImported,
}

impl ReadmeImportState
{
    fn is_imported(self) -> bool
    {
        return matches!(self, Self::Imported);
    }
}

fn readme_proposal_panel(ui: &mut egui::Ui, proposal: &ReadmeProposal, import_state: ReadmeImportState) -> bool
{
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
            ui.weak(format!("{:.0}%", proposal.confidence * PERCENT_SCALE));
            ui.weak(proposal.review_state.to_string());
            if readme_proposal_is_actionable(proposal)
            {
                accepted = ui
                    .add_enabled(import_state.is_imported(), egui::Button::new("Add to config"))
                    .on_hover_text(
                        "Append this copy as an install root on the imported mod's config",
                    )
                    .on_disabled_hover_text("Import this package to the library first")
                    .clicked();
            }
        });
        ui.label(format!("→ {}", proposal.proposed_install));
        let clipped_evidence = clip_text(&evidence, README_EVIDENCE_CLIP);
        ui.small(clipped_evidence).on_hover_text(format!(
            "{evidence}\nNormalized: {}",
            proposal.normalized_text
        ));
        if !proposal.reasons.is_empty()
        {
            ui.small(format!("Why: {}", proposal.reasons.join("; ")));
        }
    });
    return accepted;
}

fn readme_proposal_title(proposal: &ReadmeProposal) -> String
{
    return match proposal.review_state
    {
        ReadmeProposalState::AutoSelected => format!("Ready {}", proposal.action),
        ReadmeProposalState::NeedsReview => format!("Review {}", proposal.action),
        ReadmeProposalState::WarningOnly => format!("Warning {}", proposal.action),
    };
}

/// Only a `copy` proposal with a concrete source and target maps to an install
/// root; relationship hints (requires/conflict/…) are informational only.
fn readme_proposal_is_actionable(proposal: &ReadmeProposal) -> bool
{
    return proposal.action == "copy" && proposal.source.is_some() && proposal.target.is_some();
}

impl SanAndreasModUi
{
    /// View, compare and edit every ModLoader mod's priority for this profile,
    /// and enable/disable them within ModLoader (priority 0). Edits persist per
    /// profile and are written to modloader.ini via Apply. Requires a content
    /// scan to know which mods are ModLoader mods.
    pub(super) fn modloader_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.heading("ModLoader Priority");
        ui.label(
            "Order and compare ModLoader mods for this profile. Higher priority wins file \
             conflicts; priority 0 disables the mod in ModLoader — its files stay installed and it \
             still shows in the in-game Mod Configuration menu. These override the load-order \
             defaults and are applied automatically when you Play through the manager. Use Apply to \
             also write them into your active ModLoader profile for playing outside the manager.",
        );
        ui.separator();

        let Some(index) = &self.content.index else {
            ui.weak("Run Analyze content (left panel) to list ModLoader mods.");
            return;
        };
        let mut folders: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in &index.entries
        {
            if entry.category == ContentCategory::ModLoader
            {
                if let Some(folder) = modloader_folder_of(&entry.target)
                {
                    folders.insert(folder);
                }
            }
        }
        if folders.is_empty()
        {
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
                    .unwrap_or(MODLOADER_DEFAULT_PRIORITY);
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
            if overrides > 0
            {
                ui.weak(format!("{overrides} override(s) set for this profile"));
            }
        });
        ui.separator();

        egui::Grid::new("modloader_priority_grid")
            .striped(true)
            .num_columns(MODLOADER_GRID_COLUMNS)
            .min_col_width(GRID_MIN_COL_WIDTH)
            .show(ui, |ui| {
                ui.strong("On");
                ui.strong("Priority");
                ui.strong("ModLoader folder");
                ui.end_row();
                for (folder, priority) in &rows
                {
                    let mut enabled = *priority > 0;
                    if ui
                        .checkbox(&mut enabled, "")
                        .on_hover_text(
                            "Enabled in ModLoader — uncheck to set priority 0 (disabled, but still \
                             installed and shown in the in-game menu)",
                        )
                        .changed()
                    {
                        set_priority = Some((
                            folder.clone(),
                            if enabled
                            {
                                MODLOADER_DEFAULT_PRIORITY
                            }
                            else
                            {
                                MODLOADER_MIN_PRIORITY
                            },
                        ));
                    }
                    let mut value = *priority;
                    let response = ui.add_enabled(
                        *priority > 0,
                        egui::DragValue::new(&mut value)
                            .range(MODLOADER_MIN_PRIORITY..=MODLOADER_MAX_PRIORITY)
                            .speed(MODLOADER_PRIORITY_STEP),
                    );
                    if response.changed()
                    {
                        set_priority = Some((
                            folder.clone(),
                            value.clamp(MODLOADER_MIN_PRIORITY, MODLOADER_MAX_PRIORITY),
                        ));
                    }
                    ui.label(folder);
                    ui.end_row();
                }
            });

        if let Some((folder, priority)) = set_priority
        {
            self.set_modloader_priority(&folder, priority);
        }
        if apply
        {
            self.apply_modloader_priorities();
        }
    }

    pub(super) fn run_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.heading("Play Selected Profile");
        ui.separator();
        let active_count = selected_profile_entries(&self.state, &self.selected_profile)
            .iter()
            .filter(|entry| entry.enabled)
            .count();
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, SummaryTile { label: "Profile", value: &self.selected_profile });
            summary_tile(ui, SummaryTile { label: "Enabled mods", value: &active_count.to_string() });
            summary_tile(ui, SummaryTile { label: "Cleanup", value: if self.pending_runs.is_empty() { "clear" } else { "needed" } });
            summary_tile(ui, SummaryTile { label: "Readiness", value: readiness_label(self) });
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
            if ui.button("Check status").clicked()
            {
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
        if self.pending_runs.is_empty()
        {
            ui.label("No temporary mod files are waiting for cleanup.");
        }
        else
        {
            let stale = count_pending_status(self, PendingRunStatus::Stale);
            let running = count_pending_status(self, PendingRunStatus::Running);
            let unknown = count_pending_status(self, PendingRunStatus::Unknown);
            let invalid = count_pending_status(self, PendingRunStatus::Invalid);
            ui.horizontal_wrapped(|ui| {
                summary_tile(ui, SummaryTile { label: "Finished", value: &stale.to_string() });
                summary_tile(ui, SummaryTile { label: "Running", value: &running.to_string() });
                summary_tile(ui, SummaryTile { label: "Unknown", value: &unknown.to_string() });
                summary_tile(ui, SummaryTile { label: "Invalid", value: &invalid.to_string() });
            });
            ui.label("Running games block cleanup. Finished runs can be cleaned safely from here.");
            let pending_runs = self.pending_runs.to_vec();
            egui::Grid::new("pending_run_grid")
                .striped(true)
                .min_col_width(PENDING_RUN_GRID_MIN_COL_WIDTH)
                .show(ui, |ui| {
                    ui.label("Status");
                    ui.label("PID");
                    ui.label("Detail");
                    ui.label("Cleanup record");
                    ui.label("");
                    ui.end_row();
                    for record in pending_runs
                    {
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
        ui.heading("Game Infrastructure");
        infrastructure_grid(ui, &self.state.infrastructure);
    }

    /// Manage external run targets (MO2's executables): list the configured
    /// tools with a Remove button, and a form to add a new one.
    fn executables_editor(&mut self, ui: &mut egui::Ui)
    {
        egui::CollapsingHeader::new("Run targets / tools")
            .id_salt("executables_editor")
            .show(ui, |ui| {
                ui.label(
                    "Configure external tools (map editors, IMG tools, a CLEO debugger, or the \
                     game with custom args) to launch from the dropdown above.",
                );
                if self.executables.is_empty()
                {
                    ui.weak("No tools configured yet.");
                }
                else
                {
                    let mut remove_index = None;
                    for (index, tool) in self.executables.iter().enumerate()
                    {
                        ui.horizontal(|ui| {
                            ui.strong(&tool.name);
                            ui.monospace(&tool.path);
                            if !tool.args.is_empty()
                            {
                                ui.weak(format!("args: {}", tool.args));
                            }
                            if ui.button("Remove").clicked()
                            {
                                remove_index = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove_index
                    {
                        self.remove_executable(index);
                    }
                }
                ui.separator();
                ui.label("Add a tool");
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add_sized(
                        [COMPACT_FIELD_WIDTH, ROW_HEIGHT],
                        egui::TextEdit::singleline(&mut self.new_tool_name),
                    );
                    ui.label("Args");
                    ui.add_sized(
                        [MEDIUM_FIELD_WIDTH, ROW_HEIGHT],
                        egui::TextEdit::singleline(&mut self.new_tool_args)
                            .hint_text("-optional -flags"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Path");
                    ui.add_sized(
                        [TOOL_PATH_FIELD_WIDTH, ROW_HEIGHT],
                        egui::TextEdit::singleline(&mut self.new_tool_path)
                            .hint_text("full path to a .exe"),
                    );
                    if ui.button("Browse…").clicked()
                    {
                        self.browse_tool_path();
                    }
                    if ui.button("Add").clicked()
                    {
                        self.add_executable();
                    }
                });
            });
    }

    /// MO2's "overwrite": game-folder files under mod areas that no enabled mod
    /// provides — installed by hand or left by a tool. Computed on the last
    /// content scan; the manager never touches these, so they persist across runs.
    fn overwrite_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.heading("Unmanaged Files (Overwrite)");
        if self.content.index.is_none()
        {
            ui.weak("Run Analyze content (left panel) to detect unmanaged files.");
            return;
        }
        if self.overwrite_files.is_empty()
        {
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
            .max_height(OVERWRITE_PANEL_MAX_HEIGHT)
            .show(ui, |ui| {
                for file in self
                    .overwrite_files
                    .iter()
                    .take(OVERWRITE_VISIBLE_FILE_LIMIT)
                {
                    ui.monospace(file);
                }
                if self.overwrite_files.len() > OVERWRITE_VISIBLE_FILE_LIMIT
                {
                    ui.weak(format!(
                        "{} more not shown",
                        self.overwrite_files.len() - OVERWRITE_VISIBLE_FILE_LIMIT
                    ));
                }
            });
    }
}






