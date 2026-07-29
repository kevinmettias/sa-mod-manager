
impl SanAndreasModUi
{
    pub(super) fn telemetry_panel(&mut self, ui: &mut egui::Ui)
    {
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
                [TELEMETRY_FILTER_WIDTH, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.telemetry_search),
            );
            egui::ComboBox::from_id_salt("filters.telemetry_kind")
                .selected_text(&self.filters.telemetry_kind)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filters.telemetry_kind, "all".to_string(), "all");
                    ui.selectable_value(
                        &mut self.filters.telemetry_kind,
                        "import".to_string(),
                        "imports",
                    );
                    ui.selectable_value(&mut self.filters.telemetry_kind, "run".to_string(), "runs");
                    ui.selectable_value(
                        &mut self.filters.telemetry_kind,
                        "install".to_string(),
                        "installs",
                    );
                });
            if ui.button("Export JSON").clicked()
            {
                self.export_telemetry();
            }
        });
        ui.separator();
        ui.heading("Recent Activity");
        let recent_events = filtered_telemetry_events(TelemetryEventFilter { events: &self.telemetry.recent_events, search: &self.telemetry_search, kind_filter: &self.filters.telemetry_kind });
        if recent_events.is_empty()
        {
            ui.label("No telemetry records yet.");
        }
        else
        {
            egui::Grid::new("telemetry_recent_events")
                .striped(true)
                .min_col_width(TELEMETRY_GRID_MIN_COL_WIDTH)
                .show(ui, |ui| {
                    ui.strong("Type");
                    ui.strong("When");
                    ui.strong("Name");
                    ui.strong("Details");
                    ui.end_row();
                    for event in recent_events
                        .into_iter()
                        .take(TELEMETRY_VISIBLE_EVENT_LIMIT)
                    {
                        ui.label(&event.kind);
                        ui.monospace(human_datetime(event.created_unix))
                            .on_hover_text(format!("unix {}", event.created_unix));
                        let clipped_title = clip_text(&event.title, TELEMETRY_TITLE_CLIP);
                        ui.label(clipped_title).on_hover_text(&event.title);
                        let clipped_detail = clip_text(&event.detail, TELEMETRY_DETAIL_CLIP);
                        ui.label(clipped_detail).on_hover_text(&event.detail);
                        ui.end_row();
                    }
                });
        }
        ui.separator();
        ui.heading("Per-Mod History");
        let mod_rows = filtered_mod_history(&self.telemetry.mod_history, &self.telemetry_search);
        if mod_rows.is_empty()
        {
            ui.label("No per-mod telemetry yet.");
            return;
        }
        egui::Grid::new("telemetry_mod_history")
            .striped(true)
            .min_col_width(TELEMETRY_GRID_MIN_COL_WIDTH)
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
                for row in mod_rows.into_iter().take(TELEMETRY_VISIBLE_EVENT_LIMIT)
                {
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

struct TelemetryEventFilter<'a>
{
    events: &'a [TelemetryEvent],
    search: &'a str,
    kind_filter: &'a str,
}

fn filtered_telemetry_events(filter: TelemetryEventFilter<'_>) -> Vec<TelemetryEvent>
{
    let events = filter.events;
    let search = filter.search;
    let kind_filter = filter.kind_filter;
    let needle = search.trim().to_ascii_lowercase();
    return events
        .iter()
        .filter(|event| kind_filter == "all" || event.kind == kind_filter)
        .filter(|event| {
            needle.is_empty()
                || event.title.to_ascii_lowercase().contains(&needle)
                || event.detail.to_ascii_lowercase().contains(&needle)
                || event.kind.to_ascii_lowercase().contains(&needle)
        })
        .cloned()
        .collect();
}

fn filtered_mod_history(rows: &[ModTelemetry], search: &str) -> Vec<ModTelemetry>
{
    let needle = search.trim().to_ascii_lowercase();
    return rows.iter()
        .filter(|row| needle.is_empty() || row.id.to_ascii_lowercase().contains(&needle))
        .cloned()
        .collect();
}




