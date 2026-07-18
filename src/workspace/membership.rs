use crate::prelude::*;

pub(crate) fn set_profile_mod_activation(
    game_root: &Path,
    profile_name: &str,
    mod_id: &str,
    activation: ProfileModActivation,
) -> Result<(), AppError> {
    let mut profile = load_profile_for_edit(game_root, profile_name)?;
    let Some(entry) = profile.mods.iter_mut().find(|entry| entry.id == mod_id) else {
        return Err(AppError::Usage(format!(
            "mod `{mod_id}` is not in profile `{profile_name}`"
        )));
    };
    entry.enabled = activation.is_enabled();
    normalize_profile_mod_entries(&mut profile.mods);
    write_profile_json(game_root, &profile)?;
    println!(
        "{} `{mod_id}` in profile `{profile_name}`",
        activation.label()
    );
    Ok(())
}

pub(crate) fn set_profile_mod_order(
    game_root: &Path,
    profile_name: &str,
    mod_id: &str,
    order: i32,
) -> Result<(), AppError> {
    let mut profile = load_profile_for_edit(game_root, profile_name)?;
    let Some(entry) = profile.mods.iter_mut().find(|entry| entry.id == mod_id) else {
        return Err(AppError::Usage(format!(
            "mod `{mod_id}` is not in profile `{profile_name}`"
        )));
    };
    entry.load_order = order;
    normalize_profile_mod_entries(&mut profile.mods);
    write_profile_json(game_root, &profile)?;
    println!("set `{mod_id}` load_order to {order} in profile `{profile_name}`");
    Ok(())
}

/// Persist an explicit load order for a profile from a full ordered list of mod
/// ids (position 0 loads first). Positions are assigned contiguously, so the UI
/// can drag/reorder freely and store the whole result in one write. Any mod not
/// present in `ordered_ids` keeps its place after the listed ones.
pub(crate) fn set_profile_mod_order_list(
    game_root: &Path,
    profile_name: &str,
    ordered_ids: &[String],
) -> Result<(), AppError> {
    let mut profile = load_profile_for_edit(game_root, profile_name)?;
    let position: BTreeMap<&str, usize> = ordered_ids
        .iter()
        .enumerate()
        .map(|(idx, id)| (id.as_str(), idx))
        .collect();
    // Unlisted mods (should not normally occur) sort after every listed one,
    // preserving their relative order through the id tiebreak in normalize.
    let fallback = ordered_ids.len();
    for entry in &mut profile.mods {
        let slot = position.get(entry.id.as_str()).copied().unwrap_or(fallback);
        entry.load_order = slot as i32;
    }
    normalize_profile_mod_entries(&mut profile.mods);
    write_profile_json(game_root, &profile)?;
    println!("updated load order for profile `{profile_name}`");
    Ok(())
}

/// Enable or disable every mod in a profile at once. With `Disabled` this is the
/// "all mods off" / vanilla mode: the profile stays intact but materializes
/// nothing, so the next run is vanilla.
pub(crate) fn set_all_profile_mods(
    game_root: &Path,
    profile_name: &str,
    activation: ProfileModActivation,
) -> Result<(), AppError> {
    let mut profile = load_profile_for_edit(game_root, profile_name)?;
    let enabled = activation.is_enabled();
    for entry in &mut profile.mods {
        entry.enabled = enabled;
    }
    normalize_profile_mod_entries(&mut profile.mods);
    let count = profile.mods.len();
    write_profile_json(game_root, &profile)?;
    println!(
        "{} all {count} mods in profile `{profile_name}`",
        activation.label()
    );
    Ok(())
}

pub(crate) fn remove_mod_from_profile_json(
    game_root: &Path,
    profile_name: &str,
    mod_id: &str,
) -> Result<(), AppError> {
    let mut profile = load_profile_for_edit(game_root, profile_name)?;
    let before = profile.mods.len();
    profile.mods.retain(|entry| entry.id != mod_id);
    if profile.mods.len() == before {
        return Err(AppError::Usage(format!(
            "mod `{mod_id}` is not in profile `{profile_name}`"
        )));
    }
    normalize_profile_mod_entries(&mut profile.mods);
    write_profile_json(game_root, &profile)?;
    println!("removed `{mod_id}` from profile `{profile_name}`");
    Ok(())
}

fn normalize_profile_mod_entries(mods: &mut Vec<ProfileModEntry>) {
    let mut seen = BTreeSet::new();
    mods.retain(|entry| seen.insert(entry.id.clone()));
    mods.sort_by(|a, b| {
        a.load_order
            .cmp(&b.load_order)
            .then_with(|| a.id.cmp(&b.id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_profile_mod_entries_removes_duplicate_ids_and_sorts() {
        let mut entries = vec![
            test_entry("late", 300),
            test_entry("dup", 200),
            test_entry("dup", 100),
            test_entry("early", 50),
        ];

        normalize_profile_mod_entries(&mut entries);

        let ids = entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["early", "dup", "late"]);
        let orders = entries
            .iter()
            .map(|entry| entry.load_order)
            .collect::<Vec<_>>();
        assert_eq!(orders, vec![50, 200, 300]);
    }

    #[test]
    fn set_all_profile_mods_toggles_every_entry() {
        let game_root = env::temp_dir().join(format!(
            "sa-mod-manager-allmods-{}-{}",
            std::process::id(),
            unix_now()
        ));
        ensure_state(&game_root).unwrap();
        let profile = ProfileJson {
            name: "vanilla".to_string(),
            mods: vec![test_entry("a", 100), test_entry("b", 200)],
            ..Default::default()
        };
        write_profile_json(&game_root, &profile).unwrap();

        // All off → every mod disabled (vanilla), then all on → every mod enabled.
        set_all_profile_mods(&game_root, "vanilla", ProfileModActivation::Disabled).unwrap();
        let read = load_profile_for_edit(&game_root, "vanilla").unwrap();
        assert!(read.mods.iter().all(|entry| !entry.enabled));

        set_all_profile_mods(&game_root, "vanilla", ProfileModActivation::Enabled).unwrap();
        let read = load_profile_for_edit(&game_root, "vanilla").unwrap();
        assert!(read.mods.iter().all(|entry| entry.enabled));
        fs::remove_dir_all(&game_root).unwrap();
    }

    #[test]
    fn set_profile_mod_order_list_applies_explicit_sequence() {
        let game_root = env::temp_dir().join(format!(
            "sa-mod-manager-orderlist-{}-{}",
            std::process::id(),
            unix_now()
        ));
        ensure_state(&game_root).unwrap();
        // Start with a deliberately scrambled load order.
        let profile = ProfileJson {
            name: "load".to_string(),
            mods: vec![
                test_entry("alpha", 300),
                test_entry("bravo", 100),
                test_entry("charlie", 200),
            ],
            ..Default::default()
        };
        write_profile_json(&game_root, &profile).unwrap();

        // Ask for an explicit top-to-bottom order and confirm it round-trips.
        let desired = vec![
            "charlie".to_string(),
            "alpha".to_string(),
            "bravo".to_string(),
        ];
        set_profile_mod_order_list(&game_root, "load", &desired).unwrap();

        let read = load_profile_for_edit(&game_root, "load").unwrap();
        let ids = read
            .mods
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["charlie", "alpha", "bravo"]);
        // Positions are contiguous so later drags/edits start from a clean slate.
        let orders = read
            .mods
            .iter()
            .map(|entry| entry.load_order)
            .collect::<Vec<_>>();
        assert_eq!(orders, vec![0, 1, 2]);
        fs::remove_dir_all(&game_root).unwrap();
    }

    fn test_entry(id: &str, load_order: i32) -> ProfileModEntry {
        ProfileModEntry {
            id: id.to_string(),
            enabled: true,
            load_order,
            config: PathBuf::from(format!("{id}.json")),
            ..Default::default()
        }
    }
}
