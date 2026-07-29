    use super::{EXPECTED_GROUP_ENTRY_COUNT, EXPECTED_HANDLING_FILE_COUNT, EXPECTED_LOG_ERROR_COUNT, MODLOADER_BUILTIN_DEFAULT_PRIORITY, MODLOADER_PRIORITY_PAIR_COUNT, MODLOADER_PRIORITY_TRIPLE_COUNT, MODLOADER_TEST_DEFAULT_PRIORITY, MODLOADER_TEST_HIGH_PRIORITY, MODLOADER_TEST_LIMIT, MODLOADER_TEST_LOW_PRIORITY, MODLOADER_TEST_MID_PRIORITY, MODLOADER_TEST_EXTENDED_LIMIT, ModLoaderManagedProfileRender, asi_view, build_content_index, cleo_view, group_entries, is_mergeable_data_file, modloader_conflicts, modloader_folder_from_target, modloader_virtual_asset, modloader_active_profile, modloader_managed_profile_name, modloader_priority_limit, parse_modloader_log, parse_modloader_priorities, per_mod_flags, render_modloader_managed_profile, render_modloader_priority_ini, spread_priority};
    use crate::prelude::*;

    struct TestInstallRoot<'a>
    {
        source: &'a str,
        target: &'a str,
        kind: &'a str,
    }

    fn root(root: TestInstallRoot<'_>) -> ModInstallRootJson
    {
        let source = root.source;
        let target = root.target;
        let kind = root.kind;
        return ModInstallRootJson {
            source: source.to_string(),
            target: target.to_string(),
            kind: kind.to_string(),
            enabled: true,
            optional: false,
        };
    }

    #[test]
    fn later_mod_wins_a_conflicting_target_and_is_flagged()
    {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-content-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let early = base.join("early");
        let late = base.join("late");
        write_file(&early.join("payload").join("handling.cfg"), "early");
        write_file(&late.join("payload").join("handling.cfg"), "late");

        let mods = vec![
            IndexedMod {
                id: "early".to_string(),
                source_root: early.clone(),
                roots: vec![root(TestInstallRoot { source: "payload", target: "data", kind: "direct" })],
            },
            IndexedMod {
                id: "late".to_string(),
                source_root: late.clone(),
                roots: vec![root(TestInstallRoot { source: "payload", target: "data", kind: "direct" })],
            },
        ];

        let index = build_content_index(&mods);
        assert_eq!(index.entries.len(), 1);
        let entry = &index.entries[0];
        assert_eq!(entry.target, "data/handling.cfg");
        assert_eq!(entry.category, ContentCategory::Data);
        assert_eq!(entry.providers, vec!["early", "late"]);
        assert_eq!(entry.winner(), "late");
        assert!(entry.is_conflict());
        assert_eq!(index.conflict_count(), 1);
        fs::remove_dir_all(&base).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn per_mod_flags_mark_winner_and_loser_of_a_conflict()
    {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-flags-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let early = base.join("early");
        let late = base.join("late");
        write_file(&early.join("p").join("handling.cfg"), "e");
        write_file(&early.join("p").join("solo.dat"), "e");
        write_file(&late.join("p").join("handling.cfg"), "l");

        let mods = vec![
            IndexedMod {
                id: "early".to_string(),
                source_root: early.clone(),
                roots: vec![root(TestInstallRoot { source: "p", target: "data", kind: "direct" })],
            },
            IndexedMod {
                id: "late".to_string(),
                source_root: late.clone(),
                roots: vec![root(TestInstallRoot { source: "p", target: "data", kind: "direct" })],
            },
        ];
        let index = build_content_index(&mods);
        let flags = per_mod_flags(&index);

        let early_flags = flags.get("early").expect("the test fixture is created before this assertion reads it");
        assert!(early_flags.overwritten, "early loses handling.cfg");
        assert!(!early_flags.overwrites_others);
        assert_eq!(early_flags.file_count, EXPECTED_HANDLING_FILE_COUNT);

        let late_flags = flags.get("late").expect("the test fixture is created before this assertion reads it");
        assert!(late_flags.overwrites_others, "late wins handling.cfg");
        assert!(!late_flags.overwritten);
        assert!(late_flags.categories.contains(&ContentCategory::Data));
        fs::remove_dir_all(&base).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn cleo_saves_are_excluded_from_the_content_index()
    {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-saves-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let a = base.join("a");
        let b = base.join("b");
        // Two mods both ship the same CLEO save file — normally a conflict.
        write_file(&a.join("s").join("slot1.sav"), "a");
        write_file(&b.join("s").join("slot1.sav"), "b");

        let mods = vec![
            IndexedMod {
                id: "a".to_string(),
                source_root: a.clone(),
                roots: vec![root(TestInstallRoot { source: "s", target: "CLEO/cleo_saves", kind: "cleo_saves" })],
            },
            IndexedMod {
                id: "b".to_string(),
                source_root: b.clone(),
                roots: vec![root(TestInstallRoot { source: "s", target: "CLEO/cleo_saves", kind: "cleo_saves" })],
            },
        ];

        let index = build_content_index(&mods);
        assert!(
            index.entries.is_empty(),
            "CLEO save data is user data and must not be indexed"
        );
        assert_eq!(index.conflict_count(), 0, "save data is never a conflict");
        assert!(
            per_mod_flags(&index).is_empty(),
            "no mod is flagged for saves"
        );
        fs::remove_dir_all(&base).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn cleo_view_pairs_scripts_with_their_companions()
    {
        let script = entry("cleo/speedo.cs");
        let ini = entry("cleo/speedo.ini");
        let fxt = entry("cleo/speedo.fxt");
        let orphan = entry("cleo/shared_data.dat");
        let refs = vec![&script, &ini, &fxt, &orphan];

        let view = cleo_view(&refs);
        assert!(view.plugins.is_empty());
        assert_eq!(view.scripts.len(), 1);
        assert_eq!(view.scripts[0].script.target, "cleo/speedo.cs");
        let companions: Vec<&str> = view.scripts[0]
            .companions
            .iter()
            .map(|c| c.target.as_str())
            .collect();
        assert_eq!(companions, vec!["cleo/speedo.fxt", "cleo/speedo.ini"]);
        assert_eq!(view.loose.len(), 1);
        assert_eq!(view.loose[0].target, "cleo/shared_data.dat");
    }

    #[test]
    fn cleo_view_separates_plugins_from_scripts()
    {
        let script = entry("cleo/mod.cs");
        let compat = entry("cleo/legacy.cs4");
        let plugin = entry("cleo/cleo_plugins/SA.IniFiles.cleo");
        let refs = vec![&script, &compat, &plugin];

        let view = cleo_view(&refs);
        assert_eq!(view.plugins.len(), 1);
        assert_eq!(view.plugins[0].target, "cleo/cleo_plugins/SA.IniFiles.cleo");
        let scripts: Vec<&str> = view
            .scripts
            .iter()
            .map(|s| s.script.target.as_str())
            .collect();
        assert_eq!(scripts, vec!["cleo/legacy.cs4", "cleo/mod.cs"]);
    }

    #[test]
    fn asi_view_separates_plugins_from_loader_dlls()
    {
        let plugin = entry("scripts/CLEO.asi");
        let loader = entry("dinput8.dll");
        let config = entry("scripts/CLEO.ini");
        let refs = vec![&plugin, &loader, &config];

        let view = asi_view(&refs);
        assert_eq!(view.plugins.len(), 1);
        assert_eq!(view.plugins[0].target, "scripts/CLEO.asi");
        assert_eq!(view.loaders.len(), 1);
        assert_eq!(view.loaders[0].target, "dinput8.dll");
        assert_eq!(view.other.len(), 1);
        assert_eq!(view.other[0].target, "scripts/CLEO.ini");
    }

    #[test]
    fn modloader_priorities_parse_section_and_default()
    {
        let ini = "\
[Config]
DefaultPriority = 60

[Config.Priority]
; comment line
ImVehFt = 100
HD_Roads=30   ; inline comment
";
        let priorities = parse_modloader_priorities(ini);
        assert_eq!(priorities.default, MODLOADER_TEST_DEFAULT_PRIORITY);
        assert_eq!(priorities.for_folder("ImVehFt"), MODLOADER_TEST_LIMIT);
        // Case-insensitive folder lookup.
        assert_eq!(
            priorities.for_folder("hd_roads"),
            MODLOADER_TEST_LOW_PRIORITY
        );
        // Unknown folder falls back to the default.
        assert_eq!(
            priorities.for_folder("SomethingElse"),
            MODLOADER_TEST_DEFAULT_PRIORITY
        );
    }

    #[test]
    fn modloader_entries_group_by_their_sandboxed_folder()
    {
        let make = |target: &str| ContentEntry {
            target: target.to_string(),
            category: ContentCategory::ModLoader,
            providers: vec!["m".to_string()],
        };
        let a = make("modloader/ImVehFt/veh.dff");
        let b = make("modloader/ImVehFt/veh.txd");
        let c = make("modloader/HD_Roads/road.txd");
        let refs = vec![&a, &b, &c];

        let groups = group_entries(ContentCategory::ModLoader, &refs);
        let labels: Vec<&str> = groups.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(labels, vec!["HD_Roads", "ImVehFt"]);
        let imvehft = groups.iter().find(|(label, _)| label == "ImVehFt").expect("the test fixture is created before this assertion reads it");
        assert_eq!(imvehft.1.len(), EXPECTED_GROUP_ENTRY_COUNT);
    }

    #[test]
    fn non_modloader_entries_group_by_winning_mod()
    {
        let entry = ContentEntry {
            target: "data/handling.cfg".to_string(),
            category: ContentCategory::Data,
            providers: vec!["early".to_string(), "late".to_string()],
        };
        let refs = vec![&entry];
        let groups = group_entries(ContentCategory::Data, &refs);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "late");
    }

    #[test]
    fn kinds_and_paths_route_to_the_right_viewers()
    {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-content-cat-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let m = base.join("mod");
        write_file(&m.join("cleo").join("script.cs"), "x");
        write_file(&m.join("asi").join("plugin.asi"), "x");
        write_file(&m.join("ml").join("car.dff"), "x");
        write_file(&m.join("direct").join("text").join("american.gxt"), "x");

        let mods = vec![IndexedMod {
            id: "mod".to_string(),
            source_root: m.clone(),
            roots: vec![
                root(TestInstallRoot { source: "cleo", target: "cleo", kind: "cleo" }),
                root(TestInstallRoot { source: "asi", target: ".", kind: "asi" }),
                root(TestInstallRoot { source: "ml", target: "modloader/mymod", kind: "modloader" }),
                root(TestInstallRoot { source: "direct", target: ".", kind: "direct" }),
            ],
        }];

        let index = build_content_index(&mods);
        let category_of = |target: &str| {
            index
                .entries
                .iter()
                .find(|entry| entry.target == target)
                .map(|entry| entry.category)
        };
        assert_eq!(category_of("cleo/script.cs"), Some(ContentCategory::Cleo));
        assert_eq!(category_of("plugin.asi"), Some(ContentCategory::Asi));
        assert_eq!(
            category_of("modloader/mymod/car.dff"),
            Some(ContentCategory::ModLoader)
        );
        assert_eq!(
            category_of("text/american.gxt"),
            Some(ContentCategory::Text)
        );
        fs::remove_dir_all(&base).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn virtual_asset_strips_sandbox_and_img_container()
    {
        // Streamed model resolves by base name, ignoring the img container.
        assert_eq!(
            modloader_virtual_asset("modloader/ModA/gta3.img/infernus.dff").as_deref(),
            Some("infernus.dff")
        );
        assert_eq!(
            modloader_virtual_asset("modloader/ModB/infernus.dff").as_deref(),
            Some("infernus.dff")
        );
        // Non-streamed data keeps its relative path (minus the sandbox folder).
        assert_eq!(
            modloader_virtual_asset("modloader/ModA/data/handling.cfg").as_deref(),
            Some("data/handling.cfg")
        );
        // A bare sandbox folder with no file is not comparable.
        assert_eq!(modloader_virtual_asset("modloader/ModA").as_deref(), None);
    }

    #[test]
    fn modloader_conflicts_detected_across_sandbox_folders_with_priority_winner()
    {
        // Two vehicle mods each replace infernus.dff in their own folder — no
        // literal target collision, but a real ModLoader runtime conflict.
        let a = ml_entry("modloader/ModA/infernus.dff");
        let b = ml_entry("modloader/ModB/gta3.img/infernus.dff");
        let refs = vec![&a, &b];

        let mut priorities = ModLoaderPriorities {
            default: MODLOADER_BUILTIN_DEFAULT_PRIORITY,
            by_folder: BTreeMap::new(),
        };
        priorities
            .by_folder
            .insert("moda".to_string(), MODLOADER_TEST_LOW_PRIORITY);
        priorities
            .by_folder
            .insert("modb".to_string(), MODLOADER_TEST_HIGH_PRIORITY);

        let conflicts = modloader_conflicts(&refs, &priorities);
        assert_eq!(conflicts.len(), 1);
        let conflict = &conflicts[0];
        assert_eq!(conflict.asset, "infernus.dff");
        // Higher priority wins.
        assert_eq!(conflict.winner(), "ModB");
        assert!(!conflict.ambiguous);
    }

    fn ml_entry(target: &str) -> ContentEntry
    {
        return ContentEntry {
            target: target.to_string(),
            category: ContentCategory::ModLoader,
            providers: vec!["m".to_string()],
        };
    }

    fn write_file(path: &Path, contents: &str)
    {
        if let Some(parent) = path.parent()
        {
            fs::create_dir_all(parent).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        }
        fs::write(path, contents).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
    }

    fn entry(target: &str) -> ContentEntry
    {
        return ContentEntry {
            target: target.to_string(),
            category: ContentCategory::Cleo,
            providers: vec!["m".to_string()],
        };
    }


