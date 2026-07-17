use crate::prelude::*;

use super::rollback::rollback_journal;

/// A record of the permanent install that currently holds the game folder.
///
/// Written when `apply_install_plan` begins and removed on success. If the
/// process dies before the record is removed, its presence marks an install
/// that was interrupted mid-transaction and can be rolled back with `recover`.
pub(crate) struct InstallLock {
    pub(crate) txid: String,
    pub(crate) journal: PathBuf,
    pub(crate) pid: Option<u32>,
}

fn install_lock_path(game_root: &Path) -> PathBuf {
    state_directory(game_root).join("install.lock")
}

pub(crate) fn read_install_lock(game_root: &Path) -> Result<Option<InstallLock>, AppError> {
    let path = install_lock_path(game_root);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    Ok(parse_install_lock(&text))
}

fn parse_install_lock(text: &str) -> Option<InstallLock> {
    let mut txid = None;
    let mut journal = None;
    let mut pid = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("txid=") {
            txid = Some(unescape_value(value));
        } else if let Some(value) = line.strip_prefix("journal=") {
            journal = Some(PathBuf::from(unescape_value(value)));
        } else if let Some(value) = line.strip_prefix("pid=") {
            pid = value.parse::<u32>().ok();
        }
    }
    Some(InstallLock {
        txid: txid?,
        journal: journal?,
        pid,
    })
}

fn write_install_lock(
    game_root: &Path,
    txid: &str,
    journal: &Path,
    pid: u32,
) -> Result<(), AppError> {
    let path = install_lock_path(game_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(&path)?;
    writeln!(file, "version=1")?;
    writeln!(file, "txid={}", escape_value(txid))?;
    writeln!(
        file,
        "journal={}",
        escape_value(&journal.display().to_string())
    )?;
    writeln!(file, "pid={pid}")?;
    writeln!(file, "created_unix={}", unix_now())?;
    // Persist the marker durably so a crash immediately after it is written is
    // still detectable as an interrupted install.
    file.sync_all()?;
    Ok(())
}

fn lock_owner_is_running(lock: &InstallLock) -> bool {
    lock.pid
        .map(crate::game_launch::process_is_running)
        .unwrap_or(false)
}

fn unknown_pid(lock: &InstallLock) -> String {
    lock.pid
        .map(|pid| pid.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Claim the game folder for a permanent install.
///
/// Refuses if another install is already running (concurrency guard) or if a
/// previous install was interrupted and not yet recovered.
pub(crate) fn acquire_install_lock(
    game_root: &Path,
    txid: &str,
    journal: &Path,
) -> Result<(), AppError> {
    if let Some(existing) = read_install_lock(game_root)? {
        if lock_owner_is_running(&existing) {
            return Err(AppError::Usage(format!(
                "another install is already in progress (pid {}); wait for it to finish before installing again",
                unknown_pid(&existing)
            )));
        }
        return Err(AppError::Usage(format!(
            "a previous install (txid {}) was interrupted and left the game folder partially modified; run `recover` to roll it back before installing again",
            existing.txid
        )));
    }
    write_install_lock(game_root, txid, journal, std::process::id())
}

pub(crate) fn release_install_lock(game_root: &Path) -> Result<(), AppError> {
    let path = install_lock_path(game_root);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Report an interrupted install (lock present, owning process gone), if any.
pub(crate) fn interrupted_install(game_root: &Path) -> Result<Option<InstallLock>, AppError> {
    match read_install_lock(game_root)? {
        Some(lock) if !lock_owner_is_running(&lock) => Ok(Some(lock)),
        _ => Ok(None),
    }
}

/// Roll back an interrupted install and clear its lock.
///
/// Returns the recovered transaction id, or `None` when there is nothing to
/// recover. Refuses while the owning install is still running.
pub(crate) fn recover_interrupted_install(game_root: &Path) -> Result<Option<String>, AppError> {
    let Some(lock) = read_install_lock(game_root)? else {
        return Ok(None);
    };
    if lock_owner_is_running(&lock) {
        return Err(AppError::Usage(format!(
            "an install is currently in progress (pid {}); cannot recover while it is running",
            unknown_pid(&lock)
        )));
    }
    if lock.journal.exists() {
        rollback_journal(&lock.journal, game_root)?;
    }
    release_install_lock(game_root)?;
    Ok(Some(lock.txid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_writes_lock_and_release_removes_it() {
        let game_root = test_root("lock_roundtrip");
        let journal = journal_path(&game_root, "tx-1");

        acquire_install_lock(&game_root, "tx-1", &journal).unwrap();
        let lock = read_install_lock(&game_root).unwrap().unwrap();
        assert_eq!(lock.txid, "tx-1");
        assert_eq!(lock.journal, journal);
        assert_eq!(lock.pid, Some(std::process::id()));

        release_install_lock(&game_root).unwrap();
        assert!(read_install_lock(&game_root).unwrap().is_none());
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn acquire_blocks_second_install_while_owner_is_running() {
        let game_root = test_root("lock_blocks_running");
        // A lock owned by this very process is, by definition, still running.
        write_install_lock(
            &game_root,
            "tx-live",
            &journal_path(&game_root, "tx-live"),
            std::process::id(),
        )
        .unwrap();

        let err = acquire_install_lock(&game_root, "tx-new", &journal_path(&game_root, "tx-new"))
            .unwrap_err()
            .to_string();

        assert!(err.contains("already in progress"));
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn acquire_reports_interrupted_install_when_owner_is_gone() {
        let game_root = test_root("lock_interrupted");
        write_install_lock(
            &game_root,
            "tx-dead",
            &journal_path(&game_root, "tx-dead"),
            u32::MAX,
        )
        .unwrap();

        let err = acquire_install_lock(&game_root, "tx-new", &journal_path(&game_root, "tx-new"))
            .unwrap_err()
            .to_string();

        assert!(err.contains("interrupted"));
        assert!(err.contains("recover"));
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn interrupted_install_only_reports_dead_owners() {
        let game_root = test_root("lock_detect");
        assert!(interrupted_install(&game_root).unwrap().is_none());

        write_install_lock(
            &game_root,
            "tx-live",
            &journal_path(&game_root, "tx-live"),
            std::process::id(),
        )
        .unwrap();
        assert!(interrupted_install(&game_root).unwrap().is_none());

        write_install_lock(
            &game_root,
            "tx-dead",
            &journal_path(&game_root, "tx-dead"),
            u32::MAX,
        )
        .unwrap();
        let detected = interrupted_install(&game_root).unwrap().unwrap();
        assert_eq!(detected.txid, "tx-dead");
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn recover_rolls_back_interrupted_install_and_clears_lock() {
        let game_root = test_root("lock_recover");
        let dest = game_root.join("CLEO").join("newmod.cs");
        let journal = journal_path(&game_root, "tx-recover");
        write_file(&dest, "materialized");
        write_file(
            &journal,
            &format!(
                "version=1\nnew={}\n",
                escape_value(&dest.display().to_string())
            ),
        );
        write_install_lock(&game_root, "tx-recover", &journal, u32::MAX).unwrap();

        let recovered = recover_interrupted_install(&game_root).unwrap();

        assert_eq!(recovered.as_deref(), Some("tx-recover"));
        assert!(!dest.exists());
        assert!(read_install_lock(&game_root).unwrap().is_none());
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn recover_refuses_while_install_is_running() {
        let game_root = test_root("lock_recover_running");
        let journal = journal_path(&game_root, "tx-live");
        write_file(&journal, "version=1\n");
        write_install_lock(&game_root, "tx-live", &journal, std::process::id()).unwrap();

        let err = recover_interrupted_install(&game_root)
            .unwrap_err()
            .to_string();

        assert!(err.contains("in progress"));
        assert!(read_install_lock(&game_root).unwrap().is_some());
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn recover_returns_none_without_a_lock() {
        let game_root = test_root("lock_recover_none");
        assert!(recover_interrupted_install(&game_root).unwrap().is_none());
        remove_dir_if_exists(&game_root).unwrap();
    }

    fn journal_path(game_root: &Path, txid: &str) -> PathBuf {
        state_directory(game_root)
            .join("journals")
            .join(format!("{txid}.journal"))
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
        fs::create_dir_all(state_directory(&root).join("journals")).unwrap();
        root
    }

    fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}
