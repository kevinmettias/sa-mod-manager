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
    /// Creation-time fingerprint of the owning process (Windows only). Lets the
    /// liveness check reject a recycled PID now belonging to another process.
    /// Absent in older locks and off Windows, where the plain PID check stands.
    pub(crate) pid_start: Option<u64>,
}

fn install_lock_path(game_root: &Path) -> PathBuf {
    state_directory(game_root).join("install.lock")
}

pub(crate) fn read_install_lock(game_root: &Path) -> Result<Option<InstallLock>, AppError> {
    let path = install_lock_path(game_root);
    if !path.exists() {
        return Ok(None);
    }
    // The lock is a handful of `key=value` lines; cap the read so a corrupt or
    // oversized file fails cleanly instead of being slurped whole.
    let text = read_capped(&path, 64 * 1024)
        .with_context(|| format!("read install lock {}", path.display()))?;
    parse_install_lock(&text)
}

fn parse_install_lock(text: &str) -> Result<Option<InstallLock>, AppError> {
    let mut txid = None;
    let mut journal = None;
    let mut pid = None;
    let mut pid_start = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("txid=") {
            txid = Some(unescape_value(value));
        } else if let Some(value) = line.strip_prefix("journal=") {
            journal = Some(PathBuf::from(unescape_value(value)));
        } else if let Some(value) = line.strip_prefix("pid=") {
            // A present-but-unparseable pid is corruption, not "no pid": collapsing
            // it to None would make a live install look dead and invite a second
            // install to trample it, so surface it loudly instead.
            pid = Some(parse_lock_number(value, "pid")?);
        } else if let Some(value) = line.strip_prefix("pid_start=") {
            pid_start = Some(parse_lock_number(value, "pid_start")?);
        }
    }
    // A lock missing its required identity fields is treated as "no lock": the
    // marker is written before any file is modified, so a crash mid-write leaves
    // an incomplete lock with nothing yet to roll back.
    match (txid, journal) {
        (Some(txid), Some(journal)) => Ok(Some(InstallLock {
            txid,
            journal,
            pid,
            pid_start,
        })),
        _ => Ok(None),
    }
}

fn parse_lock_number<T>(value: &str, field: &str) -> Result<T, AppError>
where
    T: std::str::FromStr,
{
    value.trim().parse::<T>().map_err(|_| {
        AppError::Usage(format!(
            "install lock is corrupt: `{field}` value `{}` is not a valid number; delete `install.lock` or run `recover`",
            value.trim()
        ))
    })
}

fn write_lock_fields(
    file: &mut fs::File,
    txid: &str,
    journal: &Path,
    pid: u32,
) -> Result<(), AppError> {
    writeln!(file, "version=1")?;
    writeln!(file, "txid={}", escape_value(txid))?;
    writeln!(
        file,
        "journal={}",
        escape_value(&journal.display().to_string())
    )?;
    writeln!(file, "pid={pid}")?;
    if let Some(start) = crate::game_launch::current_process_start_ticks() {
        writeln!(file, "pid_start={start}")?;
    }
    writeln!(file, "created_unix={}", unix_now())?;
    // Persist the marker durably so a crash immediately after it is written is
    // still detectable as an interrupted install. We fsync the file's contents
    // but not the parent directory entry: this targets Windows/NTFS, whose
    // metadata is journaled, and Windows offers no reliable directory-fsync
    // (`FlushFileBuffers` on a directory handle is unsupported) — PostgreSQL and
    // SQLite skip directory fsync on Windows for the same reason.
    file.sync_all()?;
    Ok(())
}

fn ensure_lock_parent(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create state directory {}", parent.display()))?;
    }
    Ok(())
}

/// Truncating writer used by tests to seed a lock in an arbitrary state.
/// Production code claims the lock atomically in [`acquire_install_lock`].
#[cfg(test)]
fn write_install_lock(
    game_root: &Path,
    txid: &str,
    journal: &Path,
    pid: u32,
) -> Result<(), AppError> {
    let path = install_lock_path(game_root);
    ensure_lock_parent(&path)?;
    let mut file = fs::File::create(&path)
        .with_context(|| format!("create install lock {}", path.display()))?;
    write_lock_fields(&mut file, txid, journal, pid)
        .with_context(|| format!("write install lock {}", path.display()))
}

fn lock_owner_is_running(lock: &InstallLock) -> bool {
    let Some(pid) = lock.pid else {
        return false;
    };
    if !crate::game_launch::process_is_running(pid) {
        return false;
    }
    // If the owner's creation-time fingerprint was recorded, require the live
    // PID to still carry it. A mismatch means the original install process
    // exited and the OS handed its PID to an unrelated process, so the install
    // is not actually running and recovery must not be blocked on it.
    match lock.pid_start {
        Some(expected) => crate::game_launch::process_start_ticks(pid) == Some(expected),
        None => true,
    }
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
        return Err(lock_conflict_error(&existing));
    }
    let path = install_lock_path(game_root);
    ensure_lock_parent(&path)?;
    // Create the marker atomically: `create_new` fails if the file already
    // exists, so a second install that raced past the check above cannot
    // silently overwrite our record (or we, theirs). The check above still runs
    // first only to produce a friendlier message in the common, uncontended case.
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => write_lock_fields(&mut file, txid, journal, std::process::id())
            .with_context(|| format!("write install lock {}", path.display())),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            // Lost the race between the check and the create. Re-read to report
            // who won; fall back to a generic message if the winner already
            // released it (the lock file vanished between create and re-read).
            match read_install_lock(game_root)? {
                Some(existing) => Err(lock_conflict_error(&existing)),
                None => Err(AppError::Usage(
                    "another install claimed the game folder concurrently; try again".to_string(),
                )),
            }
        }
        Err(err) => Err(AppError::from(err).context(format!("create install lock {}", path.display()))),
    }
}

fn lock_conflict_error(existing: &InstallLock) -> AppError {
    if lock_owner_is_running(existing) {
        return AppError::Usage(format!(
            "another install is already in progress (pid {}); wait for it to finish before installing again",
            unknown_pid(existing)
        ));
    }
    AppError::Usage(format!(
        "a previous install (txid {}) was interrupted and left the game folder partially modified; run `recover` to roll it back before installing again",
        existing.txid
    ))
}

pub(crate) fn release_install_lock(game_root: &Path) -> Result<(), AppError> {
    let path = install_lock_path(game_root);
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("remove install lock {}", path.display()))?;
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

    // On Windows the lock also fingerprints the owner's process creation time,
    // so a live PID that has been recycled by an unrelated process is correctly
    // treated as a dead owner instead of wedging recovery.
    #[cfg(windows)]
    #[test]
    fn acquire_reports_interrupted_when_pid_is_reused_by_another_process() {
        let game_root = test_root("lock_pid_reuse");
        let journal = journal_path(&game_root, "tx-reused");
        let path = install_lock_path(&game_root);
        ensure_lock_parent(&path).unwrap();
        // Owned by *this* live PID, but stamped with a creation-time fingerprint
        // that cannot be ours (1 == 100ns after 1601) -> the PID was recycled.
        fs::write(
            &path,
            format!(
                "version=1\ntxid=tx-reused\njournal={}\npid={}\npid_start=1\ncreated_unix=0\n",
                escape_value(&journal.display().to_string()),
                std::process::id(),
            ),
        )
        .unwrap();
        assert_ne!(crate::game_launch::current_process_start_ticks(), Some(1));

        let detected = interrupted_install(&game_root).unwrap();
        assert_eq!(detected.unwrap().txid, "tx-reused");

        let err = acquire_install_lock(&game_root, "tx-new", &journal_path(&game_root, "tx-new"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("interrupted"));
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
    fn corrupt_pid_in_lock_is_reported_not_silently_ignored() {
        let game_root = test_root("lock_corrupt_pid");
        let path = install_lock_path(&game_root);
        ensure_lock_parent(&path).unwrap();
        // A present-but-garbage pid must surface as corruption rather than
        // collapsing to "no pid" (which would read as a recoverable dead lock).
        fs::write(
            &path,
            "version=1\ntxid=tx\njournal=j\npid=not-a-number\n",
        )
        .unwrap();

        let err = match read_install_lock(&game_root) {
            Ok(_) => panic!("expected a corruption error for a non-numeric pid"),
            Err(err) => err.to_string(),
        };
        assert!(err.contains("corrupt"), "{err}");
        assert!(err.contains("pid"), "{err}");
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
