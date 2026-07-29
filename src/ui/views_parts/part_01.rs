use crate::prelude::*;
use crate::ui::actions::{ContentFocus, ModCategoryToggle, ProfileRootTargetEdit, ProfileRootToggle};
use crate::workspace::ProfileRootEnabledState;
use eframe::egui;

use super::san_andreas_mod_ui::{
    ModDetailsTab, ModStatusFilter, ModTelemetry, PendingRunStatus, ROW_HEIGHT, ReadmeProposal,
    ReadmeProposalState, SanAndreasModUi, TelemetryEvent,
};
use super::state::{ModConfigItem, UiTab};
use super::widgets::{infrastructure_grid, selected_profile_entries, should_add_mod_to_profile};
const UI_TINY_GAP: f32 = 2.0;
const UI_SMALL_GAP: f32 = 4.0;
const PROFILE_COMBO_WIDTH: f32 = 160.0;
const NEW_PROFILE_FIELD_WIDTH: f32 = 110.0;
const RUN_TARGET_COMBO_WIDTH: f32 = 200.0;
const GAME_ROOT_FIELD_WIDTH: f32 = 340.0;
const RUNNING_STATUS_RED: u8 = 120;
const RUNNING_STATUS_GREEN: u8 = 190;
const RUNNING_STATUS_BLUE: u8 = 120;
const DETAILS_PANEL_MAX_HEIGHT: f32 = 360.0;
const DETAIL_ROW_GAP: f32 = 8.0;
const COMPACT_FIELD_WIDTH: f32 = 140.0;
const MEDIUM_FIELD_WIDTH: f32 = 160.0;
const WIDE_FIELD_WIDTH: f32 = 260.0;
const TOOL_PATH_FIELD_WIDTH: f32 = 320.0;
const MOD_ROW_DRAG_SPEED: f64 = 0.1;
const MODLOADER_DEFAULT_PRIORITY: i32 = 50;
const MODLOADER_MIN_PRIORITY: i32 = 0;
const MODLOADER_MAX_PRIORITY: i32 = 100;
const MODLOADER_PRIORITY_STEP: f64 = 0.25;
const MODLOADER_GRID_COLUMNS: usize = 3;
const GRID_MIN_COL_WIDTH: f32 = 64.0;
const OVERWRITE_PANEL_MAX_HEIGHT: f32 = 200.0;
const OVERWRITE_VISIBLE_FILE_LIMIT: usize = 2000;
const PENDING_RUN_GRID_MIN_COL_WIDTH: f32 = 88.0;
const TELEMETRY_FILTER_WIDTH: f32 = 240.0;
const TELEMETRY_GRID_MIN_COL_WIDTH: f32 = 48.0;
const TELEMETRY_VISIBLE_EVENT_LIMIT: usize = 50;
const TELEMETRY_TITLE_CLIP: usize = 40;
const TELEMETRY_DETAIL_CLIP: usize = 72;
const MAX_LOG_PREVIEW_BYTES: usize = 5000;
const MOD_CONFIG_PANEL_WIDTH: f32 = 440.0;
const MOD_CONFIG_PANEL_HEIGHT: f32 = 560.0;
const ERROR_BANNER_RED: u8 = 200;
const ERROR_BANNER_GREEN: u8 = 64;
const ERROR_BANNER_BLUE: u8 = 64;
const CONFIRM_MODAL_MAX_WIDTH: f32 = 360.0;
const SUMMARY_TILE_MIN_WIDTH: f32 = 136.0;
const WORKFLOW_GRID_MIN_COL_WIDTH: f32 = 120.0;
const LAUNCH_ARGS_FIELD_WIDTH: f32 = 360.0;
const MOD_ROW_NAME_RESERVED_WIDTH: f32 = 160.0;
const MOD_ROW_NAME_MIN_WIDTH: f32 = 140.0;
const MODLOADER_BADGE_RED: u8 = 90;
const MODLOADER_BADGE_GREEN: u8 = 150;
const MODLOADER_BADGE_BLUE: u8 = 220;
const ASI_BADGE_RED: u8 = 220;
const ASI_BADGE_GREEN: u8 = 140;
const ASI_BADGE_BLUE: u8 = 60;
const ROOT_OVERRIDE_TARGET_WIDTH: f32 = 180.0;
const CARD_VERTICAL_GAP: f32 = 6.0;
const DROP_TARGET_STROKE_WIDTH: f32 = 2.0;
const SEPARATOR_EDIT_WIDTH: f32 = 220.0;
const CONTENT_GRID_COLUMNS: usize = 4;
const CONTENT_GRID_MIN_COL_WIDTH: f32 = 90.0;
const DEFAULT_OPEN_GROUP_LIMIT: usize = 8;
const LOG_PREVIEW_GRID_MIN_COL_WIDTH: f32 = 60.0;
const MOD_COLOR_SWATCH_WIDTH: f32 = 22.0;
const MOD_COLOR_SWATCH_HEIGHT: f32 = 18.0;
const IMPORT_PATH_FIELD_WIDTH: f32 = 470.0;
const PERCENT_SCALE: f32 = 100.0;
const CATEGORY_INPUT_WIDTH: f32 = 160.0;
const NOTE_EDITOR_ROWS: usize = 4;
const README_EVIDENCE_CLIP: usize = 100;
impl SanAndreasModUi
{
    /// The top toolbar: profile + run target on the first row (the things you
    /// reach for constantly), game folder + maintenance on the second.
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui)
    {
        ui.add_space(UI_TINY_GAP);
        ui.horizontal_wrapped(|ui| {
            ui.strong("SA Mod Manager");
            ui.separator();

            ui.label("Profile");
            let mut changed_profile = false;
            egui::ComboBox::from_id_salt("toolbar_profile")
                .selected_text(&self.selected_profile)
                .width(PROFILE_COMBO_WIDTH)
                .show_ui(ui, |ui| {
                    for profile in &self.state.profiles
                    {
                        let choice = profile.clone();
                        if ui
                            .selectable_value(&mut self.selected_profile, choice, profile)
                            .changed()
                        {
                            changed_profile = true;
                        }
                    }
                });
            if changed_profile
            {
                self.refresh();
            }
            ui.add_sized(
                [NEW_PROFILE_FIELD_WIDTH, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.inputs.new_profile).hint_text("new profile"),
            );
            if ui.button("New").clicked()
            {
                self.create_profile();
            }

            ui.separator();

            // Run target (MO2's run dropdown), always at hand in the toolbar.
            let selected_label = if self.selected_run_target == 0 {
                "â–¶ Play current profile".to_string()
            } else {
                self.executables
                    .get(self.selected_run_target - 1)
                    .map(|tool| tool.name.clone())
                    .unwrap_or_else(|| "â–¶ Play current profile".to_string())
            };
            let tool_names: Vec<String> = self
                .executables
                .iter()
                .map(|tool| tool.name.clone())
                .collect();
            egui::ComboBox::from_id_salt("toolbar_run_target")
                .selected_text(selected_label)
                .width(RUN_TARGET_COMBO_WIDTH)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.selected_run_target, 0, "â–¶ Play current profile");
                    for (index, name) in tool_names.iter().enumerate()
                    {
                        ui.selectable_value(&mut self.selected_run_target, index + 1, name);
                    }
                });
            let idle = !self.is_busy();
            let run_label = if self.selected_run_target == 0 {
                "Run â–¶"
            } else {
                "Run tool â–¶"
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
            ui.label("Game");
            ui.add_sized(
                [GAME_ROOT_FIELD_WIDTH, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.inputs.game_root),
            );
            if ui
                .button("Browseâ€¦")
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
            if let Some(summary) = self.pending_cleanup_summary()
            {
                ui.separator();
                let warn = ui.visuals().warn_fg_color;
                ui.colored_label(warn, summary);
                if ui.button("Clean finished").clicked()
                {
                    self.request_cleanup_finished();
                }
            }
        });
    }
}
