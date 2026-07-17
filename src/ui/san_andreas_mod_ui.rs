use crate::prelude::*;
use eframe::egui;

use super::state::{UiState, UiTab, load_ui_state};

pub(super) const ROW_HEIGHT: f32 = 28.0;
const PENDING_RUN_WATCH_INTERVAL: Duration = Duration::from_secs(2);

pub(crate) fn run_ui(game_root: PathBuf) -> Result<(), AppError> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1180.0, 760.0]),
        ..Default::default()
    };
    let application_factory: eframe::AppCreator<'_> = Box::new(|_| {
        let application = SanAndreasModUi::new(game_root);
        let boxed_application: Box<dyn eframe::App> = Box::new(application);
        Ok(boxed_application)
    });
    eframe::run_native(
        "SA Mod Manager", // literal: allow external interface text or file-format spelling
        options,
        application_factory,
    )
    .map_err(|err| AppError::Tool(format!("failed to open UI: {err}")))
}

pub(super) struct SanAndreasModUi {
    pub(super) game_root_input: String,
    pub(super) selected_profile: String,
    pub(super) import_path_input: String,
    pub(super) new_profile_input: String,
    pub(super) status: String,
    pub(super) state: UiState,
    pub(super) tab: UiTab,
    pub(super) pending_journal: Option<PathBuf>,
    pub(super) pending_runs: Vec<PendingRunRecord>,
    pub(super) game_child: Option<std::process::Child>,
    pub(super) recovery_focus_applied: bool,
    pub(super) last_pending_watch: Instant,
    pub(super) readme_proposals: Vec<ReadmeProposal>,
    pub(super) analysis_summary: Option<String>,
    pub(super) mod_root_edits: BTreeMap<String, ModInstallRootJson>,
    pub(super) telemetry: TelemetrySummary,
    pub(super) telemetry_search: String,
    pub(super) telemetry_kind_filter: String,
}

#[derive(Clone)]
pub(super) struct PendingRunRecord {
    pub(super) journal: PathBuf,
    pub(super) pid: Option<u32>,
    pub(super) status: PendingRunStatus,
    pub(super) detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PendingRunStatus {
    Running,
    Stale,
    Unknown,
    Invalid,
}

#[derive(Clone)]
pub(super) struct ReadmeProposal {
    pub(super) action: String,
    pub(super) proposed_install: String,
    pub(super) evidence: String,
    pub(super) source_readme: String,
    pub(super) line_number: usize,
    pub(super) confidence: f32,
    pub(super) review_state: ReadmeProposalState,
    pub(super) normalized_text: String,
    pub(super) reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReadmeProposalState {
    AutoSelected,
    NeedsReview,
    WarningOnly,
}

#[derive(Clone, Default)]
pub(super) struct TelemetrySummary {
    pub(super) imports: usize,
    pub(super) journals: usize,
    pub(super) run_journals: usize,
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
pub(super) struct TelemetryEvent {
    pub(super) created_unix: u64,
    pub(super) kind: String,
    pub(super) title: String,
    pub(super) detail: String,
}

#[derive(Clone, Default)]
pub(super) struct ModTelemetry {
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

impl fmt::Display for ReadmeProposalState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadmeProposalState::AutoSelected => write!(f, "auto-selected"),
            ReadmeProposalState::NeedsReview => write!(f, "needs review"),
            ReadmeProposalState::WarningOnly => write!(f, "warning only"),
        }
    }
}

impl fmt::Display for PendingRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PendingRunStatus::Running => write!(f, "running"),
            PendingRunStatus::Stale => write!(f, "finished"),
            PendingRunStatus::Unknown => write!(f, "unknown"),
            PendingRunStatus::Invalid => write!(f, "invalid"),
        }
    }
}

impl SanAndreasModUi {
    pub(super) fn new(game_root: PathBuf) -> Self {
        let selected_profile = "default".to_string(); // literal: allow external interface text or file-format spelling
        let mut ui = Self {
            game_root_input: game_root.display().to_string(),
            selected_profile,
            import_path_input: String::new(),
            new_profile_input: String::new(),
            status: String::new(),
            state: UiState::default(),
            tab: UiTab::Home,
            pending_journal: None,
            pending_runs: Vec::new(),
            game_child: None,
            recovery_focus_applied: false,
            last_pending_watch: Instant::now(),
            readme_proposals: Vec::new(),
            analysis_summary: None,
            mod_root_edits: BTreeMap::new(),
            telemetry: TelemetrySummary::default(),
            telemetry_search: String::new(),
            telemetry_kind_filter: "all".to_string(),
        };
        ui.refresh();
        ui
    }

    pub(super) fn game_root(&self) -> PathBuf {
        let trimmed_root = self.game_root_input.trim();
        PathBuf::from(trimmed_root)
    }

    pub(super) fn refresh(&mut self) {
        match self.reload_state() {
            Ok(()) => {
                self.status = self
                    .pending_cleanup_summary()
                    .unwrap_or_else(|| "ready".to_string());
            }
            Err(err) => self.status = err.to_string(),
        }
    }

    pub(super) fn reload_state(&mut self) -> Result<(), AppError> {
        let state = load_ui_state(&self.game_root(), &self.selected_profile)?;
        if !state
            .profiles
            .iter()
            .any(|name| name == &self.selected_profile)
        {
            self.selected_profile = state
                .profiles
                .first()
                .cloned()
                .unwrap_or_else(|| "default".to_string()); // literal: allow external interface text or file-format spelling
            self.state = load_ui_state(&self.game_root(), &self.selected_profile)?;
        } else {
            self.state = state;
        }
        self.refresh_pending_runs();
        self.telemetry =
            load_telemetry_summary(&self.game_root(), self.pending_runs.len()).unwrap_or_default();
        Ok(())
    }

    pub(super) fn run_action(
        &mut self,
        success_message: &str,
        action: impl FnOnce(&Path) -> Result<(), AppError>,
    ) {
        let result = action(&self.game_root());
        let status_result = result.map(|_| success_message.to_string());
        self.set_action_result(success_message, status_result);
    }

    pub(super) fn set_action_result(
        &mut self,
        success_message: &str,
        result: Result<String, AppError>,
    ) {
        match result {
            Ok(detail) => {
                self.status = if detail == success_message {
                    success_message.to_string()
                } else {
                    format!("{success_message}: {detail}")
                };
                if let Err(err) = self.reload_state() {
                    self.status = err.to_string();
                }
            }
            Err(err) => self.status = err.to_string(),
        }
    }
}

impl eframe::App for SanAndreasModUi {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_pending_run_watcher();
        context.request_repaint_after(PENDING_RUN_WATCH_INTERVAL);
        egui::TopBottomPanel::top("top_bar").show(context, |ui| self.top_bar(ui)); // literal: allow external interface text or file-format spelling
        egui::SidePanel::left("navigation") // literal: allow external interface text or file-format spelling
            .exact_width(184.0)
            .show(context, |ui| self.navigation(ui));
        egui::TopBottomPanel::bottom("status") // literal: allow external interface text or file-format spelling
            .exact_height(34.0)
            .show(context, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(context, |ui| match self.tab {
            UiTab::Home => self.home_panel(ui),
            UiTab::Profiles => self.profiles_panel(ui),
            UiTab::Mods => self.mods_panel(ui),
            UiTab::Import => self.import_panel(ui),
            UiTab::Run => self.run_panel(ui),
            UiTab::Telemetry => self.telemetry_panel(ui),
        });
    }
}

fn load_telemetry_summary(
    game_root: &Path,
    pending_cleanup: usize,
) -> Result<TelemetrySummary, AppError> {
    let state_root = state_directory(game_root);
    let mut summary = TelemetrySummary {
        pending_cleanup,
        ..TelemetrySummary::default()
    };
    load_import_telemetry(&state_root, &mut summary)?;
    load_journal_telemetry(&state_root, &mut summary)?;
    summary.recent_events.sort_by(|a, b| {
        b.created_unix
            .cmp(&a.created_unix)
            .then_with(|| a.title.cmp(&b.title))
    });
    summary.recent_events.truncate(200);
    summary.mod_history.sort_by(|a, b| {
        b.last_seen_unix
            .cmp(&a.last_seen_unix)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(summary)
}

fn load_import_telemetry(
    state_root: &Path,
    summary: &mut TelemetrySummary,
) -> Result<(), AppError> {
    let library_root = state_root.join("library");
    if !library_root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(library_root)? {
        let manifest = entry?.path().join("import.json");
        if !manifest.exists() {
            continue;
        }
        let text = fs::read_to_string(&manifest)?;
        summary.imports += 1;
        let id = telemetry_value(&text, "id").unwrap_or_else(|| "imported mod".to_string());
        let created_unix = telemetry_value(&text, "imported_unix")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let operations = telemetry_value(&text, "operation_count")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        upsert_mod_telemetry(summary, &id, |mod_row| {
            mod_row.imports += 1;
            mod_row.copied_files += operations;
            mod_row.last_seen_unix = mod_row.last_seen_unix.max(created_unix);
        });
        summary.recent_events.push(TelemetryEvent {
            created_unix,
            kind: "import".to_string(),
            title: id,
            detail: format!(
                "{} entries, {} operations",
                telemetry_value(&text, "entry_count").unwrap_or_else(|| "unknown".to_string()),
                telemetry_value(&text, "operation_count").unwrap_or_else(|| "unknown".to_string())
            ),
        });
    }
    Ok(())
}

fn load_journal_telemetry(
    state_root: &Path,
    summary: &mut TelemetrySummary,
) -> Result<(), AppError> {
    let journals_root = state_root.join("journals");
    if !journals_root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(journals_root)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        summary.journals += 1;
        let mode = telemetry_line_value(&text, "mode").unwrap_or_default();
        let created_unix = telemetry_line_value(&text, "created_unix")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        if mode == "ephemeral-run" {
            summary.run_journals += 1;
            add_run_mod_telemetry(summary, &text, created_unix);
        } else {
            summary.install_journals += 1;
            add_install_mod_telemetry(summary, &text, created_unix);
        }
        summary.copied_files += text
            .lines()
            .filter(|line| line.starts_with("copy="))
            .count();
        summary.new_files += text.lines().filter(|line| line.starts_with("new=")).count();
        summary.overwritten_files += text
            .lines()
            .filter(|line| line.starts_with("backup="))
            .count();
        summary.blocked_bootstrap += text
            .lines()
            .filter(|line| line.starts_with("blocked_bootstrap="))
            .count();
        summary.missing_sources += text
            .lines()
            .filter(|line| line.starts_with("missing_source="))
            .count();
        summary.recent_events.push(TelemetryEvent {
            created_unix,
            kind: if mode == "ephemeral-run" {
                "run".to_string()
            } else {
                "install".to_string()
            },
            title: telemetry_line_value(&text, "profile")
                .or_else(|| telemetry_line_value(&text, "package_id"))
                .unwrap_or_else(|| file_name(&path.display().to_string()).to_string()),
            detail: journal_event_detail(&text),
        });
    }
    Ok(())
}

fn add_install_mod_telemetry(summary: &mut TelemetrySummary, text: &str, created_unix: u64) {
    let Some(package_id) = telemetry_line_value(text, "package_id") else {
        return;
    };
    let copies = line_count(text, "copy=");
    let backups = line_count(text, "backup=");
    let missing = line_count(text, "missing_source=");
    let blocked = line_count(text, "blocked_bootstrap=");
    upsert_mod_telemetry(summary, &package_id, |mod_row| {
        mod_row.installs += 1;
        mod_row.copied_files += copies;
        mod_row.overwritten_files += backups;
        mod_row.missing_sources += missing;
        mod_row.blocked_bootstrap += blocked;
        mod_row.last_seen_unix = mod_row.last_seen_unix.max(created_unix);
    });
}

fn add_run_mod_telemetry(summary: &mut TelemetrySummary, text: &str, created_unix: u64) {
    let mut current_mod = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("profile_mod=") {
            let id = value
                .split('|')
                .next()
                .map(unescape_value)
                .unwrap_or_default();
            upsert_mod_telemetry(summary, &id, |mod_row| {
                mod_row.runs += 1;
                mod_row.last_seen_unix = mod_row.last_seen_unix.max(created_unix);
            });
            current_mod = Some(id);
            continue;
        }
        let Some(id) = current_mod.as_deref() else {
            continue;
        };
        if line.starts_with("copy=") {
            upsert_mod_telemetry(summary, id, |mod_row| mod_row.copied_files += 1);
        } else if line.starts_with("backup=") {
            upsert_mod_telemetry(summary, id, |mod_row| mod_row.overwritten_files += 1);
        } else if line.starts_with("missing_source=") {
            upsert_mod_telemetry(summary, id, |mod_row| mod_row.missing_sources += 1);
        } else if line.starts_with("blocked_bootstrap=") {
            upsert_mod_telemetry(summary, id, |mod_row| mod_row.blocked_bootstrap += 1);
        }
    }
}

fn upsert_mod_telemetry(
    summary: &mut TelemetrySummary,
    id: &str,
    update: impl FnOnce(&mut ModTelemetry),
) {
    if let Some(row) = summary.mod_history.iter_mut().find(|row| row.id == id) {
        update(row);
        return;
    }
    let mut row = ModTelemetry {
        id: id.to_string(),
        ..ModTelemetry::default()
    };
    update(&mut row);
    summary.mod_history.push(row);
}

fn journal_event_detail(text: &str) -> String {
    let copies = line_count(text, "copy=");
    let new_files = line_count(text, "new=");
    let backups = line_count(text, "backup=");
    let blocked = line_count(text, "blocked_bootstrap=");
    let missing = line_count(text, "missing_source=");
    format!(
        "{copies} copied, {new_files} new, {backups} overwritten, {blocked} blocked, {missing} missing"
    )
}

fn line_count(text: &str, prefix: &str) -> usize {
    text.lines().filter(|line| line.starts_with(prefix)).count()
}

fn telemetry_line_value(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix).map(unescape_value))
}

fn telemetry_value(text: &str, key: &str) -> Option<String> {
    let quoted_prefix = format!("\"{key}\":");
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&quoted_prefix)?.trim();
        let value = value.trim_end_matches(',');
        Some(value.trim_matches('"').to_string())
    })
}

pub(super) fn export_telemetry_summary(
    game_root: &Path,
    summary: &TelemetrySummary,
) -> Result<PathBuf, AppError> {
    let export_dir = state_directory(game_root).join("telemetry");
    fs::create_dir_all(&export_dir)?;
    let path = export_dir.join(format!("telemetry-{}.json", unix_now()));
    let mut file = fs::File::create(&path)?;
    writeln!(file, "{{")?;
    writeln!(file, "  \"version\": 1,")?;
    writeln!(file, "  \"exported_unix\": {},", unix_now())?;
    writeln!(file, "  \"summary\": {{")?;
    writeln!(file, "    \"imports\": {},", summary.imports)?;
    writeln!(file, "    \"journals\": {},", summary.journals)?;
    writeln!(file, "    \"runs\": {},", summary.run_journals)?;
    writeln!(file, "    \"installs\": {},", summary.install_journals)?;
    writeln!(file, "    \"copied_files\": {},", summary.copied_files)?;
    writeln!(file, "    \"new_files\": {},", summary.new_files)?;
    writeln!(
        file,
        "    \"overwritten_files\": {},",
        summary.overwritten_files
    )?;
    writeln!(
        file,
        "    \"blocked_bootstrap\": {},",
        summary.blocked_bootstrap
    )?;
    writeln!(
        file,
        "    \"missing_sources\": {},",
        summary.missing_sources
    )?;
    writeln!(file, "    \"pending_cleanup\": {}", summary.pending_cleanup)?;
    writeln!(file, "  }},")?;
    write_mod_history_json(&mut file, &summary.mod_history)?;
    writeln!(file, ",")?;
    write_recent_events_json(&mut file, &summary.recent_events)?;
    writeln!(file)?;
    writeln!(file, "}}")?;
    Ok(path)
}

fn write_mod_history_json(file: &mut fs::File, rows: &[ModTelemetry]) -> Result<(), AppError> {
    writeln!(file, "  \"mods\": [")?;
    for (idx, row) in rows.iter().enumerate() {
        writeln!(file, "    {{")?;
        writeln!(file, "      \"id\": \"{}\",", json_escape(&row.id))?;
        writeln!(file, "      \"imports\": {},", row.imports)?;
        writeln!(file, "      \"runs\": {},", row.runs)?;
        writeln!(file, "      \"installs\": {},", row.installs)?;
        writeln!(file, "      \"copied_files\": {},", row.copied_files)?;
        writeln!(
            file,
            "      \"overwritten_files\": {},",
            row.overwritten_files
        )?;
        writeln!(file, "      \"missing_sources\": {},", row.missing_sources)?;
        writeln!(
            file,
            "      \"blocked_bootstrap\": {},",
            row.blocked_bootstrap
        )?;
        writeln!(file, "      \"last_seen_unix\": {}", row.last_seen_unix)?;
        write!(file, "    }}")?;
        if idx + 1 == rows.len() {
            writeln!(file)?;
        } else {
            writeln!(file, ",")?;
        }
    }
    write!(file, "  ]")?;
    Ok(())
}

fn write_recent_events_json(
    file: &mut fs::File,
    events: &[TelemetryEvent],
) -> Result<(), AppError> {
    writeln!(file, "  \"events\": [")?;
    for (idx, event) in events.iter().enumerate() {
        writeln!(file, "    {{")?;
        writeln!(file, "      \"created_unix\": {},", event.created_unix)?;
        writeln!(file, "      \"kind\": \"{}\",", json_escape(&event.kind))?;
        writeln!(file, "      \"title\": \"{}\",", json_escape(&event.title))?;
        writeln!(file, "      \"detail\": \"{}\"", json_escape(&event.detail))?;
        write!(file, "    }}")?;
        if idx + 1 == events.len() {
            writeln!(file)?;
        } else {
            writeln!(file, ",")?;
        }
    }
    write!(file, "  ]")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_summary_aggregates_imports_and_journals() {
        let game_root = test_root("telemetry_summary");
        let state_root = state_directory(&game_root);
        let import_root = state_root.join("library").join("test_mod");
        let journals_root = state_root.join("journals");
        fs::create_dir_all(&import_root).unwrap();
        fs::create_dir_all(&journals_root).unwrap();
        fs::write(
            import_root.join("import.json"),
            concat!(
                "{\n",
                "  \"version\": 1,\n",
                "  \"id\": \"test_mod\",\n",
                "  \"imported_unix\": 100,\n",
                "  \"entry_count\": 5,\n",
                "  \"operation_count\": 2\n",
                "}\n"
            ),
        )
        .unwrap();
        fs::write(
            journals_root.join("run-default-200.journal"),
            concat!(
                "version=1\n",
                "txid=run-default-200\n",
                "profile=default\n",
                "mode=ephemeral-run\n",
                "created_unix=200\n",
                "profile_mod=test_mod|100|mods/test_mod/mod.json\n",
                "new=CLEO/test.cs\n",
                "backup=data/file.dat|backup/file.dat|fnv64:1\n",
                "copy=src|dst|fnv64:2\n",
                "missing_source=missing\n"
            ),
        )
        .unwrap();

        let summary = load_telemetry_summary(&game_root, 1).unwrap();

        assert_eq!(summary.imports, 1);
        assert_eq!(summary.journals, 1);
        assert_eq!(summary.run_journals, 1);
        assert_eq!(summary.copied_files, 1);
        assert_eq!(summary.new_files, 1);
        assert_eq!(summary.overwritten_files, 1);
        assert_eq!(summary.missing_sources, 1);
        assert_eq!(summary.pending_cleanup, 1);
        assert_eq!(summary.recent_events.len(), 2);
        assert_eq!(summary.recent_events[0].kind, "run");
        assert_eq!(summary.mod_history.len(), 1);
        assert_eq!(summary.mod_history[0].id, "test_mod");
        assert_eq!(summary.mod_history[0].imports, 1);
        assert_eq!(summary.mod_history[0].runs, 1);
        assert_eq!(summary.mod_history[0].copied_files, 3);
        assert_eq!(summary.mod_history[0].overwritten_files, 1);
        assert_eq!(summary.mod_history[0].missing_sources, 1);
        let export = export_telemetry_summary(&game_root, &summary).unwrap();
        let export_text = fs::read_to_string(export).unwrap();
        assert!(export_text.contains("\"mods\""));
        assert!(export_text.contains("\"events\""));
        assert!(export_text.contains("test_mod"));
        remove_dir_if_exists(&game_root).unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        remove_dir_if_exists(&root).unwrap();
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}
