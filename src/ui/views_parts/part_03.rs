
impl SanAndreasModUi
{
    /// Show a hint while files hover and consume any dropped onto the window.
    pub(super) fn handle_file_drops(&mut self, ctx: &egui::Context)
    {
        let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
        if hovering
        {
            egui::Area::new(egui::Id::new("drop_hint"))
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.heading("Drop a mod package or GTA folder");
                    });
                });
        }
        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .find_map(|file| file.path.clone())
        });
        if let Some(path) = dropped
        {
            self.handle_dropped_path(path);
        }
    }

    pub(super) fn home_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.heading("Overview");
        ui.label("Your recommended next step and where this profile sits in the workflow.");
        ui.separator();
        // Profile / enabled / cleanup live in the toolbar + status bar now, and
        // the current profile is the always-visible centre list, so Overview is
        // just guidance: next action, workflow stage, and recent signals. Game
        // Setup moved to the left panel so it is visible on every screen.
        self.next_action_panel(ui);
        ui.separator();
        self.workflow_panel(ui);
        ui.separator();
        self.recent_signal_panel(ui);
    }
}

struct SummaryTile<'a>
{
    label: &'a str,
    value: &'a str,
}

fn summary_tile(ui: &mut egui::Ui, tile: SummaryTile<'_>)
{
    let label = tile.label;
    let value = tile.value;
    ui.group(|ui| {
        ui.set_min_width(SUMMARY_TILE_MIN_WIDTH);
        ui.label(label);
        ui.strong(value);
    });
}

/// A dense inline stat (`label value`) with a trailing separator, for packing
/// many counts onto one wrapping line instead of a wall of boxed tiles.
fn stat_inline(ui: &mut egui::Ui, label: &str, value: impl std::fmt::Display)
{
    ui.label(label);
    ui.strong(value.to_string());
    ui.separator();
}

/// Shorten `text` to at most `max` characters with an ellipsis, so long free-text
/// cells cannot blow out a grid's width (full text stays available on hover).
fn clip_text(text: &str, max: usize) -> String
{
    return if text.chars().count() > max
    {
        let mut clipped: String = text.chars().take(max.saturating_sub(1)).collect();
        clipped.push('…');
        clipped
    }
    else
    {
        text.to_string()
    };
}

/// A compact install-status list for the left panel: one ✓/✗ line per detected
/// component (executable, ModLoader, CLEO, ASI), hovering shows the full path.
/// Fits the narrow column, unlike the wide `infrastructure_grid`.
fn game_setup_summary(ui: &mut egui::Ui, infrastructure: &[super::state::InfrastructureItem])
{
    if infrastructure.is_empty()
    {
        ui.weak("Set the game folder to detect components.");
        return;
    }
    let present_color = egui::Color32::from_rgb(
        RUNNING_STATUS_RED,
        RUNNING_STATUS_GREEN,
        RUNNING_STATUS_BLUE,
    );
    let missing_color = ui.visuals().warn_fg_color;
    for item in infrastructure
    {
        let (mark, color) = if item.present {
            ("✓", present_color)
        } else {
            ("✗", missing_color)
        };
        ui.horizontal(|ui| {
            ui.colored_label(color, mark);
            ui.label(&item.label);
        })
        .response
        .on_hover_text(item.path.display().to_string());
    }
}

fn recommended_action(ui_state: &SanAndreasModUi) -> RecommendedAction
{
    if has_cleanable_pending_run(ui_state)
    {
        return RecommendedAction {
            detail: "Finished temporary files are still materialized. Clean them before launching another profile.",
            primary_label: "Clean finished runs",
            primary_tab: None,
            secondary: Some(("Review cleanup", UiTab::Run)),
        };
    }
    if !game_executable_present(ui_state)
    {
        return RecommendedAction {
            detail: "The game executable was not detected in the configured folder.",
            primary_label: "Review setup",
            primary_tab: Some(UiTab::Home),
            secondary: None,
        };
    }
    if ui_state.state.mods.is_empty()
    {
        return RecommendedAction {
            detail: "Import a package, review its readme hints, then add it to a profile.",
            primary_label: "Import a mod",
            primary_tab: Some(UiTab::Import),
            secondary: None,
        };
    }
    if enabled_profile_count(ui_state) == 0
    {
        return RecommendedAction {
            detail: "The selected profile has no enabled mods yet — tick them in the centre list.",
            primary_label: "Open library",
            primary_tab: Some(UiTab::Mods),
            secondary: None,
        };
    }
    return RecommendedAction {
        detail: "This profile is ready for an ephemeral launch.",
        primary_label: "Play profile",
        primary_tab: Some(UiTab::Run),
        secondary: Some(("View telemetry", UiTab::Telemetry)),
    };
}

impl SanAndreasModUi
{
    fn next_action_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.group(|ui| {
            ui.heading("Recommended Next Step");
            let action = recommended_action(self);
            ui.label(action.detail);
            ui.horizontal(|ui| {
                if ui.button(action.primary_label).clicked()
                {
                    match action.primary_tab
                    {
                        Some(tab) => self.tab = tab,
                        None => self.cleanup_stale_pending_runs(),
                    }
                }
                if let Some((label, tab)) = action.secondary
                {
                    if ui.button(label).clicked()
                    {
                        self.tab = tab;
                    }
                }
            });
        });
    }

    fn workflow_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.heading("Workflow");
        // Game setup lives in the always-visible left panel, so it is not repeated
        // here — the workflow starts from importing mods.
        egui::Grid::new("home_workflow")
            .striped(true)
            .min_col_width(WORKFLOW_GRID_MIN_COL_WIDTH)
            .show(ui, |ui| {
                workflow_row(
                    ui,
                    "1. Import",
                    import_status(self),
                    "Review archive contents and readme-driven install proposals.",
                );
                workflow_row(
                    ui,
                    "2. Profile",
                    profile_status(self),
                    "Enable mods and set load order in the centre list.",
                );
                workflow_row(
                    ui,
                    "3. Play",
                    run_status(self),
                    "Launch temporarily, then clean the game folder back to vanilla.",
                );
            });
    }

    fn recent_signal_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.heading("Recent Signals");
        let issues = self.telemetry.missing_sources + self.telemetry.blocked_bootstrap;
        ui.horizontal_wrapped(|ui| {
            summary_tile(ui, SummaryTile { label: "Readme proposals", value: &self.readme_proposals.len().to_string() });
            summary_tile(ui, SummaryTile { label: "Pending cleanup", value: &self.pending_runs.len().to_string() });
            summary_tile(ui, SummaryTile { label: "Overwrites tracked", value: &self.telemetry.overwritten_files.to_string() });
            summary_tile(ui, SummaryTile { label: "Issues tracked", value: &issues.to_string() });
        });
        ui.horizontal(|ui| {
            if ui.button("Review telemetry").clicked()
            {
                self.tab = UiTab::Telemetry;
            }
            if ui.button("Check cleanup").clicked()
            {
                self.tab = UiTab::Run;
            }
        });
    }
}

struct RecommendedAction
{
    detail: &'static str,
    primary_label: &'static str,
    primary_tab: Option<UiTab>,
    secondary: Option<(&'static str, UiTab)>,
}

fn has_cleanable_pending_run(ui_state: &SanAndreasModUi) -> bool
{
    return ui_state
        .pending_runs
        .iter()
        .any(|record| record.status != PendingRunStatus::Running);
}

fn game_executable_present(ui_state: &SanAndreasModUi) -> bool
{
    return ui_state.state.infrastructure.iter().any(|item| {
        item.present && (item.label == "Steam executable" || item.label == "Classic executable")
    });
}

#[derive(Clone, Copy)]
enum WorkflowStatus
{
    Ready,
    Review,
    Waiting,
}

impl WorkflowStatus
{
    fn label(self) -> &'static str
    {
        return match self
        {
            WorkflowStatus::Ready => "ready",
            WorkflowStatus::Review => "needs review",
            WorkflowStatus::Waiting => "waiting",
        };
    }
}

fn workflow_row(ui: &mut egui::Ui, stage: &str, status: WorkflowStatus, detail: &str)
{
    ui.strong(stage);
    ui.label(status.label());
    ui.label(detail);
    ui.end_row();
}

fn import_status(ui_state: &SanAndreasModUi) -> WorkflowStatus
{
    return if !ui_state.readme_proposals.is_empty()
    {
        WorkflowStatus::Review
    }
    else if ui_state.state.mods.is_empty()
    {
        WorkflowStatus::Waiting
    }
    else
    {
        WorkflowStatus::Ready
    };
}

fn profile_status(ui_state: &SanAndreasModUi) -> WorkflowStatus
{
    return if enabled_profile_count(ui_state) > 0
    {
        WorkflowStatus::Ready
    }
    else if ui_state.state.mods.is_empty()
    {
        WorkflowStatus::Waiting
    }
    else
    {
        WorkflowStatus::Review
    };
}

fn readiness_label(ui_state: &SanAndreasModUi) -> &'static str
{
    return match run_status(ui_state)
    {
        WorkflowStatus::Ready => "Ready",
        WorkflowStatus::Review => "Needs review",
        WorkflowStatus::Waiting => "Setup incomplete",
    };
}

fn run_status(ui_state: &SanAndreasModUi) -> WorkflowStatus
{
    return if !ui_state.pending_runs.is_empty()
    {
        WorkflowStatus::Review
    }
    else if enabled_profile_count(ui_state) > 0
    {
        WorkflowStatus::Ready
    }
    else
    {
        WorkflowStatus::Waiting
    };
}

fn count_pending_status(ui_state: &SanAndreasModUi, status: PendingRunStatus) -> usize
{
    return ui_state
        .pending_runs
        .iter()
        .filter(|record| record.status == status)
        .count();
}

fn enabled_profile_count(ui_state: &SanAndreasModUi) -> usize
{
    return selected_profile_entries(&ui_state.state, &ui_state.selected_profile)
        .iter()
        .filter(|entry| entry.enabled)
        .count();
}

impl SanAndreasModUi
{
    /// Profile lifecycle actions that were previously CLI-only: set-active,
    /// copy, rename, and delete.
    fn profile_management_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                if ui
                    .button("Set active")
                    .on_hover_text("Use this profile by default (like `profile-use`)")
                    .clicked()
                {
                    self.set_active_selected_profile();
                }
                if ui
                    .button("All off (vanilla)")
                    .on_hover_text("Disable every mod so the next run is vanilla")
                    .clicked()
                {
                    self.set_all_selected_profile_mods(ProfileModActivation::Disabled);
                }
                if ui
                    .button("All on")
                    .on_hover_text("Enable every mod in this profile")
                    .clicked()
                {
                    self.set_all_selected_profile_mods(ProfileModActivation::Enabled);
                }
                if ui
                    .button("Delete")
                    .on_hover_text("Delete this profile (asks first)")
                    .clicked()
                {
                    self.request_delete_profile();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Copy to");
                ui.add_sized(
                    [COMPACT_FIELD_WIDTH, ROW_HEIGHT],
                    egui::TextEdit::singleline(&mut self.inputs.copy_profile),
                );
                if ui.button("Copy").clicked()
                {
                    self.copy_selected_profile();
                }
                ui.label("Rename to");
                ui.add_sized(
                    [COMPACT_FIELD_WIDTH, ROW_HEIGHT],
                    egui::TextEdit::singleline(&mut self.inputs.rename_profile),
                );
                if ui.button("Rename").clicked()
                {
                    self.rename_selected_profile();
                }
            });
        });
    }

    /// Editor for the profile's launch arguments (space-separated), mirroring the
    /// CLI `profile-args` verb.
    fn profile_launch_args_panel(&mut self, ui: &mut egui::Ui)
    {
        ui.horizontal(|ui| {
            ui.label("Launch args");
            ui.add_sized(
                [LAUNCH_ARGS_FIELD_WIDTH, ROW_HEIGHT],
                egui::TextEdit::singleline(&mut self.inputs.launch_args)
                    .hint_text("-nointro -windowed"),
            );
            if ui
                .button("Save args")
                .on_hover_text("Set this profile's launch arguments")
                .clicked()
            {
                self.save_launch_args();
            }
        });
    }
}




