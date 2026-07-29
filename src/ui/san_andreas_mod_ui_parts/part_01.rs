use crate::prelude::*;
use eframe::egui;

use super::preferences::UiPreferences;
use super::state::{UiState, UiTab, load_ui_state};

pub(super) const ROW_HEIGHT: f32 = 28.0;
/// Throttle preference writes so dragging the window edge cannot flood the disk;
/// discrete changes (tab, profile, folder) still persist within this window.
const PREF_SAVE_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) fn run_ui(cli_game_root: Option<PathBuf>) -> Result<(), AppError>
{
    let preferences = UiPreferences::load();
    let game_root = resolve_initial_game_root(cli_game_root, &preferences);
    let window_size = preferences.window_size();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(window_size),
        // Use the wgpu backend (DirectX 12 on Windows) instead of glow/OpenGL:
        // OpenGL context creation aborts the process on some Windows driver
        // setups, whereas wgpu is far more broadly compatible.
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let application_factory: eframe::AppCreator<'_> = Box::new(move |_| {
        let application = SanAndreasModUi::new(game_root, preferences);
        let boxed_application: Box<dyn eframe::App> = Box::new(application);
        Ok(boxed_application)
    });
    return eframe::run_native("SA Mod Manager", options, application_factory)
        .map_err(|err| AppError::Tool(format!("failed to open UI: {err}")));
}

/// An explicit `--game`/positional argument always wins; otherwise fall back to
/// the last folder used, then to the compiled default. This keeps a scripted
/// `ui <path>` deterministic while letting the plain `ui` command remember.
fn resolve_initial_game_root(
    cli_game_root: Option<PathBuf>,
    preferences: &UiPreferences,
) -> PathBuf
{
    if let Some(root) = cli_game_root
    {
        return root;
    }
    if !preferences.game_root.trim().is_empty()
    {
        return PathBuf::from(preferences.game_root.trim());
    }
    return crate::settings::default_game_root();
}

/// How the profile mod list filters by activation state, mirroring MO2's
/// Checked/Unchecked category shortcuts.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ModStatusFilter
{
    #[default]
    All,
    Enabled,
    Disabled,
}

/// The active tab of the per-mod info window (MO2's Mod Info dialog).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ModDetailsTab
{
    #[default]
    Files,
    Conflicts,
    Roots,
    Readme,
    /// User annotations: color label, categories, and a freeform note.
    Notes,
}

/// A per-mod detail view (MO2's Mod Info dialog): the file list and any readme
/// text are gathered once when the window opens; conflicts and install roots are
/// read live from the content index / config each frame.
pub(super) struct ModDetailsView
{
    pub(super) id: String,
    pub(super) tab: ModDetailsTab,
    /// Relative file paths under the mod's library source root.
    pub(super) files: Vec<String>,
    /// `(relative path, contents)` for readme-like files found in the mod.
    pub(super) readmes: Vec<(String, String)>,
    pub(super) source_root: Option<PathBuf>,
    /// Edit buffers for the Notes tab, seeded from the mod's metadata on open.
    pub(super) note_edit: String,
    pub(super) new_category: String,
}

pub(super) struct UiInputs
{
    pub(super) game_root: String,
    pub(super) import_path: String,
    pub(super) new_profile: String,
    pub(super) copy_profile: String,
    pub(super) rename_profile: String,
    pub(super) launch_args: String,
}


pub(super) struct UiContentView
{
    pub(super) index: Option<ContentIndex>,
    pub(super) category: Option<ContentCategory>,
    pub(super) search: String,
    pub(super) conflicts_only: bool,
}
pub(super) struct UiFilters
{
    pub(super) telemetry_kind: String,
    pub(super) mod_text: String,
    pub(super) mod_status: ModStatusFilter,
    pub(super) mod_category: Option<ContentCategory>,
    pub(super) mod_conflicts: bool,
    pub(super) mod_user_category: Option<String>,
}

pub(super) struct SanAndreasModUi
{
    pub(super) inputs: UiInputs,
    pub(super) selected_profile: String,
    pub(super) status: String,
    pub(super) state: UiState,
    pub(super) tab: UiTab,
    pub(super) pending_journal: Option<PathBuf>,
    pub(super) pending_runs: Vec<PendingRunRecord>,
    pub(super) game_child: Option<std::process::Child>,
    pub(super) active_run: Option<ActiveRun>,
    pub(super) recovery_focus_applied: bool,
    pub(super) last_pending_watch: Instant,
    pub(super) readme_proposals: Vec<ReadmeProposal>,
    pub(super) analysis_summary: Option<String>,
    pub(super) mod_root_edits: BTreeMap<String, ModInstallRootJson>,
    pub(super) telemetry: TelemetrySummary,
    pub(super) telemetry_search: String,
    pub(super) last_error: Option<String>,
    pub(super) pending_confirm: Option<PendingConfirm>,
    pub(super) task: Option<BackgroundTask>,
    pub(super) dark_mode: bool,
    /// Text inputs for the profile lifecycle actions (copy/rename destinations)
    /// and the launch-args editor, mirroring the CLI's profile verbs in the UI.

    /// In-progress per-profile install-root target edits, keyed by
    /// `<mod-id>#<root-source>` (mirrors `mod_root_edits`).
    pub(super) profile_root_target_edits: BTreeMap<String, String>,
    /// Cached content/conflict index and active filter state for the selected profile.
    pub(super) content: UiContentView,
    /// ModLoader's own per-folder priorities, read alongside a content scan.
    pub(super) modloader_priorities: Option<ModLoaderPriorities>,
    /// Game-folder files under mod areas that no enabled mod owns (MO2's
    /// "overwrite"), computed during a content scan.
    pub(super) overwrite_files: Vec<String>,
    /// Summary of the last run's `modloader.log` (what ModLoader actually loaded
    /// or failed to), read alongside a content scan. `None` until scanned.
    pub(super) modloader_log: Option<ModLoaderLogSummary>,
    /// CLEO health for the installed game folder (blacklisted plugins, FXT key
    /// conflicts, per-script missing-plugin/capability issues), read alongside a
    /// content scan. `None` until scanned.
    pub(super) cleo_diagnostics: Option<CleoDiagnostics>,
    /// Profile mod-list filters (MO2's Categories sidebar + Filter box). While
    /// any is active, reordering is disabled so hidden rows can't misdirect it.
    pub(super) filters: UiFilters,
    /// User annotations (categories / color / note) keyed by mod id.
    pub(super) mod_meta: BTreeMap<String, ModMeta>,
    /// Per-profile ModLoader priority overrides (folder â†’ priority; 0 = disabled
    /// in ModLoader), applied to modloader.ini on demand.
    pub(super) modloader_overrides: BTreeMap<String, i32>,
    /// Load-order separators (labeled group dividers) for the selected profile.
    pub(super) separators: Vec<Separator>,
    /// Ids of separators whose sections are collapsed in the list (UI-only).
    pub(super) collapsed_separators: BTreeSet<String>,
    /// The separator currently being renamed: `(id, in-progress name)`.
    pub(super) separator_edit: Option<(String, String)>,
    /// Configured external run targets (MO2's executables), loaded from disk.
    pub(super) executables: Vec<Executable>,
    /// Selected run target: 0 = play the current profile, 1.. = `executables[n-1]`.
    pub(super) selected_run_target: usize,
    pub(super) new_tool_name: String,
    pub(super) new_tool_path: String,
    pub(super) new_tool_args: String,
    /// The per-mod info window, when open (MO2's Mod Info dialog).
    pub(super) mod_info: Option<ModDetailsView>,
    window_size: [f32; 2], // literal: allow UI tuning threshold is local to this control
    last_pref_save: Instant,
    prefs_signature: String,
}

/// A destructive action awaiting user confirmation in a modal dialog.
pub(super) struct PendingConfirm
{
    pub(super) title: String,
    pub(super) message: String,
    pub(super) confirm_label: String,
    pub(super) action: ConfirmAction,
}

pub(super) enum ConfirmAction
{
    RemoveMod(String),
    CleanSelectedRun,
    CleanRunRecord(PendingRunRecord),
    CleanFinishedRuns,
    DeleteProfile(String),
}

/// A long-running operation executing off the UI thread. The UI polls
/// `receiver` each frame and stays responsive (spinner) until it completes.
pub(super) struct BackgroundTask
{
    pub(super) label: String,
    receiver: std::sync::mpsc::Receiver<TaskResult>,
}

pub(super) enum TaskResult
{
    Import(Result<(), AppError>),
    Analyze(Result<PackageReport, AppError>),
    Play(Result<(ActiveRun, std::process::Child), AppError>),
}

/// The in-memory context of the run currently launched by this manager session.
/// Held so that when the game process exits we can record_log_message_from_arguments a full outcome (exit
/// code, duration, launch args) â€” detail that only exists while we own the child
/// handle and is lost across a manager restart.
pub(super) struct ActiveRun
{
    pub(super) journal: PathBuf,
    pub(super) txid: String,
    pub(super) profile: String,
    pub(super) launch_args: Vec<String>,
    pub(super) started_unix: u64,
}

#[derive(Clone)]
pub(super) struct PendingRunRecord
{
    pub(super) journal: PathBuf,
    pub(super) pid: Option<u32>,
    pub(super) status: PendingRunStatus,
    pub(super) detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PendingRunStatus
{
    Running,
    Stale,
    Unknown,
    Invalid,
}

#[derive(Clone)]
pub(super) struct ReadmeProposal
{
    pub(super) action: String,
    pub(super) proposed_install: String,
    pub(super) evidence: String,
    pub(super) source_readme: String,
    pub(super) line_number: usize,
    pub(super) confidence: f32,
    pub(super) review_state: ReadmeProposalState,
    pub(super) normalized_text: String,
    pub(super) reasons: Vec<String>,
    /// Raw source/target the instruction referenced, retained so a Copy proposal
    /// can be promoted into a mod-config install root on demand.
    pub(super) source: Option<String>,
    pub(super) target: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReadmeProposalState
{
    AutoSelected,
    NeedsReview,
    WarningOnly,
}

#[derive(Clone, Default)]
pub(super) struct TelemetrySummary
{
    pub(super) imports: usize,
    pub(super) journals: usize,
    pub(super) run_journals: usize,
    pub(super) failed_launches: usize,
    pub(super) install_journals: usize,
    pub(super) copied_files: usize,
    pub(super) new_files: usize,
    pub(super) overwritten_files: usize,
    pub(super) blocked_bootstrap: usize,
    pub(super) missing_sources: usize,
    pub(super) pending_cleanup: usize,
    pub(super) recent_events: Vec<TelemetryEvent>,
    pub(super) mod_history: Vec<ModTelemetry>,
}

#[derive(Clone)]
pub(super) struct TelemetryEvent
{
    pub(super) created_unix: u64,
    pub(super) kind: String,
    pub(super) title: String,
    pub(super) detail: String,
}

#[derive(Clone, Default)]
pub(super) struct ModTelemetry
{
    pub(super) id: String,
    pub(super) imports: usize,
    pub(super) runs: usize,
    pub(super) installs: usize,
    pub(super) copied_files: usize,
    pub(super) overwritten_files: usize,
    pub(super) missing_sources: usize,
    pub(super) blocked_bootstrap: usize,
    pub(super) last_seen_unix: u64,
}

impl fmt::Display for ReadmeProposalState
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        return match self
        {
            ReadmeProposalState::AutoSelected => write!(f, "auto-selected"),
            ReadmeProposalState::NeedsReview => write!(f, "needs review"),
            ReadmeProposalState::WarningOnly => write!(f, "warning only"),
        };
    }
}

impl fmt::Display for PendingRunStatus
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        return match self
        {
            PendingRunStatus::Running => write!(f, "running"),
            PendingRunStatus::Stale => write!(f, "finished"),
            PendingRunStatus::Unknown => write!(f, "unknown"),
            PendingRunStatus::Invalid => write!(f, "invalid"),
        };
    }
}
