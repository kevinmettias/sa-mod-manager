    use super::{extract_ignore_files, should_launch_args_select_modloader_mode, materialize_profile_for_run, modloader_run_profile};
    use crate::prelude::*;

    #[test]
    fn user_modloader_flags_suppress_our_modprof()
    {
        // A user who set any ModLoader mode keeps it; we don't append a dead -modprof.
        for arg in ["-nomods", "-NoMods", "-mod", "-modprof"]
        {
            assert!(
                should_launch_args_select_modloader_mode(&[arg.to_string()]),
                "{arg}"
            );
        }
        // Unrelated args leave us free to add -modprof.
        assert!(!should_launch_args_select_modloader_mode(&[
            "-nointro".to_string(),
            "-windowed".to_string()
        ]));
        assert!(!should_launch_args_select_modloader_mode(&[]));
    }

    #[test]
    fn ignore_files_are_pulled_from_profile_extra()
    {
        let mut extra = BTreeMap::new();
        extra.insert(
            "ignore_files".to_string(),
            serde_json::json!(["*.dff", "  to_ignore/x.txd  ", "", 42]), // literal: allow test fixture value is the specimen under judgment
        );
        let files = extract_ignore_files(&extra);
        // Strings are trimmed and kept; blanks and non-string_list dropped.
        assert_eq!(
            files,
            vec!["*.dff".to_string(), "to_ignore/x.txd".to_string()]
        );
        // No key -> empty.
        assert!(extract_ignore_files(&BTreeMap::new()).is_empty());
    }

    #[test]
    fn materialize_profile_rolls_back_files_when_later_mod_fails()
    {
        let game_root = test_root("materialize_rollback");
        let first_source = game_root.join("sources").join("first");
        let second_source = game_root.join("sources").join("second");
        let first_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("first")
            .join("mod.json");
        let second_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("second")
            .join("mod.json");

        fs::create_dir_all(first_source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(second_source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::write(first_source.join("payload").join("first.txt"), "first").expect("the test fixture is created before this assertion reads it");
        fs::write(second_source.join("payload").join("second.txt"), "second").expect("the test fixture is created before this assertion reads it");

        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        write_test_mod_config(
            &first_config,
            "first",
            &first_source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/first",
            },
        );
        write_test_mod_config(
            &second_config,
            "second",
            &second_source,
            TestModConfigRoot {
                source: "payload",
                target: "../bad",
            },
        );
        write_test_profile(&game_root, &first_config, &second_config);

        let result = materialize_profile_for_run(&game_root, "default");

        assert!(result.is_err());
        assert!(
            !game_root
                .join("modloader")
                .join("first")
                .join("first.txt")
                .exists()
        );
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn materialize_profile_applies_enabled_mods_in_load_order()
    {
        let game_root = test_root("materialize_load_order");
        let early_source = game_root.join("sources").join("early");
        let late_source = game_root.join("sources").join("late");
        let disabled_source = game_root.join("sources").join("disabled");
        let early_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("early")
            .join("mod.json");
        let late_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("late")
            .join("mod.json");
        let disabled_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("disabled")
            .join("mod.json");

        fs::create_dir_all(early_source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(late_source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(disabled_source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::write(early_source.join("payload").join("shared.txt"), "early").expect("the test fixture is created before this assertion reads it");
        fs::write(late_source.join("payload").join("shared.txt"), "late").expect("the test fixture is created before this assertion reads it");
        fs::write(
            disabled_source.join("payload").join("disabled.txt"),
            "disabled",
        )
        .expect("the test fixture is created before this assertion reads it");

        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        write_test_mod_config(
            &early_config,
            "early",
            &early_source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/order",
            },
        );
        write_test_mod_config(
            &late_config,
            "late",
            &late_source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/order",
            },
        );
        write_test_mod_config(
            &disabled_config,
            "disabled",
            &disabled_source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/order",
            },
        );
        write_profile_entries(
            &game_root,
            &[
                test_profile_entry("late", TestModState::Enabled, 200, &late_config), // literal: allow test fixture value is the specimen under judgment
                test_profile_entry("disabled", TestModState::Disabled, 300, &disabled_config), // literal: allow test fixture value is the specimen under judgment
                test_profile_entry("early", TestModState::Enabled, 100, &early_config), // literal: allow test fixture value is the specimen under judgment
            ],
        );

        let journal = materialize_profile_for_run(&game_root, "default").expect("the test fixture is created before this assertion reads it");

        assert_eq!(
            fs::read_to_string(game_root.join("modloader").join("order").join("shared.txt"))
                .expect("the test fixture is created before this assertion reads it"),
            "late"
        );
        assert!(
            !game_root
                .join("modloader")
                .join("order")
                .join("disabled.txt")
                .exists()
        );
        let journal_text = fs::read_to_string(journal).expect("the test fixture is created before this assertion reads it");
        let early_idx = journal_text.find("profile_mod=early|100").expect("the test fixture is created before this assertion reads it");
        let late_idx = journal_text.find("profile_mod=late|200").expect("the test fixture is created before this assertion reads it");
        assert!(early_idx < late_idx);
        assert!(!journal_text.contains("profile_mod=disabled"));
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn profile_root_override_disables_a_specific_install_root()
    {
        let game_root = test_root("root_override");
        let source = game_root.join("sources").join("mod");
        let config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("mod")
            .join("mod.json");
        fs::create_dir_all(source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::write(source.join("payload").join("file.txt"), "payload").expect("the test fixture is created before this assertion reads it");

        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        write_test_mod_config(
            &config,
            "mod",
            &source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/target",
            },
        );

        let mut overrides = BTreeMap::new();
        overrides.insert(
            "payload".to_string(),
            ProfileRootOverride {
                enabled: Some(false),
                target: None,
            },
        );
        let entry = ProfileModEntry {
            id: "mod".to_string(),
            enabled: true,
            load_order: 100, // literal: allow test fixture value is the specimen under judgment
            config: config.clone(),
            root_overrides: overrides,
        };
        write_profile_entries(&game_root, &[entry]);

        materialize_profile_for_run(&game_root, "default").expect("the test fixture is created before this assertion reads it");

        assert!(
            !game_root
                .join("modloader")
                .join("target")
                .join("file.txt")
                .exists(),
            "root disabled by profile override should not materialize"
        );
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn profile_root_override_retargets_a_specific_install_root()
    {
        let game_root = test_root("root_retarget");
        let source = game_root.join("sources").join("mod");
        let config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("mod")
            .join("mod.json");
        fs::create_dir_all(source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::write(source.join("payload").join("file.txt"), "payload").expect("the test fixture is created before this assertion reads it");

        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        write_test_mod_config(
            &config,
            "mod",
            &source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/target",
            },
        );

        let mut overrides = BTreeMap::new();
        overrides.insert(
            "payload".to_string(),
            ProfileRootOverride {
                enabled: None,
                target: Some("modloader/retargeted".to_string()),
            },
        );
        let entry = ProfileModEntry {
            id: "mod".to_string(),
            enabled: true,
            load_order: 100, // literal: allow test fixture value is the specimen under judgment
            config: config.clone(),
            root_overrides: overrides,
        };
        write_profile_entries(&game_root, &[entry]);

        materialize_profile_for_run(&game_root, "default").expect("the test fixture is created before this assertion reads it");

        // Files land at the profile's overridden target, not the mod's default.
        assert!(
            game_root
                .join("modloader")
                .join("retargeted")
                .join("file.txt")
                .exists(),
            "root should materialize at the profile-overridden target"
        );
        assert!(
            !game_root
                .join("modloader")
                .join("target")
                .join("file.txt")
                .exists(),
            "root should not materialize at the mod's default target"
        );
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn materialize_profile_rejects_duplicate_enabled_mod_ids()
    {
        let game_root = test_root("materialize_duplicate_profile_id");
        let source = game_root.join("sources").join("dup");
        let config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("dup")
            .join("mod.json");
        fs::create_dir_all(source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::write(source.join("payload").join("file.txt"), "payload").expect("the test fixture is created before this assertion reads it");
        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        write_test_mod_config(
            &config,
            "dup",
            &source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/dup",
            },
        );
        write_profile_entries(
            &game_root,
            &[
                test_profile_entry("dup", TestModState::Enabled, 100, &config), // literal: allow test fixture value is the specimen under judgment
                test_profile_entry("dup", TestModState::Enabled, 200, &config), // literal: allow test fixture value is the specimen under judgment
            ],
        );

        let err = materialize_profile_for_run(&game_root, "default")
            .unwrap_err()
            .to_string();

        assert!(err.contains("duplicate enabled mod id"));
        assert!(!game_root.join("modloader").join("dup").exists());
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn materialize_profile_rejects_missing_enabled_mod_config_before_copying()
    {
        let game_root = test_root("materialize_missing_config");
        let missing_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("missing")
            .join("mod.json");
        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        write_profile_entries(
            &game_root,
            &[test_profile_entry("missing", TestModState::Enabled, 100, &missing_config)], // literal: allow test fixture value is the specimen under judgment
        );

        let err = materialize_profile_for_run(&game_root, "default")
            .unwrap_err()
            .to_string();

        assert!(err.contains("references missing mod config"));
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }
