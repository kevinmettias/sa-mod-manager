use crate::prelude::*;
use eframe::egui;

use super::state::{InfrastructureItem, ModConfigItem, UiState};

pub(super) fn infrastructure_grid(ui: &mut egui::Ui, infrastructure: &[InfrastructureItem])
{
    egui::Grid::new("infra_grid").striped(true).show(ui, |ui| {
        for item in infrastructure
        {
            infrastructure_row(ui, item);
        }
    });
}

pub(super) fn infrastructure_row(ui: &mut egui::Ui, item: &InfrastructureItem)
{
    ui.label(&item.label);
    ui.label(if item.present { "present" } else { "missing" });
    let path_display = item.path.display().to_string();
    ui.monospace(path_display);
    ui.end_row();
}

pub(super) fn selected_profile_entries(
    state: &UiState,
    selected_profile: &str,
) -> Vec<ProfileModEntry>
{
    let Some(profile) = &state.selected_profile else {
        return Vec::new();
    };
    if profile.name != selected_profile
    {
        return Vec::new();
    }
    let mut entries = profile.mods.clone();
    entries.sort_by(|a, b| {
        a.load_order
            .cmp(&b.load_order)
            .then_with(|| a.id.cmp(&b.id))
    });
    return entries;
}

pub(super) fn should_add_mod_to_profile(ui: &mut egui::Ui, item: &ModConfigItem) -> bool
{
    if item.in_selected_profile
    {
        return false;
    }
    return ui.button("Select").clicked();
}
