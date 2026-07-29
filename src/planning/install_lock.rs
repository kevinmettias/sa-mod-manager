use crate::prelude::*;

use super::rollback::rollback_journal;

/// A record_log_message_from_arguments of the permanent install that currently holds the game folder.
///
/// Written when `apply_install_plan` begins and removed on success. If the
/// process dies before the record_log_message_from_arguments is removed, its presence marks an install
/// that was interrupted mid-transaction and can be rolled back with `recover`.
pub(crate) struct InstallLock
{
    pub(crate) txid: String,
    pub(crate) journal: PathBuf,
    pub(crate) pid: Option<u32>,
    /// Creation-time fingerprint of the owning process (Windows only). Lets the
    /// liveness check reject a recycled PID now belonging to another process.
    /// Absent in older locks and off Windows, where the plain PID check stands.
    pub(crate) pid_start: Option<u64>,
}

pub(crate) fn read_install_lock(game_root: &Path) -> Result<Option<InstallLock>, AppError>
{
    let path = install_lock_path(game_root);
    if !path.exists()
    {
        return Ok(None);
    }
    // The lock is a handful of `key=value` lines; cap the read so a corrupt or
    // oversized file fails cleanly instead of being slurped whole.
    let text = read_capped(&path, 64 * 1024) // literal: allow external format or runtime boundary value means itself here
        .with_context(|| format!("read install lock {}", path.display()))?;
    return parse_install_lock(&text);
}

fn parse_install_lock(text: &str) -> Result<Option<InstallLock>, AppError>
{
    let mut txid = None;
    let mut journal = None;
    let mut pid = None;
    let mut pid_start = None;
    for line in text.lines()
    {
        if let Some(value) = line.strip_prefix("txid=")
        {
            txid = Some(unescape_value(value));
        }
        else if let Some(value) = line.strip_prefix("journal=")
        {
            journal = Some(PathBuf::from(unescape_value(value)));
        }
        else if let Some(value) = line.strip_prefix("pid=")
        {
            // A present-but-unparseable pid is corruption, not "no pid": collapsing
            // it to None would make a live install look dead and invite a second
            // install to trample it, so surface it loudly instead.
            pid = Some(parse_lock_number(LockNumberField { value, name: "pid" })?);
        }
        else if let Some(value) = line.strip_prefix("pid_start=")
        {
            pid_start = Some(parse_lock_number(LockNumberField {
                value,
                name: "pid_start",
            })?);
        }
    }
    // A lock missing its required identity fields is treated as "no lock": the
    // marker is written before any file is modified, so a crash mid-write leaves
    // an incomplete lock with nothing yet to roll back.
    return match (txid, journal) {
        (Some(txid), Some(journal)) => Ok(Some(InstallLock {
            txid,
            journal,
            pid,
            pid_start,
        })),
        _ => Ok(None),
    };
}

fn parse_lock_number<Number>(field: LockNumberField<'_>) -> Result<Number, AppError>
where
    Number: std::str::FromStr,
    Number::Err: std::fmt::Debug,
{
    let value = field.value;
    let field = field.name;
    return value.trim().parse::<Number>().map_err(|err| {
        AppError::Usage(format!(
            "install lock is corrupt: `{field}` value `{}` is not a valid number; delete `install.lock` or run `recover`",
            value.trim()
        ))
        .context(format!("parse install lock `{field}` value: {err:?}"))
    });
}

struct LockNumberField<'a>
{
    value: &'a str,
    name: &'a str,
}

/// Truncating writer used by tests to seed a lock in an arbitrary state.
/// Production code claims the lock atomically in [`acquire_install_lock`].
#[cfg(test)]
fn write_install_lock(
    game_root: &Path,
    txid: &str,
    journal: &Path,
    pid: u32,
) -> Result<(), AppError>
{
    let path = install_lock_path(game_root);
    ensure_lock_parent(&path)?;
    let mut file = fs::File::create(&path)
        .with_context(|| format!("create install lock {}", path.display()))?;
    return write_lock_fields(&mut file, txid, journal, pid)
        .with_context(|| format!("write install lock {}", path.display()));
}

fn unknown_pid(lock: &InstallLock) -> String
{
    return lock
        .pid
        .map(|pid| pid.to_string())
        .unwrap_or_else(|| "unknown".to_string());
}

/// Claim the game folder for a permanent install.
///
/// Refuses if another install is already running (concurrency guard) or if a
/// previous install was interrupted and not yet recovered.
pub(crate) fn acquire_install_lock(
    game_root: &Path,
    txid: &str,
    journal: &Path,
) -> Result<(), AppError>
{
    if let Some(existing) = read_install_lock(game_root)?
    {
        return Err(lock_conflict_error(&existing));
    }
    let path = install_lock_path(game_root);
    ensure_lock_parent(&path)?;
    // Create the marker atomically: `create_new` fails if the file already
    // exists, so a second install that raced past the check above cannot
    // silently overwrite our record_log_message_from_arguments (or we, theirs). The check above still runs
    // first only to produce a friendlier message in the common, uncontended case.
    return match fs::OpenOptions::new()
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
            match read_install_lock(game_root)?
            {
                Some(existing) => Err(lock_conflict_error(&existing)),
                None => Err(AppError::Usage(
                    "another install claimed the game folder concurrently; try again".to_string(),
                )),
            }
        }
        Err(err) => {
            Err(AppError::from(err).context(format!("create install lock {}", path.display())))
        }
    };
}

fn lock_conflict_error(existing: &InstallLock) -> AppError
{
    if is_lock_owner_running(existing)
    {
        return AppError::Usage(format!(
            "another install is already in progress (pid {}); wait for it to finish before installing again",
            unknown_pid(existing)
        ));
    }
    return AppError::Usage(format!(
        "a previous install (txid {}) was interrupted and left the game folder partially modified; run `recover` to roll it back before installing again",
        existing.txid
    ));
}

pub(crate) fn release_install_lock(game_root: &Path) -> Result<(), AppError>
{
    let path = install_lock_path(game_root);
    if path.exists()
    {
        fs::remove_file(&path)
            .with_context(|| format!("remove_profile_fixture install lock {}", path.display()))?;
    }
    return Ok(());
}

/// Report an interrupted install (lock present, owning process gone), if any.
pub(crate) fn interrupted_install(game_root: &Path) -> Result<Option<InstallLock>, AppError>
{
    return match read_install_lock(game_root)? {
        Some(lock) if !is_lock_owner_running(&lock) => Ok(Some(lock)),
        _ => Ok(None),
    };
}

/// Roll back an interrupted install and clear its lock.
///
/// Returns the recovered transaction id, or `None` when there is nothing to
/// recover. Refuses while the owning install is still running.
pub(crate) fn recover_interrupted_install(game_root: &Path) -> Result<Option<String>, AppError>
{
    let Some(lock) = read_install_lock(game_root)? else {
        return Ok(None);
    };
    if is_lock_owner_running(&lock)
    {
        return Err(AppError::Usage(format!(
            "an install is currently in progress (pid {}); cannot recover while it is running",
            unknown_pid(&lock)
        )));
    }
    if lock.journal.exists()
    {
        rollback_journal(&lock.journal, game_root)?;
    }
    release_install_lock(game_root)?;
    return Ok(Some(lock.txid));
}

fn install_lock_path(game_root: &Path) -> PathBuf
{
    return state_directory(game_root).join("install.lock");
}

fn write_lock_fields(
    file: &mut fs::File,
    txid: &str,
    journal: &Path,
    pid: u32,
) -> Result<(), AppError>
{
    writeln!(file, "version=1")?;
    writeln!(file, "txid={}", escape_value(txid))?;
    writeln!(
        file,
        "journal={}",
        escape_value(&journal.display().to_string())
    )?;
    writeln!(file, "pid={pid}")?;
    if let Some(start) = crate::game_launch::current_program_start_ticks()
    {
        writeln!(file, "pid_start={start}")?;
    }
    writeln!(file, "created_unix={}", unix_now())?;
    // Persist the marker durably so a crash immediately after it is written is
    // still detectable as an interrupted install. We fsync the file's contents
    // but not the parent directory entry: this targets Windows/NTFS, whose
    // metadata is journaled, and Windows offers no reliable directory-fsync
    // (`FlushFileBuffers` on a directory handle is unsupported) â€” PostgreSQL and
    // SQLite skip directory fsync on Windows for the same reason.
    file.sync_all()?;
    return Ok(());
}

fn ensure_lock_parent(path: &Path) -> Result<(), AppError>
{
    if let Some(parent) = path.parent()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create state directory {}", parent.display()))?;
    }
    return Ok(());
}

fn is_lock_owner_running(lock: &InstallLock) -> bool
{
    let Some(pid) = lock.pid else {
        return false;
    };
    if !crate::game_launch::is_child_program_running(pid)
    {
        return false;
    }
    // If the owner's creation-time fingerprint was recorded, require the live
    // PID to still carry it. A mismatch means the original install process
    // exited and the OS handed its PID to an unrelated process, so the install
    // is not actually running and recovery must not be blocked on it.
    return match lock.pid_start {
        Some(expected) => crate::game_launch::program_start_ticks(pid) == Some(expected),
        None => true,
    };
}

#[cfg(test)]
include!("install_lock_tests_01.rs");
