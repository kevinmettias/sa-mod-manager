use crate::prelude::*;

pub(crate) fn read_profile_json(path: &Path) -> Result<ProfileJson, AppError> {
    let text = fs::read_to_string(path)?;
    let raw: ProfileJsonFile = serde_json::from_str(&text).map_err(|err| {
        AppError::Usage(format!("invalid profile json {}: {err}", path.display()))
    })?;
    let name = raw.name.unwrap_or_else(|| profile_name_from_path(path));
    let game_root = game_root_from_profile_path(path);
    let mods = raw
        .mods
        .into_iter()
        .map(|entry| profile_mod_entry_from_json(entry, &game_root))
        .collect();
    Ok(ProfileJson { name, mods })
}

#[derive(Deserialize)]
struct ProfileJsonFile {
    name: Option<String>,
    #[serde(default)]
    mods: Vec<ProfileModEntryFile>,
}

#[derive(Deserialize)]
struct ProfileModEntryFile {
    id: Option<String>,
    enabled: Option<bool>,
    load_order: Option<i32>,
    config: Option<PathBuf>,
}

fn profile_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("default") // literal: allow external interface text or file-format spelling
        .to_string()
}

fn game_root_from_profile_path(path: &Path) -> PathBuf {
    path.parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_GAME_ROOT))
}

fn profile_mod_entry_from_json(entry: ProfileModEntryFile, game_root: &Path) -> ProfileModEntry {
    let id = entry.id.unwrap_or_else(|| "unnamed".to_string()); // literal: allow external interface text or file-format spelling
    let enabled = entry.enabled.unwrap_or(true);
    let load_order = entry.load_order.unwrap_or(100);
    let config = entry
        .config
        .unwrap_or_else(|| default_mod_config_path(game_root, &id));
    ProfileModEntry {
        id,
        enabled,
        load_order,
        config,
    }
}

fn default_mod_config_path(game_root: &Path, id: &str) -> PathBuf {
    let state_root = state_directory(game_root);
    state_root.join("mods").join(id).join("mod.json") // literal: allow external interface text or file-format spelling
}

pub(crate) fn read_mod_config_json(path: &Path) -> Result<ModConfigJson, AppError> {
    let text = fs::read_to_string(path)?;
    let raw: ModConfigJsonFile = serde_json::from_str(&text)
        .map_err(|err| AppError::Usage(format!("invalid mod json {}: {err}", path.display())))?;
    let id = raw.id.unwrap_or_else(|| mod_id_from_path(path));
    let package = raw.package.ok_or_else(|| {
        AppError::Usage(format!("mod config missing package: {}", path.display()))
    })?;
    let source_root = raw.source_root;
    let enabled = raw.enabled.unwrap_or(true);
    let install_roots = raw
        .install_roots
        .into_iter()
        .map(mod_install_root_from_json)
        .collect();
    Ok(ModConfigJson {
        id,
        package,
        source_root,
        enabled,
        install_roots,
    })
}

#[derive(Deserialize)]
struct ModConfigJsonFile {
    id: Option<String>,
    package: Option<PathBuf>,
    source_root: Option<PathBuf>,
    enabled: Option<bool>,
    #[serde(default)]
    install_roots: Vec<ModInstallRootJsonFile>,
}

#[derive(Deserialize)]
struct ModInstallRootJsonFile {
    source: Option<String>,
    target: Option<String>,
    kind: Option<String>,
    enabled: Option<bool>,
    optional: Option<bool>,
}

fn mod_id_from_path(path: &Path) -> String {
    path.parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .unwrap_or("mod") // literal: allow external interface text or file-format spelling
        .to_string()
}

fn mod_install_root_from_json(object: ModInstallRootJsonFile) -> ModInstallRootJson {
    let source = object.source.unwrap_or_else(|| ".".to_string()); // literal: allow external interface text or file-format spelling
    let target = object.target.unwrap_or_else(|| ".".to_string()); // literal: allow external interface text or file-format spelling
    let kind = object.kind.unwrap_or_else(|| "modloader".to_string()); // literal: allow external interface text or file-format spelling
    let enabled = object.enabled.unwrap_or(true);
    let optional = object.optional.unwrap_or(false);
    ModInstallRootJson {
        source,
        target,
        kind,
        enabled,
        optional,
    }
}
