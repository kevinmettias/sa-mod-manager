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
    #[cfg(windows)]
    {
        windows_process_is_running(pid)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn windows_process_is_running(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    let output = Command::new("tasklist").arg("/FI").arg(filter).output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().any(|line| line.contains(&pid.to_string()))
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
