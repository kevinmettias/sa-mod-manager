
    #[test]
    fn materialize_writes_modloader_priorities_from_load_order()
    {
        let game_root = test_root("materialize_ml_priorities");
        let early_source = game_root.join("sources").join("early");
        let late_source = game_root.join("sources").join("late");
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
        fs::create_dir_all(early_source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(late_source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::write(early_source.join("payload").join("left.dff"), "e").expect("the test fixture is created before this assertion reads it");
        fs::write(late_source.join("payload").join("right.dff"), "l").expect("the test fixture is created before this assertion reads it");

        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        // Each mod is sandboxed in its own modloader/<folder>; on disk they never
        // collide, so only the written priority can order them.
        write_test_mod_config(
            &early_config,
            "early",
            &early_source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/early_mod",
            },
        );
        write_test_mod_config(
            &late_config,
            "late",
            &late_source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/late_mod",
            },
        );
        write_profile_entries(
            &game_root,
            &[
                test_profile_entry("late", TestModState::Enabled, 200, &late_config), // literal: allow test fixture value is the specimen under judgment
                test_profile_entry("early", TestModState::Enabled, 100, &early_config), // literal: allow test fixture value is the specimen under judgment
            ],
        );

        let journal = materialize_profile_for_run(&game_root, "default").expect("the test fixture is created before this assertion reads it");

        let ini = fs::read_to_string(game_root.join("modloader").join("modloader.ini")).expect("the test fixture is created before this assertion reads it");
        // Later load order -> higher ModLoader priority (wins at runtime).
        let priorities = crate::planning::read_modloader_priorities(&game_root).expect("the test fixture is created before this assertion reads it");
        assert!(
            priorities.for_folder("late_mod") > priorities.for_folder("early_mod"),
            "later mod must get higher ModLoader priority; ini was:\n{ini}"
        );
        // Written into a manager-owned native profile that inherits Default.
        assert!(
            ini.contains("[Profiles.SAMM_default.Priority]"),
            "ini was:\n{ini}"
        );
        assert!(
            ini.contains("[Profiles.SAMM_default.Config]"),
            "ini was:\n{ini}"
        );
        assert!(ini.contains("Parents = Default"), "ini was:\n{ini}");
        // That profile is what the run activates via -modprof.
        assert_eq!(
            modloader_run_profile(&game_root, "default").as_deref(),
            Some("SAMM_default")
        );
        // The ini write is journaled, so an ephemeral run's rollback restores it.
        let journal_text = fs::read_to_string(&journal).expect("the test fixture is created before this assertion reads it");
        assert!(
            journal_text.contains("modloader.ini"),
            "ini write should be journaled for rollback"
        );
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn disabled_modloader_mod_is_written_as_ignoremods()
    {
        let game_root = test_root("materialize_ignoremods");
        let on_source = game_root.join("sources").join("on");
        let off_source = game_root.join("sources").join("off");
        let on_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("on")
            .join("mod.json");
        let off_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("off")
            .join("mod.json");
        fs::create_dir_all(on_source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(off_source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::write(on_source.join("payload").join("left.dff"), "on").expect("the test fixture is created before this assertion reads it");
        fs::write(off_source.join("payload").join("right.dff"), "off").expect("the test fixture is created before this assertion reads it");

        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        write_test_mod_config(
            &on_config,
            "on",
            &on_source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/on_mod",
            },
        );
        write_test_mod_config(
            &off_config,
            "off",
            &off_source,
            TestModConfigRoot {
                source: "payload",
                target: "modloader/off_mod",
            },
        );
        write_profile_entries(
            &game_root,
            &[
                test_profile_entry("on", TestModState::Enabled, 100, &on_config), // literal: allow test fixture value is the specimen under judgment
                // Disabled: its folder should be ignored, not materialized.
                test_profile_entry("off", TestModState::Disabled, 200, &off_config), // literal: allow test fixture value is the specimen under judgment
            ],
        );

        materialize_profile_for_run(&game_root, "default").expect("the test fixture is created before this assertion reads it");

        let ini = fs::read_to_string(game_root.join("modloader").join("modloader.ini")).expect("the test fixture is created before this assertion reads it");
        assert!(
            ini.contains("[Profiles.SAMM_default.IgnoreMods]"),
            "ini was:\n{ini}"
        );
        assert!(
            ini.contains("off_mod"),
            "disabled folder should be ignored; ini was:\n{ini}"
        );
        // The disabled mod is never copied into the sandbox.
        assert!(!game_root.join("modloader").join("off_mod").exists());
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn no_modloader_mods_writes_no_profile_and_no_modprof()
    {
        let game_root = test_root("materialize_no_ml");
        let source = game_root.join("sources").join("cleo_only");
        let config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("cleo_only")
            .join("mod.json");
        fs::create_dir_all(source.join("payload")).expect("the test fixture is created before this assertion reads it");
        fs::write(source.join("payload").join("script.cs"), "x").expect("the test fixture is created before this assertion reads it");

        ensure_state(&game_root).expect("the test fixture is created before this assertion reads it");
        // A CLEO-only mod targets CLEO/, never a modloader sandbox.
        write_test_mod_config(
            &config,
            "cleo_only",
            &source,
            TestModConfigRoot {
                source: "payload",
                target: "CLEO",
            },
        );
        write_profile_entries(
            &game_root,
            &[test_profile_entry("cleo_only", TestModState::Enabled, 100, &config)], // literal: allow test fixture value is the specimen under judgment
        );

        materialize_profile_for_run(&game_root, "default").expect("the test fixture is created before this assertion reads it");

        // No modloader mods -> no managed profile written, so nothing to activate.
        assert!(!game_root.join("modloader").join("modloader.ini").exists());
        assert_eq!(modloader_run_profile(&game_root, "default"), None);
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    struct TestModConfigRoot<'a>
    {
        source: &'a str,
        target: &'a str,
    }

    fn write_test_profile(game_root: &Path, first_config: &Path, second_config: &Path)
    {
        write_profile_entries(
            game_root,
            &[
                test_profile_entry("first", TestModState::Enabled, 100, first_config), // literal: allow domain threshold is documented by the surrounding code
                test_profile_entry("second", TestModState::Enabled, 200, second_config), // literal: allow domain threshold is documented by the surrounding code
            ],
        );
    }

    fn write_test_mod_config(
        path: &Path,
        id: &str,
        source_root: &Path,
        root: TestModConfigRoot<'_>,
    )
    {
        if let Some(parent) = path.parent()
        {
            fs::create_dir_all(parent).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        }
        let text = format!(
            concat!(
                "{{\n",
                "  \"version\": 1,\n",
                "  \"id\": \"{}\",\n",
                "  \"package\": \"{}\",\n",
                "  \"source_root\": \"{}\",\n",
                "  \"enabled\": true,\n",
                "  \"install_roots\": [\n",
                "    {{\n",
                "      \"source\": \"{}\",\n",
                "      \"target\": \"{}\",\n",
                "      \"kind\": \"modloader\",\n",
                "      \"enabled\": true\n",
                "    }}\n",
                "  ]\n",
                "}}\n"
            ),
            json_escape(id),
            json_escape(&source_root.display().to_string()),
            json_escape(&source_root.display().to_string()),
            json_escape(root.source),
            json_escape(root.target)
        );
        fs::write(path, text).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
    }

    fn write_profile_entries(game_root: &Path, entries: &[ProfileModEntry])
    {
        let profile_path = game_root
            .join(".sa-mod-manager")
            .join("profiles")
            .join("default.json");
        let profile = ProfileJson {
            name: "default".to_string(),
            mods: entries.to_vec(),
            ..Default::default()
        };
        write_profile_json_file(&profile_path, game_root, &profile).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
    }

    enum TestModState
    {
        Enabled,
        Disabled,
    }

    impl TestModState
    {
        fn as_bool(self) -> bool
        {
            return matches!(self, Self::Enabled);
        }
    }

    fn test_profile_entry(
        id: &str,
        state: TestModState,
        load_order: i32,
        config: &Path,
    ) -> ProfileModEntry
    {
        return ProfileModEntry {
            id: id.to_string(),
            enabled: state.as_bool(),
            load_order,
            config: config.to_path_buf(),
            ..Default::default()
        };
    }

    fn test_root(name: &str) -> PathBuf
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        fs::create_dir_all(&root).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        // Materialization now requires a real-looking GTA install; give the
        // temp root the executable ensure_gta_install checks for.
        fs::write(root.join("gta_sa.exe"), b"").expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        return root;
    }

    fn remove_directory_if_exists(path: &Path) -> Result<(), AppError>
    {
        if path.exists()
        {
            fs::remove_dir_all(path)?;
        }
        return Ok(());
    }
