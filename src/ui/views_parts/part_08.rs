
impl SanAndreasModUi
{
    /// The per-mod info window (MO2's Mod Info dialog): Files / Conflicts / Install
    /// roots / Readme for one mod. Shown as a floating, closable window.
    pub(super) fn mod_details_window(&mut self, ctx: &egui::Context)
    {
        let Some(details) = &self.mod_info else {
            return;
        };
        let id = details.id.clone();
        let mut tab = details.tab;
        let mut open = true;
        egui::Window::new(format!("Mod: {id}"))
            .id(egui::Id::new("mod_details_window"))
            .open(&mut open)
            .resizable(true)
            .default_size([MOD_CONFIG_PANEL_HEIGHT, MOD_CONFIG_PANEL_WIDTH])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut tab, ModDetailsTab::Files, "Files");
                    ui.selectable_value(&mut tab, ModDetailsTab::Conflicts, "Conflicts");
                    ui.selectable_value(&mut tab, ModDetailsTab::Roots, "Install roots");
                    ui.selectable_value(&mut tab, ModDetailsTab::Readme, "Readme");
                    ui.selectable_value(&mut tab, ModDetailsTab::Notes, "Notes");
                });
                ui.separator();
                match tab
                {
                    ModDetailsTab::Files => self.mod_details_files_tab(ui),
                    ModDetailsTab::Conflicts => self.mod_details_conflicts_tab(ui, &id),
                    ModDetailsTab::Roots => self.mod_details_roots_tab(ui, &id),
                    ModDetailsTab::Readme => self.mod_details_readme_tab(ui),
                    ModDetailsTab::Notes => self.mod_details_notes_tab(ui, &id),
                }
            });
        if let Some(info) = self.mod_info.as_mut()
        {
            info.tab = tab;
        }
        if !open
        {
            self.mod_info = None;
        }
    }

    fn mod_details_files_tab(&mut self, ui: &mut egui::Ui)
    {
        let Some(details) = &self.mod_info else {
            return;
        };
        match &details.source_root
        {
            Some(root) => {
                ui.weak(format!("Library: {}", root.display()));
            }
            None => {
                ui.weak("Not extracted to the library â€” file list unavailable.");
            }
        }
        ui.label(format!("{} files", details.files.len()));
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for file in details.files.iter().take(MAX_LOG_PREVIEW_BYTES)
                {
                    ui.monospace(file);
                }
                if details.files.len() > MAX_LOG_PREVIEW_BYTES
                {
                    ui.weak(format!(
                        "{} more not shown",
                        details.files.len() - MAX_LOG_PREVIEW_BYTES
                    ));
                }
            });
    }

    fn mod_details_conflicts_tab(&mut self, ui: &mut egui::Ui, id: &str)
    {
        let Some(index) = &self.content.index else {
            ui.label("Run Analyze content (left panel) to compute conflicts.");
            return;
        };
        let conflicts: Vec<&ContentEntry> = index
            .entries
            .iter()
            .filter(|entry| entry.is_conflict() && entry.providers.iter().any(|provider| provider == id))
            .collect();
        if conflicts.is_empty()
        {
            ui.label("None of this mod's files conflict with another enabled mod.");
            return;
        }
        ui.label(format!("{} conflicting files", conflicts.len()));
        let win_color = egui::Color32::from_rgb(
            RUNNING_STATUS_RED,
            RUNNING_STATUS_GREEN,
            RUNNING_STATUS_BLUE,
        );
        let lose_color = ui.visuals().warn_fg_color;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("mod_info_conflicts")
                    .striped(true)
                    .num_columns(MODLOADER_GRID_COLUMNS)
                    .min_col_width(LOG_PREVIEW_GRID_MIN_COL_WIDTH)
                    .show(ui, |ui| {
                        ui.strong("File");
                        ui.strong("This mod");
                        ui.strong("Winner");
                        ui.end_row();
                        for entry in &conflicts
                        {
                            ui.monospace(&entry.target);
                            if entry.winner() == id
                            {
                                ui.colored_label(win_color, "wins");
                            }
                            else
                            {
                                ui.colored_label(lose_color, "overwritten");
                            }
                            ui.label(entry.winner());
                            ui.end_row();
                        }
                    });
            });
    }

    fn mod_details_roots_tab(&mut self, ui: &mut egui::Ui, id: &str)
    {
        let Some(item) = self.state.mods.iter().find(|item| item.config.id == id) else {
            ui.label("Mod not found in the library.");
            return;
        };
        ui.monospace(format!("package: {}", item.config.package.display()));
        if let Some(source_root) = &item.config.source_root
        {
            ui.monospace(format!("library: {}", source_root.display()));
        }
        ui.separator();
        if item.config.install_roots.is_empty()
        {
            ui.label("No install roots detected for this mod.");
            return;
        }
        egui::Grid::new("mod_info_roots")
            .striped(true)
            .num_columns(CONTENT_GRID_COLUMNS)
            .min_col_width(LOG_PREVIEW_GRID_MIN_COL_WIDTH)
            .show(ui, |ui| {
                ui.strong("Source");
                ui.strong("Target");
                ui.strong("Kind");
                ui.strong("On");
                ui.end_row();
                for root in &item.config.install_roots
                {
                    ui.monospace(&root.source);
                    ui.monospace(&root.target);
                    ui.label(&root.kind);
                    ui.label(if root.enabled { "yes" } else { "no" });
                    ui.end_row();
                }
            });
    }

    fn mod_details_readme_tab(&mut self, ui: &mut egui::Ui)
    {
        let Some(details) = &self.mod_info else {
            return;
        };
        if details.readmes.is_empty()
        {
            ui.label("No readme files found in this mod.");
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (name, text) in &details.readmes
                {
                    egui::CollapsingHeader::new(name)
                        .id_salt(name)
                        .default_open(details.readmes.len() == 1)
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
    fn mod_details_notes_tab(&mut self, ui: &mut egui::Ui, id: &str)
    {
        let meta = self.mod_meta.get(id).cloned().unwrap_or_default();
        let mut set_color: Option<Option<String>> = None;
        let mut toggle_category: Option<String> = None;
        let mut add_category = false;
        let mut save_note = false;

        ui.horizontal(|ui| {
            ui.label("Color");
            let swatch_size = egui::vec2(MOD_COLOR_SWATCH_WIDTH, MOD_COLOR_SWATCH_HEIGHT);
            for (name, color) in MOD_COLORS
            {
                let selected = meta.color.as_deref() == Some(name);
                let label = if selected { "â—" } else { "  " };
                if ui
                    .add(egui::Button::new(label).fill(color).min_size(swatch_size))
                    .on_hover_text(name)
                    .clicked()
                {
                    set_color = Some(Some(name.to_string()));
                }
            }
            if ui.button("none").clicked()
            {
                set_color = Some(None);
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Categories");
            for category in &meta.categories
            {
                if ui
                    .button(format!("{category} âœ•"))
                    .on_hover_text("Remove this category")
                    .clicked()
                {
                    toggle_category = Some(category.clone());
                }
            }
            if meta.categories.is_empty()
            {
                ui.weak("none yet");
            }
        });
        ui.horizontal(|ui| {
            if let Some(info) = self.mod_info.as_mut()
            {
                ui.add(
                    egui::TextEdit::singleline(&mut info.new_category)
                        .hint_text("new category")
                        .desired_width(CATEGORY_INPUT_WIDTH),
                );
            }
            if ui.button("Add").clicked()
            {
                add_category = true;
            }
        });

        ui.separator();
        ui.label("Note");
        if let Some(info) = self.mod_info.as_mut()
        {
            ui.add(
                egui::TextEdit::multiline(&mut info.note_edit)
                    .desired_rows(NOTE_EDITOR_ROWS)
                    .desired_width(f32::INFINITY),
            );
        }
        if ui.button("Save note").clicked()
        {
            save_note = true;
        }

        if let Some(color) = set_color
        {
            self.set_mod_color(id, color);
        }
        if let Some(category) = toggle_category
        {
            self.toggle_mod_category(ModCategoryToggle { mod_id: id, category: &category });
        }
        if add_category
        {
            let category = self
                .mod_info
                .as_ref()
                .map(|info| info.new_category.trim().to_string())
                .unwrap_or_default();
            if !category.is_empty()
            {
                self.toggle_mod_category(ModCategoryToggle { mod_id: id, category: &category });
                if let Some(info) = self.mod_info.as_mut()
                {
                    info.new_category.clear();
                }
            }
        }
        if save_note
        {
            let note = self
                .mod_info
                .as_ref()
                .map(|info| info.note_edit.clone())
                .unwrap_or_default();
            self.set_mod_note(id, note);
        }
    }

    pub(super) fn import_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.heading("Import Mod");
        ui.label("Review first, then import. Medium-confidence readme matches stay visible for human judgment.");
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Package or folder");
            let package_editor = egui::TextEdit::singleline(&mut self.inputs.import_path);
            let package_response =
                ui.add_sized([IMPORT_PATH_FIELD_WIDTH, ROW_HEIGHT], package_editor);
            if package_response.changed()
            {
                // Editing the path invalidates proposals from the last review, so
                // "Add to config" can never target a different package.
                self.clear_readme_review();
            }
            if ui
                .button("Fileâ€¦")
                .on_hover_text("Pick a .zip / .wrap / .7z / .rar package")
                .clicked()
            {
                self.browse_package_file();
            }
            if ui
                .button("Folderâ€¦")
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
        ui.label("Supported: folders, .zip, .wrap, .7z, .rar.");
        if self.analysis_summary.is_none()
        {
            ui.label("Use Review package to inspect files, readmes, and proposed install roots before committing it to the library.");
        }
        self.readme_proposals_panel(ui);
    }
}

impl SanAndreasModUi
{
    fn readme_proposals_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.separator();
        ui.heading("Readme Review");
        if let Some(summary) = &self.analysis_summary
        {
            ui.label(summary);
        }
        if self.readme_proposals.is_empty()
        {
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
        if !imported
        {
            ui.label(
                "Import this package to the library to enable applying a proposal to its config.",
            );
        }
        let proposals = self.readme_proposals.to_vec();
        let mut accepted = None;
        egui::ScrollArea::vertical()
            .max_height(DETAILS_PANEL_MAX_HEIGHT)
            .show(ui, |ui| {
                for proposal in &proposals
                {
                    let import_state = if imported {
                        ReadmeImportState::Imported
                    } else {
                        ReadmeImportState::NotImported
                    };
                    if should_show_readme_proposal_panel(ui, proposal, import_state)
                    {
                        accepted = Some(proposal.clone());
                    }
                    ui.add_space(CARD_VERTICAL_GAP);
                }
            });
        if let Some(proposal) = accepted
        {
            self.accept_readme_proposal(&proposal);
        }
    }
}
