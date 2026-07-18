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
