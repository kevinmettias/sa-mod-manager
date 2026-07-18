use crate::prelude::*;

/// Highest profile-format version this manager can read. A profile stamped with
/// a higher version is rejected rather than silently misinterpreted.
const CURRENT_PROFILE_VERSION: u32 = 1;

pub(crate) fn read_profile_json(path: &Path) -> Result<ProfileJson, AppError> {
    let text = fs::read_to_string(path)?;
    let raw: ProfileJsonFile = serde_json::from_str(&text).map_err(|err| {
        AppError::Usage(format!("invalid profile json {}: {err}", path.display()))
    })?;
    if let Some(version) = raw.version
        && version > CURRENT_PROFILE_VERSION
    {
        return Err(AppError::Usage(format!(
            "profile {} is format version {version}, newer than this manager supports (max {CURRENT_PROFILE_VERSION}); update sa-mod-manager",
            path.display()
        )));
    }
    let name = raw.name.unwrap_or_else(|| profile_name_from_path(path));
    // An explicit `game_root` makes a profile portable; otherwise derive it from
    // the profile's location under `<game>/.sa-mod-manager/profiles/`. The explicit
    // value is retained so it round-trips through a manager edit.
    let game_root_override = raw
        .game_root
        .filter(|root| !root.trim().is_empty())
        .map(PathBuf::from);
    let game_root = game_root_override
        .clone()
        .unwrap_or_else(|| game_root_from_profile_path(path));
    let mods = raw
        .mods
        .into_iter()
        .map(|entry| profile_mod_entry_from_json(entry, &game_root))
        .collect();
    let launch_args = raw.launch_args.unwrap_or_default();
    let launch_env = raw.launch_env.unwrap_or_default();
    Ok(ProfileJson {
        name,
        mods,
        launch_args,
        launch_env,
        extra: raw.extra,
        game_root_override,
    })
}

#[derive(Deserialize)]
struct ProfileJsonFile {
    version: Option<u32>,
    name: Option<String>,
    game_root: Option<String>,
    #[serde(default)]
    mods: Vec<ProfileModEntryFile>,
    launch_args: Option<Vec<String>>,
    launch_env: Option<BTreeMap<String, String>>,
    /// Any field this manager does not model, kept so hand-added profile data is
    /// preserved verbatim across edits instead of silently dropped.
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct ProfileModEntryFile {
    id: Option<String>,
    enabled: Option<bool>,
    load_order: Option<i32>,
    config: Option<PathBuf>,
    root_overrides: Option<BTreeMap<String, RootOverrideFile>>,
}

/// A root override is read as either the legacy bare bool (enable/disable) or
/// the richer object form, so older profiles keep working.
#[derive(Deserialize)]
#[serde(untagged)]
enum RootOverrideFile {
    Enabled(bool),
    Full {
        enabled: Option<bool>,
        target: Option<String>,
    },
}

fn root_override_from_file(raw: RootOverrideFile) -> ProfileRootOverride {
    match raw {
        RootOverrideFile::Enabled(enabled) => ProfileRootOverride {
            enabled: Some(enabled),
            target: None,
        },
        RootOverrideFile::Full { enabled, target } => ProfileRootOverride { enabled, target },
    }
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
        .unwrap_or_else(crate::settings::default_game_root)
}

fn profile_mod_entry_from_json(entry: ProfileModEntryFile, game_root: &Path) -> ProfileModEntry {
    let id = entry.id.unwrap_or_else(|| "unnamed".to_string()); // literal: allow external interface text or file-format spelling
    let enabled = entry.enabled.unwrap_or(true);
    let load_order = entry.load_order.unwrap_or(100);
    let config = entry
        .config
        .unwrap_or_else(|| default_mod_config_path(game_root, &id));
    let root_overrides = entry
        .root_overrides
        .unwrap_or_default()
        .into_iter()
        .map(|(source, raw)| (source, root_override_from_file(raw)))
        .collect();
    ProfileModEntry {
        id,
        enabled,
        load_order,
        config,
        root_overrides,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_profile(name: &str, contents: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-cfg-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let path = root
            .join(".sa-mod-manager")
            .join("profiles")
            .join(format!("{name}.json"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn rejects_profile_with_future_format_version() {
        let path = temp_profile(
            "future",
            "{\"version\": 99, \"name\": \"future\", \"mods\": []}",
        );
        let err = read_profile_json(&path).unwrap_err().to_string();
        assert!(err.contains("newer than this manager supports"), "{err}");
        fs::remove_dir_all(path.parent().unwrap().parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn honors_explicit_game_root_and_preserves_unknown_fields() {
        // An explicit game_root overrides the location-derived one, and a mod
        // entry with no config path resolves under it.
        let path = temp_profile(
            "portable",
            "{\"name\": \"portable\", \"game_root\": \"Z:/Custom Install\", \"custom_note\": \"keep me\", \"mods\": [{\"id\": \"cleo\"}]}",
        );
        let profile = read_profile_json(&path).unwrap();
        // The mod config resolves under the explicit game root (the `Z:/…` prefix
        // is preserved verbatim in the joined path root).
        assert!(
            profile.mods[0]
                .config
                .to_string_lossy()
                .starts_with("Z:/Custom Install"),
            "config was {}",
            profile.mods[0].config.display()
        );
        // The unmodeled top-level field is preserved.
        assert_eq!(
            profile.extra.get("custom_note").and_then(serde_json::Value::as_str),
            Some("keep me")
        );
        fs::remove_dir_all(path.parent().unwrap().parent().unwrap().parent().unwrap()).unwrap();
    }
}
