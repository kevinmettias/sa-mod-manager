
    #[test]
    fn managed_profile_preserves_user_config_and_leaves_default_active()
    {
        let existing = "\
[Folder.Config]
Profile = Default
PriorityLimit = 100

[Profiles.Default.Priority]
UserMod = 70
";
        let mut priorities = BTreeMap::new();
        priorities.insert("my_mod".to_string(), MODLOADER_TEST_LIMIT);
        let ini = render_modloader_managed_profile(ModLoaderManagedProfileRender {
            existing: Some(existing),
            profile: "SAMM_default",
            parent: "Default",
            limit: MODLOADER_TEST_LIMIT,
            folder_priorities: &priorities,
            ignore_folders: &[],
            ignore_files: &[],
        })
        .expect("the test fixture is created before this assertion reads it");

        // The user's own profile, active profile, and priorities are untouched.
        assert!(ini.contains("Profile = Default"));
        assert!(ini.contains("UserMod = 70"));
        // Our managed profile is added, inheriting the user's Default.
        assert!(ini.contains("[Profiles.SAMM_default.Config]"));
        assert!(ini.contains("Parents = Default"));
        assert!(ini.contains("[Profiles.SAMM_default.Priority]"));
        assert!(ini.contains("my_mod = 100"));
    }

    #[test]
    fn managed_profile_returns_none_when_already_current()
    {
        let existing = "\
[Profiles.SAMM_default.Config]
Parents = Default

[Profiles.SAMM_default.Priority]
my_mod = 100
";
        let mut priorities = BTreeMap::new();
        priorities.insert("my_mod".to_string(), MODLOADER_TEST_LIMIT);
        assert!(
            render_modloader_managed_profile(ModLoaderManagedProfileRender {
                existing: Some(existing),
                profile: "SAMM_default",
                parent: "Default",
                limit: MODLOADER_TEST_LIMIT,
                folder_priorities: &priorities,
                ignore_folders: &[],
                ignore_files: &[],
            })
            .is_none()
        );
    }

    #[test]
    fn modloader_log_scrapes_version_errors_and_warnings()
    {
        let log = "\
========================== Mod Loader 0.3.7 ==========================
Using data from \"modloader/late_mod\"
Warning: ignoring file \"readme.txt\"
Failed to read \"modloader/broken/veh.dff\"
Everything loaded fine
Could not open archive gta3.img
";
        let summary = parse_modloader_log(log);
        assert_eq!(summary.version.as_deref(), Some("0.3.7"));
        assert_eq!(
            summary.errors.len(),
            EXPECTED_LOG_ERROR_COUNT,
            "errors: {:?}",
            summary.errors
        );
        assert!(summary.errors.iter().any(|l| l.contains("Failed to read")));
        assert!(summary.errors.iter().any(|l| l.contains("Could not open")));
        assert_eq!(summary.warnings.len(), 1);
        assert!(summary.warnings[0].contains("ignoring"));
        assert!(!summary.is_clean());
    }

    #[test]
    fn modloader_log_clean_run_has_no_findings()
    {
        let log = "\
========================== Mod Loader 0.3.7 ==========================
Using data from \"modloader/a\"
Using data from \"modloader/b\"
";
        let summary = parse_modloader_log(log);
        assert_eq!(summary.version.as_deref(), Some("0.3.7"));
        assert!(summary.is_clean());
        assert!(!summary.truncated);
    }
