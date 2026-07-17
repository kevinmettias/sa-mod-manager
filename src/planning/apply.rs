use crate::prelude::*;

use super::copy_journal::apply_copy_tree_with_journal;

pub(crate) fn apply_install_plan(package: &Path, plan: &InstallPlan) -> Result<(), AppError> {
    if plan.operations.is_empty() {
        return Err(usage_error("install plan has no operations")); // literal: allow external interface text or file-format spelling
    }

    let install_state = create_install_state(package, plan)?;
    let mut journal = fs::File::create(&install_state.journal_path)?;
    write_install_journal_header(&mut journal, package, plan, &install_state.txid)?;

    for operation in &plan.operations {
        let mut copy_context = CopyJournalContext {
            game_root: &plan.game_root,
            backup_root: &install_state.backup_root,
            journal: &mut journal,
        };
        apply_install_operation(operation, &install_state.staging_root, &mut copy_context)?;
    }

    println!("installed transaction: {}", install_state.txid);
    println!("journal: {}", install_state.journal_path.display());
    println!("backups: {}", install_state.backup_root.display());
    Ok(())
}

fn create_install_state(package: &Path, plan: &InstallPlan) -> Result<InstallApplyState, AppError> {
    ensure_state(&plan.game_root)?;
    let txid = format!("{}-{}", plan.package_id, unix_now());
    let journal_path = state_directory(&plan.game_root)
        .join("journals") // literal: allow external interface text or file-format spelling
        .join(format!("{txid}.journal"));
    let backup_root = state_directory(&plan.game_root).join("backups").join(&txid); // literal: allow external interface text or file-format spelling
    fs::create_dir_all(&backup_root)?;
    let staging_root = staging_root_for_install(package, plan)?;
    Ok(InstallApplyState {
        txid,
        journal_path,
        backup_root,
        staging_root,
    })
}

fn staging_root_for_install(package: &Path, plan: &InstallPlan) -> Result<PathBuf, AppError> {
    if package.is_dir() {
        Ok(package.to_path_buf())
    } else {
        extract_archive_to_named_staging(package, &plan.game_root, &plan.package_id)
    }
}

fn write_install_journal_header(
    journal: &mut fs::File,
    package: &Path,
    plan: &InstallPlan,
    txid: &str,
) -> Result<(), AppError> {
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
    Ok(())
}

fn apply_install_operation(
    operation: &InstallOperation,
    staging_root: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    if matches!(operation.target_kind, TargetKind::Bootstrap) {
        writeln!(
            context.journal,
            "blocked_bootstrap={}|{}",
            escape_value(&operation.source_root),
            escape_value(&operation.target_root.display().to_string())
        )?;
        println!("blocked bootstrap operation: {}", operation.source_root);
        return Ok(());
    }

    let source_abs = staging_root.join(path_from_package_root(&operation.source_root)?);
    if !source_abs.exists() {
        writeln!(
            context.journal,
            "missing_source={}",
            escape_value(&source_abs.display().to_string())
        )?;
        return Ok(());
    }

    apply_copy_tree_with_journal(&source_abs, &operation.target_root, context)
}
