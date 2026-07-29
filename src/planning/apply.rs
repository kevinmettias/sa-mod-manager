use crate::prelude::*;

use super::copy_journal::{apply_copy_tree_with_journal, sync_journal};
use super::install_lock::{acquire_install_lock, release_install_lock};
use super::rollback::rollback_journal;

pub(crate) fn apply_install_plan(package: &Path, plan: &InstallPlan) -> Result<(), AppError>
{
    ensure_gta_install(&plan.game_root)?;
    if plan.operations.is_empty()
    {
        return Err(usage_error("install plan has no operations"));
    }

    let install_state = create_install_state(package, plan)?;
    // Claim the game folder before the destructive copy loop: blocks a second
    // concurrent install and refuses if a previous install is still unrecovered.
    acquire_install_lock(
        &plan.game_root,
        &install_state.txid,
        &install_state.journal_path,
    )?;
    let apply_result = apply_locked_install(package, plan, &install_state);
    return finish_install(&plan.game_root, &install_state, apply_result);
}

fn create_install_state(package: &Path, plan: &InstallPlan) -> Result<InstallApplyState, AppError>
{
    ensure_state(&plan.game_root)?;
    let txid = format!("{}-{}", plan.package_id, unix_now());
    let journal_path = state_directory(&plan.game_root)
        .join("journals")
        .join(format!("{txid}.journal"));
    let backup_root = state_directory(&plan.game_root).join("backups").join(&txid);
    fs::create_dir_all(&backup_root)?;
    let staging_root = staging_root_for_install(package, plan)?;
    return Ok(InstallApplyState {
        txid,
        journal_path,
        backup_root,
        staging_root,
    });
}

fn staging_root_for_install(package: &Path, plan: &InstallPlan) -> Result<PathBuf, AppError>
{
    return if package.is_dir() {
        Ok(package.to_path_buf())
    } else {
        extract_archive_to_named_staging(package, &plan.game_root, &plan.package_id)
    };
}

fn apply_locked_install(
    package: &Path,
    plan: &InstallPlan,
    install_state: &InstallApplyState,
) -> Result<(), AppError>
{
    let mut journal =
        fs::File::create(&install_state.journal_path).context("create install journal")?;
    write_install_journal_header(&mut journal, package, plan, &install_state.txid)?;

    for operation in &plan.operations
    {
        let mut copy_context = CopyJournalContext {
            game_root: &plan.game_root,
            backup_root: &install_state.backup_root,
            journal: &mut journal,
        };
        apply_install_operation(operation, &install_state.staging_root, &mut copy_context)?;
    }
    return sync_journal(&mut journal);
}
fn write_install_journal_header(
    journal: &mut fs::File,
    package: &Path,
    plan: &InstallPlan,
    txid: &str,
) -> Result<(), AppError>
{
    writeln!(journal, "version=1")?;
    writeln!(journal, "txid={}", escape_value(txid))?;
    writeln!(journal, "package_id={}", escape_value(&plan.package_id))?;
    writeln!(
        journal,
        "package={}",
        escape_value(&package.display().to_string())
    )?;
    writeln!(journal, "profile={}", escape_value(&plan.profile))?;
    writeln!(journal, "created_unix={}", unix_now())?;
    return Ok(());
}

fn apply_install_operation(
    operation: &InstallOperation,
    staging_root: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError>
{
    if matches!(operation.target_kind, TargetKind::Bootstrap)
    {
        writeln!(
            context.journal,
            "blocked_bootstrap={}|{}",
            escape_value(&operation.source_root),
            escape_value(&operation.target_root.display().to_string())
        )?;
        log_warn!("blocked bootstrap operation: {}", operation.source_root);
        return Ok(());
    }

    let source_abs = staging_root.join(path_from_package_root(&operation.source_root)?);
    if !source_abs.exists()
    {
        writeln!(
            context.journal,
            "missing_source={}",
            escape_value(&source_abs.display().to_string())
        )?;
        return Ok(());
    }

    return apply_copy_tree_with_journal(&source_abs, &operation.target_root, context);
}

fn finish_install(
    game_root: &Path,
    install_state: &InstallApplyState,
    apply_result: Result<(), AppError>,
) -> Result<(), AppError>
{
    return match apply_result {
        Ok(()) => finish_successful_install(game_root, install_state),
        Err(err) => recover_failed_install(game_root, install_state, err),
    };
}

fn finish_successful_install(
    game_root: &Path,
    install_state: &InstallApplyState,
) -> Result<(), AppError>
{
    release_install_lock(game_root)?;
    log_info!("installed transaction: {}", install_state.txid);
    log_info!("journal: {}", install_state.journal_path.display());
    log_info!("backups: {}", install_state.backup_root.display());
    return Ok(());
}

fn recover_failed_install(
    game_root: &Path,
    install_state: &InstallApplyState,
    install_error: AppError,
) -> Result<(), AppError>
{
    // Roll the partial changes back automatically so a failed install leaves a
    // clean game folder. If rollback itself fails, keep the lock so `recover`
    // can retry once the underlying problem is resolved.
    let rollback_result = if install_state.journal_path.exists() {
        rollback_journal(&install_state.journal_path, game_root)
    } else {
        Ok(())
    };
    return match rollback_result {
        Ok(()) => {
            release_install_lock(game_root)?;
            Err(install_error)
        }
        Err(rollback_error) => Err(AppError::Usage(format!(
            "install failed: {install_error}; automatic rollback also failed: {rollback_error}; run `recover --game {}` after resolving the issue",
            game_root.display()
        ))),
    };
}
