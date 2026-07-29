
    #[test]
    fn readme_fuses_multiline_install_paragraph_context()
    {
        let root = test_root("readme_multiline_context");
        let package = root.join("multiline.zip");
        write_zip_package(
            &package,
            &[
                (
                    "README.txt",
                    "Installation:\nCopy the files.\nDestination: CLEO folder.",
                ),
                ("scripts/gravityfix.cs", "script"),
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
        let copy = report
            .readme_instructions
            .iter()
            .find(|instruction| matches!(instruction.action, ReadmeAction::Copy))
            .expect("the test fixture is created before this assertion reads it");
        assert_eq!(copy.source.as_deref(), Some("scripts"));
        assert_eq!(copy.target.as_deref(), Some("CLEO"));
        assert!(copy.normalized_text.contains("destination cleo folder"));
        assert!(copy.confidence >= REVIEW_CONFIDENCE_ASSERTION);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "scripts");
        assert_eq!(plan.operations[0].target_root, root.join("CLEO"));
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn readme_maps_loose_model_files_to_modloader_gta3_img()
    {
        let root = test_root("readme_loose_models");
        let package = root.join("vehicle.zip");
        write_zip_package(
            &package,
            &[
                ("README.txt", "Copy the files into gta3.img."),
                ("models/infernus.dff", "model"),
                ("models/infernus.txd", "texture"),
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
        assert_eq!(instruction.source.as_deref(), Some("models"));
        assert_eq!(
            instruction.target.as_deref(),
            Some("modloader/vehicle/gta3.img")
        );
        assert!(instruction.confidence >= REVIEW_CONFIDENCE_ASSERTION);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "models");
        assert_eq!(
            plan.operations[0].target_root,
            root.join("modloader").join("vehicle").join("gta3.img")
        );
        remove_directory_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    fn analyze_package_error(package: &Path, root: &Path) -> AppError
    {
        return analyze_package(package, root)
            .map(|_| ())
            .expect_err("test package should fail analysis");
    }

    fn write_zip_package(path: &Path, entries: &[(&str, &str)])
    {
        let file = fs::File::create(path).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, text) in entries
        {
            zip.start_file(*name, options).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
            zip.write_all(text.as_bytes()).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        }
        zip.finish().expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
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
