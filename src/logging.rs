use crate::prelude::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};

/// Severity of a diagnostic message. Ordered so `Error` is the most severe and
/// always shown; higher variants are progressively more verbose.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Level
{
    Error = 0,
    Warn = 1,
    Info = 2,  // literal: allow external format or runtime boundary value means itself here
    Debug = 3, // literal: allow external format or runtime boundary value means itself here
    Trace = 4, // literal: allow external format or runtime boundary value means itself here
}

impl Level
{
    fn label(self) -> &'static str
    {
        return match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };
    }

    fn from_u8(value: u8) -> Self
    {
        return match value {
            0 => Level::Error,
            1 => Level::Warn,
            2 => Level::Info, // literal: allow external format or runtime boundary value means itself here
            3 => Level::Debug, // literal: allow external format or runtime boundary value means itself here
            _ => Level::Trace,
        };
    }

    /// Parse a config `log_level` name, ignoring case; unrecognized names yield
    /// `None` so the caller can fall back to the default.
    pub(crate) fn from_name(name: &str) -> Option<Self>
    {
        return match name.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Level::Error),
            "warn" | "warning" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "debug" => Some(Level::Debug),
            "trace" => Some(Level::Trace),
            _ => None,
        };
    }
}

/// Messages at or below the console level are printed to stderr. Defaults to
/// `Info`; `--verbose`/`--quiet` adjust it.
static CONSOLE_LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// The persistent log file, opened once per process under a game folder's state
/// directory. `None` until [`open_log_file`] succeeds.
static LOG_FILE: Mutex<Option<fs::File>> = Mutex::new(None);

/// The log file captures this level and below regardless of console verbosity,
/// so a quiet console still leaves a full diagnostic trail on disk.
const FILE_LEVEL: Level = Level::Debug;

pub(crate) fn set_console_level(level: Level)
{
    // atomic-ordering: allow: console level is a standalone byte, not a data publication signal
    CONSOLE_LEVEL.store(level as u8, Ordering::Relaxed);
}

/// Open the persistent log under a game folder's state directory. The standard
/// entry point for commands that operate on a game root.
pub(crate) fn open_for_game_root(game_root: &Path)
{
    open_log_file(
        &state_directory(game_root)
            .join("logs")
            .join("sa-mod-manager.log"),
    );
}

/// Open the persistent log file (append) once per process. Repeat calls and any
/// I/O failure are ignored so logging never breaks a command.
pub(crate) fn open_log_file(path: &Path)
{
    let Ok(mut guard) = LOG_FILE.lock() else {
        return;
    };
    if guard.is_some()
    {
        return;
    }
    if let Some(parent) = path.parent()
    {
        if let Err(err) = fs::create_dir_all(parent)
        {
            eprintln!("WARN: could not create log directory {}: {err}", parent.display());
        }
    }
    rotate_if_over(path, MAX_LOG_BYTES);
    if let Ok(file) = fs::OpenOptions::new().create(true).append(true).open(path)
    {
        *guard = Some(file);
    }
}

/// Roll the log over once it passes this size, keeping one `.1` backup, so the
/// diagnostic trail is bounded to roughly twice this on disk.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// If `path` is larger than `max_bytes`, move it to its `.1` backup (replacing
/// any previous backup) so the next open starts fresh. Returns whether it
/// rotated. All I/O errors are treated as "did not rotate".
fn rotate_if_over(path: &Path, max_bytes: u64) -> bool
{
    let size = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if size <= max_bytes
    {
        return false;
    }
    let backup = log_backup_path(path);
    if let Err(err) = fs::remove_file(&backup)
        && err.kind() != io::ErrorKind::NotFound
    {
        eprintln!("WARN: could not remove old log backup {}: {err}", backup.display());
        return false;
    }
    return fs::rename(path, &backup).is_ok();
}

/// The rotated-backup path for a log file (`<name>.1`).
fn log_backup_path(path: &Path) -> PathBuf
{
    let mut name = path.as_os_str().to_owned();
    name.push(".1");
    return PathBuf::from(name);
}

/// Emit a diagnostic. Console output goes to stderr (keeping stdout clean for
/// command results); the log file additionally captures it with a timestamp.
pub(crate) fn record(level: Level, args: fmt::Arguments)
{
    if level <= console_level()
    {
        eprintln!("{args}");
    }
    if level <= FILE_LEVEL
    {
        write_to_file(level, args);
    }
}

fn console_level() -> Level
{
    // atomic-ordering: allow: loading the standalone console level does not synchronize other state
    return Level::from_u8(CONSOLE_LEVEL.load(Ordering::Relaxed));
}

fn write_to_file(level: Level, args: fmt::Arguments)
{
    let Ok(mut guard) = LOG_FILE.lock() else {
        return;
    };
    if let Some(file) = guard.as_mut()
    {
        // Human-readable UTC timestamp, matching how times are shown in the UI,
        // rather than a raw unix-seconds integer.
        let _ = writeln!(
            file,
            "{} {} {args}",
            human_datetime(unix_now()),
            level.label()
        );
    }
}

macro_rules! log_error {
    ($($arg:tt)*) => { $crate::logging::record($crate::logging::Level::Error, format_args!($($arg)*)) };
}
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::logging::record($crate::logging::Level::Warn, format_args!($($arg)*)) };
}
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::logging::record($crate::logging::Level::Info, format_args!($($arg)*)) };
}
macro_rules! log_debug {
    ($($arg:tt)*) => { $crate::logging::record($crate::logging::Level::Debug, format_args!($($arg)*)) };
}

pub(crate) use {log_debug, log_error, log_info, log_warn};

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn rotate_moves_oversized_log_to_backup_and_replaces_prior_backup()
    {
        let dir = env::temp_dir().join(format!(
            "sa-mod-manager-log-{}-{}",
            std::process::id(),
            unix_now()
        ));
        fs::create_dir_all(&dir)
            .expect("the test fixture is created before this assertion reads it");
        let log = dir.join("sa-mod-manager.log");
        let backup = log_backup_path(&log);

        // A small file is left in place.
        fs::write(&log, b"tiny")
            .expect("the test fixture is created before this assertion reads it");
        assert!(!rotate_if_over(&log, 100)); // literal: allow test fixture value is the specimen under judgment
        assert!(log.exists());
        assert!(!backup.exists());

        // An oversized file is moved to `.1` so the next open starts fresh.
        const OVERSIZED_LOG_BYTES: usize = 200;
        fs::write(&log, vec![b'x'; OVERSIZED_LOG_BYTES])
            .expect("the test fixture is created before this assertion reads it"); // literal: allow test fixture value is the specimen under judgment
        assert!(rotate_if_over(&log, 100)); // literal: allow test fixture value is the specimen under judgment
        assert!(!log.exists());
        assert_eq!(
            fs::read(&backup)
                .expect("the test fixture is created before this assertion reads it")
                .len(),
            OVERSIZED_LOG_BYTES
        ); // literal: allow test fixture value is the specimen under judgment

        // A second rotation replaces the previous backup rather than piling up.
        fs::write(&log, vec![b'y'; 150])
            .expect("the test fixture is created before this assertion reads it"); // literal: allow test fixture value is the specimen under judgment
        assert!(rotate_if_over(&log, 100)); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(
            fs::read(&backup).expect("the test fixture is created before this assertion reads it")
                [0],
            b'y'
        );

        fs::remove_dir_all(&dir)
            .expect("the test fixture is created before this assertion reads it");
    }
}



