use crate::prelude::*;

pub(super) fn apply_copy_tree_with_journal(
    source_abs: &Path,
    target_root: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    let files = collect_files_recursive(source_abs)?;
    for file in files {
        apply_copy_file_with_journal(source_abs, target_root, &file, context)?;
    }
    Ok(())
}

fn apply_copy_file_with_journal(
    source_abs: &Path,
    target_root: &Path,
    file: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    let rel = file.strip_prefix(source_abs).unwrap_or(file);
    let dest = target_root.join(rel);
    ensure_destination_allowed(context.game_root, &dest)?;
    journal_destination_state(&dest, context)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(file, &dest)?;
    let copied_hash = file_hash(&dest)?;
    write_copy_line(context.journal, file, &dest, &copied_hash)?;
    Ok(())
}

fn journal_destination_state(
    dest: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    if dest.exists() {
        let backup = context
            .backup_root
            .join(backup_relative_for_destination(context.game_root, dest)?);
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(dest, &backup)?;
        let backup_hash = file_hash(&backup)?;
        write_backup_line(context.journal, dest, &backup, &backup_hash)?;
    } else {
        write_new_line(context.journal, dest)?;
    }
    Ok(())
}

fn write_backup_line(
    journal: &mut fs::File,
    dest: &Path,
    backup: &Path,
    backup_hash: &str,
) -> Result<(), AppError> {
    writeln!(
        journal,
        "backup={}|{}|{}",
        escape_value(&dest.display().to_string()),
        escape_value(&backup.display().to_string()),
        backup_hash
    )?;
    Ok(())
}

fn write_new_line(journal: &mut fs::File, dest: &Path) -> Result<(), AppError> {
    writeln!(journal, "new={}", escape_value(&dest.display().to_string()))?;
    Ok(())
}

fn write_copy_line(
    journal: &mut fs::File,
    file: &Path,
    dest: &Path,
    copied_hash: &str,
) -> Result<(), AppError> {
    writeln!(
        journal,
        "copy={}|{}|{}",
        escape_value(&file.display().to_string()),
        escape_value(&dest.display().to_string()),
        copied_hash
    )?;
    Ok(())
}

pub(super) fn file_hash(path: &Path) -> Result<String, AppError> {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let bytes = fs::read(path)?;
    let mut hash = FNV_OFFSET;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    Ok(format!("fnv64:{hash:016x}"))
}
