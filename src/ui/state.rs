use crate::prelude::*;

mod infrastructure_item;
mod mod_config_item;
mod ui_state;
mod ui_tab;

pub(super) use infrastructure_item::InfrastructureItem;
pub(super) use mod_config_item::ModConfigItem;
pub(super) use ui_state::UiState;
pub(super) use ui_tab::UiTab;

pub(super) fn load_ui_state(
    game_root: &Path,
    selected_profile_name: &str,
) -> Result<UiState, AppError>
{
    ensure_state(game_root)?;
    let profiles = list_profile_names(game_root)?;
    let profile_name = if profiles.iter().any(|name| name == selected_profile_name) {
        selected_profile_name
    } else {
        profiles.first().map(String::as_str).unwrap_or("default")
    };
    let selected_profile = load_profile_for_edit(game_root, profile_name).ok();
    let selected_profile_ref = selected_profile.as_ref();
    let selected_ids = selected_profile_ids(selected_profile_ref);
    let mods = list_mod_configs(game_root, &selected_ids)?;
    let infrastructure = inspect_infrastructure(game_root);
    return Ok(UiState {
        profiles,
        selected_profile,
        mods,
        infrastructure,
    });
}

fn list_profile_names(game_root: &Path) -> Result<Vec<String>, AppError>
{
    let profiles_root = state_directory(game_root).join("profiles");
    let mut names = Vec::new();
    if !profiles_root.exists()
    {
        return Ok(names);
    }
    for entry in fs::read_dir(profiles_root)?
    {
        let path = entry?.path();
        if extension_eq(&path, "json")
        {
            let name = path
                .file_stem()
                .and_then(OsStr::to_str)
                .unwrap_or("default")
                .to_string();
            names.push(name);
        }
    }
    names.sort();
    return Ok(names);
}

fn selected_profile_ids(profile: Option<&ProfileJson>) -> BTreeSet<String>
{
    let mut ids = BTreeSet::new();
    if let Some(profile) = profile
    {
        for entry in &profile.mods
        {
            let entry_id = entry.id.clone();
            ids.insert(entry_id);
        }
    }
    return ids;
}

fn list_mod_configs(
    game_root: &Path,
    selected_ids: &BTreeSet<String>,
) -> Result<Vec<ModConfigItem>, AppError>
{
    let mods_root = state_directory(game_root).join("mods");
    let mut items = Vec::new();
    if !mods_root.exists()
    {
        return Ok(items);
    }
    for entry in fs::read_dir(mods_root)?
    {
        let path = entry?.path().join("mod.json");
        if !path.exists()
        {
            continue;
        }
        let config = read_mod_config_json(&path)?;
        let in_selected_profile = selected_ids.contains(&config.id);
        items.push(ModConfigItem {
            path,
            config,
            in_selected_profile,
        });
    }
    items.sort_by(|a, b| a.config.id.cmp(&b.config.id));
    return Ok(items);
}

fn inspect_infrastructure(game_root: &Path) -> Vec<InfrastructureItem>
{
    let builtin = [
        ("Steam executable", "gta-sa.exe"),
        ("Classic executable", "gta_sa.exe"),
        ("Mod Loader ASI", "modloader.asi"),
        ("Mod Loader folder", "modloader"),
        ("CLEO ASI", "CLEO.asi"),
        ("CLEO folder", "CLEO"),
        ("ASI loader DLL", "vorbisFile.dll"),
    ];
    let mut items: Vec<InfrastructureItem> = builtin
        .into_iter()
        .map(|(label, relative)| {
            infrastructure_item(
                game_root,
                InfrastructureItemCheck {
                    label: label,
                    relative: relative,
                },
            )
        })
        .collect();
    // Config-declared checks (e.g. an alternate ASI loader like dinput8.dll) are
    // appended so the status panel can recognize non-default setups.
    for (label, relative) in crate::settings::extra_infrastructure_checks()
    {
        let item = infrastructure_item(
            game_root,
            InfrastructureItemCheck {
                label: label,
                relative: relative,
            },
        );
        items.push(item);
    }
    return items;
}

struct InfrastructureItemCheck<'a>
{
    label: &'a str,
    relative: &'a str,
}

fn infrastructure_item(game_root: &Path, check: InfrastructureItemCheck<'_>) -> InfrastructureItem
{
    let label = check.label;
    let relative = check.relative;
    let path = game_root.join(relative);
    return InfrastructureItem {
        label: label.to_string(),
        present: path.exists(),
        path,
    };
}
