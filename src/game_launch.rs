use crate::prelude::*;

pub(crate) fn launch_game_executable(
    game_root: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<std::process::Child, AppError> {
    let executable = game_executable_path(game_root).ok_or_else(|| {
        AppError::Usage(format!(
            "game executable not found under {}",
            game_root.display()
        ))
    })?;
    let child = Command::new(&executable)
        .current_dir(game_root)
        .args(args)
        .envs(env)
        .spawn()?;
    Ok(child)
}

/// Guard destructive/state-creating operations against a wrong `game_root`.
///
/// Refuses to scaffold manager state or copy files into a directory that does
/// not look like a GTA San Andreas install (no `gta_sa.exe`/`gta-sa.exe`), so a
/// mistyped `--game <path>` or UI game-folder field cannot write into an
/// unrelated directory.
pub(crate) fn ensure_gta_install(game_root: &Path) -> Result<(), AppError> {
    if game_executable_path(game_root).is_some() {
        return Ok(());
    }
    Err(AppError::Usage(format!(
        "`{}` does not look like a GTA San Andreas install (expected gta_sa.exe or gta-sa.exe); point --game or the game folder at the real install",
        game_root.display()
    )))
}

pub(crate) fn game_executable_path(game_root: &Path) -> Option<PathBuf> {
    for name in [
        "gta_sa.exe", // literal: allow external interface text or file-format spelling
        "gta-sa.exe", // literal: allow external interface text or file-format spelling
    ] {
        let candidate = game_root.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Best-effort check for whether a process id is currently alive.
///
/// Used to tell an install that is genuinely still running from one whose
/// process died mid-transaction (leaving an interrupted install to recover).
/// Non-Windows targets conservatively report `false`.
pub(crate) fn process_is_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    process_start_ticks(pid).is_some()
}

/// A Windows creation-time fingerprint for `pid`, used to tell a live process
/// apart from a *different* process that later reused the same PID.
///
/// Returns `None` when the process cannot be opened (gone, or not ours to
/// query) and always `None` off Windows. This replaces spawning `tasklist`:
/// `OpenProcess` is cheaper, needs no output parsing, and cannot be fooled by a
/// PID's digits appearing in another column of a text table.
pub(crate) fn process_start_ticks(pid: u32) -> Option<u64> {
    if pid == 0 {
        return None;
    }
    #[cfg(windows)]
    {
        winproc::process_start_ticks(pid)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Creation-time fingerprint of the current process, recorded in the install
/// lock so a later PID reuse can be detected on recovery. `None` off Windows.
pub(crate) fn current_process_start_ticks() -> Option<u64> {
    #[cfg(windows)]
    {
        winproc::current_process_start_ticks()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Minimal, dependency-free FFI to `kernel32` for process liveness and the
/// creation-time fingerprint used to detect PID reuse. Declared inline rather
/// than pulling in the `windows`/`winapi` crates, matching this project's
/// deliberately small dependency set.
#[cfg(windows)]
mod winproc {
    use std::os::raw::c_void;

    type Handle = *mut c_void;
    type Bool = i32;
    type Dword = u32;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Filetime {
        low_date_time: Dword,
        high_date_time: Dword,
    }

    // Enough access to read process timing; grantable for our own install
    // processes without elevated rights (unlike PROCESS_QUERY_INFORMATION).
    const PROCESS_QUERY_LIMITED_INFORMATION: Dword = 0x1000;

    unsafe extern "system" {
        fn OpenProcess(desired_access: Dword, inherit_handle: Bool, process_id: Dword) -> Handle;
        fn CloseHandle(object: Handle) -> Bool;
        fn GetCurrentProcess() -> Handle;
        fn GetProcessTimes(
            process: Handle,
            creation: *mut Filetime,
            exit: *mut Filetime,
            kernel: *mut Filetime,
            user: *mut Filetime,
        ) -> Bool;
    }

    fn creation_ticks(handle: Handle) -> Option<u64> {
        let mut creation = Filetime::default();
        let mut exit = Filetime::default();
        let mut kernel = Filetime::default();
        let mut user = Filetime::default();
        // SAFETY: `handle` is a valid process handle for the call's duration and
        // every out-pointer references distinct stack storage we own.
        let ok = unsafe {
            GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user)
        };
        if ok == 0 {
            return None;
        }
        Some((u64::from(creation.high_date_time) << 32) | u64::from(creation.low_date_time))
    }

    pub(super) fn process_start_ticks(pid: u32) -> Option<u64> {
        // SAFETY: FFI call taking a plain PID; returns a null handle on failure.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let ticks = creation_ticks(handle);
        // SAFETY: `handle` came from the `OpenProcess` above and is closed once.
        unsafe { CloseHandle(handle) };
        ticks
    }

    pub(super) fn current_process_start_ticks() -> Option<u64> {
        // `GetCurrentProcess` returns a pseudo-handle that must NOT be closed.
        // SAFETY: the pseudo-handle is always valid for the current process.
        let handle = unsafe { GetCurrentProcess() };
        creation_ticks(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn ensure_gta_install_rejects_directory_without_executable() {
        let root = temp_root("ensure_gta_missing");
        let err = ensure_gta_install(&root).unwrap_err().to_string();
        assert!(err.contains("does not look like a GTA San Andreas install"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ensure_gta_install_accepts_either_executable_name() {
        for exe in ["gta_sa.exe", "gta-sa.exe"] {
            let root = temp_root(&format!("ensure_gta_{exe}"));
            fs::write(root.join(exe), b"").unwrap();
            assert!(ensure_gta_install(&root).is_ok());
            fs::remove_dir_all(&root).unwrap();
        }
    }
}
