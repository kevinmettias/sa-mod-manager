
pub(crate) struct ProfileRenameRequest<'a>
{
    pub(crate) old_name: &'a str,
    pub(crate) new_name: &'a str,
}

pub(crate) fn rename_profile(game_root: &Path, request: ProfileRenameRequest<'_>) -> Result<(), AppError>
{
    let old_name = request.old_name;
    let new_name = request.new_name;
    let old_safe = safe_name(old_name);
    if old_safe == "default"
    {
        return Err(AppError::Usage(
            "cannot rename the default profile".to_string(),
        ));
    }
    copy_profile(game_root, ProfileCopyRequest { source_name: &old_safe, dest_name: new_name })?;
    let old_profile_path = profile_json_path(game_root, &old_safe);
    fs::remove_file(old_profile_path)?;
    if read_active_profile(game_root) == old_safe
    {
        set_active_profile(game_root, &safe_name(new_name))?;
    }
    println!("renamed profile {old_safe} -> {}", safe_name(new_name));
    return Ok(());
}

pub(crate) fn delete_profile(game_root: &Path, name: &str) -> Result<(), AppError>
{
    let safe = safe_name(name);
    if safe == "default"
    {
        return Err(AppError::Usage(
            "cannot delete the default profile".to_string(),
        ));
    }
    let path = profile_json_path(game_root, &safe);
    if !path.exists()
    {
        return Err(AppError::Usage(format!(
            "profile json not found: {}",
            path.display()
        )));
    }
    fs::remove_file(&path)?;
    if read_active_profile(game_root) == safe
    {
        set_active_profile(game_root, "default")?;
    }
    println!("deleted profile: {safe}");
    return Ok(());
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn launch_defaults_apply_under_profile_settings()
    {
        // Explicit defaults keep this independent of the developer's config file.
        let defaults = vec!["-nointro".to_string()];
        // A profile with its own args ignores the default; an empty profile uses it.
        assert_eq!(
            resolve_launch_args(vec!["-window".to_string()], &defaults),
            vec!["-window".to_string()]
        );
        assert_eq!(resolve_launch_args(Vec::new(), &defaults), defaults);

        // Env merges with the config default as the base; a profile key overrides.
        let default_env = BTreeMap::from([
            ("A".to_string(), "1".to_string()),
            ("B".to_string(), "1".to_string()),
        ]);
        let profile_env = BTreeMap::from([
            ("B".to_string(), "2".to_string()),
            ("C".to_string(), "3".to_string()),
        ]);
        let merged = resolve_launch_environment(profile_env, &default_env);
        assert_eq!(merged.get("A").map(String::as_str), Some("1"));
        assert_eq!(merged.get("B").map(String::as_str), Some("2"));
        assert_eq!(merged.get("C").map(String::as_str), Some("3"));
    }

    #[test]
    fn active_profile_defaults_then_persists()
    {
        let game_root = test_root("active_profile");
        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        assert_eq!(read_active_profile(&game_root), "default");

        create_profile(&game_root, "racing").expect("the test fixture is created before this assertion reads it");
        set_active_profile(&game_root, "racing").expect("the test fixture is created before this assertion reads it");
        assert_eq!(read_active_profile(&game_root), "racing");
        remove_profile_fixture(&game_root);
    }

    #[test]
    fn set_active_rejects_unknown_profile()
    {
        let game_root = test_root("active_unknown");
        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        let err = set_active_profile(&game_root, "ghost")
            .unwrap_err()
            .to_string();
        assert!(err.contains("profile json not found"));
        remove_profile_fixture(&game_root);
    }

    #[test]
    fn copy_rename_delete_lifecycle_and_default_is_protected()
    {
        let game_root = test_root("profile_lifecycle");
        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        create_profile(&game_root, "base").expect("the test fixture is created before this assertion reads it");

        copy_profile(&game_root, ProfileCopyRequest { source_name: "base", dest_name: "clone" }).expect("the test fixture is created before this assertion reads it");
        assert!(profile_json_path(&game_root, "clone").exists());
        assert!(copy_profile(&game_root, ProfileCopyRequest { source_name: "base", dest_name: "clone" }).is_err()); // no clobber

        rename_profile(&game_root, ProfileRenameRequest { old_name: "clone", new_name: "renamed" }).expect("the test fixture is created before this assertion reads it");
        assert!(!profile_json_path(&game_root, "clone").exists());
        assert!(profile_json_path(&game_root, "renamed").exists());

        delete_profile(&game_root, "renamed").expect("the test fixture is created before this assertion reads it");
        assert!(!profile_json_path(&game_root, "renamed").exists());

        assert!(delete_profile(&game_root, "default").is_err());
        assert!(rename_profile(&game_root, ProfileRenameRequest { old_name: "default", new_name: "x" }).is_err());
        remove_profile_fixture(&game_root);
    }

    #[test]
    fn deleting_active_profile_resets_active_to_default()
    {
        let game_root = test_root("delete_active");
        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        create_profile(&game_root, "temp").expect("the test fixture is created before this assertion reads it");
        set_active_profile(&game_root, "temp").expect("the test fixture is created before this assertion reads it");

        delete_profile(&game_root, "temp").expect("the test fixture is created before this assertion reads it");
        assert_eq!(read_active_profile(&game_root), "default");
        remove_profile_fixture(&game_root);
    }

    #[test]
    fn launch_args_persist_and_survive_profile_edits()
    {
        let game_root = test_root("launch_args");
        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        create_profile(&game_root, "windowed").expect("the test fixture is created before this assertion reads it");
        let expected = vec!["-windowed".to_string(), "-nointro".to_string()];
        set_profile_launch_args(&game_root, "windowed", expected.clone()).expect("the test fixture is created before this assertion reads it");

        let (args, _env) = profile_launch_settings(&game_root, "windowed").expect("the test fixture is created before this assertion reads it");
        assert_eq!(args, expected);

        // Editing the mod list (add_mod reads then rewrites) must not drop launch args.
        let config = write_minimal_mod_config(&game_root, "cleo");
        add_mod_to_profile_json(&game_root, "windowed", &config).expect("the test fixture is created before this assertion reads it");
        let (after_edit, _) = profile_launch_settings(&game_root, "windowed").expect("the test fixture is created before this assertion reads it");
        assert_eq!(after_edit, expected);

        // Copy carries launch args to the new profile.
        copy_profile(&game_root, ProfileCopyRequest { source_name: "windowed", dest_name: "windowed_copy" }).expect("the test fixture is created before this assertion reads it");
        let (copied, _) = profile_launch_settings(&game_root, "windowed_copy").expect("the test fixture is created before this assertion reads it");
        assert_eq!(copied, expected);
        remove_profile_fixture(&game_root);
    }

    fn write_minimal_mod_config(game_root: &Path, id: &str) -> PathBuf
    {
        let path = state_directory(game_root)
            .join("mods")
            .join(id)
            .join("mod.json");
        fs::create_dir_all(path.parent().expect("the test fixture is created before this assertion reads it")).expect("the test fixture is created before this assertion reads it");
        fs::write(
            &path,
            format!(
                "{{\"version\":1,\"id\":\"{id}\",\"package\":\"pkg.zip\",\"enabled\":true,\"install_roots\":[]}}"
            ),
        )
        .expect("the test fixture is created before this assertion reads it");
        return path;
    }

    #[test]
    fn profile_json_round_trips_through_serde()
    {
        let game_root = test_root("profile_round_trip");
        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        let mut root_overrides = BTreeMap::new();
        root_overrides.insert(
            "cleo".to_string(),
            ProfileRootOverride {
                enabled: Some(false),
                target: Some("CLEO_custom".to_string()),
            },
        );
        let mut launch_env = BTreeMap::new();
        launch_env.insert("SA_TEST".to_string(), "1".to_string());
        // A hand-added custom field the manager does not model.
        let mut extra = BTreeMap::new();
        extra.insert(
            "notes".to_string(),
            serde_json::Value::String("my custom profile note".to_string()),
        );
        let profile = ProfileJson {
            name: "roundtrip".to_string(),
            mods: vec![ProfileModEntry {
                id: "cleo".to_string(),
                enabled: true,
                load_order: 150, // literal: allow test fixture value is the specimen under judgment
                config: game_root
                    .join(".sa-mod-manager")
                    .join("mods")
                    .join("cleo")
                    .join("mod.json"),
                root_overrides,
            }],
            launch_args: vec!["-windowed".to_string(), "-nointro".to_string()],
            launch_env,
            extra,
            game_root_override: Some(PathBuf::from("Z:/Custom Install")),
        };

        write_profile_json(&game_root, &profile).expect("the test fixture is created before this assertion reads it");
        let read = load_profile_for_edit(&game_root, "roundtrip").expect("the test fixture is created before this assertion reads it");

        assert_eq!(read.name, profile.name);
        assert_eq!(read.launch_args, profile.launch_args);
        assert_eq!(read.launch_env, profile.launch_env);
        assert_eq!(read.mods.len(), 1);
        assert_eq!(read.mods[0].id, "cleo");
        assert!(read.mods[0].enabled);
        assert_eq!(read.mods[0].load_order, 150); // literal: allow test fixture value is the specimen under judgment
        let over = read.mods[0].root_overrides.get("cleo").expect("the test fixture is created before this assertion reads it");
        assert_eq!(over.enabled, Some(false));
        assert_eq!(over.target.as_deref(), Some("CLEO_custom"));
        // The unmodeled field survived the write/read round-trip.
        assert_eq!(
            read.extra.get("notes").and_then(serde_json::Value::as_str),
            Some("my custom profile note")
        );
        // A portable profile's explicit game root survives the round-trip instead
        // of being overwritten with the operating install.
        assert_eq!(
            read.game_root_override,
            Some(PathBuf::from("Z:/Custom Install"))
        );
        remove_profile_fixture(&game_root);
    }

    #[test]
    fn legacy_bare_bool_root_override_still_reads()
    {
        let game_root = test_root("legacy_override");
        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        let path = state_directory(&game_root)
            .join("profiles")
            .join("legacy.json");
        // Old on-disk format used a bare bool for the override value.
        fs::write(
            &path,
            concat!(
                "{\n",
                "  \"name\": \"legacy\",\n",
                "  \"mods\": [\n",
                "    { \"id\": \"cleo\", \"enabled\": true, \"load_order\": 100,\n",
                "      \"config\": \"x/mod.json\", \"root_overrides\": { \"CLEO\": false } }\n",
                "  ]\n",
                "}\n"
            ),
        )
        .expect("the test fixture is created before this assertion reads it");

        let read = load_profile_for_edit(&game_root, "legacy").expect("the test fixture is created before this assertion reads it");
        let over = read.mods[0].root_overrides.get("CLEO").expect("the test fixture is created before this assertion reads it");
        assert_eq!(over.enabled, Some(false));
        assert_eq!(over.target, None);
        remove_profile_fixture(&game_root);
    }

    fn test_root(name: &str) -> PathBuf
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        remove_profile_fixture(&root);
        fs::create_dir_all(&root).expect("the test fixture is created before this assertion reads it");
        return root;
    }

    fn remove_profile_fixture(path: &Path)
    {
        if path.exists()
        {
            fs::remove_dir_all(path).expect("the test fixture is created before this assertion reads it");
        }
    }
}
