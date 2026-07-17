use crate::prelude::*;

pub(crate) fn list_profiles(game_root: &Path) -> Result<(), AppError> {
    let profiles = state_directory(game_root).join("profiles"); // literal: allow external interface text or file-format spelling
    if !profiles.exists() {
        println!("no profiles found; run `init` first");
        return Ok(());
    }

    let found = profile_files(&profiles)?;
    print_profile_files(found);
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

fn print_profile_files(found: Vec<PathBuf>) {
    if found.is_empty() {
        println!("no profiles found");
    } else {
        for profile in found {
            println!(
                "{}",
                profile
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .unwrap_or("unknown")
            );
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
    write_profile_json(game_root, &safe, &[])?;
    println!("created profile: {safe}");
    Ok(())
}

pub(crate) fn write_profile_json(
    game_root: &Path,
    name: &str,
    mods: &[ProfileModEntry],
) -> Result<(), AppError> {
    ensure_state(game_root)?;
    let path = state_directory(game_root)
        .join("profiles") // literal: allow external interface text or file-format spelling
        .join(format!("{name}.json"));
    write_profile_json_file(&path, game_root, name, mods)?;
    println!("profile json: {}", path.display());
    Ok(())
}

pub(crate) fn write_profile_json_file(
    path: &Path,
    game_root: &Path,
    name: &str,
    mods: &[ProfileModEntry],
) -> Result<(), AppError> {
    let mut file = fs::File::create(path)?;
    writeln!(file, "{{")?;
    writeln!(file, "  \"version\": 1,")?;
    writeln!(file, "  \"name\": \"{}\",", json_escape(name))?;
    writeln!(
        file,
        "  \"game_root\": \"{}\",",
        json_escape(&game_root.display().to_string())
    )?;
    writeln!(file, "  \"ephemeral\": true,")?;
    writeln!(file, "  \"mods\": [")?;
    write_profile_json_entries(&mut file, mods)?;
    writeln!(file, "  ]")?;
    writeln!(file, "}}")?;
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
            "      \"config\": \"{}\"",
            json_escape(&entry.config.display().to_string())
        )?;
        write!(file, "    }}")?;
        if idx + 1 != mods.len() {
            writeln!(file, ",")?;
        } else {
            writeln!(file)?;
        }
    }
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
            mods: Vec::new(),
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
    });
    profile.mods.sort_by(|a, b| {
        a.load_order
            .cmp(&b.load_order)
            .then_with(|| a.id.cmp(&b.id))
    });
    write_profile_json(game_root, &profile.name, &profile.mods)
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
    let profile_path = state_directory(game_root)
        .join("profiles") // literal: allow external interface text or file-format spelling
        .join(format!("{profile_name}.json"));
    if !profile_path.exists() {
        return Err(AppError::Usage(format!(
            "profile json not found: {}",
            profile_path.display()
        )));
    }
    read_profile_json(&profile_path)
}
