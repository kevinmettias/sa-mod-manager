    use super::{MIN_PLAN_NOTE_COUNT, REVIEW_CONFIDENCE_ASSERTION, is_streaming_nodes, has_readme_injectable_table};
    use crate::prelude::*;
    use zip::write::SimpleFileOptions;

    #[test]
    fn readme_with_table_rows_is_flagged_but_prose_is_not()
    {
        // A handling.cfg-style row has many numeric fields.
        let handling_row = "INFERNUS 1000.0 5000.0 2.0 0.0 0.3 -0.1 75 0.8 0.9 27.0 200.0";
        assert!(has_readme_injectable_table(handling_row));
        // Ordinary prose (and a couple of version numbers) is not flagged.
        let prose = "Install v1.2 into your game. Requires CLEO 4.3. Enjoy!";
        assert!(!has_readme_injectable_table(prose));
        // Comment lines are ignored.
        assert!(!has_readme_injectable_table("; 1 2 3 4 5 6 7 8 9 10"));
    }

    #[test]
    fn streaming_nodes_files_are_recognized()
    {
        assert!(is_streaming_nodes("modloader/x/nodes0.dat"));
        assert!(is_streaming_nodes("NODES63.DAT"));
        assert!(!is_streaming_nodes("nodes.dat"));
        assert!(!is_streaming_nodes("handling.cfg"));
    }

    #[test]
    fn readme_table_injection_becomes_a_package_risk()
    {
        let root = test_root("readme_injection_risk");
        let package = root.join("carpack.zip");
        write_zip_package(
            &package,
            &[
                (
                    "readme.txt",
                    "Paste this handling line:\nINFERNUS 1000.0 5000.0 2.0 0.0 0.3 -0.1 75 0.8 0.9 27.0 200.0 10.0",
                ),
                ("infernus.dff", "model"),
            ],
        );

        let report = analyze_package(&package, &root).expect("the test fixture is created before this assertion reads it");
        assert!(
            report
                .risks
                .iter()
                .any(|r| r.contains("std.data scans .txt")),
            "risks: {:?}",
            report.risks
        );
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn wrap_manifest_roots_are_parsed_and_preferred_over_heuristics()
    {
        let root = test_root("wrap_manifest");
        let package = root.join("explicit.wrap");
        write_zip_package(
            &package,
            &[
                (
                    "wrap.json",
                    r#"{
                        "install_roots": [
                            {
                                "source": "files/CLEO",
                                "target": "CLEO",
                                "kind": "cleo",
                                "notes": ["declared by manifest"]
                            }
                        ]
                    }"#,
                ),
                (
                    "README.txt",
                    "Copy the CLEO folder to your game root. Requires CLEO.",
                ),
                ("files/CLEO/gravityfix.cs", "script"),
            ],
        );

        let report = analyze_package(&package, &root).expect("the test fixture is created before this assertion reads it");
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = build_install_plan(&report, &options);

        assert_eq!(report.manifest_roots.len(), 1);
        assert_eq!(report.readme_documents.len(), 1);
        assert!(report.readme_documents[0].text.contains("Requires CLEO"));
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "files/CLEO");
        assert_eq!(plan.operations[0].target_root, root.join("CLEO"));
        assert_eq!(plan.operations[0].notes, vec!["declared by manifest"]);
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn wrap_without_manifest_is_rejected()
    {
        let root = test_root("wrap_missing_manifest");
        let package = root.join("missing.wrap");
        write_zip_package(&package, &[("CLEO/gravityfix.cs", "script")]);

        let err = analyze_package_error(&package, &root).to_string();

        assert!(err.contains("must contain wrap.json"));
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn wrap_manifest_validation_reports_bad_roots()
    {
        let root = test_root("wrap_bad_manifest");
        let package = root.join("bad.wrap");
        write_zip_package(
            &package,
            &[(
                "wrap.json",
                r#"{
                    "install_roots": [
                        {
                            "source": "../outside",
                            "target": "CLEO",
                            "kind": "cleo"
                        }
                    ]
                }"#,
            )],
        );

        let err = analyze_package_error(&package, &root).to_string();

        assert!(err.contains("install_roots[0].source"));
        assert!(err.contains("relative package path"));
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn wrap_manifest_validation_requires_existing_source()
    {
        let root = test_root("wrap_missing_source");
        let package = root.join("missing-source.wrap");
        write_zip_package(
            &package,
            &[(
                "wrap.json",
                r#"{
                    "install_roots": [
                        {
                            "source": "files/CLEO",
                            "target": "CLEO",
                            "kind": "cleo"
                        }
                    ]
                }"#,
            )],
        );

        let err = analyze_package_error(&package, &root).to_string();

        assert!(err.contains("source does not exist"));
        assert!(err.contains("files/CLEO"));
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn messy_readme_copy_instruction_drives_plan_when_confident()
    {
        let root = test_root("readme_instruction");
        let package = root.join("messy.zip");
        write_zip_package(
            &package,
            &[
                (
                    "InstallEN.txt",
                    "Manual install:\n- Transfer the contents of the folder CLEO into your GTA SA directory.",
                ),
                ("CLEO/gravityfix.cs", "script"),
            ],
        );

        let report = analyze_package(&package, &root).expect("the test fixture is created before this assertion reads it");
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        // Inject explicit thresholds so plan inclusion depends only on the code
        // under test, not on the developer's local config file.
        let plan = crate::planning::build::build_install_plan_with(
            &report,
            &options,
            crate::settings::ReadmeThresholds::default(),
        );

        assert_eq!(report.readme_instructions.len(), 1);
        assert_eq!(
            report.readme_instructions[0].source.as_deref(),
            Some("CLEO")
        );
        assert_eq!(
            report.readme_instructions[0].target.as_deref(),
            Some("CLEO")
        );
        assert!(report.readme_instructions[0].confidence >= REVIEW_CONFIDENCE_ASSERTION);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "CLEO");
        assert_eq!(plan.operations[0].target_root, root.join("CLEO"));
        assert!(plan.operations[0].notes[0].contains("readme evidence"));
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn vague_readme_instruction_does_not_override_heuristics()
    {
        let root = test_root("vague_readme_instruction");
        let package = root.join("vague.zip");
        write_zip_package(
            &package,
            &[
                ("README.txt", "Install the files as usual."),
                ("CLEO/gravityfix.cs", "script"),
            ],
        );

        let report = analyze_package(&package, &root).expect("the test fixture is created before this assertion reads it");
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = crate::planning::build::build_install_plan_with(
            &report,
            &options,
            crate::settings::ReadmeThresholds::default(),
        );

        assert_eq!(report.readme_instructions.len(), 1);
        assert!(report.readme_instructions[0].confidence < REVIEW_CONFIDENCE_ASSERTION);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "CLEO");
        assert!(
            !plan.operations[0]
                .notes
                .iter()
                .any(|note| note.contains("readme evidence"))
        );
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn readme_infers_missing_cleo_source_from_package_contents()
    {
        let root = test_root("readme_infer_cleo_source");
        let package = root.join("cleo-source.zip");
        write_zip_package(
            &package,
            &[
                ("README.txt", "Copy the files into the CLEO folder."),
                ("scripts/gravityfix.cs", "script"),
                ("scripts/gravityfix.fxt", "text"),
            ],
        );

        let report = analyze_package(&package, &root).expect("the test fixture is created before this assertion reads it");
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = build_install_plan(&report, &options);

        assert_eq!(report.readme_instructions.len(), 1);
        assert_eq!(
            report.readme_instructions[0].source.as_deref(),
            Some("scripts")
        );
        assert_eq!(
            report.readme_instructions[0].target.as_deref(),
            Some("CLEO")
        );
        assert!(report.readme_instructions[0].confidence >= REVIEW_CONFIDENCE_ASSERTION);
        assert!(
            report.readme_instructions[0]
                .confidence_reasons
                .iter()
                .any(|reason| reason.contains("package contents"))
        );
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "scripts");
        assert_eq!(plan.operations[0].target_root, root.join("CLEO"));
        assert!(plan.operations[0].notes.len() >= MIN_PLAN_NOTE_COUNT);
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn readme_normalizes_weird_formatting_and_fuzzy_root_targets()
    {
        let root = test_root("readme_weird_formatting");
        let package = root.join("root-asi.zip");
        write_zip_package(
            &package,
            &[
                (
                    "README.txt",
                    "INSTALLATION:\n* DRAG-and-DROP the ASI files -> the folder where gta_sa.exe is.",
                ),
                ("plugin/limit_adjuster.asi", "asi"),
                ("plugin/limit_adjuster.ini", "ini"),
            ],
        );

        let report = analyze_package(&package, &root).expect("the test fixture is created before this assertion reads it");
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = build_install_plan(&report, &options);

        assert_eq!(report.readme_instructions.len(), 1);
        let instruction = &report.readme_instructions[0];
        assert_eq!(instruction.source.as_deref(), Some("plugin"));
        assert_eq!(instruction.target.as_deref(), Some("."));
        assert!(instruction.normalized_text.contains("drag and drop"));
        assert!(instruction.normalized_text.contains("gta sa exe"));
        assert!(instruction.confidence >= REVIEW_CONFIDENCE_ASSERTION);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "plugin");
        assert_eq!(plan.operations[0].target_root, root);
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }
