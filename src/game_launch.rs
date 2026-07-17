use crate::prelude::*;

pub(crate) fn launch_game_executable(game_root: &Path) -> Result<std::process::Child, AppError> {
    let executable = game_executable_path(game_root).ok_or_else(|| {
        AppError::Usage(format!(
            "game executable not found under {}",
            game_root.display()
        ))
    })?;
    let child = Command::new(&executable).current_dir(game_root).spawn()?;
    Ok(child)
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
