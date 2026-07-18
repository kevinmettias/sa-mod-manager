use crate::prelude::*;

pub(crate) fn list_profiles(game_root: &Path) -> Result<(), AppError> {
    let profiles = state_directory(game_root).join("profiles"); // literal: allow external interface text or file-format spelling
    if !profiles.exists() {
        println!("no profiles found; run `init` first");
        return Ok(());
    }

    let found = profile_files(&profiles)?;
    print_profile_files(found, &read_active_profile(game_root));
    Ok(())
}

fn profile_files(profiles: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut found = Vec::new();
    for entry in fs::read_dir(&profiles)? {
        let entry = entry?;
        let path = entry.path();
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        if extension_eq(&path, "profile") || extension_eq(&path, "json") {
            // literal: allow external interface text or file-format spelling
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

fn print_profile_files(found: Vec<PathBuf>, active: &str) {
    if found.is_empty() {
        println!("no profiles found");
    } else {
        for profile in found {
            let name = profile
                .file_stem()
                .and_then(OsStr::to_str)
                .unwrap_or("unknown");
            let marker = if name == active { " (active)" } else { "" };
            println!("{name}{marker}");
        }
    }
}

pub(crate) fn create_profile(game_root: &Path, name: &str) -> Result<(), AppError> {
    ensure_state(game_root)?;
    let safe = safe_name(name);
    let path = state_directory(game_root)
        .join("profiles") // literal: allow external interface text or file-format spelling
        .join(format!("{safe}.json"));
    if path.exists() {
        return Err(AppError::Usage(format!("profile already exists: {safe}")));
    }
    let profile = ProfileJson {
        name: safe.clone(),
        ..Default::default()
    };
    write_profile_json(game_root, &profile)?;
    println!("created profile: {safe}");
    Ok(())
}

pub(crate) fn write_profile_json(game_root: &Path, profile: &ProfileJson) -> Result<(), AppError> {
    ensure_state(game_root)?;
    let path = state_directory(game_root)
        .join("profiles") // literal: allow external interface text or file-format spelling
        .join(format!("{}.json", profile.name));
    write_profile_json_file(&path, game_root, profile)?;
    println!("profile json: {}", path.display());
    Ok(())
}

pub(crate) fn write_profile_json_file(
    path: &Path,
    game_root: &Path,
    profile: &ProfileJson,
) -> Result<(), AppError> {
    let mut file = fs::File::create(path)?;
    writeln!(file, "{{")?;
    writeln!(file, "  \"version\": 1,")?;
    writeln!(file, "  \"name\": \"{}\",", json_escape(&profile.name))?;
    writeln!(
        file,
        "  \"game_root\": \"{}\",",
        json_escape(&game_root.display().to_string())
    )?;
    writeln!(file, "  \"ephemeral\": true,")?;
    write_profile_launch_args(&mut file, &profile.launch_args)?;
    write_profile_launch_env(&mut file, &profile.launch_env)?;
    writeln!(file, "  \"mods\": [")?;
    write_profile_json_entries(&mut file, &profile.mods)?;
    writeln!(file, "  ]")?;
    writeln!(file, "}}")?;
    Ok(())
}

fn write_profile_launch_args(file: &mut fs::File, args: &[String]) -> Result<(), AppError> {
    if args.is_empty() {
        writeln!(file, "  \"launch_args\": [],")?;
        return Ok(());
    }
    writeln!(file, "  \"launch_args\": [")?;
    for (idx, arg) in args.iter().enumerate() {
        let comma = if idx + 1 != args.len() { "," } else { "" };
        writeln!(file, "    \"{}\"{comma}", json_escape(arg))?;
    }
    writeln!(file, "  ],")?;
    Ok(())
}

fn write_profile_launch_env(
    file: &mut fs::File,
    env: &BTreeMap<String, String>,
) -> Result<(), AppError> {
    if env.is_empty() {
        writeln!(file, "  \"launch_env\": {{}},")?;
        return Ok(());
    }
    writeln!(file, "  \"launch_env\": {{")?;
    for (idx, (key, value)) in env.iter().enumerate() {
        let comma = if idx + 1 != env.len() { "," } else { "" };
        writeln!(
            file,
            "    \"{}\": \"{}\"{comma}",
            json_escape(key),
            json_escape(value)
        )?;
    }
    writeln!(file, "  }},")?;
    Ok(())
}

fn write_profile_json_entries(
    file: &mut fs::File,
    mods: &[ProfileModEntry],
) -> Result<(), AppError> {
    for (idx, entry) in mods.iter().enumerate() {
        writeln!(file, "    {{")?;
        writeln!(file, "      \"id\": \"{}\",", json_escape(&entry.id))?;
        writeln!(file, "      \"enabled\": {},", entry.enabled)?;
        writeln!(file, "      \"load_order\": {},", entry.load_order)?;
        writeln!(
            file,
            "      \"config\": \"{}\",",
            json_escape(&entry.config.display().to_string())
        )?;
        write_profile_entry_root_overrides(file, &entry.root_overrides)?;
        write!(file, "    }}")?;
        if idx + 1 != mods.len() {
            writeln!(file, ",")?;
        } else {
            writeln!(file)?;
        }
    }
    Ok(())
}

fn write_profile_entry_root_overrides(
    file: &mut fs::File,
    overrides: &BTreeMap<String, bool>,
) -> Result<(), AppError> {
    if overrides.is_empty() {
        writeln!(file, "      \"root_overrides\": {{}}")?;
        return Ok(());
    }
    writeln!(file, "      \"root_overrides\": {{")?;
    for (idx, (source, enabled)) in overrides.iter().enumerate() {
        let comma = if idx + 1 != overrides.len() { "," } else { "" };
        writeln!(file, "        \"{}\": {enabled}{comma}", json_escape(source))?;
    }
    writeln!(file, "      }}")?;
    Ok(())
}

pub(crate) fn add_mod_to_profile_json(
    game_root: &Path,
    profile_name: &str,
    config_path: &Path,
) -> Result<(), AppError> {
    ensure_state(game_root)?;
    let profile_path = state_directory(game_root)
        .join("profiles") // literal: allow external interface text or file-format spelling
        .join(format!("{profile_name}.json"));
    let mut profile = if profile_path.exists() {
        read_profile_json(&profile_path)?
    } else {
        ProfileJson {
            name: profile_name.to_string(),
            ..Default::default()
        }
    };
    let config = read_mod_config_json(config_path)?;
    profile.mods.retain(|entry| entry.id != config.id);
    let next_order = profile
        .mods
        .iter()
        .map(|entry| entry.load_order)
        .max()
        .unwrap_or(0)
        + 100;
    profile.mods.push(ProfileModEntry {
        id: config.id,
        enabled: true,
        load_order: next_order,
        config: config_path.to_path_buf(),
        ..Default::default()
    });
    profile.mods.sort_by(|a, b| {
        a.load_order
            .cmp(&b.load_order)
            .then_with(|| a.id.cmp(&b.id))
    });
    write_profile_json(game_root, &profile)
}

pub(crate) fn show_profile_json(game_root: &Path, profile_name: &str) -> Result<(), AppError> {
    let profile = load_profile_for_edit(game_root, profile_name)?;
    println!("profile: {}", profile.name);
    println!("mods   : {}", profile.mods.len());
    println!();

    let mut mods = profile.mods;
    mods.sort_by(|a, b| {
        a.load_order
            .cmp(&b.load_order)
            .then_with(|| a.id.cmp(&b.id))
    });

    if mods.is_empty() {
        println!("no mods in profile");
        return Ok(());
    }

    print_profile_mod_entries(mods);
    Ok(())
}

fn print_profile_mod_entries(mods: Vec<ProfileModEntry>) {
    for entry in mods {
        println!(
            "{:>5}  {:8}  {:24}  {}",
            entry.load_order,
            if entry.enabled { "enabled" } else { "disabled" },
            entry.id,
            entry.config.display()
        );
    }
}

pub(crate) fn load_profile_for_edit(
    game_root: &Path,
    profile_name: &str,
) -> Result<ProfileJson, AppError> {
    let profile_path = profile_json_path(game_root, profile_name);
    if !profile_path.exists() {
        return Err(AppError::Usage(format!(
            "profile json not found: {}",
            profile_path.display()
        )));
    }
    read_profile_json(&profile_path)
}

fn profiles_directory(game_root: &Path) -> PathBuf {
    state_directory(game_root).join("profiles") // literal: allow external interface text or file-format spelling
}

fn profile_json_path(game_root: &Path, profile_name: &str) -> PathBuf {
    profiles_directory(game_root).join(format!("{profile_name}.json"))
}

fn active_profile_path(game_root: &Path) -> PathBuf {
    state_directory(game_root).join("active-profile")
}

/// The profile selected as active for this game folder, or `default` when none
/// has been chosen. Infallible: a missing or unreadable marker falls back to
/// `default`.
pub(crate) fn read_active_profile(game_root: &Path) -> String {
    match fs::read_to_string(active_profile_path(game_root)) {
        Ok(text) => {
            let name = text.trim();
            if name.is_empty() {
                "default".to_string()
            } else {
                safe_name(name)
            }
        }
        Err(_) => "default".to_string(),
    }
}

pub(crate) fn set_active_profile(game_root: &Path, name: &str) -> Result<(), AppError> {
    ensure_state(game_root)?;
    let safe = safe_name(name);
    if !profile_json_path(game_root, &safe).exists() {
        return Err(AppError::Usage(format!("profile json not found: {safe}")));
    }
    fs::write(active_profile_path(game_root), format!("{safe}\n"))?;
    println!("active profile: {safe}");
    Ok(())
}

/// The launch arguments and environment for a profile, defaulting to empty when
/// the profile file is absent so the caller can surface its own missing-profile
/// error.
pub(crate) fn profile_launch_settings(
    game_root: &Path,
    profile_name: &str,
) -> Result<(Vec<String>, BTreeMap<String, String>), AppError> {
    let profile_path = profile_json_path(game_root, profile_name);
    if !profile_path.exists() {
        return Ok((Vec::new(), BTreeMap::new()));
    }
    let profile = read_profile_json(&profile_path)?;
    Ok((profile.launch_args, profile.launch_env))
}

pub(crate) fn set_profile_launch_args(
    game_root: &Path,
    profile_name: &str,
    args: Vec<String>,
) -> Result<(), AppError> {
    let mut profile = load_profile_for_edit(game_root, profile_name)?;
    profile.launch_args = args;
    write_profile_json(game_root, &profile)?;
    println!(
        "set {} launch args for profile `{profile_name}`",
        profile.launch_args.len()
    );
    Ok(())
}

pub(crate) fn set_profile_root_override(
    game_root: &Path,
    profile_name: &str,
    mod_id: &str,
    source: &str,
    enabled: bool,
) -> Result<(), AppError> {
    let mut profile = load_profile_for_edit(game_root, profile_name)?;
    let Some(entry) = profile.mods.iter_mut().find(|entry| entry.id == mod_id) else {
        return Err(AppError::Usage(format!(
            "mod `{mod_id}` is not in profile `{profile_name}`"
        )));
    };
    entry.root_overrides.insert(source.to_string(), enabled);
    write_profile_json(game_root, &profile)?;
    println!(
        "set root `{source}` of `{mod_id}` to {} in profile `{profile_name}`",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

pub(crate) fn copy_profile(
    game_root: &Path,
    source_name: &str,
    dest_name: &str,
) -> Result<(), AppError> {
    let source = load_profile_for_edit(game_root, source_name)?;
    let dest_safe = safe_name(dest_name);
    if profile_json_path(game_root, &dest_safe).exists() {
        return Err(AppError::Usage(format!(
            "profile already exists: {dest_safe}"
        )));
    }
    let profile = ProfileJson {
        name: dest_safe.clone(),
        mods: source.mods,
        launch_args: source.launch_args,
        launch_env: source.launch_env,
    };
    write_profile_json(game_root, &profile)?;
    println!("copied profile {source_name} -> {dest_safe}");
    Ok(())
}

pub(crate) fn rename_profile(
    game_root: &Path,
    old_name: &str,
    new_name: &str,
) -> Result<(), AppError> {
    let old_safe = safe_name(old_name);
    if old_safe == "default" {
        return Err(AppError::Usage(
            "cannot rename the default profile".to_string(),
        ));
    }
    copy_profile(game_root, &old_safe, new_name)?;
    fs::remove_file(profile_json_path(game_root, &old_safe))?;
    if read_active_profile(game_root) == old_safe {
        set_active_profile(game_root, &safe_name(new_name))?;
    }
    println!("renamed profile {old_safe} -> {}", safe_name(new_name));
    Ok(())
}

pub(crate) fn delete_profile(game_root: &Path, name: &str) -> Result<(), AppError> {
    let safe = safe_name(name);
    if safe == "default" {
        return Err(AppError::Usage(
            "cannot delete the default profile".to_string(),
        ));
    }
    let path = profile_json_path(game_root, &safe);
    if !path.exists() {
        return Err(AppError::Usage(format!(
            "profile json not found: {}",
            path.display()
        )));
    }
    fs::remove_file(&path)?;
    if read_active_profile(game_root) == safe {
        set_active_profile(game_root, "default")?;
    }
    println!("deleted profile: {safe}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_profile_defaults_then_persists() {
        let game_root = test_root("active_profile");
        ensure_state(&game_root).unwrap();
        assert_eq!(read_active_profile(&game_root), "default");

        create_profile(&game_root, "racing").unwrap();
        set_active_profile(&game_root, "racing").unwrap();
        assert_eq!(read_active_profile(&game_root), "racing");
        remove(&game_root);
    }

    #[test]
    fn set_active_rejects_unknown_profile() {
        let game_root = test_root("active_unknown");
        ensure_state(&game_root).unwrap();
        let err = set_active_profile(&game_root, "ghost")
            .unwrap_err()
            .to_string();
        assert!(err.contains("profile json not found"));
        remove(&game_root);
    }

    #[test]
    fn copy_rename_delete_lifecycle_and_default_is_protected() {
        let game_root = test_root("profile_lifecycle");
        ensure_state(&game_root).unwrap();
        create_profile(&game_root, "base").unwrap();

        copy_profile(&game_root, "base", "clone").unwrap();
        assert!(profile_json_path(&game_root, "clone").exists());
        assert!(copy_profile(&game_root, "base", "clone").is_err()); // no clobber

        rename_profile(&game_root, "clone", "renamed").unwrap();
        assert!(!profile_json_path(&game_root, "clone").exists());
        assert!(profile_json_path(&game_root, "renamed").exists());

        delete_profile(&game_root, "renamed").unwrap();
        assert!(!profile_json_path(&game_root, "renamed").exists());

        assert!(delete_profile(&game_root, "default").is_err());
        assert!(rename_profile(&game_root, "default", "x").is_err());
        remove(&game_root);
    }

    #[test]
    fn deleting_active_profile_resets_active_to_default() {
        let game_root = test_root("delete_active");
        ensure_state(&game_root).unwrap();
        create_profile(&game_root, "temp").unwrap();
        set_active_profile(&game_root, "temp").unwrap();

        delete_profile(&game_root, "temp").unwrap();
        assert_eq!(read_active_profile(&game_root), "default");
        remove(&game_root);
    }

    #[test]
    fn launch_args_persist_and_survive_profile_edits() {
        let game_root = test_root("launch_args");
        ensure_state(&game_root).unwrap();
        create_profile(&game_root, "windowed").unwrap();
        let expected = vec!["-windowed".to_string(), "-nointro".to_string()];
        set_profile_launch_args(&game_root, "windowed", expected.clone()).unwrap();

        let (args, _env) = profile_launch_settings(&game_root, "windowed").unwrap();
        assert_eq!(args, expected);

        // Editing the mod list (add_mod reads then rewrites) must not drop launch args.
        let config = write_minimal_mod_config(&game_root, "cleo");
        add_mod_to_profile_json(&game_root, "windowed", &config).unwrap();
        let (after_edit, _) = profile_launch_settings(&game_root, "windowed").unwrap();
        assert_eq!(after_edit, expected);

        // Copy carries launch args to the new profile.
        copy_profile(&game_root, "windowed", "windowed_copy").unwrap();
        let (copied, _) = profile_launch_settings(&game_root, "windowed_copy").unwrap();
        assert_eq!(copied, expected);
        remove(&game_root);
    }

    fn write_minimal_mod_config(game_root: &Path, id: &str) -> PathBuf {
        let path = state_directory(game_root)
            .join("mods")
            .join(id)
            .join("mod.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!(
                "{{\"version\":1,\"id\":\"{id}\",\"package\":\"pkg.zip\",\"enabled\":true,\"install_roots\":[]}}"
            ),
        )
        .unwrap();
        path
    }

    fn test_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        remove(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn remove(path: &Path) {
        if path.exists() {
            fs::remove_dir_all(path).unwrap();
        }
    }
}
