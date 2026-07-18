use crate::prelude::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};

/// Severity of a diagnostic message. Ordered so `Error` is the most severe and
/// always shown; higher variants are progressively more verbose.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => Level::Error,
            1 => Level::Warn,
            2 => Level::Info,
            3 => Level::Debug,
            _ => Level::Trace,
        }
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

pub(crate) fn set_console_level(level: Level) {
    CONSOLE_LEVEL.store(level as u8, Ordering::Relaxed);
}

fn console_level() -> Level {
    Level::from_u8(CONSOLE_LEVEL.load(Ordering::Relaxed))
}

/// Open the persistent log under a game folder's state directory. The standard
/// entry point for commands that operate on a game root.
pub(crate) fn open_for_game_root(game_root: &Path) {
    open_log_file(&state_directory(game_root).join("logs").join("sa-mod-manager.log"));
}

/// Open the persistent log file (append) once per process. Repeat calls and any
/// I/O failure are ignored so logging never breaks a command.
pub(crate) fn open_log_file(path: &Path) {
    let Ok(mut guard) = LOG_FILE.lock() else {
        return;
    };
    if guard.is_some() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        *guard = Some(file);
    }
}

/// Emit a diagnostic. Console output goes to stderr (keeping stdout clean for
/// command results); the log file additionally captures it with a timestamp.
pub(crate) fn record(level: Level, args: fmt::Arguments) {
    if level <= console_level() {
        eprintln!("{args}");
    }
    if level <= FILE_LEVEL {
        write_to_file(level, args);
    }
}

fn write_to_file(level: Level, args: fmt::Arguments) {
    let Ok(mut guard) = LOG_FILE.lock() else {
        return;
    };
    if let Some(file) = guard.as_mut() {
        let _ = writeln!(file, "{} {} {args}", unix_now(), level.label());
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
