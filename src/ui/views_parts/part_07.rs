
/// The ModLoader viewer: one section per sandboxed folder, ordered and labeled by
/// ModLoader's own priority (from modloader.ini) â€” the order ModLoader itself
/// applies them in, which is independent of this manager's profile load order.
fn content_modloader_view(
    ui: &mut egui::Ui,
    rows: &[&ContentEntry],
    context: ModLoaderViewContext<'_>,
)
{
    if let Some(log) = context.modloader_log
    {
        content_modloader_log_summary(ui, log, context.conflict_color);
    }
    match context.priorities
    {
        Some(_) => {
            ui.weak(
                "ModLoader applies these folders by its own priority (higher = later = wins), \
                 from modloader/modloader.ini â€” separate from the profile load order.",
            );
    }
        None => {
            ui.weak(
                "modloader.ini not found, so ModLoader's per-folder priority is unknown â€” it will \
                 use its default. Run ModLoader once to generate it.",
            );
}
    }

    // Cross-folder conflicts: two sandbox folders that both provide the same
    // underlying asset never collide on disk, so they are invisible in the plain
    // file tables below â€” ModLoader resolves them by priority at runtime. Surface
    // them explicitly, since this is the conflict class the viewer exists to show.
    let effective_priorities = context.priorities.cloned().unwrap_or_default();
    let conflicts = modloader_conflicts(rows, &effective_priorities);
    if !conflicts.is_empty()
    {
        // Mergeable data files (handling.cfg, *.ideâ€¦) are soft: ModLoader combines
        // them entry-by-entry, so they collide only on overlapping entries. The
        // rest are hard: the highest-priority folder wins the whole file.
        let override_count = conflicts.iter().filter(|c| !c.mergeable).count();
        let merge_count = conflicts.len() - override_count;
        let ambiguous = conflicts
            .iter()
            .filter(|c| c.ambiguous && !c.mergeable)
            .count();
        let mut parts = Vec::new();
        if override_count > 0
        {
            parts.push(format!("{override_count} override"));
        }
        if merge_count > 0
        {
            parts.push(format!("{merge_count} mergeable"));
        }
        if ambiguous > 0
        {
            parts.push(format!("{ambiguous} tied"));
        }
        let header = format!("Cross-folder conflicts â€” {}", parts.join(", "));
        egui::CollapsingHeader::new(header)
            .id_salt("modloader_virtual_conflicts")
            .default_open(override_count > 0)
            .show(ui, |ui| {
                ui.weak(
                    "Same file provided by more than one folder. Override: the highest-priority \
                     folder wins the whole file. Mergeable: ModLoader combines entries, so folders \
                     clash only where they edit the same entry. Tied priorities are \
                     non-deterministic â€” give them distinct priorities to pin the winner.",
                );
                egui::Grid::new("modloader_conflict_grid")
                    .striped(true)
                    .num_columns(CONTENT_GRID_COLUMNS)
                    .show(ui, |ui| {
                        ui.strong("Asset");
                        ui.strong("Kind");
                        ui.strong("Result");
                        ui.strong("Other folders");
                        ui.end_row();
                        for conflict in &conflicts
                        {
                            ui.label(&conflict.asset);
                            let winner = conflict.winner();
                            let winner_priority = conflict
                                .contenders
                                .first()
                                .map(|c| c.priority)
                                .unwrap_or_default();
                            if conflict.mergeable
                            {
                                ui.weak("merge").on_hover_text(
                                    "ModLoader merges this file entry-by-entry; only entries edited \
                                     by more than one folder actually conflict.",
                                );
                                ui.label("combined").on_hover_text(
                                    "All folders' entries are kept; overlapping entries resolve by \
                                     priority.",
                                );
                            }
                            else if conflict.ambiguous
                            {
                                ui.colored_label(context.conflict_color, "override");
                                ui.colored_label(
                                    context.conflict_color,
                                    format!("{winner} (priority {winner_priority}, tied)"),
                                )
                                .on_hover_text(
                                    "Two folders share the top priority; ModLoader's winner here \
                                     is not guaranteed.",
                                );
                            }
                            else
                            {
                                ui.label("override");
                                ui.label(format!("{winner} wins (priority {winner_priority})"));
                            }
                            let others: Vec<String> = conflict.contenders[1..]
                                .iter()
                                .map(|c| format!("{} ({})", c.folder, c.priority))
                                .collect();
                            if others.is_empty()
                            {
                                ui.weak("â€”");
                            }
                            else
                            {
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
                let priority = context.priorities.map(|table| table.for_folder(&folder));
                (priority, folder, folder_rows)
            })
            .collect();
    groups.sort_by(|a, b| {
        a.0.unwrap_or(MODLOADER_DEFAULT_PRIORITY)
            .cmp(&b.0.unwrap_or(MODLOADER_DEFAULT_PRIORITY))
            .then_with(|| a.1.cmp(&b.1))
    });

    let default_open = groups.len() <= DEFAULT_OPEN_GROUP_LIMIT;
    for (priority, folder, folder_rows) in groups
    {
        let conflicts = folder_rows
            .iter()
            .filter(|entry| entry.is_conflict())
            .count();
        let priority_label = match priority
        {
            Some(value) => format!("priority {value}"),
            None => "priority â€”".to_string(),
        };
        let header = if conflicts > 0
        {
            format!(
                "{folder} â€” {priority_label} â€” {} files, {conflicts} conflicts",
                folder_rows.len()
            )
        }
        else
        {
            format!("{folder} â€” {priority_label} â€” {} files", folder_rows.len())
        };
        egui::CollapsingHeader::new(header)
            .id_salt(format!("modloader_folder_{folder}"))
            .default_open(default_open)
            .show(ui, |ui| {
                content_table(
                    ui,
                    &format!("modloader_grid_{folder}"),
                    &folder_rows,
                    context.conflict_color,
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
)
{
    let groups = group_entries(category, rows);
    // Open every section when there are only a handful, so small profiles read at
    // a glance; collapse by default once the list would get long.
    let default_open = groups.len() <= DEFAULT_OPEN_GROUP_LIMIT;
    for (label, group_rows) in groups
    {
        let conflicts = group_rows
            .iter()
            .filter(|entry| entry.is_conflict())
            .count();
        let header = if conflicts > 0
        {
            format!(
                "{label} â€” {} files, {conflicts} conflicts",
                group_rows.len()
            )
        }
        else
        {
            format!("{label} â€” {} files", group_rows.len())
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

/// The CLEO viewer: an installed-folder health panel (blacklisted plugins, text
/// key conflicts, per-script missing-plugin/capability issues), then each script
/// listed with its `.ini`/`.fxt`/data companions, then loose files.
fn content_cleo_view(
    ui: &mut egui::Ui,
    rows: &[&ContentEntry],
    conflict_color: egui::Color32,
    diagnostics: Option<&CleoDiagnostics>,
)
{
    if let Some(diagnostics) = diagnostics
    {
        content_cleo_diagnostics(ui, diagnostics, conflict_color);
    }
    let view = cleo_view(rows);
    if !view.plugins.is_empty()
    {
        ui.strong(format!("{} plugin modules (.cleo)", view.plugins.len()));
        ui.weak("CLEO5 loads these from CLEO/cleo_plugins. Scripts may require a matching plugin (e.g. SA.IniFiles).");
        content_table(ui, "cleo_plugins_grid", &view.plugins, conflict_color);
        ui.add_space(CARD_VERTICAL_GAP);
    }
    ui.strong(format!("{} scripts", view.scripts.len()));
    egui::Grid::new("cleo_scripts_grid")
        .striped(true)
        .num_columns(CONTENT_GRID_COLUMNS)
        .min_col_width(CONTENT_GRID_MIN_COL_WIDTH)
        .show(ui, |ui| {
            ui.strong("Script");
            ui.strong("Winner");
            ui.strong("Companions");
            ui.strong("Overwrites");
            ui.end_row();
            for script in &view.scripts
            {
                ui.monospace(&script.script.target);
                if script.script.is_conflict()
                {
                    ui.colored_label(conflict_color, script.script.winner())
                        .on_hover_text("Another mod ships a script with the same file name");
                }
                else
                {
                    ui.label(script.script.winner());
                }
                if script.companions.is_empty()
                {
                    ui.weak("â€”");
                }
                else
                {
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
                if overwritten.is_empty()
                {
                    ui.weak("â€”");
                }
                else
                {
                    ui.label(overwritten.join(", "));
                }
                ui.end_row();
            }
        });
    if !view.loose.is_empty()
    {
        ui.add_space(CARD_VERTICAL_GAP);
        ui.strong(format!(
            "{} loose files (no matching script)",
            view.loose.len()
        ));
        content_table(ui, "cleo_loose_grid", &view.loose, conflict_color);
    }
}

/// The installed-CLEO health panel shown atop the CLEO viewer â€” what the actual
/// game folder's CLEO setup will do (from the last content scan), as opposed to
/// the profile plan the rest of the viewer shows. Hidden when there is nothing to
/// report.
fn content_cleo_diagnostics(
    ui: &mut egui::Ui,
    diagnostics: &CleoDiagnostics,
    conflict_color: egui::Color32,
)
{
    if diagnostics.is_empty()
    {
        return;
    }
    egui::CollapsingHeader::new("CLEO health (installed game folder)")
        .id_salt("cleo_health")
        .default_open(true)
        .show(ui, |ui| {
            if !diagnostics.blacklisted_plugins.is_empty()
            {
                ui.colored_label(
                    conflict_color,
                    format!(
                        "{} blacklisted plugin(s) â€” legacy, superseded by SA.*, will not load:",
                        diagnostics.blacklisted_plugins.len()
                    ),
                );
                for name in &diagnostics.blacklisted_plugins
                {
                    ui.monospace(format!("    {name}"));
                }
            }
            if !diagnostics.fxt_conflicts.is_empty()
            {
                ui.colored_label(
                    conflict_color,
                    format!(
                        "{} text key conflict(s) â€” same GXT key in multiple .fxt (last wins):",
                        diagnostics.fxt_conflicts.len()
                    ),
                );
                for conflict in &diagnostics.fxt_conflicts
                {
                    ui.label(format!(
                        "    {}: {}",
                        conflict.key,
                        conflict.files.join(", ")
                    ));
                }
            }
            if !diagnostics.script_issues.is_empty()
            {
                ui.strong(format!(
                    "{} script(s) with dependencies / elevated access:",
                    diagnostics.script_issues.len()
                ));
                for issue in &diagnostics.script_issues
                {
                    let mut parts = Vec::new();
                    if !issue.missing_plugins.is_empty()
                    {
                        parts.push(format!("MISSING {}", issue.missing_plugins.join(", ")));
                    }
                    if !issue.capabilities.is_empty()
                    {
                        parts.push(format!("elevated: {}", issue.capabilities.join(", ")));
                    }
                    let text = format!("    {}: {}", issue.script, parts.join("; "));
                    if issue.missing_plugins.is_empty()
                    {
                        ui.label(text);
                    }
                    else
                    {
                        ui.colored_label(conflict_color, text);
                    }
                }
            }
        });
    ui.add_space(CARD_VERTICAL_GAP);
}

/// The last path segment of a forward-slash target (its file name).
fn base_name(target: &str) -> &str
{
    return target.rsplit('/').next().unwrap_or(target);
}

/// The ASI viewer: plugins, then the loader/proxy DLLs that boot them, then any
/// other files â€” so a clashing loader is obvious versus a duplicate plugin.
fn content_asi_view(ui: &mut egui::Ui, rows: &[&ContentEntry], conflict_color: egui::Color32)
{
    let view = asi_view(rows);
    ui.strong(format!("{} ASI plugins", view.plugins.len()));
    if view.plugins.is_empty()
    {
        ui.weak("No .asi plugins in this profile.");
    }
    else
    {
        content_table(ui, "asi_plugins_grid", &view.plugins, conflict_color);
    }
    if !view.loaders.is_empty()
    {
        ui.add_space(CARD_VERTICAL_GAP);
        ui.strong(format!("{} loader / proxy DLLs", view.loaders.len()));
        ui.weak("These hook the game to load ASI plugins (e.g. Ultimate ASI Loader). Normally only one should win.");
        content_table(ui, "asi_loaders_grid", &view.loaders, conflict_color);
    }
    if !view.other.is_empty()
    {
        ui.add_space(CARD_VERTICAL_GAP);
        ui.strong(format!("{} other files", view.other.len()));
        content_table(ui, "asi_other_grid", &view.other, conflict_color);
    }
}

/// The ModLoader mod folder a materialized target belongs to: the segment right
/// after a `modloader` segment, when there is a path component beneath it (so a
/// loose file at `modloader/` isn't mistaken for a mod folder). Reserved
/// dot-folders (`.data`, â€¦) yield `None`.
fn modloader_folder_of(target: &str) -> Option<String>
{
    let mut segments = target.split('/');
    while let Some(segment) = segments.next()
    {
        if segment.eq_ignore_ascii_case("modloader")
        {
            let folder = segments.next()?;
            if folder.starts_with('.')
            {
                return None;
            }
            segments.next()?; // require a component beneath the folder
            return Some(folder.to_string());
        }
    }
    return None;
}



