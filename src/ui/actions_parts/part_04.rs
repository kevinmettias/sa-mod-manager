
impl SanAndreasModUi
{
    pub(super) fn analyze_package(&mut self, ctx: &egui::Context)
    {
        let trimmed_package = self.inputs.import_path.trim();
        let package = PathBuf::from(trimmed_package);
        if package.as_os_str().is_empty()
        {
            self.status = "package path is required".to_string();
            return;
        }
        let game_root = self.game_root();
        // Listing/reading an archive can be slow; run it off the UI thread.
        self.spawn_task(ctx, "reviewing package", move || {
            let analysis_result = analyze_package(&package, &game_root);
            TaskResult::Analyze(analysis_result)
        });
    }

    pub(super) fn apply_analysis(&mut self, result: Result<PackageReport, AppError>)
    {
        match result
        {
            Ok(report) => self.after_analysis_success(report),
            Err(err) => {
                self.readme_proposals.clear();
                self.analysis_summary = None;
                self.record_error(err);
            }
        }
    }
    fn after_analysis_success(&mut self, report: PackageReport)
    {
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
}

impl SanAndreasModUi
{
    /// The `mod.json` for the package currently under review, if that package has
    /// been imported into the library. `None` when no package is entered.
    pub(super) fn reviewed_mod_config_path(&self) -> Option<PathBuf>
    {
        let package = self.inputs.import_path.trim();
        if package.is_empty()
        {
            return None;
        }
        let id = package_id(&PathBuf::from(package));
        return Some(
            state_directory(&self.game_root())
                .join("mods")
                .join(id)
                .join("mod.json"),
        );
    }

    /// Promote a readme "copy source -> target" proposal into a real install root
    /// on the imported mod's config. This is how a below-auto-threshold proposal
    /// (needs-review / warning) gets applied without hand-editing JSON.
    pub(super) fn accept_readme_proposal(&mut self, proposal: &ReadmeProposal)
    {
        let Some(config_path) = self.reviewed_mod_config_path() else {
            self.status = "review a package before accepting a proposal".to_string();
            return;
        };
        if !config_path.exists()
        {
            self.status = "import this package to the library first, then accept".to_string();
            return;
        }
        let (Some(source), Some(target)) = (&proposal.source, &proposal.target) else {
            self.status = "this proposal has no concrete source and target to apply".to_string();
            return;
        };
        let package_id = package_id(&PathBuf::from(self.inputs.import_path.trim()));
        let root = readme_copy_install_root(ReadmeCopyInstallRoot { source: source, target: target, package_id: &package_id });
        // Gate the one-click accept through the same containment check the manual
        // editor uses, so a garbled readme source cannot land an escaping path in
        // the config that only fails much later at install time.
        if let Err(err) = validate_install_root(&root)
        {
            self.record_error(err);
            return;
        }
        match append_mod_config_install_root(&config_path, &root)
        {
            Ok(true) => self.set_action_result(
                "added install root from readme",
                Ok(format!("{} -> {}", root.source, root.target)),
            ),
            Ok(false) => self.status = "that install root is already in the mod config".to_string(),
            Err(err) => self.record_error(err),
        }
    }
}

fn readme_proposals_from_report(report: &PackageReport) -> Vec<ReadmeProposal>
{
    // Resolve the thresholds once, then classify every instruction against them.
    let thresholds = crate::settings::ReadmeThresholds::from_settings();
    return report
        .readme_instructions
        .iter()
        .map(|instruction| readme_proposal_from_instruction(instruction, thresholds))
        .collect();
}

fn readme_proposal_from_instruction(
    instruction: &ReadmeInstruction,
    thresholds: crate::settings::ReadmeThresholds,
) -> ReadmeProposal
{
    let source = instruction.source.as_deref().unwrap_or("unknown source");
    let target = instruction.target.as_deref().unwrap_or("unknown target");
    return ReadmeProposal {
        action: instruction.action.to_string(),
        proposed_install: format!("{source} -> {target}"),
        evidence: instruction.text.clone(),
        source_readme: instruction.source_readme.clone(),
        line_number: instruction.line_number,
        confidence: instruction.confidence,
        review_state: readme_proposal_state(instruction, thresholds),
        normalized_text: instruction.normalized_text.clone(),
        reasons: instruction.confidence_reasons.clone(),
        source: instruction.source.clone(),
        target: instruction.target.clone(),
    };
}

fn readme_proposal_state(
    instruction: &ReadmeInstruction,
    thresholds: crate::settings::ReadmeThresholds,
) -> ReadmeProposalState
{
    return if matches!(instruction.action, ReadmeAction::Copy) && instruction.confidence >= thresholds.auto
    {
        ReadmeProposalState::AutoSelected
    }
    else if instruction.confidence >= thresholds.review
    {
        ReadmeProposalState::NeedsReview
    }
    else
    {
        ReadmeProposalState::WarningOnly
    };
}

fn materialize_and_launch_profile(
    game_root: &Path,
    profile: &str,
) -> Result<(ActiveRun, std::process::Child), AppError>
{
    let (launch_args, launch_env) = profile_launch_settings(game_root, profile)?;
    let journal = materialize_profile_for_run(game_root, profile)?;
    let started_unix = unix_now();
    remember_pending_run(game_root, &journal, None)?;
    return match launch_game_executable(game_root, &launch_args, &launch_env)
    {
        Ok(child) => {
            remember_pending_run(game_root, &journal, Some(child.id()))?;
            let active = ActiveRun {
                txid: txid_from_journal(&journal),
                journal,
                profile: profile.to_string(),
                launch_args,
                started_unix,
            };
            Ok((active, child))
        }
        Err(launch_error) => {
            // The executable never started: record a launch-failed outcome so
            // this rolled-back attempt is not later counted as a run.
            record_launch_failed_outcome(LaunchFailedOutcomeContext {
                game_root,
                journal: &journal,
                profile,
                launch_args: &launch_args,
                started_unix,
            });
            let rollback_result = rollback_journal(&journal, game_root)
                .and_then(|_| forget_pending_run(game_root, &journal));
            match rollback_result
            {
                Ok(()) => Err(launch_error),
                Err(rollback_error) => Err(AppError::Usage(format!(
                    "failed to launch game: {launch_error}; rollback also failed: {rollback_error}; journal: {}",
                    journal.display()
                ))),
            }
        }
    };
}

fn remember_pending_run(
    game_root: &Path,
    journal: &Path,
    pid: Option<u32>,
) -> Result<(), AppError>
{
    let dir = pending_runs_directory(game_root);
    fs::create_dir_all(&dir)?;
    let path = pending_record_path(game_root, journal);
    let mut file = fs::File::create(path)?;
    writeln!(file, "journal={}", journal.display())?;
    if let Some(pid) = pid
    {
        writeln!(file, "pid={pid}")?;
    }
    writeln!(file, "created_unix={}", unix_now())?;
    return Ok(());
}

struct LaunchFailedOutcomeContext<'a>
{
    game_root: &'a Path,
    journal: &'a Path,
    profile: &'a str,
    launch_args: &'a [String],
    started_unix: u64,
}

fn record_launch_failed_outcome(context: LaunchFailedOutcomeContext<'_>)
{
    let finished_unix = unix_now();
    let outcome = RunOutcome {
        version: 1,
        txid: txid_from_journal(context.journal),
        profile: context.profile.to_string(),
        result: RUN_RESULT_LAUNCH_FAILED.to_string(),
        exit_code: None,
        duration_ms: Some(
            finished_unix
                .saturating_sub(context.started_unix)
                .saturating_mul(MILLISECONDS_PER_SECOND),
        ),
        launch_args: context.launch_args.to_vec(),
        started_unix: context.started_unix,
        finished_unix,
    };
    if let Err(err) = write_run_outcome(&state_directory(context.game_root), &outcome)
    {
        log_warn!("could not record launch-failed outcome: {err}");
    }
}

fn cleanup_journal_for_game_root(game_root: &Path, journal: &Path) -> Result<(), AppError>
{
    return validate_pending_journal(game_root, journal)
        .and_then(|_| rollback_journal(journal, game_root))
        .and_then(|_| forget_pending_run(game_root, journal));
}

fn load_pending_run_records(game_root: &Path) -> Result<Vec<PendingRunRecord>, AppError>
{
    let dir = pending_runs_directory(game_root);
    let mut records = Vec::new();
    if !dir.exists()
    {
        return Ok(records);
    }
    for entry in fs::read_dir(dir)?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file()
        {
            continue;
        }
        if let Some(record) = read_pending_run_record(&path)?
        {
            records.push(record);
        }
    }
    records.sort_by(|a, b| a.journal.cmp(&b.journal));
    return Ok(records);
}

fn read_pending_run_record(path: &Path) -> Result<Option<PendingRunRecord>, AppError>
{
    // A pending-run record is a handful of `key=value` lines; cap the read so a
    // corrupt/oversized file can't dictate our memory use.
    let text = read_capped(path, README_ACCEPT_MAX_BYTES)?;
    let mut journal = None;
    let mut pid = None;
    for line in text.lines()
    {
        if let Some(value) = line.strip_prefix("journal=")
        {
            journal = Some(PathBuf::from(value));
        }
        else if let Some(value) = line.strip_prefix("pid=")
        {
            pid = value.parse::<u32>().ok();
        }
    }
    let Some(journal) = journal else {
        return Ok(None);
    };
    return Ok(Some(classify_pending_run_record(path, journal, pid)));
}

fn classify_pending_run_record(
    record_path: &Path,
    journal: PathBuf,
    pid: Option<u32>,
) -> PendingRunRecord
{
    let game_root = record_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(crate::settings::default_game_root);
    return match validate_pending_journal(&game_root, &journal)
    {
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
    };
}

fn pending_run_matches_current_session(
    record: &PendingRunRecord,
    current_pid: Option<u32>,
) -> bool
{
    return current_pid.is_some() && record.status == PendingRunStatus::Running && record.pid == current_pid;
}

fn forget_pending_run(game_root: &Path, journal: &Path) -> Result<(), AppError>
{
    let path = pending_record_path(game_root, journal);
    if path.exists()
    {
        fs::remove_file(path)?;
    }
    return Ok(());
}

fn validate_pending_journal(game_root: &Path, journal: &Path) -> Result<(), AppError>
{
    let journals_root = state_directory(game_root).join("journals");
    let root = journals_root.canonicalize()?;
    let journal_path = journal.canonicalize()?;
    if !journal_path.starts_with(&root)
    {
        return Err(AppError::Usage(format!(
            "pending journal must be under manager journals: {}",
            journals_root.display()
        )));
    }
    let content = read_capped(journal, RUN_OUTCOME_JOURNAL_MAX_BYTES)?;
    if !content.lines().any(|line| line == "mode=ephemeral-run")
    {
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
    return Ok(());
}

fn pending_runs_directory(game_root: &Path) -> PathBuf
{
    return state_directory(game_root).join("pending-runs");
}

fn pending_record_path(game_root: &Path, journal: &Path) -> PathBuf
{
    let name = journal
        .file_stem()
        .and_then(OsStr::to_str)
        .map(safe_name)
        .unwrap_or_else(|| format!("pending-{}", unix_now()));
    return pending_runs_directory(game_root).join(format!("{name}.pending"));
}

fn process_is_running(pid: u32) -> bool
{
    // Single source of truth: the same OpenProcess-based liveness check the
    // install lock uses, rather than a second copy that could drift from it.
    return crate::game_launch::process_is_running(pid);
}

#[cfg(test)]
mod tests
{
    include!("part_04_tests_01.rs");
}



