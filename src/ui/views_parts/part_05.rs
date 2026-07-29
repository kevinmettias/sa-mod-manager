
impl SanAndreasModUi
{
    pub(super) fn mods_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.heading("Library");
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, SummaryTile { label: "Imported mods", value: &self.state.mods.len().to_string() });
            summary_tile(ui, SummaryTile { label: "Selected", value: &self.state.mods.iter().filter(|item| item.in_selected_profile).count().to_string() });
            summary_tile(ui, SummaryTile { label: "Install roots", value: &self.state.mods.iter().map(|item| item.config.install_roots.len()).sum::<usize>().to_string() });
        });        ui.label(
            "Review imported packages, edit install roots, and select mods for the active profile.",
        );
        ui.separator();
        if self.state.mods.is_empty()
        {
            ui.label("No imported mods.");
            if ui.button("Import a mod").clicked()
            {
                self.tab = UiTab::Import;
            }
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for item in self.state.mods.to_vec()
            {
                self.mod_config_card(ui, &item);
                ui.add_space(CARD_VERTICAL_GAP);
            }
        });
    }
}

impl SanAndreasModUi
{
    pub(super) fn mod_config_card(&mut self, ui: &mut egui::Ui, item: &ModConfigItem)
    {
        ui.group(|ui| {
            self.mod_config_card_header(ui, item);
            let path_display = item.path.display().to_string();
            ui.monospace(format!("config: {path_display}"));
            ui.monospace(format!("package: {}", item.config.package.display()));
            if let Some(source_root) = &item.config.source_root
            {
                ui.monospace(format!("library files: {}", source_root.display()));
            }
            ui.separator();
            ui.strong("Install locations");
            if item.config.install_roots.is_empty()
            {
                ui.label("No install roots were detected. Review this package before running it.");
            }
            for (idx, root) in item.config.install_roots.iter().enumerate()
            {
                self.mod_install_root_editor(ui, item, idx, root);
            }
        });
    }
}

impl SanAndreasModUi
{
    fn mod_install_root_editor(
        &mut self,
        ui: &mut egui::Ui,
        item: &ModConfigItem,
        root_index: usize,
        root: &ModInstallRootJson,
    )
    {
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
                    [WIDE_FIELD_WIDTH, ROW_HEIGHT],
                    egui::TextEdit::singleline(&mut edit.source),
                );
                ui.label("target");
                ui.add_sized(
                    [WIDE_FIELD_WIDTH, ROW_HEIGHT],
                    egui::TextEdit::singleline(&mut edit.target),
                );
            });
            ui.horizontal(|ui| {
                ui.label("kind");
                install_kind_combo(ui, &key, &mut edit.kind);
                if ui.button("Save root").clicked()
                {
                    save_root = Some(edit.clone());
                }
                if ui.button("Reset").clicked()
                {
                    *edit = root.clone();
                }
            });
        });
        if let Some(updated) = save_root
        {
            // Keep the in-progress edit on failure so a rejected source/target can
            // be corrected instead of silently reverting.
            if self.save_mod_install_root(&item.path, root_index, updated)
            {
                self.mod_root_edits.remove(&key);
            }
        }
    }
}

fn mod_root_edit_key(path: &Path, root_index: usize) -> String
{
    return format!("{}#{root_index}", path.display());
}

/// The preset color-label palette for mod annotations (name → swatch), mirroring
/// MO2's color labels. Names are what get persisted.
const MOD_COLORS: [(&str, egui::Color32); 7] = [
    ("red", egui::Color32::from_rgb(210, 80, 80)),
    (
        "orange",
        egui::Color32::from_rgb(ASI_BADGE_RED, ASI_BADGE_GREEN, ASI_BADGE_BLUE),
    ),
    ("yellow", egui::Color32::from_rgb(210, 190, 80)),
    (
        "green",
        egui::Color32::from_rgb(
            RUNNING_STATUS_RED,
            RUNNING_STATUS_GREEN,
            RUNNING_STATUS_BLUE,
        ),
    ),
    (
        "blue",
        egui::Color32::from_rgb(
            MODLOADER_BADGE_RED,
            MODLOADER_BADGE_GREEN,
            MODLOADER_BADGE_BLUE,
        ),
    ),
    ("purple", egui::Color32::from_rgb(170, 120, 210)),
    ("gray", egui::Color32::from_rgb(150, 150, 150)),
];

/// Resolve a stored color-label name to its swatch color.
fn meta_color(name: &str) -> Option<egui::Color32>
{
    return MOD_COLORS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, color)| *color);
}

/// An action a separator header row can request.
#[derive(Clone, Copy)]
enum SepAction
{
    ToggleCollapse,
    StartEdit,
    Commit,
    Remove,
    MoveUp,
    MoveDown,
}

/// Render one separator (group divider) row: collapse toggle, name or rename
/// field, and move/remove controls. Returns the action the user requested.
struct SeparatorHeaderState<'a>
{
    collapsed: bool,
    editing: bool,
    buf: &'a mut String,
}
fn mod_row_drop_release(
    ui: &egui::Ui,
    row: &egui::Response,
    row_zone: &egui::Response,
    idx: usize,
) -> Option<(usize, usize)>
{
    let payload = row_zone.dnd_release_payload::<usize>()?;
    let center_y = row.rect.center().y;
    let pointer_y = ui
        .input(|input| input.pointer.interact_pos().map(|pos| pos.y))
        .unwrap_or(center_y);
    let drop_index = if pointer_y < center_y { idx } else { idx + 1 };
    return Some((*payload, drop_index));
}
fn draw_mod_row_drop_hover(ui: &egui::Ui, row: &egui::Response, row_zone: &egui::Response)
{
    if row_zone.dnd_hover_payload::<usize>().is_none()
    {
        return;
    }
    let center_y = row.rect.center().y;
    let pointer_y = ui
        .input(|input| input.pointer.interact_pos().map(|pos| pos.y))
        .unwrap_or(center_y);
    let line_y = if pointer_y < center_y {
        row.rect.top()
    } else {
        row.rect.bottom()
    };
    let selection_stroke =
        egui::Stroke::new(DROP_TARGET_STROKE_WIDTH, ui.visuals().selection.bg_fill);
    ui.painter()
        .hline(row.rect.x_range(), line_y, selection_stroke);
}
enum SeparatorEditState
{
    Editing,
    Idle,
}

impl SeparatorEditState
{
    fn from_editing(editing: bool) -> Self
    {
        return if editing
        {
            Self::Editing
        }
        else
        {
            Self::Idle
        };
    }
}

fn update_separator_edit_buffer(state: SeparatorEditState, current: Option<&mut (String, String)>, buf: &str)
{
    if matches!(state, SeparatorEditState::Idle)
    {
        return;
    }
    let Some((_, name)) = current else {
        return;
    };
    *name = buf.to_string();
}
fn separator_header(
    ui: &mut egui::Ui,
    separator: &Separator,
    state: SeparatorHeaderState<'_>,
) -> Option<SepAction>
{
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(if state.collapsed { "▶" } else { "▼" })
            .on_hover_text("Collapse or expand this section")
            .clicked()
        {
            action = Some(SepAction::ToggleCollapse);
        }
        if state.editing
        {
            let response =
                ui.add(egui::TextEdit::singleline(state.buf).desired_width(SEPARATOR_EDIT_WIDTH));
            let committed_with_enter =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let save_clicked = ui.small_button("✔").on_hover_text("Save name").clicked();
            if save_clicked || committed_with_enter
            {
                action = Some(SepAction::Commit);
            }
        }
        else
        {
            ui.strong(format!("═══  {}  ═══", separator.name));
            if ui.small_button("✎").on_hover_text("Rename").clicked()
            {
                action = Some(SepAction::StartEdit);
            }
        }
        if ui
            .small_button("▲")
            .on_hover_text("Move divider up")
            .clicked()
        {
            action = Some(SepAction::MoveUp);
        }
        if ui
            .small_button("▼")
            .on_hover_text("Move divider down")
            .clicked()
        {
            action = Some(SepAction::MoveDown);
        }
        if ui
            .small_button("✕")
            .on_hover_text("Remove divider")
            .clicked()
        {
            action = Some(SepAction::Remove);
        }
    });
    return action;
}

/// The set of mod display indices hidden inside a collapsed separator section
/// (from the separator's slot up to the next separator, or the end).
fn collapsed_hidden_indices(
    separators: &[Separator],
    collapsed: &std::collections::BTreeSet<String>,
    count: usize,
) -> std::collections::BTreeSet<usize>
{
    let mut positions: Vec<usize> = separators.iter().map(|sep| sep.position).collect();
    positions.sort_unstable();
    let mut hidden = std::collections::BTreeSet::new();
    for separator in separators
    {
        if collapsed.contains(&separator.id)
        {
            let start = separator.position.min(count);
            let end = positions
                .iter()
                .copied()
                .find(|&position| position > separator.position)
                .unwrap_or(count)
                .min(count);
            for index in start..end
            {
                hidden.insert(index);
            }
        }
    }
    return hidden;
}

/// A fixed-width, vertically-centred table cell, so the mod list's columns line
/// up row-to-row instead of drifting with each row's content.
fn table_cell(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui))
{
    let cell_size = egui::vec2(width, ROW_HEIGHT);
    ui.allocate_ui_with_layout(
        cell_size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| add(ui),
    );
}

/// Return a copy of `ids` with the entry at `from` moved so it lands at index
/// `to`. Out-of-range indices are clamped, so callers can pass a raw drop slot
/// or a typed priority without extra bounds checks.
fn move_in_list(ids: &[String], from: usize, to: usize) -> Vec<String>
{
    let mut ids = ids.to_vec();
    if from >= ids.len()
    {
        return ids;
    }
    let item = ids.remove(from);
    let to = to.min(ids.len());
    ids.insert(to, item);
    return ids;
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

fn install_kind_combo(ui: &mut egui::Ui, id_source: &str, kind: &mut String)
{
    egui::ComboBox::from_id_salt(format!("kind_{id_source}"))
        .selected_text(kind.clone())
        .width(PROFILE_COMBO_WIDTH)
        .show_ui(ui, |ui| {
            for option in INSTALL_KIND_OPTIONS
            {
                ui.selectable_value(kind, option.to_string(), option);
            }
        });
}

impl SanAndreasModUi
{
    pub(super) fn mod_config_card_header(&mut self, ui: &mut egui::Ui, item: &ModConfigItem)
    {
        ui.horizontal(|ui| {
            ui.heading(&item.config.id);
            ui.separator();
            ui.label(format!(
                "{} install locations",
                item.config.install_roots.len()
            ));
            ui.label(if item.in_selected_profile {
                "selected"
            } else {
                "not selected"
            });
            if should_add_mod_to_profile(ui, item)
            {
                self.add_mod_to_profile(&item.config.id);
            }
        });
    }
}




