
impl SanAndreasModUi
{
    /// The SA-native "Data tab": what the selected profile actually materializes,
    /// grouped by category (ModLoader / CLEO / ASI / direct resources) with the
    /// load-order winner for every file and the mods it overwrites.
    pub(super) fn content_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.heading("Content");
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
        if rescan
        {
            self.rescan_content();
        }

        // Take the cached index out so the filter controls can mutate `self`
        // freely while the (borrowed) index is rendered; it is restored after.
        let Some(index) = self.content.index.take() else {
            ui.add_space(DETAIL_ROW_GAP);
            ui.weak(
                "No content scanned yet. Click Scan / refresh to analyze the profile's enabled mods.",
            );
            return;
        };
        self.content_body(ui, &index);
        self.content.index = Some(index);
    }

    fn content_body(&mut self, ui: &mut egui::Ui, index: &ContentIndex)
    {
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, SummaryTile { label: "Files", value: &index.entries.len().to_string() });
            summary_tile(ui, SummaryTile { label: "Conflicts", value: &index.conflict_count().to_string() });
            summary_tile(ui, SummaryTile { label: "Not indexed", value: &index.not_indexed.len().to_string() });
        });        if !index.not_indexed.is_empty()
        {
            ui.weak(format!(
                "Not indexed (import to the library to include): {}",
                index.not_indexed.join(", ")
            ));
        }
        ui.separator();

        // Category chips (only those with content) plus an "All" reset.
        ui.horizontal_wrapped(|ui| {
            let all_clicked = ui
                .selectable_label(
                    self.content.category.is_none(),
                    format!("All ({})", index.entries.len()),
                )
                .clicked();
            if all_clicked
            {
                self.content.category = None;
            }
            for category in ContentCategory::all()
            {
                let count = index.category_count(category);
                if count == 0
                {
                    continue;
                }
                let selected = self.content.category == Some(category);
                let category_clicked = ui
                    .selectable_label(selected, format!("{} ({count})", category.label()))
                    .clicked();
                if category_clicked
                {
                    self.content.category = Some(category);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.content.conflicts_only, "Conflicts only")
                .on_hover_text("Show only files written by more than one enabled mod");
            ui.separator();
            ui.label("Filter");
            ui.add_sized(
                [WIDE_FIELD_WIDTH, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.content.search)
                    .hint_text("target path or mod id"),
            );
        });
        ui.separator();

        let needle = self.content.search.trim().to_ascii_lowercase();
        let selected_category = self.content.category;
        let conflicts_only = self.content.conflicts_only;
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

        if rows.is_empty()
        {
            ui.label("No files match the current filter.");
            return;
        }
        ui.label(format!("{} files shown", rows.len()));
        if let Some(category) = selected_category
        {
            ui.weak(category.description());
        }
        ui.add_space(UI_SMALL_GAP);

        let conflict_color = ui.visuals().warn_fg_color;
        // Cloned out so the tailored viewers can borrow them inside the closure.
        let priorities = self.modloader_priorities.to_owned();
        let modloader_log = self.modloader_log.to_owned();
        let cleo_diagnostics = self.cleo_diagnostics.to_owned();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match selected_category
            {
                // A specific category becomes its own tailored viewer: ModLoader
                // grouped by its sandboxed mod folders, others by winning mod.
                Some(category) => content_group_view(
                    ui,
                    category,
                    &rows,
                    ContentViewContext {
                        conflict_color,
                        priorities: priorities.as_ref(),
                        modloader_log: modloader_log.as_ref(),
                        cleo_diagnostics: cleo_diagnostics.as_ref(),
                    },
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
)
{
    const ROW_CAP: usize = 3000;
    egui::Grid::new(id.to_string())
        .striped(true)
        .num_columns(CONTENT_GRID_COLUMNS)
        .min_col_width(CONTENT_GRID_MIN_COL_WIDTH)
        .show(ui, |ui| {
            ui.strong("Category");
            ui.strong("Target file");
            ui.strong("Winner");
            ui.strong("Overwrites");
            ui.end_row();
            for entry in rows.iter().take(ROW_CAP)
            {
                ui.label(entry.category.label());
                ui.monospace(&entry.target);
                if entry.is_conflict()
                {
                    ui.colored_label(conflict_color, entry.winner())
                        .on_hover_text("Wins this file after load order is applied");
                }
                else
                {
                    ui.label(entry.winner());
                }
                let overwritten = &entry.providers[..entry.providers.len() - 1];
                if overwritten.is_empty()
                {
                    ui.weak("â€”");
                }
                else
                {
                    ui.label(overwritten.join(", ")).on_hover_text(
                        "These mods' copies of this file are overwritten by the winner",
                    );
                }
                ui.end_row();
            }
        });
    if rows.len() > ROW_CAP
    {
        ui.weak(format!("{} more files not shown", rows.len() - ROW_CAP));
    }
}

/// Route a category to its tailored viewer. CLEO, ASI, and ModLoader get bespoke
/// layouts (scripts+companions; plugins vs loaders; folders by ModLoader
/// priority); everything else groups into collapsing sections by winning mod.
#[derive(Clone, Copy)]
struct ContentViewContext<'a>
{
    conflict_color: egui::Color32,
    priorities: Option<&'a ModLoaderPriorities>,
    modloader_log: Option<&'a ModLoaderLogSummary>,
    cleo_diagnostics: Option<&'a CleoDiagnostics>,
}

#[derive(Clone, Copy)]
struct ModLoaderViewContext<'a>
{
    conflict_color: egui::Color32,
    priorities: Option<&'a ModLoaderPriorities>,
    modloader_log: Option<&'a ModLoaderLogSummary>,
}
fn content_group_view(
    ui: &mut egui::Ui,
    category: ContentCategory,
    rows: &[&ContentEntry],
    context: ContentViewContext<'_>,
)
{
    match category
    {
        ContentCategory::Cleo => {
            content_cleo_view(ui, rows, context.conflict_color, context.cleo_diagnostics)
    }
        ContentCategory::Asi => content_asi_view(ui, rows, context.conflict_color),
        ContentCategory::ModLoader => content_modloader_view(
            ui,
            rows,
            ModLoaderViewContext {
                conflict_color: context.conflict_color,
                priorities: context.priorities,
                modloader_log: context.modloader_log,
            },
        ),
        _ => content_grouped(ui, category, rows, context.conflict_color),
    }
}

/// A compact "what ModLoader actually did last run" panel from `modloader.log`:
/// the version banner, and any lines that read as errors/warnings. Best-effort â€”
/// the log is free text â€” so it is framed as a reality check, not authoritative.
fn content_modloader_log_summary(
    ui: &mut egui::Ui,
    log: &ModLoaderLogSummary,
    conflict_color: egui::Color32,
)
{
    let version = log
        .version
        .as_deref()
        .map(|version| format!("Mod Loader {version}"))
        .unwrap_or_else(|| "Mod Loader".to_string());
    if log.is_clean()
    {
        ui.weak(format!(
            "Last run: {version} â€” loaded cleanly (no warnings or errors)."
        ));
        ui.add_space(UI_SMALL_GAP);
        return;
    }
    let header = format!(
        "Last run: {version} â€” {} errors, {} warnings{}",
        log.errors.len(),
        log.warnings.len(),
        if log.truncated { " (truncated)" } else { "" }
    );
    egui::CollapsingHeader::new(header)
        .id_salt("modloader_log_summary")
        .default_open(!log.errors.is_empty())
        .show(ui, |ui| {
            ui.weak(
                "Scraped from modloader/modloader.log â€” what ModLoader reported on the last \
                 launch. Free-text, so treat as a hint, not a guarantee.",
            );
            for line in &log.errors
            {
                ui.colored_label(conflict_color, line);
            }
            for line in &log.warnings
            {
                ui.label(line);
            }
        });
    ui.add_space(UI_SMALL_GAP);
}
