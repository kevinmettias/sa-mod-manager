use crate::prelude::*;

use super::copy_journal::file_hash;

/// Upper bound on a rollback journal's size. One line per installed file plus a
/// small header; 64 MiB covers installs with millions of files while refusing a
/// pathological/corrupt journal instead of reading it whole into memory.
const MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn rollback_journal(journal_path: &Path, game_root: &Path) -> Result<(), AppError> {
    crate::logging::open_for_game_root(game_root);
    ensure_journal_allowed(game_root, journal_path)?;
    let content = read_capped(journal_path, MAX_JOURNAL_BYTES)
        .with_context(|| format!("read rollback journal {}", journal_path.display()))?;
    let rollback = parse_rollback_journal(&content)?;

    preflight_new_files(game_root, &rollback.new_files)?;
    preflight_backups(game_root, &rollback.backups)?;

    apply_rollback_actions(game_root, &rollback)?;

    log_info!("rollback complete: {}", journal_path.display());
    Ok(())
}

struct RollbackJournal {
    backups: Vec<BackupEntry>,
    new_files: Vec<NewFileEntry>,
    copy_hashes: BTreeMap<PathBuf, String>,
    actions: Vec<RollbackAction>,
}

enum RollbackAction {
    Backup(usize),
    NewFile(usize),
}

struct BackupEntry {
    dest: PathBuf,
    backup: PathBuf,
    backup_hash: Option<String>,
    copied_hash: Option<String>,
}

struct NewFileEntry {
    dest: PathBuf,
    copied_hash: Option<String>,
}

fn ensure_journal_allowed(game_root: &Path, journal_path: &Path) -> Result<(), AppError> {
    let journals_root = state_directory(game_root).join("journals"); // literal: allow external interface text or file-format spelling
    let root = journals_root
        .canonicalize()
        .with_context(|| format!("resolve journals directory {}", journals_root.display()))?;
    let journal = journal_path
        .canonicalize()
        .with_context(|| format!("resolve journal path {}", journal_path.display()))?;
    if journal.starts_with(&root) {
        Ok(())
    } else {
        Err(AppError::Usage(format!(
            "journal must be under manager journals: {}",
            journals_root.display()
        )))
    }
}

fn parse_rollback_journal(content: &str) -> Result<RollbackJournal, AppError> {
    let mut journal = RollbackJournal {
        backups: Vec::new(),
        new_files: Vec::new(),
        copy_hashes: BTreeMap::new(),
        actions: Vec::new(),
    };
    for line in content.lines() {
        parse_rollback_line(line, &mut journal)?;
    }
    attach_copy_hashes(&mut journal);
    Ok(journal)
}

fn parse_rollback_line(line: &str, journal: &mut RollbackJournal) -> Result<(), AppError> {
    // Lines without a recognized entry prefix (version=, mode=, blocked_bootstrap=,
    // etc.) are metadata the rollback ignores. But a line that *does* begin with a
    // known prefix yet has too few fields is a truncated/garbled record: fail
    // loudly rather than silently dropping it, which would produce a partial
    // rollback that under-restores the game folder.
    if let Some(value) = line.strip_prefix("backup=") {
        let fields = split_escaped_fields(value);
        if fields.len() < 2 || unescape_value(&fields[0]).is_empty() {
            return Err(malformed_journal_line_error("backup", line));
        }
        let index = journal.backups.len();
        journal.backups.push(BackupEntry {
            dest: PathBuf::from(unescape_value(&fields[0])),
            backup: PathBuf::from(unescape_value(&fields[1])),
            backup_hash: fields.get(2).map(|value| unescape_value(value)),
            copied_hash: None,
        });
        journal.actions.push(RollbackAction::Backup(index));
    } else if let Some(value) = line.strip_prefix("new=") {
        let fields = split_escaped_fields(value);
        let dest = fields.first().map(|d| unescape_value(d)).unwrap_or_default();
        if dest.is_empty() {
            return Err(malformed_journal_line_error("new", line));
        }
        let index = journal.new_files.len();
        journal.new_files.push(NewFileEntry {
            dest: PathBuf::from(dest),
            copied_hash: fields.get(1).map(|value| unescape_value(value)),
        });
        journal.actions.push(RollbackAction::NewFile(index));
    } else if let Some(value) = line.strip_prefix("copy=") {
        let fields = split_escaped_fields(value);
        if fields.len() < 3 || unescape_value(&fields[1]).is_empty() {
            return Err(malformed_journal_line_error("copy", line));
        }
        let dest = PathBuf::from(unescape_value(&fields[1]));
        let hash = unescape_value(&fields[2]);
        journal.copy_hashes.insert(dest, hash);
    }
    Ok(())
}

fn malformed_journal_line_error(kind: &str, line: &str) -> AppError {
    let preview: String = line.chars().take(120).collect();
    AppError::Usage(format!(
        "rollback journal is corrupt: malformed `{kind}` entry `{preview}`; refusing to roll back a partially recorded transaction"
    ))
}

fn attach_copy_hashes(journal: &mut RollbackJournal) {
    for backup in &mut journal.backups {
        backup.copied_hash = journal.copy_hashes.get(&backup.dest).cloned();
    }
    for new_file in &mut journal.new_files {
        if new_file.copied_hash.is_none() {
            new_file.copied_hash = journal.copy_hashes.get(&new_file.dest).cloned();
        }
    }
}

fn split_escaped_fields(value: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            current.push('\\');
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '|' {
            fields.push(current);
            current = String::new();
        } else {
            current.push(ch);
        }
    }
    if escaped {
        current.push('\\');
    }
    fields.push(current);
    fields
}

fn preflight_new_files(game_root: &Path, new_files: &[NewFileEntry]) -> Result<(), AppError> {
    for new_file in new_files {
        ensure_destination_allowed(game_root, &new_file.dest)?;
        ensure_current_hash_matches(&new_file.dest, new_file.copied_hash.as_deref())?;
    }
    Ok(())
}

fn preflight_backups(game_root: &Path, backups: &[BackupEntry]) -> Result<(), AppError> {
    for backup in backups {
        ensure_destination_allowed(game_root, &backup.dest)?;
        ensure_backup_allowed(game_root, &backup.backup)?;
        if !backup.backup.exists() {
            return Err(AppError::Usage(format!(
                "missing backup for rollback: {}",
                backup.backup.display()
            )));
        }
        ensure_current_hash_matches(&backup.dest, backup.copied_hash.as_deref())?;
        ensure_backup_hash_matches(&backup.backup, backup.backup_hash.as_deref())?;
    }
    Ok(())
}

fn ensure_current_hash_matches(dest: &Path, expected: Option<&str>) -> Result<(), AppError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if !dest.exists() {
        return Ok(());
    }
    let actual = file_hash(dest)?;
    if actual != expected {
        return Err(AppError::Usage(format!(
            "rollback conflict: {} changed after this transaction (expected {expected}, found {actual})",
            dest.display()
        )));
    }
    Ok(())
}

fn ensure_backup_hash_matches(backup: &Path, expected: Option<&str>) -> Result<(), AppError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = file_hash(backup)?;
    if actual != expected {
        return Err(AppError::Usage(format!(
            "backup hash mismatch for rollback: {} (expected {expected}, found {actual})",
            backup.display()
        )));
    }
    Ok(())
}

fn remove_new_file(game_root: &Path, new_file: &NewFileEntry) -> Result<(), AppError> {
    ensure_destination_allowed(game_root, &new_file.dest)?;
    if new_file.dest.exists() {
        fs::remove_file(&new_file.dest)
            .with_context(|| format!("remove {}", new_file.dest.display()))?;
        log_debug!("removed: {}", new_file.dest.display());
        remove_empty_parent_dirs(game_root, &new_file.dest)?;
    }
    Ok(())
}

fn apply_rollback_actions(game_root: &Path, rollback: &RollbackJournal) -> Result<(), AppError> {
    for action in rollback.actions.iter().rev() {
        match action {
            RollbackAction::Backup(index) => {
                restore_backup_file(game_root, &rollback.backups[*index])?
            }
            RollbackAction::NewFile(index) => {
                remove_new_file(game_root, &rollback.new_files[*index])?
            }
        }
    }
    Ok(())
}

fn restore_backup_file(game_root: &Path, backup: &BackupEntry) -> Result<(), AppError> {
    ensure_destination_allowed(game_root, &backup.dest)?;
    ensure_backup_allowed(game_root, &backup.backup)?;
    if let Some(parent) = backup.dest.parent() {
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
    Ok(())
}

fn remove_empty_parent_dirs(game_root: &Path, file: &Path) -> Result<(), AppError> {
    let game_root = game_root
        .canonicalize()
        .with_context(|| format!("resolve game root {}", game_root.display()))?;
    let mut current = match file.parent() {
        Some(parent) => parent.to_path_buf(),
        None => return Ok(()),
    };
    loop {
        if !current.exists() {
            break;
        }
        let canonical = current
            .canonicalize()
            .with_context(|| format!("resolve {}", current.display()))?;
        if canonical == game_root || !canonical.starts_with(&game_root) {
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
        } else {
            break;
        }
        if !current.pop() {
            break;
        }
    }
    Ok(())
}

fn ensure_backup_allowed(game_root: &Path, backup: &Path) -> Result<(), AppError> {
    let backups_root = state_directory(game_root).join("backups"); // literal: allow external interface text or file-format spelling
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
    if backup_parent.starts_with(&root) {
        Ok(())
    } else {
        Err(AppError::Usage(format!(
            "backup must be under manager backups: {}",
            backups_root.display()
        )))
    }
}

fn nearest_existing_parent(path: &Path) -> Result<PathBuf, AppError> {
    let mut current = path.to_path_buf();
    while !current.exists() {
        if !current.pop() {
            return Err(AppError::Usage(format!(
                "no existing parent for {}",
                path.display()
            )));
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_restores_overwritten_file_from_backup() {
        let game_root = test_root("rollback_restore_overwrite");
        let dest = game_root.join("data").join("handling.cfg");
        let backup = game_root
            .join(".sa-mod-manager")
            .join("backups")
            .join("tx")
            .join("data")
            .join("handling.cfg");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "modded");
        write_file(&backup, "vanilla");
        write_journal(
            &journal,
            &[
                backup_line(&dest, &backup),
                copy_line(&game_root.join("source").join("handling.cfg"), &dest),
            ],
        );

        rollback_journal(&journal, &game_root).unwrap();

        assert_eq!(fs::read_to_string(&dest).unwrap(), "vanilla");
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn rollback_removes_nested_new_file_and_empty_parent_dirs() {
        let game_root = test_root("rollback_nested_new");
        let dest = game_root
            .join("modloader")
            .join("test_mod")
            .join("deep")
            .join("folder")
            .join("file.txt");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "payload");
        write_journal(
            &journal,
            &[
                new_line(&dest),
                copy_line(&game_root.join("source").join("file.txt"), &dest),
            ],
        );

        rollback_journal(&journal, &game_root).unwrap();

        assert!(!dest.exists());
        assert!(!game_root.join("modloader").join("test_mod").exists());
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn rollback_fails_loudly_when_backup_is_missing_without_mutating() {
        let game_root = test_root("rollback_missing_backup");
        let dest = game_root.join("data").join("foo.dat");
        let backup = game_root
            .join(".sa-mod-manager")
            .join("backups")
            .join("tx")
            .join("data")
            .join("foo.dat");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        fs::create_dir_all(backup.parent().unwrap()).unwrap();
        write_file(&dest, "modded");
        write_journal(
            &journal,
            &[
                format!(
                    "backup={}|{}|fnv64:0000000000000000",
                    escape_value(&dest.display().to_string()),
                    escape_value(&backup.display().to_string())
                ),
                copy_line(&game_root.join("source").join("foo.dat"), &dest),
            ],
        );

        let err = rollback_journal(&journal, &game_root)
            .unwrap_err()
            .to_string();

        assert!(err.contains("missing backup"));
        assert_eq!(fs::read_to_string(&dest).unwrap(), "modded");
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn rollback_detects_direct_overwrite_conflict_before_restore() {
        let game_root = test_root("rollback_conflict");
        let dest = game_root.join("data").join("foo.dat");
        let backup = game_root
            .join(".sa-mod-manager")
            .join("backups")
            .join("tx")
            .join("data")
            .join("foo.dat");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "modded");
        write_file(&backup, "vanilla");
        let copied_hash = file_hash(&dest).unwrap();
        fs::write(&dest, "changed by later mod").unwrap();
        write_journal(
            &journal,
            &[
                backup_line(&dest, &backup),
                format!(
                    "copy={}|{}|{}",
                    escape_value(
                        &game_root
                            .join("source")
                            .join("foo.dat")
                            .display()
                            .to_string()
                    ),
                    escape_value(&dest.display().to_string()),
                    copied_hash
                ),
            ],
        );

        let err = rollback_journal(&journal, &game_root)
            .unwrap_err()
            .to_string();

        assert!(err.contains("rollback conflict"));
        assert_eq!(fs::read_to_string(&dest).unwrap(), "changed by later mod");
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn rollback_removes_file_created_then_overwritten_in_same_transaction() {
        let game_root = test_root("rollback_new_then_overwrite");
        let dest = game_root
            .join("modloader")
            .join("test_mod")
            .join("file.txt");
        let backup = game_root
            .join(".sa-mod-manager")
            .join("backups")
            .join("tx")
            .join("modloader")
            .join("test_mod")
            .join("file.txt");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "first mod version");
        write_file(&backup, "first mod version");
        let first_hash = file_hash(&dest).unwrap();
        fs::write(&dest, "second mod version").unwrap();
        let second_hash = file_hash(&dest).unwrap();
        write_journal(
            &journal,
            &[
                new_line(&dest),
                format!(
                    "copy={}|{}|{}",
                    escape_value(
                        &game_root
                            .join("source-one")
                            .join("file.txt")
                            .display()
                            .to_string()
                    ),
                    escape_value(&dest.display().to_string()),
                    first_hash
                ),
                backup_line(&dest, &backup),
                format!(
                    "copy={}|{}|{}",
                    escape_value(
                        &game_root
                            .join("source-two")
                            .join("file.txt")
                            .display()
                            .to_string()
                    ),
                    escape_value(&dest.display().to_string()),
                    second_hash
                ),
            ],
        );

        rollback_journal(&journal, &game_root).unwrap();

        assert!(!dest.exists());
        assert!(!game_root.join("modloader").join("test_mod").exists());
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn malformed_journal_entry_aborts_rollback_loudly() {
        let game_root = test_root("rollback_malformed");
        let dest = game_root.join("data").join("foo.dat");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "modded");
        // A truncated `backup=` line (only one field) must fail the whole rollback
        // rather than being silently skipped into a partial restore.
        write_journal(
            &journal,
            &[format!(
                "backup={}",
                escape_value(&dest.display().to_string())
            )],
        );

        let err = rollback_journal(&journal, &game_root)
            .unwrap_err()
            .to_string();

        assert!(err.contains("corrupt"), "{err}");
        assert!(err.contains("backup"), "{err}");
        // Nothing was mutated: the destructive restore never ran.
        assert_eq!(fs::read_to_string(&dest).unwrap(), "modded");
        remove_dir_if_exists(&game_root).unwrap();
    }

    fn backup_line(dest: &Path, backup: &Path) -> String {
        format!(
            "backup={}|{}|{}",
            escape_value(&dest.display().to_string()),
            escape_value(&backup.display().to_string()),
            file_hash(backup).unwrap()
        )
    }

    fn new_line(dest: &Path) -> String {
        format!("new={}", escape_value(&dest.display().to_string()))
    }

    fn copy_line(source: &Path, dest: &Path) -> String {
        format!(
            "copy={}|{}|{}",
            escape_value(&source.display().to_string()),
            escape_value(&dest.display().to_string()),
            file_hash(dest).unwrap()
        )
    }

    fn write_journal(path: &Path, lines: &[String]) {
        let mut text = "version=1\nmode=ephemeral-run\n".to_string();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        write_file(path, &text);
    }

    fn write_file(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        remove_dir_if_exists(&root).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(root.join(".sa-mod-manager").join("journals")).unwrap();
        fs::create_dir_all(root.join(".sa-mod-manager").join("backups")).unwrap();
        root
    }

    fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}
