use crate::prelude::*;

pub(crate) fn init_state(game_root: &Path) -> Result<(), AppError> {
    ensure_state(game_root)?;
    println!("initialized: {}", state_directory(game_root).display());
    Ok(())
}

pub(crate) fn ensure_state(game_root: &Path) -> Result<(), AppError> {
    let root = state_directory(game_root);
    let state_children = [
        /* literal: allow external interface text or file-format spelling */ "profiles",
        /* literal: allow external interface text or file-format spelling */ "packages",
        "plans", "journals", "backups",
        "staging", // literal: allow external interface text or file-format spelling
    ];
    for child in state_children {
        let child_path = root.join(child);
        fs::create_dir_all(child_path)?;
    }

    let default_profile = root.join("profiles").join("default.json"); // literal: allow external interface text or file-format spelling
    if !default_profile.exists() {
        write_profile_json_file(&default_profile, game_root, "default", &[])?; // literal: allow external interface text or file-format spelling
    }

    Ok(())
}
