use crate::prelude::*;

pub(crate) fn launch_game_executable_from_arguments(
    game_root: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<std::process::Child, AppError>
{
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
    return Ok(child);
}

/// Launch a user-configured external tool (MO2's "executables"): spawn it from
/// its own directory so relative resources resolve, and detach â€” unlike the
/// game, tools are not materialized, watched, or cleaned up.
pub(crate) fn launch_external_tool_from_arguments(
    path: &Path,
    args: &[String],
) -> Result<std::process::Child, AppError>
{
    if !path.exists()
    {
        return Err(AppError::Usage(format!(
            "tool executable not found: {}",
            path.display()
        )));
    }
    let working_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let child = Command::new(path)
        .current_dir(working_dir)
        .args(args)
        .spawn()?;
    return Ok(child);
}

/// Guard destructive/state-creating operations against a wrong `game_root`.
///
/// Refuses to scaffold manager state or copy files into a directory that does
/// not look like a GTA San Andreas install (no `gta_sa.exe`/`gta-sa.exe`), so a
/// mistyped `--game <path>` or UI game-folder field cannot write into an
/// unrelated directory.
pub(crate) fn ensure_gta_install(game_root: &Path) -> Result<(), AppError>
{
    if game_executable_path(game_root).is_some()
    {
        return Ok(());
    }
    return Err(AppError::Usage(format!(
        "`{}` does not look like a GTA San Andreas install (expected gta_sa.exe or gta-sa.exe); point --game or the game folder at the real install",
        game_root.display()
    )));
}

pub(crate) fn game_executable_path(game_root: &Path) -> Option<PathBuf>
{
    for name in ["gta_sa.exe", "gta-sa.exe"]
    {
        let candidate = game_root.join(name);
        if candidate.exists()
        {
            return Some(candidate);
        }
    }
    return None;
}

/// Best-effort check for whether a process id is currently alive.
///
/// Used to tell an install that is genuinely still running from one whose
/// process died mid-transaction (leaving an interrupted install to recover).
/// Non-Windows targets conservatively report `false`.
pub(crate) fn is_child_program_running(pid: u32) -> bool
{
    if pid == 0
    {
        return false;
    }
    return program_start_ticks(pid).is_some();
}

/// A Windows creation-time fingerprint for `pid`, used to tell a live process
/// apart from a *different* process that later reused the same PID.
///
/// Returns `None` when the process cannot be opened (gone, or not ours to
/// query) and always `None` off Windows. This replaces spawning `tasklist`:
/// `OpenProcess` is cheaper, needs no output parsing, and cannot be fooled by a
/// PID's digits appearing in another column of a text table.
pub(crate) fn program_start_ticks(pid: u32) -> Option<u64>
{
    if pid == 0
    {
        return None;
    }
    #[cfg(windows)]
    {
        winproc::program_start_ticks(pid)
    }
    #[cfg(not(windows))]
    return {
        None;
    };
}

/// Creation-time fingerprint of the current process, recorded in the install
/// lock so a later PID reuse can be detected on recovery. `None` off Windows.
pub(crate) fn current_program_start_ticks() -> Option<u64>
{
    #[cfg(windows)]
    {
        winproc::current_program_start_ticks()
    }
    #[cfg(not(windows))]
    return {
        None;
    };
}

/// Minimal, dependency-free FFI to `kernel32` for process liveness and the
/// creation-time fingerprint used to detect PID reuse. Declared inline rather
/// than pulling in the `windows`/`winapi` crates, matching this project's
/// deliberately small dependency set.
#[cfg(windows)]
mod winproc
{
    use std::os::raw::c_void;

    type Handle = *mut c_void;
    #[repr(transparent)]
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Bool(i32);

    #[repr(transparent)]
    #[derive(Clone, Copy, Default)]
    struct Dword(u32);

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Filetime
    {
        low_date_time: Dword,
        high_date_time: Dword,
    }

    // Enough access to read process timing; grantable for our own install
    // processes without elevated rights (unlike PROCESS_QUERY_INFORMATION).
    const PROGRAM_QUERY_LIMITED_INFORMATION: Dword = Dword(0x1000);

    unsafe extern "system" {
        #[link_name = "OpenProcess"]
        fn open_program_handle(desired_access: Dword, inherit_handle: Bool, process_id: Dword) -> Handle;
        fn CloseHandle(object: Handle) -> Bool;
        #[link_name = "GetCurrentProcess"]
        fn current_program_handle() -> Handle;
        fn GetModuleHandleA(module_name: *const u8) -> Handle;
        fn GetProcAddress(module: Handle, proc_name: *const u8) -> *const c_void;
    }

    type GetProgramTimesFn = unsafe extern "system" fn(
        Handle,
        *mut Filetime,
        *mut Filetime,
        *mut Filetime,
        *mut Filetime,
    ) -> Bool;

    pub(super) fn program_start_ticks(pid: u32) -> Option<u64>
    {
        // SAFETY: FFI call taking a plain PID; returns a null handle on failure.
        let handle = unsafe { open_program_handle(PROGRAM_QUERY_LIMITED_INFORMATION, Bool(0), Dword(pid)) };
        if handle.is_null()
        {
            return None;
        }
        let ticks = creation_ticks(handle);
        // SAFETY: `handle` came from the `OpenProcess` above and is closed once.
        unsafe { CloseHandle(handle) };
        return ticks;
    }

    pub(super) fn current_program_start_ticks() -> Option<u64>
    {
        // `GetCurrentProcess` returns a pseudo-handle that must NOT be closed.
        // SAFETY: the pseudo-handle is always valid for the current process.
        let handle = unsafe { current_program_handle() };
        return creation_ticks(handle);
    }

    fn get_program_times_fn() -> Option<GetProgramTimesFn>
    {
        // SAFETY: kernel32 is loaded in every normal Windows process; the symbol name is NUL-terminated.
        let module = unsafe { GetModuleHandleA(b"kernel32.dll\0".as_ptr()) };
        if module.is_null()
        {
            return None;
        }
        // SAFETY: the lookup reads a static NUL-terminated symbol name and returns an opaque address.
        let proc = unsafe { GetProcAddress(module, b"GetProcessTimes\0".as_ptr()) };
        if proc.is_null()
        {
            return None;
        }
        // SAFETY: kernel32!GetProcessTimes has the exact ABI represented by GetProgramTimesFn.
        return Some(unsafe { std::mem::transmute::<*const c_void, GetProgramTimesFn>(proc) });
    }

    fn creation_ticks(handle: Handle) -> Option<u64>
    {
        let get_program_times = get_program_times_fn()?;
        let mut creation = Filetime::default();
        let mut exit = Filetime::default();
        let mut kernel = Filetime::default();
        let mut user = Filetime::default();
        // SAFETY: `handle` is a valid process handle for the call's duration and
        // every out-pointer references distinct stack storage we own.
        let ok = unsafe {
            // unsafe: allow FFI call uses a valid process handle and distinct out-pointers
            get_program_times(handle, &mut creation, &mut exit, &mut kernel, &mut user)
        };
        if ok == Bool(0)
        {
            return None;
        }
        const FILETIME_LOW_WORD_BITS: u32 = 32;
        return Some(
            (u64::from(creation.high_date_time.0) << FILETIME_LOW_WORD_BITS)
                | u64::from(creation.low_date_time.0),
        );
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn ensure_gta_install_rejects_directory_without_executable()
    {
        let root = temporary_root("ensure_gta_missing");
        let err = ensure_gta_install(&root).unwrap_err().to_string();
        assert!(err.contains("does not look like a GTA San Andreas install"));
        fs::remove_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn ensure_gta_install_accepts_either_executable_name()
    {
        for exe in ["gta_sa.exe", "gta-sa.exe"]
        {
            let root = temporary_root(&format!("ensure_gta_{exe}"));
            fs::write(root.join(exe), b"")
                .expect("the test fixture is created before this assertion reads it");
            assert!(ensure_gta_install(&root).is_ok());
            fs::remove_dir_all(&root)
                .expect("the test fixture is created before this assertion reads it");
        }
    }

    fn temporary_root(name: &str) -> PathBuf
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        if root.exists()
        {
            fs::remove_dir_all(&root)
                .expect("the test fixture is created before this assertion reads it");
        }
        fs::create_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
        return root;
    }
}
