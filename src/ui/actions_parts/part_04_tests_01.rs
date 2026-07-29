    use super::{OTHER_TEST_PID, TEST_PENDING_PID, TEST_PID, forget_pending_run, load_pending_run_records, is_pending_run_match_for_current_session, is_child_program_running, readme_proposal_from_instruction, readme_copy_install_root, remember_pending_run, validate_install_root};
    use crate::prelude::*;
    use crate::ui::actions::{PendingRunRecord, PendingRunStatus, ReadmeCopyInstallRoot, ReadmeProposalState};
    use crate::ui::san_andreas_mod_ui::SanAndreasModUi;

    #[test]
    fn pending_run_records_survive_reload_and_can_be_forgotten()
    {
        let game_root = test_root("pending_run_records");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("run-default-123.journal");
        fs::create_dir_all(journal.parent().expect("the test fixture is created before this assertion reads it")).expect("the test fixture is created before this assertion reads it");
        let materialized = game_root.join("CLEO").join("gravityfix.cs");
        fs::create_dir_all(materialized.parent().expect("the test fixture is created before this assertion reads it")).expect("the test fixture is created before this assertion reads it");
        fs::write(&materialized, "script").expect("the test fixture is created before this assertion reads it");
        fs::write(
            &journal,
            format!(
                "version=1\nmode=ephemeral-run\nnew={}\n",
                escape_value(&materialized.display().to_string())
            ),
        )
        .expect("the test fixture is created before this assertion reads it");

        remember_pending_run(&game_root, &journal, Some(TEST_PID)).expect("the test fixture is created before this assertion reads it");
        let records = load_pending_run_records(&game_root).expect("the test fixture is created before this assertion reads it");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].journal, journal);
        assert_eq!(records[0].pid, Some(TEST_PID));

        forget_pending_run(&game_root, &journal).expect("the test fixture is created before this assertion reads it");
        assert!(load_pending_run_records(&game_root).expect("the test fixture is created before this assertion reads it").is_empty());
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn stale_pending_runs_are_bulk_cleaned_after_validation()
    {
        let game_root = test_root("stale_pending_cleanup");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("run-default-456.journal");
        let materialized = game_root.join("modloader").join("test").join("file.txt");
        fs::create_dir_all(journal.parent().expect("the test fixture is created before this assertion reads it")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(materialized.parent().expect("the test fixture is created before this assertion reads it")).expect("the test fixture is created before this assertion reads it");
        fs::write(&materialized, "payload").expect("the test fixture is created before this assertion reads it");
        fs::write(
            &journal,
            format!(
                "version=1\nmode=ephemeral-run\nnew={}\n",
                escape_value(&materialized.display().to_string())
            ),
        )
        .expect("the test fixture is created before this assertion reads it");
        remember_pending_run(&game_root, &journal, Some(u32::MAX)).expect("the test fixture is created before this assertion reads it");

        let mut ui = SanAndreasModUi::new(game_root.clone(), test_preferences());
        assert_eq!(ui.pending_runs.len(), 1);
        assert_eq!(ui.pending_runs[0].status, PendingRunStatus::Stale);

        ui.cleanup_stale_pending_runs();

        assert!(!materialized.exists());
        assert!(load_pending_run_records(&game_root).expect("the test fixture is created before this assertion reads it").is_empty());
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn pending_run_watcher_auto_cleans_finished_pid_backed_runs()
    {
        let game_root = test_root("pending_run_watcher_cleanup");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("run-default-789.journal");
        let materialized = game_root.join("CLEO").join("watcher.cs");
        fs::create_dir_all(journal.parent().expect("the test fixture is created before this assertion reads it")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(materialized.parent().expect("the test fixture is created before this assertion reads it")).expect("the test fixture is created before this assertion reads it");
        fs::write(&materialized, "script").expect("the test fixture is created before this assertion reads it");
        fs::write(
            &journal,
            format!(
                "version=1\nmode=ephemeral-run\nnew={}\n",
                escape_value(&materialized.display().to_string())
            ),
        )
        .expect("the test fixture is created before this assertion reads it");
        remember_pending_run(&game_root, &journal, Some(u32::MAX)).expect("the test fixture is created before this assertion reads it");

        let mut ui = SanAndreasModUi::new(game_root.clone(), test_preferences());
        ui.last_pending_watch = Instant::now() - Duration::from_secs(3); // literal: allow test fixture value is the specimen under judgment
        ui.tick_pending_run_watcher();

        assert!(!materialized.exists());
        assert!(load_pending_run_records(&game_root).expect("the test fixture is created before this assertion reads it").is_empty());
        assert!(ui.status.contains("auto-cleaned 1"));
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn readme_proposals_expose_confidence_review_states()
    {
        // Explicit thresholds keep the classification independent of any local
        // config the developer may have set.
        let thresholds = crate::settings::ReadmeThresholds::default();
        let high = test_readme_instruction(ReadmeAction::Copy, 0.90); // literal: allow test fixture value is the specimen under judgment
        let medium = test_readme_instruction(ReadmeAction::Copy, 0.72); // literal: allow test fixture value is the specimen under judgment
        let low = test_readme_instruction(ReadmeAction::Copy, 0.40); // literal: allow test fixture value is the specimen under judgment

        assert_eq!(
            readme_proposal_from_instruction(&high, thresholds).review_state,
            ReadmeProposalState::AutoSelected
        );
        assert_eq!(
            readme_proposal_from_instruction(&medium, thresholds).review_state,
            ReadmeProposalState::NeedsReview
        );
        assert_eq!(
            readme_proposal_from_instruction(&low, thresholds).review_state,
            ReadmeProposalState::WarningOnly
        );
    }

    fn test_readme_instruction(action: ReadmeAction, confidence: f32) -> ReadmeInstruction
    {
        return ReadmeInstruction {
            source_readme: "README.txt".to_string(),
            line_number: 8, // literal: allow test fixture value is the specimen under judgment
            action,
            source: Some("CLEO".to_string()),
            target: Some("CLEO".to_string()),
            confidence,
            text: "Copy CLEO to CLEO.".to_string(),
            normalized_text: "copy cleo to cleo".to_string(),
            confidence_reasons: vec!["test reason".to_string()],
        };
    }

    #[test]
    fn launch_guard_allows_only_current_session_running_pending_record()
    {
        let current = PendingRunRecord {
            journal: PathBuf::from("current.journal"),
            pid: Some(TEST_PENDING_PID),
            status: PendingRunStatus::Running,
            detail: String::new(),
        };
        let external = PendingRunRecord {
            journal: PathBuf::from("external.journal"),
            pid: Some(OTHER_TEST_PID),
            status: PendingRunStatus::Running,
            detail: String::new(),
        };
        let stale = PendingRunRecord {
            journal: PathBuf::from("stale.journal"),
            pid: Some(TEST_PENDING_PID),
            status: PendingRunStatus::Stale,
            detail: String::new(),
        };

        assert!(is_pending_run_match_for_current_session(
            &current,
            Some(TEST_PENDING_PID)
        ));
        assert!(!is_pending_run_match_for_current_session(
            &external,
            Some(TEST_PENDING_PID)
        ));
        assert!(!is_pending_run_match_for_current_session(
            &stale,
            Some(TEST_PENDING_PID)
        ));
        assert!(!is_pending_run_match_for_current_session(&current, None));
    }

    #[test]
    fn zero_pid_is_not_running()
    {
        assert!(!is_child_program_running(0));
    }

    #[test]
    fn install_root_validation_rejects_escapes_and_empties()
    {
        let valid = ModInstallRootJson {
            source: "files/CLEO".to_string(),
            target: "CLEO".to_string(),
            kind: "cleo".to_string(),
            enabled: true,
            optional: false,
        };
        assert!(validate_install_root(&valid).is_ok());

        let empty_source = ModInstallRootJson {
            source: "   ".to_string(),
            ..valid.clone()
        };
        assert!(validate_install_root(&empty_source).is_err());

        let escaping_source = ModInstallRootJson {
            source: "../evil".to_string(),
            ..valid.clone()
        };
        assert!(validate_install_root(&escaping_source).is_err());

        let escaping_target = ModInstallRootJson {
            target: "../../outside".to_string(),
            ..valid.clone()
        };
        assert!(validate_install_root(&escaping_target).is_err());
    }

    #[test]
    fn readme_accept_root_is_validated_like_manual_edits()
    {
        // A well-formed readme copy yields a root that passes validation.
        let good = readme_copy_install_root(ReadmeCopyInstallRoot { source: "files/CLEO", target: "CLEO", package_id: "gravity_fix" });
        assert!(validate_install_root(&good).is_ok());
        // A traversal source survives normalize_path but is rejected by the same
        // check the manual editor uses, so accept and edit stay consistent.
        let escaping = readme_copy_install_root(ReadmeCopyInstallRoot { source: "../../evil", target: "CLEO", package_id: "gravity_fix" });
        assert!(validate_install_root(&escaping).is_err());
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

    fn test_preferences() -> crate::ui::preferences::UiPreferences
    {
        return crate::ui::preferences::UiPreferences::default();
    }

    fn remove_directory_if_exists(path: &Path) -> Result<(), AppError>
    {
        if path.exists()
        {
            fs::remove_dir_all(path)?;
        }
        return Ok(());
    }
