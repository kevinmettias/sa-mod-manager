use crate::prelude::*;

use super::content::{
    ModLoaderManagedProfileRender, modloader_active_profile, modloader_folder_from_target,
    modloader_managed_profile_name, modloader_priority_limit, render_modloader_managed_profile,
    spread_priority,
};
use super::copy_journal::{
    apply_copy_tree_with_journal, apply_generated_file_with_journal, sync_journal,
};
use super::rollback::rollback_journal;

pub(crate) fn prepare_run(game_root: &Path, profile_name: &str) -> Result<(), AppError>
{
    let (mut launch_args, launch_env) = profile_launch_settings(game_root, profile_name)?;
    let journal_path = materialize_profile_for_run(game_root, profile_name)?;
    // Activate the manager-owned ModLoader profile written during materialize so
    // ModLoader applies this profile's per-folder priority natively for the run â€”
    // unless the user's own launch args already choose a ModLoader mode. `-nomods`,
    // `-mod`, and `-modprof` are mutually exclusive (nomods > modprof > mod), so a
    // second `-modprof` would be dead; respect the explicit choice instead.
    if !should_launch_args_select_modloader_mode(&launch_args)
    {
        if let Some(managed) = modloader_run_profile(game_root, profile_name)
        {
            launch_args.push("-modprof".to_string());
            launch_args.push(managed);
        }
    }
    let started_unix = unix_now();
    let status_result = launch_game_and_wait_from_arguments(game_root, &launch_args, &launch_env);
    // Record how the launch actually went before rolling the run back, so a
    // failed launch is distinguishable from a real play session in telemetry.
    let outcome_context = RunOutcomeContext {
        game_root,
        journal_path: &journal_path,
        profile_name,
        launch_args: &launch_args,
        started_unix,
    };
    record_run_outcome(outcome_context, &status_result);
    let launch_result = interpret_launch_status(status_result);
    let rollback_result = rollback_journal(&journal_path, game_root);
    launch_result?;
    return rollback_result;
}

struct RunOutcomeContext<'a>
{
    game_root: &'a Path,
    journal_path: &'a Path,
    profile_name: &'a str,
    launch_args: &'a [String],
    started_unix: u64,
}
fn launch_game_and_wait_from_arguments(
    game_root: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<ExitStatus, AppError>
{
    let exe = game_executable(game_root).ok_or_else(|| {
        AppError::Usage(format!(
            "game executable not found under {}",
            game_root.display()
        ))
    })?;
    let status = Command::new(&exe)
        .current_dir(game_root)
        .args(args)
        .envs(env)
        .status()?;
    return Ok(status);
}

fn game_executable(game_root: &Path) -> Option<PathBuf>
{
    return game_executable_path(game_root);
}

fn record_run_outcome(
    context: RunOutcomeContext<'_>,
    status_result: &Result<ExitStatus, AppError>,
)
{
    let finished_unix = unix_now();
    let (result, exit_code) = match status_result {
        Ok(status) if status.success() => (RUN_RESULT_SUCCESS, status.code()),
        Ok(status) => (RUN_RESULT_GAME_ERROR, status.code()),
        Err(_) => (RUN_RESULT_LAUNCH_FAILED, None),
    };
    let outcome = RunOutcome {
        version: 1,
        txid: txid_from_journal(context.journal_path),
        profile: context.profile_name.to_string(),
        result: result.to_string(),
        exit_code,
        duration_ms: Some(
            finished_unix
                .saturating_sub(context.started_unix)
                .saturating_mul(1000), // literal: allow domain threshold is documented by the surrounding code
        ),
        launch_args: context.launch_args.to_vec(),
        started_unix: context.started_unix,
        finished_unix,
    };
    // Telemetry is best-effort: a failed write must not fail the run itself.
    if let Err(err) = write_run_outcome(&state_directory(context.game_root), &outcome)
    {
        log_warn!("could not record_log_message_from_arguments run outcome: {err}");
    }
}

/// Map a finished launch to the run's result. A non-zero game exit and a spawn
/// failure both surface as errors, matching the previous behavior.
fn interpret_launch_status(status_result: Result<ExitStatus, AppError>) -> Result<(), AppError>
{
    return match status_result
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(AppError::Usage(format!(
            "game exited with status: {status}"
        ))),
        Err(err) => Err(err),
    };
}

pub(crate) fn materialize_profile_for_run(
    game_root: &Path,
    profile_name: &str,
) -> Result<PathBuf, AppError>
{
    ensure_gta_install(game_root)?;
    ensure_state(game_root)?;
    let profile = read_enabled_profile(game_root, profile_name)?;
    let run_state = create_run_state(game_root, profile_name)?;
    let mut journal = fs::File::create(&run_state.journal_path)?;
    write_run_journal_header(&mut journal, RunJournalHeader { txid: &run_state.txid, profile_name })?;

    let materialize_result = apply_profile_mods_for_run(
        &profile.mods,
        game_root,
        &run_state.backup_root,
        &mut journal,
    );
    if let Err(err) = materialize_result
    {
        return Err(rollback_failed_materialization(
            game_root,
            &run_state.journal_path,
            journal,
            err,
        ));
    }

    // Reflect the profile's load order into ModLoader's own per-folder priority,
    // so the order the user set actually governs which mod wins at runtime â€” for
    // sandboxed modloader mods, copy order into distinct folders decides nothing.
    // Journaled like any other write, so an ephemeral run's rollback restores the
    // prior modloader.ini.
    let priority_context = ModLoaderPriorityRunContext {
        entries: &profile.mods,
        profile_name,
        game_root,
        backup_root: &run_state.backup_root,
    };
    if let Err(err) = write_modloader_priorities_for_run(priority_context, &mut journal)
    {
        return Err(rollback_failed_materialization(
            game_root,
            &run_state.journal_path,
            journal,
            err,
        ));
    }

    sync_journal(&mut journal)?;
    log_info!("prepared ephemeral run: {profile_name}");
    log_info!("journal: {}", run_state.journal_path.display());
    return Ok(run_state.journal_path);
}

fn read_enabled_profile(game_root: &Path, profile_name: &str) -> Result<ProfileJson, AppError>
{
    let profile_path = state_directory(game_root)
        .join("profiles")
        .join(format!("{profile_name}.json"));
    let mut profile = read_profile_json(&profile_path)?;
    profile.mods.retain(|entry| entry.enabled);
    profile.mods.sort_by(|left, right| {
        left.load_order
            .cmp(&right.load_order)
            .then_with(|| left.id.cmp(&right.id))
    });
    validate_profile_mods(&profile)?;
    return Ok(profile);
}

fn validate_profile_mods(profile: &ProfileJson) -> Result<(), AppError>
{
    let mut ids = BTreeSet::new();
    let mut configs = BTreeSet::new();
    for entry in &profile.mods
    {
        if !ids.insert(entry.id.clone())
        {
            return Err(AppError::Usage(format!(
                "profile `{}` contains duplicate enabled mod id `{}`",
                profile.name, entry.id
            )));
        }
        let config = normalize_path(&entry.config.display().to_string());
        if !configs.insert(config.clone())
        {
            return Err(AppError::Usage(format!(
                "profile `{}` contains duplicate enabled mod config `{config}`",
                profile.name
            )));
        }
        if !entry.config.exists()
        {
            return Err(AppError::Usage(format!(
                "profile `{}` references missing mod config for `{}`: {}",
                profile.name,
                entry.id,
                entry.config.display()
            )));
        }
    }
    return Ok(());
}

fn create_run_state(game_root: &Path, profile_name: &str) -> Result<RunApplyState, AppError>
{
    let txid = format!("run-{}-{}", safe_name(profile_name), unix_now());
    let journal_path = state_directory(game_root)
        .join("journals")
        .join(format!("{txid}.journal"));
    let backup_root = state_directory(game_root).join("backups").join(&txid);
    fs::create_dir_all(&backup_root)?;
    return Ok(RunApplyState {
        txid,
        journal_path,
        backup_root,
    });
}

fn write_run_journal_header(journal: &mut fs::File, header: RunJournalHeader<'_>) -> Result<(), AppError>
{
    let txid = header.txid;
    let profile_name = header.profile_name;
    writeln!(journal, "version=1")?;
    writeln!(journal, "txid={}", escape_value(txid))?;
    writeln!(journal, "profile={}", escape_value(profile_name))?;
    writeln!(journal, "mode=ephemeral-run")?;
    writeln!(journal, "created_unix={}", unix_now())?;
    return Ok(());
}

fn apply_profile_mods_for_run(
    entries: &[ProfileModEntry],
    game_root: &Path,
    backup_root: &Path,
    journal: &mut fs::File,
) -> Result<(), AppError>
{
    for entry in entries
    {
        write_profile_mod_start(journal, entry)?;
        apply_profile_mod_for_run(entry, game_root, backup_root, journal)?;
    }
    return Ok(());
}

struct RunJournalHeader<'a>
{
    txid: &'a str,
    profile_name: &'a str,
}

fn write_profile_mod_start(
    journal: &mut fs::File,
    entry: &ProfileModEntry,
) -> Result<(), AppError>
{
    writeln!(
        journal,
        "profile_mod={}|{}|{}",
        escape_value(&entry.id),
        entry.load_order,
        escape_value(&entry.config.display().to_string())
    )?;
    return Ok(());
}

fn apply_profile_mod_for_run(
    entry: &ProfileModEntry,
    game_root: &Path,
    backup_root: &Path,
    journal: &mut fs::File,
) -> Result<(), AppError>
{
    let config = read_mod_config_json(&entry.config)?;
    if !config.enabled
    {
        return Ok(());
    }
    let staging_root = staging_root_for_mod_config(&config, game_root)?;
    let roots = enabled_install_roots(config.install_roots, &entry.root_overrides);
    let mut run_context = RunInstallContext {
        game_root,
        backup_root,
        journal,
    };
    for root in roots
    {
        apply_run_install_root(&root, &staging_root, &mut run_context)?;
    }
    return Ok(());
}

fn staging_root_for_mod_config(
    config: &ModConfigJson,
    game_root: &Path,
) -> Result<PathBuf, AppError>
{
    return if let Some(source_root) = &config.source_root
    {
        Ok(source_root.clone())
    }
    else if config.package.is_dir()
    {
        Ok(config.package.clone())
    }
    else
    {
        extract_archive_to_named_staging(&config.package, game_root, &config.id)
    };
}

fn enabled_install_roots(
    mut roots: Vec<ModInstallRootJson>,
    overrides: &BTreeMap<String, ProfileRootOverride>,
) -> Vec<ModInstallRootJson>
{
    // A per-profile override for a root's `source` wins over the mod's own
    // config, letting a profile toggle a shared mod's roots or retarget them.
    roots.retain(|root| {
        overrides
            .get(&root.source)
            .and_then(|over| over.enabled)
            .unwrap_or(root.enabled)
    });
    for root in &mut roots
    {
        if let Some(target) = overrides
            .get(&root.source)
            .and_then(|over| over.target.clone())
        {
            root.target = target;
        }
    }
    roots.sort_by(|left, right| left.source.cmp(&right.source));
    return roots;
}

fn apply_run_install_root(
    root: &ModInstallRootJson,
    staging_root: &Path,
    context: &mut RunInstallContext,
) -> Result<(), AppError>
{
    if root.kind.eq_ignore_ascii_case("bootstrap")
    {
        write_blocked_bootstrap(context.journal, BlockedBootstrapEntry { source: &root.source, target: &root.target })?;
        return Ok(());
    }

    let source_abs = staging_root.join(path_from_package_root(&root.source)?);
    if !source_abs.exists()
    {
        write_missing_source(context.journal, &source_abs)?;
        return Ok(());
    }
    let target_root = context
        .game_root
        .join(path_from_package_root(&root.target)?);
    let mut copy_context = CopyJournalContext {
        game_root: context.game_root,
        backup_root: context.backup_root,
        journal: context.journal,
    };
    return apply_copy_tree_with_journal(&source_abs, &target_root, &mut copy_context);
}

fn write_blocked_bootstrap(journal: &mut fs::File, entry: BlockedBootstrapEntry<'_>) -> Result<(), AppError>
{
    let source = entry.source;
    let target = entry.target;
    writeln!(
        journal,
        "blocked_bootstrap={}|{}",
        escape_value(source),
        escape_value(target)
    )?;
    return Ok(());
}

struct BlockedBootstrapEntry<'a>
{
    source: &'a str,
    target: &'a str,
}

fn write_missing_source(journal: &mut fs::File, source_abs: &Path) -> Result<(), AppError>
{
    writeln!(
        journal,
        "missing_source={}",
        escape_value(&source_abs.display().to_string())
    )?;
    return Ok(());
}

/// Roll back a partially-materialized run and return the error to surface.
///
/// Returns the original `materialize_error` when rollback succeeds, or a
/// combined error when rollback itself fails. Returning [`AppError`] directly
/// (rather than `Result`) makes "this function always yields an error" a
/// type-level guarantee, so the caller needs no `unreachable!`.
fn rollback_failed_materialization(
    game_root: &Path,
    journal_path: &Path,
    mut journal: fs::File,
    materialize_error: AppError,
) -> AppError
{
    let flush_result = journal.flush();
    drop(journal);

    let rollback_result = match flush_result {
        Ok(()) => rollback_journal(journal_path, game_root),
        Err(err) => Err(AppError::Io(err)),
    };

    return match rollback_result
    {
        Ok(()) => materialize_error,
        Err(rollback_error) => AppError::Usage(format!(
            "failed to materialize profile: {materialize_error}; rollback also failed: {rollback_error}; journal: {}",
            journal_path.display()
        )),
    };
}

/// Express the profile's load order as a native ModLoader profile in
/// `modloader/modloader.ini`, so ModLoader's own per-folder priority â€” the thing
/// that actually decides which sandboxed mod wins a shared asset â€” matches the
/// order the user set. Written into a manager-owned `SAMM_<profile>` profile
/// (inheriting the user's `Default`) and activated per launch with `-modprof`, so
/// the user's own ModLoader config is never disturbed. A no-op when the profile
/// has no modloader-sandboxed mods or the file already reflects the same order.
struct ModLoaderPriorityRunContext<'a>
{
    entries: &'a [ProfileModEntry],
    profile_name: &'a str,
    game_root: &'a Path,
    backup_root: &'a Path,
}
