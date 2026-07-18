use crate::prelude::*;
use crate::planning::readme_copy_install_root;
use crate::workspace::append_mod_config_install_root;
use eframe::egui;

use super::san_andreas_mod_ui::{
    ConfirmAction, PendingRunRecord, PendingRunStatus, ReadmeProposal, ReadmeProposalState,
    SanAndreasModUi, TaskResult, export_telemetry_summary,
};

impl SanAndreasModUi {
    pub(super) fn export_telemetry(&mut self) {
        match export_telemetry_summary(&self.game_root(), &self.telemetry) {
            Ok(path) => self.status = format!("exported telemetry: {}", path.display()),
            Err(err) => self.status = err.to_string(),
        }
    }
}

impl SanAndreasModUi {
    pub(super) fn browse_game_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Select GTA San Andreas folder");
        let current = self.game_root();
        if current.is_dir() {
            dialog = dialog.set_directory(&current);
        }
        if let Some(path) = dialog.pick_folder() {
            self.game_root_input = path.display().to_string();
            self.refresh();
        }
    }

    pub(super) fn browse_package_file(&mut self) {
        let dialog = rfd::FileDialog::new()
            .set_title("Select a mod package")
            .add_filter("Mod packages", &["zip", "wrap", "7z", "rar"]);
        if let Some(path) = dialog.pick_file() {
            self.set_import_path(path);
        }
    }

    pub(super) fn browse_package_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Select a mod folder")
            .pick_folder()
        {
            self.set_import_path(path);
        }
    }

    fn set_import_path(&mut self, path: PathBuf) {
        self.import_path_input = path.display().to_string();
        self.clear_readme_review();
        self.tab = crate::ui::state::UiTab::Import;
    }

    /// Drop analysis results tied to a previously reviewed package. Called
    /// whenever the package path changes so a stale proposal can never be applied
    /// to a different package's config.
    pub(super) fn clear_readme_review(&mut self) {
        self.readme_proposals.clear();
        self.analysis_summary = None;
    }

    /// Route a path dropped onto the window: a real GTA install folder fills the
    /// game-folder field; anything else fills the package field and jumps to Import.
    pub(super) fn handle_dropped_path(&mut self, path: PathBuf) {
        if path.is_dir() && game_executable_path(&path).is_some() {
            self.game_root_input = path.display().to_string();
            self.status = format!("set game folder: {}", path.display());
            self.refresh();
        } else {
            let display = path.display().to_string();
            self.set_import_path(path);
            self.status = format!("loaded package: {display}");
        }
    }
}

impl SanAndreasModUi {
    pub(super) fn initialize_state(&mut self) {
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        self.run_action("initialized manager state", |game_root| {
            // literal: allow external interface text or file-format spelling
            init_state(game_root)
        });
    }
}

impl SanAndreasModUi {
    pub(super) fn add_mod_to_profile(&mut self, mod_id: &str) {
        let config_path = self
            .game_root()
            .join(".sa-mod-manager") // literal: allow external interface text or file-format spelling
            .join("mods") // literal: allow external interface text or file-format spelling
            .join(mod_id)
            .join("mod.json"); // literal: allow external interface text or file-format spelling
        let profile = self.selected_profile.clone();
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        self.run_action("added mod to profile", |game_root| {
            // literal: allow external interface text or file-format spelling
            add_mod_to_profile_json(game_root, &profile, &config_path)
        });
    }
}

impl SanAndreasModUi {
    pub(super) fn set_mod_activation(&mut self, mod_id: &str, activation: ProfileModActivation) {
        let profile = self.selected_profile.clone();
        let mod_id = mod_id.to_string();
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        self.run_action("updated mod toggle", |game_root| {
            // literal: allow external interface text or file-format spelling
            set_profile_mod_activation(game_root, &profile, &mod_id, activation)
        });
    }
}

impl SanAndreasModUi {
    pub(super) fn set_mod_order(&mut self, mod_id: &str, order: i32) {
        let profile = self.selected_profile.clone();
        let mod_id = mod_id.to_string();
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        self.run_action("updated load order", |game_root| {
            // literal: allow external interface text or file-format spelling
            set_profile_mod_order(game_root, &profile, &mod_id, order)
        });
    }
}

impl SanAndreasModUi {
    pub(super) fn remove_mod_from_profile(&mut self, mod_id: &str) {
        let profile = self.selected_profile.clone();
        let mod_id = mod_id.to_string();
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        self.run_action("removed mod from profile", |game_root| {
            // literal: allow external interface text or file-format spelling
            remove_mod_from_profile_json(game_root, &profile, &mod_id)
        });
    }
}

impl SanAndreasModUi {
    /// Validate then persist an edited install root. Returns whether it was saved
    /// so the caller can keep an invalid edit open for correction.
    pub(super) fn save_mod_install_root(
        &mut self,
        config_path: &Path,
        root_index: usize,
        root: ModInstallRootJson,
    ) -> bool {
        if let Err(err) = validate_install_root(&root) {
            self.record_error(err);
            return false;
        }
        let result = update_mod_config_install_root(config_path, root_index, &root)
            .map(|_| "updated install root".to_string());
        let saved = result.is_ok();
        self.set_action_result("updated install root", result);
        saved
    }
}

/// Reject an install root before it reaches disk: source and target must be
/// non-empty relative paths with no `..` escape or drive-letter, the same
/// containment rule the installer enforces. `kind` comes from a fixed dropdown,
/// so it needs no check here.
fn validate_install_root(root: &ModInstallRootJson) -> Result<(), AppError> {
    validate_install_root_path("source", &root.source)?;
    validate_install_root_path("target", &root.target)
}

fn validate_install_root_path(label: &str, value: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Usage(format!("install root {label} is required")));
    }
    path_from_package_root(&normalize_path(trimmed))
        .map(|_| ())
        .map_err(|err| {
            AppError::Usage(format!("install root {label} `{trimmed}` is invalid: {err}"))
        })
}

impl SanAndreasModUi {
    pub(super) fn launch_selected_profile(&mut self, ctx: &egui::Context) {
        self.refresh_pending_runs();
        if self.has_unsafe_pending_runs_for_launch() {
            self.tab = crate::ui::state::UiTab::Run;
            self.status = "clean temporary files before playing another profile".to_string();
            return;
        }
        if self.game_child.is_some() {
            self.status = "game is already running for this manager session".to_string();
            return;
        }
        let game_root = self.game_root();
        let profile = self.selected_profile.clone();
        // Materialize (extract + copy) can be slow; run it off the UI thread.
        self.spawn_task(ctx, "preparing and launching", move || {
            TaskResult::Play(materialize_and_launch_profile(&game_root, &profile))
        });
    }

    pub(super) fn apply_play(
        &mut self,
        result: Result<(PathBuf, std::process::Child), AppError>,
    ) {
        match result {
            Ok((journal, child)) => {
                self.pending_journal = Some(journal.clone());
                self.game_child = Some(child);
                self.last_error = None;
                self.status = format!("playing profile; cleanup record {}", journal.display());
                if let Err(err) = self.reload_state() {
                    self.record_error(err);
                }
            }
            Err(err) => {
                self.record_error(err);
                self.refresh_pending_runs();
            }
        }
    }
}

impl SanAndreasModUi {
    pub(super) fn cleanup_pending_run(&mut self) {
        let Some(record) = self.selected_pending_run() else {
            self.status = "no cleanup record selected".to_string(); // literal: allow external interface text or file-format spelling
            return;
        };
        if self.pending_run_is_running(&record) {
            self.status = "game is still running; cleanup is blocked".to_string();
            return;
        }
        let journal = record.journal.clone();
        let cleanup_result = self.cleanup_journal(&journal);
        self.set_action_result(
            "cleaned temporary files",
            cleanup_result.map(|_| {
                // literal: allow external interface text or file-format spelling
                "cleaned temporary files".to_string()
            }),
        );
        if !pending_record_path(&self.game_root(), &journal).exists() {
            self.pending_journal = None;
        }
        self.refresh_pending_runs();
    }
}

impl SanAndreasModUi {
    pub(super) fn cleanup_pending_run_record(&mut self, record: PendingRunRecord) {
        if self.pending_run_is_running(&record) {
            self.status = "game is still running; cleanup is blocked".to_string();
            return;
        }
        let journal = record.journal.clone();
        let cleanup_result = self.cleanup_journal(&journal);
        self.set_action_result(
            "cleaned temporary files",
            cleanup_result.map(|_| "cleaned temporary files".to_string()),
        );
        if self
            .pending_journal
            .as_ref()
            .map(|pending| pending == &journal)
            .unwrap_or(false)
        {
            self.pending_journal = None;
        }
        self.refresh_pending_runs();
    }
}

impl SanAndreasModUi {
    pub(super) fn cleanup_stale_pending_runs(&mut self) {
        self.refresh_pending_runs();
        let stale_runs = self
            .pending_runs
            .iter()
            .filter(|record| record.status == PendingRunStatus::Stale)
            .cloned()
            .collect::<Vec<_>>();
        if stale_runs.is_empty() {
            self.status = "no finished runs need cleanup".to_string();
            return;
        }

        let game_root = self.game_root();
        let mut cleaned = 0;
        let mut first_error = None;
        for record in stale_runs {
            let result = cleanup_journal_for_game_root(&game_root, &record.journal);
            match result {
                Ok(()) => cleaned += 1,
                Err(err) => {
                    first_error = Some(err);
                    break;
                }
            }
        }
        self.refresh_pending_runs();
        self.status = match first_error {
            Some(err) => format!("cleaned {cleaned} finished runs; then failed: {err}"),
            None => format!("cleaned {cleaned} finished runs"),
        };
    }
}

impl SanAndreasModUi {
    pub(super) fn refresh_pending_runs(&mut self) {
        self.pending_runs = load_pending_run_records(&self.game_root()).unwrap_or_default();
        if self.pending_journal.is_none() {
            self.pending_journal = self
                .pending_runs
                .first()
                .map(|record| record.journal.clone());
        }
        if !self.pending_runs.is_empty() && !self.recovery_focus_applied {
            self.tab = crate::ui::state::UiTab::Run;
            self.recovery_focus_applied = true;
        }
    }

    pub(super) fn tick_pending_run_watcher(&mut self) {
        self.poll_game_child();
        if self.last_pending_watch.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_pending_watch = Instant::now();
        self.cleanup_finished_pending_runs();
    }

    pub(super) fn poll_game_child(&mut self) {
        let Some(child) = &mut self.game_child else {
            return;
        };
        match child.try_wait() {
            Ok(Some(_status)) => {
                let journal = self.pending_journal.clone();
                self.game_child = None;
                if let Some(journal) = journal {
                    match self.cleanup_journal(&journal) {
                        Ok(()) => {
                            self.pending_journal = None;
                            self.refresh_pending_runs();
                            self.status = "game exited; cleaned temporary files".to_string();
                        }
                        Err(err) => {
                            self.refresh_pending_runs();
                            self.status = format!("game exited; automatic cleanup failed: {err}");
                        }
                    }
                } else {
                    self.cleanup_finished_pending_runs();
                }
            }
            Ok(None) => {}
            Err(err) => {
                self.game_child = None;
                self.status = format!("could not check game process: {err}");
            }
        }
    }

    fn cleanup_finished_pending_runs(&mut self) {
        self.refresh_pending_runs();
        let stale_runs = self
            .pending_runs
            .iter()
            .filter(|record| record.status == PendingRunStatus::Stale)
            .cloned()
            .collect::<Vec<_>>();
        if stale_runs.is_empty() {
            return;
        }

        let game_root = self.game_root();
        let mut cleaned = 0;
        let mut first_error = None;
        for record in stale_runs {
            match cleanup_journal_for_game_root(&game_root, &record.journal) {
                Ok(()) => cleaned += 1,
                Err(err) => {
                    first_error = Some(err);
                    break;
                }
            }
        }
        self.refresh_pending_runs();
        self.status = match first_error {
            Some(err) => format!("auto-cleaned {cleaned} finished runs; then failed: {err}"),
            None => format!("auto-cleaned {cleaned} finished runs"),
        };
    }

    fn cleanup_journal(&self, journal: &Path) -> Result<(), AppError> {
        cleanup_journal_for_game_root(&self.game_root(), journal)
    }

    fn selected_pending_run(&self) -> Option<PendingRunRecord> {
        if let Some(journal) = &self.pending_journal {
            if let Some(record) = self
                .pending_runs
                .iter()
                .find(|record| &record.journal == journal)
            {
                return Some(record.clone());
            }
            return Some(PendingRunRecord {
                journal: journal.clone(),
                pid: None,
                status: PendingRunStatus::Unknown,
                detail: "selected cleanup record is not tracked".to_string(),
            });
        }
        self.pending_runs.first().cloned()
    }

    fn pending_run_is_running(&mut self, record: &PendingRunRecord) -> bool {
        self.poll_game_child();
        if self.game_child.is_some() {
            return true;
        }
        record.status == PendingRunStatus::Running
            || record.pid.map(process_is_running).unwrap_or(false)
    }

    fn has_unsafe_pending_runs_for_launch(&self) -> bool {
        self.pending_runs
            .iter()
            .any(|record| !self.pending_run_is_current_session_running(record))
    }

    fn pending_run_is_current_session_running(&self, record: &PendingRunRecord) -> bool {
        pending_run_matches_current_session(
            record,
            self.game_child.as_ref().map(|child| child.id()),
        )
    }

    pub(super) fn pending_cleanup_summary(&self) -> Option<String> {
        if self.pending_runs.is_empty() {
            return None;
        }
        let stale = self.count_pending_status(PendingRunStatus::Stale);
        let running = self.count_pending_status(PendingRunStatus::Running);
        let unknown = self.count_pending_status(PendingRunStatus::Unknown);
        let invalid = self.count_pending_status(PendingRunStatus::Invalid);
        Some(format!(
            "{} temporary runs need cleanup: {stale} finished, {running} running, {unknown} unknown, {invalid} invalid",
            self.pending_runs.len()
        ))
    }

    fn count_pending_status(&self, status: PendingRunStatus) -> usize {
        self.pending_runs
            .iter()
            .filter(|record| record.status == status)
            .count()
    }
}

impl SanAndreasModUi {
    pub(super) fn create_profile(&mut self) {
        let trimmed_name = self.new_profile_input.trim();
        let name = trimmed_name.to_string();
        if name.is_empty() {
            self.status = "profile name is required".to_string(); // literal: allow external interface text or file-format spelling
            return;
        }
        let profile_name = safe_name(&name);
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        self.run_action("created profile", |game_root| {
            // literal: allow external interface text or file-format spelling
            create_profile(game_root, &profile_name)
        });
        self.selected_profile = profile_name;
        self.new_profile_input.clear();
    }
}

impl SanAndreasModUi {
    pub(super) fn import_package(&mut self, ctx: &egui::Context) {
        let trimmed_package = self.import_path_input.trim();
        let package = PathBuf::from(trimmed_package);
        if package.as_os_str().is_empty() {
            self.status = "package path is required".to_string(); // literal: allow external interface text or file-format spelling
            return;
        }
        let profile = self.selected_profile.clone();
        let game_root = self.game_root();
        // Extraction + copy into the library can be slow; run it off the UI thread.
        self.spawn_task(ctx, "importing package", move || {
            let options = CommandOptions {
                game_root: game_root.clone(),
                profile,
                includes: BTreeSet::new(),
                excludes: BTreeSet::new(),
                write_manifest: false,
            };
            TaskResult::Import(import_package(&package, &options))
        });
    }
}

impl SanAndreasModUi {
    pub(super) fn request_remove_mod(&mut self, mod_id: &str) {
        self.request_confirm(
            "Remove mod",
            &format!(
                "Remove `{mod_id}` from profile `{}`? This is not undoable.",
                self.selected_profile
            ),
            "Remove",
            ConfirmAction::RemoveMod(mod_id.to_string()),
        );
    }

    pub(super) fn request_cleanup_selected(&mut self) {
        self.request_confirm(
            "Clean temporary files",
            "Roll back the selected run and delete its materialized files from the game folder?",
            "Clean",
            ConfirmAction::CleanSelectedRun,
        );
    }

    pub(super) fn request_cleanup_record(&mut self, record: PendingRunRecord) {
        self.request_confirm(
            "Clean temporary files",
            "Roll back this run and delete its materialized files from the game folder?",
            "Clean",
            ConfirmAction::CleanRunRecord(record),
        );
    }

    pub(super) fn request_cleanup_finished(&mut self) {
        self.request_confirm(
            "Clean finished runs",
            "Roll back every finished run and delete its materialized files from the game folder?",
            "Clean finished",
            ConfirmAction::CleanFinishedRuns,
        );
    }
}

impl SanAndreasModUi {
    pub(super) fn analyze_package(&mut self, ctx: &egui::Context) {
        let trimmed_package = self.import_path_input.trim();
        let package = PathBuf::from(trimmed_package);
        if package.as_os_str().is_empty() {
            self.status = "package path is required".to_string(); // literal: allow external interface text or file-format spelling
            return;
        }
        let game_root = self.game_root();
        // Listing/reading an archive can be slow; run it off the UI thread.
        self.spawn_task(ctx, "reviewing package", move || {
            TaskResult::Analyze(analyze_package(&package, &game_root))
        });
    }

    pub(super) fn apply_analysis(&mut self, result: Result<PackageReport, AppError>) {
        match result {
            Ok(report) => {
                self.last_error = None;
                self.readme_proposals = readme_proposals_from_report(&report);
                self.analysis_summary = Some(format!(
                    "{} entries, {} install candidates, {} readmes, {} readme proposals, {} risks",
                    report.entries.len(),
                    report.install_candidates.len(),
                    report.readmes.len(),
                    self.readme_proposals.len(),
                    report.risks.len()
                ));
                self.status = format!(
                    "{} entries, {} install candidates, {} readmes, {} risks",
                    report.entries.len(),
                    report.install_candidates.len(),
                    report.readmes.len(),
                    report.risks.len()
                );
            }
            Err(err) => {
                self.readme_proposals.clear();
                self.analysis_summary = None;
                self.record_error(err);
            }
        }
    }
}

impl SanAndreasModUi {
    /// The `mod.json` for the package currently under review, if that package has
    /// been imported into the library. `None` when no package is entered.
    pub(super) fn reviewed_mod_config_path(&self) -> Option<PathBuf> {
        let package = self.import_path_input.trim();
        if package.is_empty() {
            return None;
        }
        let id = package_id(&PathBuf::from(package));
        Some(
            state_directory(&self.game_root())
                .join("mods")
                .join(id)
                .join("mod.json"),
        )
    }

    /// Promote a readme "copy source -> target" proposal into a real install root
    /// on the imported mod's config. This is how a below-auto-threshold proposal
    /// (needs-review / warning) gets applied without hand-editing JSON.
    pub(super) fn accept_readme_proposal(&mut self, proposal: &ReadmeProposal) {
        let Some(config_path) = self.reviewed_mod_config_path() else {
            self.status = "review a package before accepting a proposal".to_string();
            return;
        };
        if !config_path.exists() {
            self.status = "import this package to the library first, then accept".to_string();
            return;
        }
        let (Some(source), Some(target)) = (&proposal.source, &proposal.target) else {
            self.status = "this proposal has no concrete source and target to apply".to_string();
            return;
        };
        let package_id = package_id(&PathBuf::from(self.import_path_input.trim()));
        let root = readme_copy_install_root(source, target, &package_id);
        // Gate the one-click accept through the same containment check the manual
        // editor uses, so a garbled readme source cannot land an escaping path in
        // the config that only fails much later at install time.
        if let Err(err) = validate_install_root(&root) {
            self.record_error(err);
            return;
        }
        match append_mod_config_install_root(&config_path, &root) {
            Ok(true) => self.set_action_result(
                "added install root from readme",
                Ok(format!("{} -> {}", root.source, root.target)),
            ),
            Ok(false) => {
                self.status = "that install root is already in the mod config".to_string()
            }
            Err(err) => self.record_error(err),
        }
    }
}

fn readme_proposals_from_report(report: &PackageReport) -> Vec<ReadmeProposal> {
    report
        .readme_instructions
        .iter()
        .map(readme_proposal_from_instruction)
        .collect()
}

fn readme_proposal_from_instruction(instruction: &ReadmeInstruction) -> ReadmeProposal {
    let source = instruction.source.as_deref().unwrap_or("unknown source");
    let target = instruction.target.as_deref().unwrap_or("unknown target");
    ReadmeProposal {
        action: instruction.action.to_string(),
        proposed_install: format!("{source} -> {target}"),
        evidence: instruction.text.clone(),
        source_readme: instruction.source_readme.clone(),
        line_number: instruction.line_number,
        confidence: instruction.confidence,
        review_state: readme_proposal_state(instruction),
        normalized_text: instruction.normalized_text.clone(),
        reasons: instruction.confidence_reasons.clone(),
        source: instruction.source.clone(),
        target: instruction.target.clone(),
    }
}

fn readme_proposal_state(instruction: &ReadmeInstruction) -> ReadmeProposalState {
    if matches!(instruction.action, ReadmeAction::Copy) && instruction.confidence >= 0.85 {
        ReadmeProposalState::AutoSelected
    } else if instruction.confidence >= 0.60 {
        ReadmeProposalState::NeedsReview
    } else {
        ReadmeProposalState::WarningOnly
    }
}

fn remember_pending_run(
    game_root: &Path,
    journal: &Path,
    pid: Option<u32>,
) -> Result<(), AppError> {
    let dir = pending_runs_directory(game_root);
    fs::create_dir_all(&dir)?;
    let path = pending_record_path(game_root, journal);
    let mut file = fs::File::create(path)?;
    writeln!(file, "journal={}", journal.display())?;
    if let Some(pid) = pid {
        writeln!(file, "pid={pid}")?;
    }
    writeln!(file, "created_unix={}", unix_now())?;
    Ok(())
}

fn materialize_and_launch_profile(
    game_root: &Path,
    profile: &str,
) -> Result<(PathBuf, std::process::Child), AppError> {
    let (launch_args, launch_env) = profile_launch_settings(game_root, profile)?;
    let journal = materialize_profile_for_run(game_root, profile)?;
    remember_pending_run(game_root, &journal, None)?;
    match launch_game_executable(game_root, &launch_args, &launch_env) {
        Ok(child) => {
            remember_pending_run(game_root, &journal, Some(child.id()))?;
            Ok((journal, child))
        }
        Err(launch_error) => {
            let rollback_result = rollback_journal(&journal, game_root)
                .and_then(|_| forget_pending_run(game_root, &journal));
            match rollback_result {
                Ok(()) => Err(launch_error),
                Err(rollback_error) => Err(AppError::Usage(format!(
                    "failed to launch game: {launch_error}; rollback also failed: {rollback_error}; journal: {}",
                    journal.display()
                ))),
            }
        }
    }
}

fn forget_pending_run(game_root: &Path, journal: &Path) -> Result<(), AppError> {
    let path = pending_record_path(game_root, journal);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn cleanup_journal_for_game_root(game_root: &Path, journal: &Path) -> Result<(), AppError> {
    validate_pending_journal(game_root, journal)
        .and_then(|_| rollback_journal(journal, game_root))
        .and_then(|_| forget_pending_run(game_root, journal))
}

fn load_pending_run_records(game_root: &Path) -> Result<Vec<PendingRunRecord>, AppError> {
    let dir = pending_runs_directory(game_root);
    let mut records = Vec::new();
    if !dir.exists() {
        return Ok(records);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(record) = read_pending_run_record(&path)? {
            records.push(record);
        }
    }
    records.sort_by(|a, b| a.journal.cmp(&b.journal));
    Ok(records)
}

fn read_pending_run_record(path: &Path) -> Result<Option<PendingRunRecord>, AppError> {
    let text = fs::read_to_string(path)?;
    let mut journal = None;
    let mut pid = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("journal=") {
            journal = Some(PathBuf::from(value));
        } else if let Some(value) = line.strip_prefix("pid=") {
            pid = value.parse::<u32>().ok();
        }
    }
    let Some(journal) = journal else {
        return Ok(None);
    };
    Ok(Some(classify_pending_run_record(path, journal, pid)))
}

fn classify_pending_run_record(
    record_path: &Path,
    journal: PathBuf,
    pid: Option<u32>,
) -> PendingRunRecord {
    let game_root = record_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_GAME_ROOT));
    match validate_pending_journal(&game_root, &journal) {
        Ok(()) => {
            let (status, detail) = match pid {
                Some(pid) if process_is_running(pid) => (
                    PendingRunStatus::Running,
                    format!("process {pid} is still running"),
                ),
                Some(pid) => (
                    PendingRunStatus::Stale,
                    format!("process {pid} is not running"),
                ),
                None => (
                    PendingRunStatus::Unknown,
                    "no process id was recorded".to_string(),
                ),
            };
            PendingRunRecord {
                journal,
                pid,
                status,
                detail,
            }
        }
        Err(err) => PendingRunRecord {
            journal,
            pid,
            status: PendingRunStatus::Invalid,
            detail: err.to_string(),
        },
    }
}

fn validate_pending_journal(game_root: &Path, journal: &Path) -> Result<(), AppError> {
    let journals_root = state_directory(game_root).join("journals");
    let root = journals_root.canonicalize()?;
    let journal_path = journal.canonicalize()?;
    if !journal_path.starts_with(&root) {
        return Err(AppError::Usage(format!(
            "pending journal must be under manager journals: {}",
            journals_root.display()
        )));
    }
    let content = fs::read_to_string(journal)?;
    if !content.lines().any(|line| line == "mode=ephemeral-run") {
        return Err(AppError::Usage(format!(
            "pending journal is not an ephemeral run journal: {}",
            journal.display()
        )));
    }
    if !content
        .lines()
        .any(|line| line.starts_with("new=") || line.starts_with("backup="))
    {
        return Err(AppError::Usage(format!(
            "pending journal has no file changes to roll back: {}",
            journal.display()
        )));
    }
    Ok(())
}

fn pending_runs_directory(game_root: &Path) -> PathBuf {
    state_directory(game_root).join("pending-runs")
}

fn pending_record_path(game_root: &Path, journal: &Path) -> PathBuf {
    let name = journal
        .file_stem()
        .and_then(OsStr::to_str)
        .map(safe_name)
        .unwrap_or_else(|| format!("pending-{}", unix_now()));
    pending_runs_directory(game_root).join(format!("{name}.pending"))
}

fn process_is_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        windows_process_is_running(pid)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn pending_run_matches_current_session(
    record: &PendingRunRecord,
    current_pid: Option<u32>,
) -> bool {
    current_pid.is_some() && record.status == PendingRunStatus::Running && record.pid == current_pid
}

#[cfg(windows)]
fn windows_process_is_running(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    let output = Command::new("tasklist").arg("/FI").arg(filter).output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().any(|line| line.contains(&pid.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_run_records_survive_reload_and_can_be_forgotten() {
        let game_root = test_root("pending_run_records");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("run-default-123.journal");
        fs::create_dir_all(journal.parent().unwrap()).unwrap();
        let materialized = game_root.join("CLEO").join("gravityfix.cs");
        fs::create_dir_all(materialized.parent().unwrap()).unwrap();
        fs::write(&materialized, "script").unwrap();
        fs::write(
            &journal,
            format!(
                "version=1\nmode=ephemeral-run\nnew={}\n",
                escape_value(&materialized.display().to_string())
            ),
        )
        .unwrap();

        remember_pending_run(&game_root, &journal, Some(1234)).unwrap();
        let records = load_pending_run_records(&game_root).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].journal, journal);
        assert_eq!(records[0].pid, Some(1234));

        forget_pending_run(&game_root, &journal).unwrap();
        assert!(load_pending_run_records(&game_root).unwrap().is_empty());
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn stale_pending_runs_are_bulk_cleaned_after_validation() {
        let game_root = test_root("stale_pending_cleanup");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("run-default-456.journal");
        let materialized = game_root.join("modloader").join("test").join("file.txt");
        fs::create_dir_all(journal.parent().unwrap()).unwrap();
        fs::create_dir_all(materialized.parent().unwrap()).unwrap();
        fs::write(&materialized, "payload").unwrap();
        fs::write(
            &journal,
            format!(
                "version=1\nmode=ephemeral-run\nnew={}\n",
                escape_value(&materialized.display().to_string())
            ),
        )
        .unwrap();
        remember_pending_run(&game_root, &journal, Some(u32::MAX)).unwrap();

        let mut ui = SanAndreasModUi::new(game_root.clone(), test_preferences());
        assert_eq!(ui.pending_runs.len(), 1);
        assert_eq!(ui.pending_runs[0].status, PendingRunStatus::Stale);

        ui.cleanup_stale_pending_runs();

        assert!(!materialized.exists());
        assert!(load_pending_run_records(&game_root).unwrap().is_empty());
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn pending_run_watcher_auto_cleans_finished_pid_backed_runs() {
        let game_root = test_root("pending_run_watcher_cleanup");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("run-default-789.journal");
        let materialized = game_root.join("CLEO").join("watcher.cs");
        fs::create_dir_all(journal.parent().unwrap()).unwrap();
        fs::create_dir_all(materialized.parent().unwrap()).unwrap();
        fs::write(&materialized, "script").unwrap();
        fs::write(
            &journal,
            format!(
                "version=1\nmode=ephemeral-run\nnew={}\n",
                escape_value(&materialized.display().to_string())
            ),
        )
        .unwrap();
        remember_pending_run(&game_root, &journal, Some(u32::MAX)).unwrap();

        let mut ui = SanAndreasModUi::new(game_root.clone(), test_preferences());
        ui.last_pending_watch = Instant::now() - Duration::from_secs(3);
        ui.tick_pending_run_watcher();

        assert!(!materialized.exists());
        assert!(load_pending_run_records(&game_root).unwrap().is_empty());
        assert!(ui.status.contains("auto-cleaned 1"));
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn readme_proposals_expose_confidence_review_states() {
        let high = test_readme_instruction(ReadmeAction::Copy, 0.90);
        let medium = test_readme_instruction(ReadmeAction::Copy, 0.72);
        let low = test_readme_instruction(ReadmeAction::Copy, 0.40);

        assert_eq!(
            readme_proposal_from_instruction(&high).review_state,
            ReadmeProposalState::AutoSelected
        );
        assert_eq!(
            readme_proposal_from_instruction(&medium).review_state,
            ReadmeProposalState::NeedsReview
        );
        assert_eq!(
            readme_proposal_from_instruction(&low).review_state,
            ReadmeProposalState::WarningOnly
        );
    }

    #[test]
    fn launch_guard_allows_only_current_session_running_pending_record() {
        let current = PendingRunRecord {
            journal: PathBuf::from("current.journal"),
            pid: Some(42),
            status: PendingRunStatus::Running,
            detail: String::new(),
        };
        let external = PendingRunRecord {
            journal: PathBuf::from("external.journal"),
            pid: Some(99),
            status: PendingRunStatus::Running,
            detail: String::new(),
        };
        let stale = PendingRunRecord {
            journal: PathBuf::from("stale.journal"),
            pid: Some(42),
            status: PendingRunStatus::Stale,
            detail: String::new(),
        };

        assert!(pending_run_matches_current_session(&current, Some(42)));
        assert!(!pending_run_matches_current_session(&external, Some(42)));
        assert!(!pending_run_matches_current_session(&stale, Some(42)));
        assert!(!pending_run_matches_current_session(&current, None));
    }

    #[test]
    fn zero_pid_is_not_running() {
        assert!(!process_is_running(0));
    }

    #[test]
    fn install_root_validation_rejects_escapes_and_empties() {
        let valid = ModInstallRootJson {
            source: "files/CLEO".to_string(),
            target: "CLEO".to_string(),
            kind: "cleo".to_string(),
            enabled: true,
            optional: false,
        };
        assert!(validate_install_root(&valid).is_ok());

        let empty_source = ModInstallRootJson {
            source: "   ".to_string(),
            ..valid.clone()
        };
        assert!(validate_install_root(&empty_source).is_err());

        let escaping_source = ModInstallRootJson {
            source: "../evil".to_string(),
            ..valid.clone()
        };
        assert!(validate_install_root(&escaping_source).is_err());

        let escaping_target = ModInstallRootJson {
            target: "../../outside".to_string(),
            ..valid.clone()
        };
        assert!(validate_install_root(&escaping_target).is_err());
    }

    #[test]
    fn readme_accept_root_is_validated_like_manual_edits() {
        // A well-formed readme copy yields a root that passes validation.
        let good = readme_copy_install_root("files/CLEO", "CLEO", "gravity_fix");
        assert!(validate_install_root(&good).is_ok());
        // A traversal source survives normalize_path but is rejected by the same
        // check the manual editor uses, so accept and edit stay consistent.
        let escaping = readme_copy_install_root("../../evil", "CLEO", "gravity_fix");
        assert!(validate_install_root(&escaping).is_err());
    }

    fn test_readme_instruction(action: ReadmeAction, confidence: f32) -> ReadmeInstruction {
        ReadmeInstruction {
            source_readme: "README.txt".to_string(),
            line_number: 8,
            action,
            source: Some("CLEO".to_string()),
            target: Some("CLEO".to_string()),
            confidence,
            text: "Copy CLEO to CLEO.".to_string(),
            normalized_text: "copy cleo to cleo".to_string(),
            confidence_reasons: vec!["test reason".to_string()],
        }
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

    fn test_preferences() -> crate::ui::preferences::UiPreferences {
        crate::ui::preferences::UiPreferences::default()
    }

    fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}
