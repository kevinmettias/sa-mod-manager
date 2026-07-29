use crate::prelude::*;

pub(crate) fn init_state(game_root: &Path) -> Result<(), AppError>
{
    ensure_gta_install(game_root)?;
    ensure_state(game_root)?;
    println!("initialized: {}", state_directory(game_root).display());
    return Ok(());
}

pub(crate) fn ensure_state(game_root: &Path) -> Result<(), AppError>
{
    let root = state_directory(game_root);
    let state_children = [
        "profiles", "packages", "plans", "journals", "backups", "staging", "logs",
    ];
    for child in state_children
    {
        let child_path = root.join(child);
        fs::create_dir_all(child_path)?;
    }

    // Route this process's diagnostics to a persistent log under the state dir.
    crate::logging::open_for_game_root(game_root);

    let default_profile = root.join("profiles").join("default.json");
    if !default_profile.exists()
    {
        let profile = ProfileJson {
            name: "default".to_string(),
            ..Default::default()
        };
        write_profile_json_file(&default_profile, game_root, &profile)?;
    }

    return Ok(());
}
