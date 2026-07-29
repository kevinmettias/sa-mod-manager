use crate::prelude::*;

use super::copy_journal::file_hash;

/// Upper bound on a rollback journal's size. One line per installed file plus a
/// small header; 64 MiB covers installs with millions of files while refusing a
/// pathological/corrupt journal instead of reading it whole into memory.
const MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;
const BACKUP_JOURNAL_FIELD_COUNT: usize = 2;
const COPY_JOURNAL_FIELD_COUNT: usize = 3;

pub(crate) fn rollback_journal(journal_path: &Path, game_root: &Path) -> Result<(), AppError>
{
    crate::logging::open_for_game_root(game_root);
    ensure_journal_allowed(game_root, journal_path)?;
    let content = read_capped(journal_path, MAX_JOURNAL_BYTES)
        .with_context(|| format!("read rollback journal {}", journal_path.display()))?;
    let rollback = parse_rollback_journal(&content)?;

    preflight_new_files(game_root, &rollback.new_files)?;
    preflight_backups(game_root, &rollback.backups)?;

    apply_rollback_actions(game_root, &rollback)?;

    log_info!("rollback complete: {}", journal_path.display());
    return Ok(());
}

struct RollbackJournal
{
    backups: Vec<BackupEntry>,
    new_files: Vec<NewFileEntry>,
    copy_hashes: BTreeMap<PathBuf, String>,
    actions: Vec<RollbackAction>,
}

enum RollbackAction
{
    Backup(usize),
    NewFile(usize),
}

struct BackupEntry
{
    dest: PathBuf,
    backup: PathBuf,
    backup_hash: Option<String>,
    copied_hash: Option<String>,
}

struct NewFileEntry
{
    dest: PathBuf,
    copied_hash: Option<String>,
}

fn ensure_journal_allowed(game_root: &Path, journal_path: &Path) -> Result<(), AppError>
{
    let journals_root = state_directory(game_root).join("journals");
    let root = journals_root
        .canonicalize()
        .with_context(|| format!("resolve journals directory {}", journals_root.display()))?;
    let journal = journal_path
        .canonicalize()
        .with_context(|| format!("resolve journal path {}", journal_path.display()))?;
    return if journal.starts_with(&root)
    {
        Ok(())
    }
    else
    {
        Err(AppError::Usage(format!(
            "journal must be under manager journals: {}",
            journals_root.display()
        )))
    };
}

fn parse_rollback_journal(content: &str) -> Result<RollbackJournal, AppError>
{
    let mut journal = RollbackJournal {
        backups: Vec::new(),
        new_files: Vec::new(),
        copy_hashes: BTreeMap::new(),
        actions: Vec::new(),
    };
    for line in content.lines()
    {
        parse_rollback_line(line, &mut journal)?;
    }
    attach_copy_hashes(&mut journal);
    return Ok(journal);
}

fn parse_rollback_line(line: &str, journal: &mut RollbackJournal) -> Result<(), AppError>
{
    // Lines without a recognized entry prefix (version=, mode=, blocked_bootstrap=,
    // etc.) are metadata the rollback ignores. But a line that *does* begin with a
    // known prefix yet has too few fields is a truncated/garbled record: fail
    // loudly rather than silently dropping it, which would produce a partial
    // rollback that under-restores the game folder.
    if let Some(value) = line.strip_prefix("backup=")
    {
        let fields = split_escaped_fields(value);
        if fields.len() < BACKUP_JOURNAL_FIELD_COUNT || escaped_field_is_empty(&fields[0])
        {
            // literal: allow domain threshold is documented by the surrounding code
            return Err(malformed_journal_line_error(MalformedJournalLine { kind: "backup", line }));
        }
        let index = journal.backups.len();
        journal.backups.push(BackupEntry {
            dest: PathBuf::from(unescape_value(&fields[0])),
            backup: PathBuf::from(unescape_value(&fields[1])),
            backup_hash: fields.get(2).map(|value| unescape_value(value)), // literal: allow domain threshold is documented by the surrounding code
            copied_hash: None,
        });
        journal.actions.push(RollbackAction::Backup(index));
    }
    else if let Some(value) = line.strip_prefix("new=")
    {
        let fields = split_escaped_fields(value);
        let dest = fields
            .first()
            .map(|d| unescape_value(d))
            .unwrap_or_default();
        if dest.is_empty()
        {
            return Err(malformed_journal_line_error(MalformedJournalLine { kind: "new", line }));
        }
        let index = journal.new_files.len();
        journal.new_files.push(NewFileEntry {
            dest: PathBuf::from(dest),
            copied_hash: fields.get(1).map(|value| unescape_value(value)),
        });
        journal.actions.push(RollbackAction::NewFile(index));
    }
    else if let Some(value) = line.strip_prefix("copy=")
    {
        let fields = split_escaped_fields(value);
        if fields.len() < COPY_JOURNAL_FIELD_COUNT || escaped_field_is_empty(&fields[1])
        {
            // literal: allow domain threshold is documented by the surrounding code
            return Err(malformed_journal_line_error(MalformedJournalLine { kind: "copy", line }));
        }
        let dest = PathBuf::from(unescape_value(&fields[1]));
        let hash = unescape_value(&fields[2]); // literal: allow domain threshold is documented by the surrounding code
        journal.copy_hashes.insert(dest, hash);
    }
    return Ok(());
}

fn split_escaped_fields(value: &str) -> Vec<String>
{
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in value.chars()
    {
        if escaped
        {
            current.push('\\');
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\'
        {
            escaped = true;
            continue;
        }
        if ch == '|'
        {
            fields.push(current);
            current = String::new();
        }
        else
        {
            current.push(ch);
        }
    }
    if escaped
    {
        current.push('\\');
    }
    fields.push(current);
    return fields;
}

struct MalformedJournalLine<'a>
{
    kind: &'a str,
    line: &'a str,
}

fn escaped_field_is_empty(field: &str) -> bool
{
    return unescape_value(field).is_empty();
}

fn malformed_journal_line_error(entry: MalformedJournalLine<'_>) -> AppError
{
    let kind = entry.kind;
    let line = entry.line;
    let preview: String = line.chars().take(120).collect(); // literal: allow domain threshold is documented by the surrounding code
    return AppError::Usage(format!(
        "rollback journal is corrupt: malformed `{kind}` entry `{preview}`; refusing to roll back a partially recorded transaction"
    ));
}

fn attach_copy_hashes(journal: &mut RollbackJournal)
{
    for backup in &mut journal.backups
    {
        backup.copied_hash = journal.copy_hashes.get(&backup.dest).cloned();
    }
    for new_file in &mut journal.new_files
    {
        if new_file.copied_hash.is_none()
        {
            new_file.copied_hash = journal.copy_hashes.get(&new_file.dest).cloned();
        }
    }
}

fn preflight_new_files(game_root: &Path, new_files: &[NewFileEntry]) -> Result<(), AppError>
{
    for new_file in new_files
    {
        ensure_destination_allowed(game_root, &new_file.dest)?;
        ensure_current_hash_matches(&new_file.dest, new_file.copied_hash.as_deref())?;
    }
    return Ok(());
}

fn preflight_backups(game_root: &Path, backups: &[BackupEntry]) -> Result<(), AppError>
{
    for backup in backups
    {
        ensure_destination_allowed(game_root, &backup.dest)?;
        ensure_backup_allowed(game_root, &backup.backup)?;
        if !backup.backup.exists()
        {
            return Err(AppError::Usage(format!(
                "missing backup for rollback: {}",
                backup.backup.display()
            )));
        }
        ensure_current_hash_matches(&backup.dest, backup.copied_hash.as_deref())?;
        ensure_backup_hash_matches(&backup.backup, backup.backup_hash.as_deref())?;
    }
    return Ok(());
}

fn ensure_backup_hash_matches(backup: &Path, expected: Option<&str>) -> Result<(), AppError>
{
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = file_hash(backup)?;
    if actual != expected
    {
        return Err(AppError::Usage(format!(
            "backup hash mismatch for rollback: {} (expected {expected}, found {actual})",
            backup.display()
        )));
    }
    return Ok(());
}

fn apply_rollback_actions(game_root: &Path, rollback: &RollbackJournal) -> Result<(), AppError>
{
    for action in rollback.actions.iter().rev()
    {
        match action
        {
            RollbackAction::Backup(index) => {
                restore_backup_file(game_root, &rollback.backups[*index])?
            }
            RollbackAction::NewFile(index) => {
                remove_new_file(game_root, &rollback.new_files[*index])?
            }
        }
    }
    return Ok(());
}

fn restore_backup_file(game_root: &Path, backup: &BackupEntry) -> Result<(), AppError>
{
    ensure_destination_allowed(game_root, &backup.dest)?;
    ensure_backup_allowed(game_root, &backup.backup)?;
    if let Some(parent) = backup.dest.parent()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    fs::copy(&backup.backup, &backup.dest).with_context(|| {
        format!(
            "restore {} from backup {}",
            backup.dest.display(),
            backup.backup.display()
        )
    })?;
    log_debug!("restored: {}", backup.dest.display());
    return Ok(());
}

fn remove_new_file(game_root: &Path, new_file: &NewFileEntry) -> Result<(), AppError>
{
    ensure_destination_allowed(game_root, &new_file.dest)?;
    if new_file.dest.exists()
    {
        fs::remove_file(&new_file.dest)
            .with_context(|| format!("remove {}", new_file.dest.display()))?;
        log_debug!("removed: {}", new_file.dest.display());
        remove_empty_parent_dirs(game_root, &new_file.dest)?;
    }
    return Ok(());
}

fn remove_empty_parent_dirs(game_root: &Path, file: &Path) -> Result<(), AppError>
{
    let game_root = game_root
        .canonicalize()
        .with_context(|| format!("resolve game root {}", game_root.display()))?;
    let mut current = match file.parent() {
        Some(parent) => parent.to_path_buf(),
        None => return Ok(()),
    };
    // unbounded-loop: allow: parent cleanup stops at a missing directory, the game root, or an occupied parent
    loop
    {
        if !current.exists()
        {
            break;
        }
        let canonical = current
            .canonicalize()
            .with_context(|| format!("resolve {}", current.display()))?;
        if canonical == game_root || !canonical.starts_with(&game_root)
        {
            break;
        }
        if fs::read_dir(&current)
            .with_context(|| format!("read directory {}", current.display()))?
            .next()
            .is_none()
        {
            fs::remove_dir(&current)
                .with_context(|| format!("remove empty directory {}", current.display()))?;
            log_debug!("removed empty dir: {}", current.display());
        }
        else
        {
            break;
        }
        if !current.pop()
        {
            break;
        }
    }
    return Ok(());
}

fn ensure_current_hash_matches(dest: &Path, expected: Option<&str>) -> Result<(), AppError>
{
    let Some(expected) = expected else {
        return Ok(());
    };
    if !dest.exists()
    {
        return Ok(());
    }
    let actual = file_hash(dest)?;
    if actual != expected
    {
        return Err(AppError::Usage(format!(
            "rollback conflict: {} changed after this transaction (expected {expected}, found {actual})",
            dest.display()
        )));
    }
    return Ok(());
}

fn ensure_backup_allowed(game_root: &Path, backup: &Path) -> Result<(), AppError>
{
    let backups_root = state_directory(game_root).join("backups");
    let root = backups_root
        .canonicalize()
        .with_context(|| format!("resolve backups directory {}", backups_root.display()))?;
    let parent = backup
        .parent()
        .ok_or_else(|| AppError::Usage(format!("backup has no parent: {}", backup.display())))?;
    let existing_parent = nearest_existing_parent(parent)?;
    let backup_parent = existing_parent
        .canonicalize()
        .with_context(|| format!("resolve backup parent {}", existing_parent.display()))?;
    return if backup_parent.starts_with(&root)
    {
        Ok(())
    }
    else
    {
        Err(AppError::Usage(format!(
            "backup must be under manager backups: {}",
            backups_root.display()
        )))
    };
}

fn nearest_existing_parent(path: &Path) -> Result<PathBuf, AppError>
{
    let mut current = path.to_path_buf();
    while !current.exists()
    {
        if !current.pop()
        {
            return Err(AppError::Usage(format!(
                "no existing parent for {}",
                path.display()
            )));
        }
    }
    return Ok(current);
}

#[cfg(test)]
mod tests
{
    include!("part_01_tests_01.rs");
}

