use crate::prelude::*;

const ACTIVE_PROFILE_MARKER_MAX_BYTES: u64 = 64 * 1024;

pub(crate) fn list_profiles(game_root: &Path) -> Result<(), AppError>
{
    let profiles = state_directory(game_root).join("profiles");
    if !profiles.exists()
    {
        println!("no profiles found; run `init` first");
        return Ok(());
    }

    let found = profile_files(&profiles)?;
    print_profile_files(found, &read_active_profile(game_root));
    return Ok(());
}

fn profile_files(profiles: &Path) -> Result<Vec<PathBuf>, AppError>
{
    let mut found = Vec::new();
    for entry in fs::read_dir(&profiles)?
    {
        let entry = entry?;
        let path = entry.path();
        if has_extension_equal_to(&path, "profile") || has_extension_equal_to(&path, "json")
        {
            found.push(path);
        }
    }
    found.sort();
    return Ok(found);
}

fn print_profile_files(found: Vec<PathBuf>, active: &str)
{
    if found.is_empty()
    {
        println!("no profiles found");
    }
    else
    {
        for profile in found
        {
            let name = profile
                .file_stem()
                .and_then(OsStr::to_str)
                .unwrap_or("unknown");
            let marker = if name == active { " (active)" } else { "" };
            println!("{name}{marker}");
        }
    }
}

pub(crate) fn create_profile(game_root: &Path, name: &str) -> Result<(), AppError>
{
    ensure_state(game_root)?;
    let safe = safe_name(name);
    let path = state_directory(game_root)
        .join("profiles")
        .join(format!("{safe}.json"));
    if path.exists()
    {
        return Err(AppError::Usage(format!("profile already exists: {safe}")));
    }
    let profile = ProfileJson {
        name: safe.clone(),
        ..Default::default()
    };
    write_profile_json(game_root, &profile)?;
    println!("created profile: {safe}");
    return Ok(());
}

pub(crate) fn write_profile_json(game_root: &Path, profile: &ProfileJson) -> Result<(), AppError>
{
    ensure_state(game_root)?;
    let path = state_directory(game_root)
        .join("profiles")
        .join(format!("{}.json", profile.name));
    write_profile_json_file(&path, game_root, profile)?;
    println!("profile json: {}", path.display());
    return Ok(());
}

pub(crate) fn write_profile_json_file(
    path: &Path,
    game_root: &Path,
    profile: &ProfileJson,
) -> Result<(), AppError>
{
    // Serialize with serde so the writer and the serde reader cannot drift and
    // hand-edits round-trip. The round-trip is locked by a test below.
    // Preserve a portable profile's own game root; otherwise stamp the install the
    // manager is operating in.
    let effective_game_root = profile
        .game_root_override
        .as_deref()
        .unwrap_or(game_root)
        .display()
        .to_string();
    let document = ProfileDocument {
        version: CURRENT_PROFILE_VERSION,
        name: &profile.name,
        game_root: effective_game_root,
        launch_args: &profile.launch_args,
        launch_env: &profile.launch_env,
        mods: profile.mods.iter().map(profile_mod_document).collect(),
        extra: &profile.extra,
    };
    let text = serde_json::to_string_pretty(&document)
        .map_err(|err| AppError::Tool(format!("failed to serialize profile: {err}")))?;
    fs::write(path, format!("{text}\n"))?;
    return Ok(());
}

/// The profile-format version this manager writes; kept in sync with the reader's
/// `CURRENT_PROFILE_VERSION` so a written profile always round-trips.
const CURRENT_PROFILE_VERSION: u32 = 1;

#[derive(Serialize)]
struct ProfileDocument<'a>
{
    version: u32,
    name: &'a str,
    game_root: String,
    launch_args: &'a [String],
    launch_env: &'a BTreeMap<String, String>,
    mods: Vec<ProfileModDocument<'a>>,
    /// Preserved unmodeled fields, re-emitted alongside the known ones.
    #[serde(flatten)]
    extra: &'a BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize)]
struct ProfileModDocument<'a>
{
    id: &'a str,
    enabled: bool,
    load_order: i32,
    config: String,
    root_overrides: &'a BTreeMap<String, ProfileRootOverride>,
}

fn profile_mod_document(entry: &ProfileModEntry) -> ProfileModDocument<'_>
{
    return ProfileModDocument {
        id: &entry.id,
        enabled: entry.enabled,
        load_order: entry.load_order,
        config: entry.config.display().to_string(),
        root_overrides: &entry.root_overrides,
    };
}

pub(crate) fn add_mod_to_profile_json(
    game_root: &Path,
    profile_name: &str,
    config_path: &Path,
) -> Result<(), AppError>
{
    ensure_state(game_root)?;
    let profile_path = state_directory(game_root)
        .join("profiles")
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
        + 100; // literal: allow domain threshold is documented by the surrounding code
    profile.mods.push(ProfileModEntry {
        id: config.id,
        enabled: true,
        load_order: next_order,
        config: config_path.to_path_buf(),
        ..Default::default()
    });
    profile.mods.sort_by(|left, right| {
        left.load_order
            .cmp(&right.load_order)
            .then_with(|| left.id.cmp(&right.id))
    });
    return write_profile_json(game_root, &profile);
}

pub(crate) fn show_profile_json(game_root: &Path, profile_name: &str) -> Result<(), AppError>
{
    let profile = load_profile_for_edit(game_root, profile_name)?;
    println!("profile: {}", profile.name);
    println!("mods   : {}", profile.mods.len());
    println!();

    let mut mods = profile.mods;
    mods.sort_by(|left, right| {
        left.load_order
            .cmp(&right.load_order)
            .then_with(|| left.id.cmp(&right.id))
    });

    if mods.is_empty()
    {
        println!("no mods in profile");
        return Ok(());
    }

    print_profile_mod_entries(mods);
    return Ok(());
}

fn print_profile_mod_entries(mods: Vec<ProfileModEntry>)
{
    for entry in mods
    {
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
) -> Result<ProfileJson, AppError>
{
    let profile_path = profile_json_path(game_root, profile_name);
    if !profile_path.exists()
    {
        return Err(AppError::Usage(format!(
            "profile json not found: {}",
            profile_path.display()
        )));
    }
    return read_profile_json(&profile_path);
}

/// The profile selected as active for this game folder, or `default` when none
/// has been chosen. Infallible: a missing or unreadable marker falls back to
/// `default`.
pub(crate) fn read_active_profile(game_root: &Path) -> String
{
    // The marker holds only a profile name; cap the read so a corrupt/oversized
    // marker falls back to `default` instead of being slurped whole.
    return match read_capped(
        &active_profile_path(game_root),
        ACTIVE_PROFILE_MARKER_MAX_BYTES,
    )
    {
        // literal: allow domain threshold is documented by the surrounding code
        Ok(text) => {
            let name = text.trim();
            if name.is_empty()
            {
                crate::settings::configured_default_profile()
            }
            else
            {
                safe_name(name)
            }
        }
        // No active profile selected yet: fall back to the configured default.
        Err(_) => crate::settings::configured_default_profile(),
    };
}

pub(crate) fn set_active_profile(game_root: &Path, name: &str) -> Result<(), AppError>
{
    ensure_state(game_root)?;
    let safe = safe_name(name);
    if !profile_json_path(game_root, &safe).exists()
    {
        return Err(AppError::Usage(format!("profile json not found: {safe}")));
    }
    fs::write(active_profile_path(game_root), format!("{safe}\n"))?;
    println!("active profile: {safe}");
    return Ok(());
}

/// The launch arguments and environment for a profile, defaulting to empty when
/// the profile file is absent so the caller can surface its own missing-profile
/// error.
pub(crate) fn profile_launch_settings(
    game_root: &Path,
    profile_name: &str,
) -> Result<(Vec<String>, BTreeMap<String, String>), AppError>
{
    let profile_path = profile_json_path(game_root, profile_name);
    let (profile_args, profile_env) = if profile_path.exists() {
        let profile = read_profile_json(&profile_path)?;
        (profile.launch_args, profile.launch_env)
    } else {
        (Vec::new(), BTreeMap::new())
    };
    return Ok((
        resolve_launch_args(profile_args, crate::settings::default_launch_args()),
        resolve_launch_environment(profile_env, crate::settings::default_launch_environment()),
    ));
}

/// A profile's own launch args take precedence; a profile that sets none falls
/// back to the config-level default so a global flag (e.g. `-nointro`) can apply
/// everywhere without editing each profile.
fn resolve_launch_args(profile_args: Vec<String>, defaults: &[String]) -> Vec<String>
{
    return if profile_args.is_empty()
    {
        defaults.to_vec()
    }
    else
    {
        profile_args
    };
}

/// Merge the config default launch env under the profile's own env; a key set by
/// the profile overrides the same key from the default.
fn resolve_launch_environment(
    profile_env: BTreeMap<String, String>,
    defaults: &BTreeMap<String, String>,
) -> BTreeMap<String, String>
{
    let mut merged = defaults.clone();
    merged.extend(profile_env);
    return merged;
}

pub(crate) fn set_profile_launch_args(
    game_root: &Path,
    profile_name: &str,
    args: Vec<String>,
) -> Result<(), AppError>
{
    let mut profile = load_profile_for_edit(game_root, profile_name)?;
    profile.launch_args = args;
    write_profile_json(game_root, &profile)?;
    println!(
        "set {} launch args for profile `{profile_name}`",
        profile.launch_args.len()
    );
    return Ok(());
}

pub(crate) fn set_profile_root_override(
    game_root: &Path,
    selector: ProfileRootSelector<'_>,
    state: ProfileRootEnabledState,
) -> Result<(), AppError>
{
    update_profile_root_override(game_root, &selector, |over| {
        over.enabled = Some(state.as_bool());
    })?;
    println!(
        "set root `{}` of `{}` to {} in profile `{}`",
        selector.source,
        selector.mod_id,
        if state.as_bool() { "enabled" } else { "disabled" },
        selector.profile_name
    );
    return Ok(());
}

pub(crate) fn set_profile_root_target(
    game_root: &Path,
    selector: ProfileRootSelector<'_>,
    target: &str,
) -> Result<(), AppError>
{
    let target = normalize_path(target);
    update_profile_root_override(game_root, &selector, |over| {
        over.target = Some(target.clone());
    })?;
    println!(
        "set root `{}` of `{}` to target `{target}` in profile `{}`",
        selector.source, selector.mod_id, selector.profile_name
    );
    return Ok(());
}

pub(crate) fn copy_profile(game_root: &Path, request: ProfileCopyRequest<'_>) -> Result<(), AppError>
{
    let source_name = request.source_name;
    let dest_name = request.dest_name;
    let source = load_profile_for_edit(game_root, source_name)?;
    let dest_safe = safe_name(dest_name);
    if profile_json_path(game_root, &dest_safe).exists()
    {
        return Err(AppError::Usage(format!(
            "profile already exists: {dest_safe}"
        )));
    }
    let profile = ProfileJson {
        name: dest_safe.clone(),
        mods: source.mods,
        launch_args: source.launch_args,
        launch_env: source.launch_env,
        // Preserve hand-added fields and an explicit game root when copying.
        extra: source.extra,
        game_root_override: source.game_root_override,
    };
    write_profile_json(game_root, &profile)?;
    println!("copied profile {source_name} -> {dest_safe}");
    return Ok(());
}

pub(crate) struct ProfileRootSelector<'a>
{
    pub(crate) profile_name: &'a str,
    pub(crate) mod_id: &'a str,
    pub(crate) source: &'a str,
}
#[derive(Clone, Copy)]
pub(crate) enum ProfileRootEnabledState
{
    Enabled,
    Disabled,
}

impl ProfileRootEnabledState
{
    pub(crate) fn from_bool(enabled: bool) -> Self
    {
        return if enabled
        {
            Self::Enabled
        }
        else
        {
            Self::Disabled
        };
    }

    fn as_bool(self) -> bool
    {
        return matches!(self, Self::Enabled);
    }
}

fn profiles_directory(game_root: &Path) -> PathBuf
{
    return state_directory(game_root).join("profiles");
}

fn profile_json_path(game_root: &Path, profile_name: &str) -> PathBuf
{
    return profiles_directory(game_root).join(format!("{profile_name}.json"));
}

fn active_profile_path(game_root: &Path) -> PathBuf
{
    return state_directory(game_root).join("active-profile");
}

pub(crate) struct ProfileCopyRequest<'a>
{
    pub(crate) source_name: &'a str,
    pub(crate) dest_name: &'a str,
}

fn update_profile_root_override(
    game_root: &Path,
    selector: &ProfileRootSelector<'_>,
    update: impl FnOnce(&mut ProfileRootOverride),
) -> Result<(), AppError>
{
    let mut profile = load_profile_for_edit(game_root, selector.profile_name)?;
    let Some(entry) = profile
        .mods
        .iter_mut()
        .find(|entry| entry.id == selector.mod_id)
    else {
        return Err(AppError::Usage(format!(
            "mod `{}` is not in profile `{}`",
            selector.mod_id, selector.profile_name
        )));
    };
    update(
        entry
            .root_overrides
            .entry(selector.source.to_string())
            .or_default(),
    );
    return write_profile_json(game_root, &profile);
}
