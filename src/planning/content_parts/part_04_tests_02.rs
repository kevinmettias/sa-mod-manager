
    #[test]
    fn equal_priority_conflict_is_flagged_ambiguous()
    {
        let a = modloader_entry("modloader/ModA/player.txd");
        let b = modloader_entry("modloader/ModB/player.txd");
        let refs = vec![&a, &b];
        // Both fall back to the default priority.
        let priorities = ModLoaderPriorities {
            default: MODLOADER_BUILTIN_DEFAULT_PRIORITY,
            by_folder: BTreeMap::new(),
        };

        let conflicts = modloader_conflicts(&refs, &priorities);
        assert_eq!(conflicts.len(), 1);
        assert!(
            conflicts[0].ambiguous,
            "equal priorities are non-deterministic"
        );
        // A single-folder asset is never a conflict.
        let solo = modloader_entry("modloader/ModA/unique.dff");
        assert!(modloader_conflicts(&[&solo], &priorities).is_empty());
    }

    #[test]
    fn mergeable_game_resource_files_are_recognized()
    {
        for f in [
            "modloader/x/data/handling.cfg",
            "modloader/x/vehicles.ide",
            "carcols.dat",
            "weapon.dat",
        ]
        {
            assert!(is_mergeable_game_resource_file(f), "{f} should be mergeable");
        }
        // Override-only data, models, and map files are winner-take-all.
        for f in [
            "modloader/x/data/timecyc.dat",
            "modloader/x/data/maps/la.ipl",
            "modloader/x/infernus.dff",
            "readme.txt",
        ]
        {
            assert!(!is_mergeable_game_resource_file(f), "{f} should not be mergeable");
        }
    }

    #[test]
    fn conflict_marks_merge_against_override()
    {
        let priorities = ModLoaderPriorities {
            default: MODLOADER_BUILTIN_DEFAULT_PRIORITY,
            by_folder: BTreeMap::new(),
        };
        // Two mods both ship handling.cfg -> mergeable (soft).
        let a = modloader_entry("modloader/ModA/data/handling.cfg");
        let b = modloader_entry("modloader/ModB/data/handling.cfg");
        let merged = modloader_conflicts(&[&a, &b], &priorities);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].mergeable, "handling.cfg is merged by ModLoader");

        // Two mods both ship an .ipl -> override (hard).
        let c = modloader_entry("modloader/ModA/data/maps/city.ipl");
        let d = modloader_entry("modloader/ModB/data/maps/city.ipl");
        let overridden = modloader_conflicts(&[&c, &d], &priorities);
        assert_eq!(overridden.len(), 1);
        assert!(!overridden[0].mergeable, ".ipl is override-only");
    }

    #[test]
    fn reserved_dot_folders_are_never_treated_as_mods()
    {
        let priorities = ModLoaderPriorities::default();
        // A file under modloader/.data must not become a conflicting "mod folder".
        let cached = modloader_entry("modloader/.data/plugins/x.dff");
        let real = modloader_entry("modloader/RealMod/x.dff");
        assert!(modloader_conflicts(&[&cached, &real], &priorities).is_empty());
        // And the priority/ignore writer never targets a reserved folder.
        assert_eq!(modloader_folder_from_target("modloader/.data/x"), None);
        assert_eq!(modloader_folder_from_target("modloader/.profiles/y"), None);
        assert_eq!(
            modloader_folder_from_target("modloader/RealMod/x").as_deref(),
            Some("RealMod")
        );
    }

    #[test]
    fn folder_from_target_extracts_named_sandbox()
    {
        assert_eq!(
            modloader_folder_from_target("modloader/vehicle/gta3.img").as_deref(),
            Some("vehicle")
        );
        // A target that is itself the sandbox folder yields that folder.
        assert_eq!(
            modloader_folder_from_target("modloader/HD_Roads").as_deref(),
            Some("HD_Roads")
        );
        // Bare `modloader` (no named subfolder) and non-modloader targets yield none.
        assert_eq!(modloader_folder_from_target("modloader").as_deref(), None);
        assert_eq!(modloader_folder_from_target("cleo").as_deref(), None);
    }

    #[test]
    fn spread_priority_is_ordered_distinct_and_centered()
    {
        // Two mods land at the extremes; three straddle the default.
        assert_eq!(
            spread_priority(0, MODLOADER_PRIORITY_PAIR_COUNT, MODLOADER_TEST_LIMIT),
            1
        );
        assert_eq!(
            spread_priority(1, MODLOADER_PRIORITY_PAIR_COUNT, MODLOADER_TEST_LIMIT),
            100 // literal: allow test fixture value is the specimen under judgment
        );
        assert_eq!(
            spread_priority(0, MODLOADER_PRIORITY_TRIPLE_COUNT, MODLOADER_TEST_LIMIT),
            1
        );
        assert_eq!(
            spread_priority(1, MODLOADER_PRIORITY_TRIPLE_COUNT, MODLOADER_TEST_LIMIT),
            MODLOADER_TEST_MID_PRIORITY
        );
        assert_eq!(
            spread_priority(2, MODLOADER_PRIORITY_TRIPLE_COUNT, MODLOADER_TEST_LIMIT), // literal: allow test fixture value is the specimen under judgment
            100 // literal: allow test fixture value is the specimen under judgment
        );
        // A lone mod sits near the default.
        assert_eq!(
            spread_priority(0, 1, MODLOADER_TEST_LIMIT),
            MODLOADER_BUILTIN_DEFAULT_PRIORITY
        );
    }

    #[test]
    fn render_priority_ini_from_scratch_sets_profile_and_section()
    {
        let mut priorities = BTreeMap::new();
        priorities.insert("HD_Roads".to_string(), 1);
        priorities.insert("ImVehFt".to_string(), MODLOADER_TEST_LIMIT);
        let ini = render_modloader_priority_ini(None, "Default", MODLOADER_TEST_LIMIT, &priorities)
            .expect("the test fixture is created before this assertion reads it");

        assert!(ini.contains("[Folder.Config]"));
        assert!(ini.contains("Profile = Default"));
        assert!(ini.contains("[Profiles.Default.Priority]"));
        assert!(ini.contains("HD_Roads = 1"));
        assert!(ini.contains("ImVehFt = 100"));
        // Round-trips through our own reader.
        let parsed = parse_modloader_priorities(&ini);
        assert_eq!(parsed.for_folder("ImVehFt"), MODLOADER_TEST_LIMIT);
        assert_eq!(parsed.for_folder("HD_Roads"), 1);
    }

    #[test]
    fn render_priority_ini_preserves_unrelated_lines_and_updates_in_place()
    {
        let existing = "\
; user notes
[Folder.Config]
Profile = Default
PriorityLimit = 100

[Profiles.Default.Config]
IgnoreAllMods = false

[Profiles.Default.Priority]
ImVehFt = 20
UserMod = 90

[Profiles.Default.IgnoreMods]
_ignore
";
        let mut priorities = BTreeMap::new();
        // Update an existing managed folder and add a new one; UserMod untouched.
        priorities.insert("ImVehFt".to_string(), MODLOADER_TEST_LIMIT);
        priorities.insert("HD_Roads".to_string(), 1);
        let ini = render_modloader_priority_ini(
            Some(existing),
            "Default",
            MODLOADER_TEST_LIMIT,
            &priorities,
        )
        .expect("the test fixture is created before this assertion reads it");

        assert!(ini.contains("; user notes"));
        assert!(ini.contains("[Profiles.Default.Config]"));
        assert!(ini.contains("UserMod = 90"), "unmanaged folder preserved");
        assert!(
            ini.contains("ImVehFt = 100"),
            "managed folder updated in place"
        );
        assert!(ini.contains("HD_Roads = 1"), "new managed folder added");
        assert!(ini.contains("[Profiles.Default.IgnoreMods]"));
        assert!(ini.contains("_ignore"));
        // The new folder is inserted inside the Priority section, before the
        // next section header.
        let priority_idx = ini.find("[Profiles.Default.Priority]").expect("the test fixture is created before this assertion reads it");
        let ignore_idx = ini.find("[Profiles.Default.IgnoreMods]").expect("the test fixture is created before this assertion reads it");
        let hd_idx = ini.find("HD_Roads = 1").expect("the test fixture is created before this assertion reads it");
        assert!(priority_idx < hd_idx && hd_idx < ignore_idx);
    }

    #[test]
    fn render_priority_ini_returns_none_when_already_current()
    {
        let existing = "\
[Folder.Config]
Profile = Default

[Profiles.Default.Priority]
ImVehFt = 100
";
        let mut priorities = BTreeMap::new();
        priorities.insert("ImVehFt".to_string(), MODLOADER_TEST_LIMIT);
        assert!(
            render_modloader_priority_ini(
                Some(existing),
                "Default",
                MODLOADER_TEST_LIMIT,
                &priorities
            )
            .is_none(),
            "no change should skip the write"
        );
    }

    #[test]
    fn active_profile_and_limit_read_from_folder_config()
    {
        let ini = "\
[Folder.Config]
Profile = SAMP
PriorityLimit = 200
";
        assert_eq!(modloader_active_profile(ini), "SAMP");
        assert_eq!(modloader_priority_limit(ini), MODLOADER_TEST_EXTENDED_LIMIT);
        // Defaults when absent.
        assert_eq!(modloader_active_profile(""), "Default");
        assert_eq!(modloader_priority_limit(""), MODLOADER_TEST_LIMIT);
    }

    #[test]
    fn managed_profile_name_is_prefixed_and_sanitized()
    {
        assert_eq!(
            modloader_managed_profile_name("vanilla-plus"),
            "SAMM_vanilla-plus"
        );
        // Spaces and unsafe characters are normalized so the name is safe in an
        // INI header and a -modprof argument.
        let name = modloader_managed_profile_name("My Profile!");
        assert!(name.starts_with("SAMM_"));
        assert!(!name.contains(' '));
        assert!(!name.contains('!'));
    }

    #[test]
    fn managed_profile_from_scratch_writes_config_and_priority_without_folder_config()
    {
        let mut priorities = BTreeMap::new();
        priorities.insert("early_mod".to_string(), 1);
        priorities.insert("late_mod".to_string(), MODLOADER_TEST_LIMIT);
        let ini = render_modloader_managed_profile(ModLoaderManagedProfileRender {
            existing: None,
            profile: "SAMM_default",
            parent: "Default",
            limit: MODLOADER_TEST_LIMIT,
            folder_priorities: &priorities,
            ignore_folders: &[],
            ignore_files: &[],
        })
        .expect("the test fixture is created before this assertion reads it");

        assert!(ini.contains("[Profiles.SAMM_default.Config]"));
        assert!(ini.contains("Parents = Default"));
        assert!(ini.contains("[Profiles.SAMM_default.Priority]"));
        assert!(ini.contains("early_mod = 1"));
        assert!(ini.contains("late_mod = 100"));
        // Activation is via -modprof, so the file's default profile is untouched.
        assert!(!ini.contains("[Folder.Config]"));
        // No disabled mods -> no IgnoreMods section.
        assert!(!ini.contains("IgnoreMods"));
    }

    #[test]
    fn managed_profile_writes_ignoremods_for_disabled_folders()
    {
        let mut priorities = BTreeMap::new();
        priorities.insert("enabled_mod".to_string(), MODLOADER_TEST_LIMIT);
        let ignore = vec!["off_mod".to_string(), "also_off".to_string()];
        let ini = render_modloader_managed_profile(ModLoaderManagedProfileRender {
            existing: None,
            profile: "SAMM_default",
            parent: "Default",
            limit: MODLOADER_TEST_LIMIT,
            folder_priorities: &priorities,
            ignore_folders: &ignore,
            ignore_files: &[],
        })
        .expect("the test fixture is created before this assertion reads it");

        assert!(
            ini.contains("[Profiles.SAMM_default.IgnoreMods]"),
            "ini was:\n{ini}"
        );
        assert!(ini.contains("off_mod"));
        assert!(ini.contains("also_off"));
        // Clearing the disabled set removes the IgnoreMods block again.
        let cleared = render_modloader_managed_profile(ModLoaderManagedProfileRender {
            existing: Some(&ini),
            profile: "SAMM_default",
            parent: "Default",
            limit: MODLOADER_TEST_LIMIT,
            folder_priorities: &priorities,
            ignore_folders: &[],
            ignore_files: &[],
        })
        .expect("the test fixture is created before this assertion reads it");
        assert!(
            !cleared.contains("IgnoreMods"),
            "cleared ini was:\n{cleared}"
        );
        assert!(!cleared.contains("off_mod"));
    }

    #[test]
    fn managed_profile_writes_ignorefiles_globs()
    {
        let mut priorities = BTreeMap::new();
        priorities.insert("mod".to_string(), MODLOADER_TEST_LIMIT);
        let ignore_files = vec!["*.dff".to_string(), "to_ignore/bad.txd".to_string()];
        let ini = render_modloader_managed_profile(ModLoaderManagedProfileRender {
            existing: None,
            profile: "SAMM_default",
            parent: "Default",
            limit: MODLOADER_TEST_LIMIT,
            folder_priorities: &priorities,
            ignore_folders: &[],
            ignore_files: &ignore_files,
        })
        .expect("the test fixture is created before this assertion reads it");

        assert!(
            ini.contains("[Profiles.SAMM_default.IgnoreFiles]"),
            "ini was:\n{ini}"
        );
        assert!(ini.contains("*.dff"));
        assert!(ini.contains("to_ignore/bad.txd"));
        // Clearing the globs drops the IgnoreFiles block.
        let cleared = render_modloader_managed_profile(ModLoaderManagedProfileRender {
            existing: Some(&ini),
            profile: "SAMM_default",
            parent: "Default",
            limit: MODLOADER_TEST_LIMIT,
            folder_priorities: &priorities,
            ignore_folders: &[],
            ignore_files: &[],
        })
        .expect("the test fixture is created before this assertion reads it");
        assert!(
            !cleared.contains("IgnoreFiles"),
            "cleared ini was:\n{cleared}"
        );
    }
