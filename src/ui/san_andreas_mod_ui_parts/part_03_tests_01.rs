    use super::{export_telemetry_summary, load_telemetry_summary};
    use crate::prelude::*;
    use crate::ui::preferences::UiPreferences;
    use crate::ui::san_andreas_mod_ui::resolve_initial_game_root;

    #[test]
    fn explicit_game_root_wins_over_saved_then_default()
    {
        let saved = UiPreferences {
            game_root: "S:/saved".to_string(),
            ..UiPreferences::default()
        };
        // An explicit CLI folder always wins.
        assert_eq!(
            resolve_initial_game_root(Some(PathBuf::from("C:/explicit")), &saved),
            PathBuf::from("C:/explicit")
        );
        // With no CLI folder, the last-used folder is restored.
        assert_eq!(
            resolve_initial_game_root(None, &saved),
            PathBuf::from("S:/saved")
        );
        // With neither, fall back to the auto-detected default.
        assert_eq!(
            resolve_initial_game_root(None, &UiPreferences::default()),
            crate::settings::default_game_root()
        );
    }

    #[test]
    fn telemetry_summary_aggregates_imports_and_journals()
    {
        let game_root = test_root("telemetry_summary");
        let state_root = state_directory(&game_root);
        let import_root = state_root.join("library").join("test_mod");
        let journals_root = state_root.join("journals");
        fs::create_dir_all(&import_root).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(&journals_root).expect("the test fixture is created before this assertion reads it");
        fs::write(
            import_root.join("import.json"),
            concat!(
                "{\n",
                "  \"version\": 1,\n",
                "  \"id\": \"test_mod\",\n",
                "  \"imported_unix\": 100,\n",
                "  \"entry_count\": 5,\n",
                "  \"operation_count\": 2\n",
                "}\n"
            ),
        )
        .expect("the test fixture is created before this assertion reads it");
        fs::write(
            journals_root.join("run-default-200.journal"),
            concat!(
                "version=1\n",
                "txid=run-default-200\n",
                "profile=default\n",
                "mode=ephemeral-run\n",
                "created_unix=200\n",
                "profile_mod=test_mod|100|mods/test_mod/mod.json\n",
                "new=CLEO/test.cs\n",
                "backup=data/file.dat|backup/file.dat|fnv64:1\n",
                "copy=src|dst|fnv64:2\n",
                "missing_source=missing\n"
            ),
        )
        .expect("the test fixture is created before this assertion reads it");

        let summary = load_telemetry_summary(&game_root, 1).expect("the test fixture is created before this assertion reads it");

        assert_eq!(summary.imports, 1);
        assert_eq!(summary.journals, 1);
        assert_eq!(summary.run_journals, 1);
        assert_eq!(summary.copied_files, 1);
        assert_eq!(summary.new_files, 1);
        assert_eq!(summary.overwritten_files, 1);
        assert_eq!(summary.missing_sources, 1);
        assert_eq!(summary.pending_cleanup, 1);
        assert_eq!(summary.recent_events.len(), 2); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(summary.recent_events[0].kind, "run");
        assert_eq!(summary.mod_history.len(), 1);
        assert_eq!(summary.mod_history[0].id, "test_mod");
        assert_eq!(summary.mod_history[0].imports, 1);
        assert_eq!(summary.mod_history[0].runs, 1);
        assert_eq!(summary.mod_history[0].copied_files, 3); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(summary.mod_history[0].overwritten_files, 1);
        assert_eq!(summary.mod_history[0].missing_sources, 1);
        let export = export_telemetry_summary(&game_root, &summary).expect("the test fixture is created before this assertion reads it");
        let export_text = fs::read_to_string(export).expect("the test fixture is created before this assertion reads it");
        assert!(export_text.contains("\"mods\""));
        assert!(export_text.contains("\"events\""));
        assert!(export_text.contains("test_mod"));
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn launch_failed_outcome_counts_as_failed_launch_not_run()
    {
        let game_root = test_root("telemetry_failed_launch");
        let state_root = state_directory(&game_root);
        fs::create_dir_all(state_root.join("journals")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(state_root.join("outcomes")).expect("the test fixture is created before this assertion reads it");
        fs::write(
            state_root.join("journals").join("run-default-300.journal"),
            concat!(
                "version=1\n",
                "profile=default\n",
                "mode=ephemeral-run\n",
                "created_unix=300\n",
                "profile_mod=test_mod|100|mods/test_mod/mod.json\n",
                "new=CLEO/test.cs\n"
            ),
        )
        .expect("the test fixture is created before this assertion reads it");
        fs::write(
            state_root.join("outcomes").join("run-default-300.json"),
            r#"{"version":1,"txid":"run-default-300","profile":"default","result":"launch_failed","exit_code":null,"duration_ms":0,"launch_args":["-w"],"started_unix":300,"finished_unix":300}"#,
        )
        .expect("the test fixture is created before this assertion reads it");

        let summary = load_telemetry_summary(&game_root, 0).expect("the test fixture is created before this assertion reads it");

        assert_eq!(summary.failed_launches, 1);
        assert_eq!(summary.run_journals, 0);
        // A rolled-back launch must not inflate file aggregates or per-mod runs.
        assert_eq!(summary.new_files, 0);
        assert!(summary.mod_history.is_empty());
        assert!(
            summary.recent_events.iter().any(
                |event| event.kind == "launch failed" && event.detail.contains("never started")
            )
        );
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn successful_run_outcome_enriches_event_detail_with_exit_and_duration()
    {
        let game_root = test_root("telemetry_run_detail");
        let state_root = state_directory(&game_root);
        fs::create_dir_all(state_root.join("journals")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(state_root.join("outcomes")).expect("the test fixture is created before this assertion reads it");
        fs::write(
            state_root.join("journals").join("run-default-400.journal"),
            concat!(
                "version=1\n",
                "profile=default\n",
                "mode=ephemeral-run\n",
                "created_unix=400\n",
                "profile_mod=test_mod|100|mods/test_mod/mod.json\n",
                "copy=src|dst|fnv64:1\n"
            ),
        )
        .expect("the test fixture is created before this assertion reads it");
        fs::write(
            state_root.join("outcomes").join("run-default-400.json"),
            r#"{"version":1,"txid":"run-default-400","profile":"default","result":"success","exit_code":0,"duration_ms":42000,"launch_args":[],"started_unix":400,"finished_unix":442}"#,
        )
        .expect("the test fixture is created before this assertion reads it");

        let summary = load_telemetry_summary(&game_root, 0).expect("the test fixture is created before this assertion reads it");

        assert_eq!(summary.run_journals, 1);
        assert_eq!(summary.failed_launches, 0);
        let run_event = summary
            .recent_events
            .iter()
            .find(|event| event.kind == "run")
            .expect("run event present");
        assert!(
            run_event.detail.contains("exited 0 after 42s"),
            "detail was: {}",
            run_event.detail
        );
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
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
