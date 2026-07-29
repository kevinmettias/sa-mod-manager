    use super::{ArchiveBackend, ExtractBudget, PARALLEL_ZIP_TEST_DIRECTORY_MODULUS, PARALLEL_ZIP_TEST_FILE_COUNT, archive_backend, ensure_contained_entry, extract_archive_to_directory, extract_zip_to_directory, is_symlink_mode, list_archive_entries_native, missing_7zip_error_for_package, read_package_text_file};
    use crate::prelude::*;
    use zip::write::SimpleFileOptions;

    #[test]
    fn zip_archives_are_listed_read_and_extracted_without_external_tools()
    {
        let root = test_root("native_zip");
        let package = root.join("package.zip");
        write_zip_package(
            &package,
            &[
                ("README.txt", "Install with Mod Loader."),
                ("modloader/Test/file.txt", "payload"),
            ],
        );

        let entries = list_archive_entries_native(&package).expect("the test fixture is created before this assertion reads it").expect("the test fixture is created before this assertion reads it");
        let readme = read_package_text_file(&package, "README.txt", 1024) // literal: allow test fixture value is the specimen under judgment
            .expect("the test fixture is created before this assertion reads it")
            .expect("the test fixture is created before this assertion reads it");
        let target = root.join("extract");
        extract_archive_to_directory(&package, &target).expect("the test fixture is created before this assertion reads it");

        assert!(entries.iter().any(|entry| entry.path == "README.txt"));
        assert!(readme.contains("Mod Loader"));
        assert_eq!(
            fs::read_to_string(target.join("modloader").join("Test").join("file.txt")).expect("the test fixture is created before this assertion reads it"),
            "payload"
        );
        remove_dir_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn extraction_enforces_total_byte_budget()
    {
        let root = test_root("zip_byte_budget");
        let package = root.join("big.zip");
        let payload = "x".repeat(64); // literal: allow test fixture value is the specimen under judgment
        write_zip_package(&package, &[("data/file.bin", payload.as_str())]);

        let budget = ExtractBudget::with_limits(16, 100); // literal: allow test fixture value is the specimen under judgment
        let err = extract_zip_to_directory(&package, &root.join("out"), &budget)
            .unwrap_err()
            .to_string();

        assert!(err.contains("extraction limit"));
        remove_dir_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn extraction_enforces_entry_count_budget()
    {
        let root = test_root("zip_entry_budget");
        let package = root.join("many.zip");
        write_zip_package(&package, &[("a.txt", "a"), ("b.txt", "b")]);

        let budget = ExtractBudget::with_limits(1 << 20, 1); // literal: allow test fixture value is the specimen under judgment
        let err = extract_zip_to_directory(&package, &root.join("out"), &budget)
            .unwrap_err()
            .to_string();

        assert!(err.contains("extraction limit"));
        remove_dir_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn symlink_modes_are_detected_and_regular_modes_are_not()
    {
        // Extraction skips any entry whose unix mode marks it a symlink (S_IFLNK),
        // the bits real unix archivers set on link entries.
        assert!(is_symlink_mode(Some(0o120777))); // literal: allow test fixture value is the specimen under judgment
        assert!(!is_symlink_mode(Some(0o100644))); // regular file // literal: allow test fixture value is the specimen under judgment
        assert!(!is_symlink_mode(Some(0o040755))); // directory // literal: allow test fixture value is the specimen under judgment
        assert!(!is_symlink_mode(None)); // no unix mode recorded
    }

    #[test]
    fn seven_zip_entry_containment_rejects_escapes_and_allows_safe_paths()
    {
        let package = Path::new("mod.7z");
        // Safe relative paths pass.
        for safe in [
            "modloader/Test/file.txt",
            "CLEO/script.cs",
            "readme.txt",
            "a/b/c.dat",
        ]
        {
            assert!(ensure_contained_entry(safe, package).is_ok(), "{safe}");
        }
        // Traversal, absolute, and drive-qualified paths are refused.
        for unsafe_path in [
            "../escape.txt",
            "modloader/../../escape.txt",
            "/etc/passwd",
            r"C:\Windows\system32\evil.dll",
            r"..\..\outside.txt",
        ]
        {
            let err = ensure_contained_entry(unsafe_path, package)
                .unwrap_err()
                .to_string();
            assert!(err.contains("unsafe entry path"), "{unsafe_path}: {err}");
        }
    }

    #[test]
    fn nested_archives_are_recursively_extracted_and_removed()
    {
        let root = test_root("zip_nested");
        let mut inner_bytes = Vec::new();
        {
            let mut inner = zip::ZipWriter::new(io::Cursor::new(&mut inner_bytes));
            inner
                .start_file("modloader/inner.txt", SimpleFileOptions::default())
                .expect("the test fixture is created before this assertion reads it");
            inner.write_all(b"nested payload").expect("the test fixture is created before this assertion reads it");
            inner.finish().expect("the test fixture is created before this assertion reads it");
        }

        let package = root.join("outer.zip");
        {
            let file = fs::File::create(&package).expect("the test fixture is created before this assertion reads it");
            let mut outer = zip::ZipWriter::new(file);
            outer
                .start_file("outer.txt", SimpleFileOptions::default())
                .expect("the test fixture is created before this assertion reads it");
            outer.write_all(b"outer payload").expect("the test fixture is created before this assertion reads it");
            outer
                .start_file("inner.zip", SimpleFileOptions::default())
                .expect("the test fixture is created before this assertion reads it");
            outer.write_all(&inner_bytes).expect("the test fixture is created before this assertion reads it");
            outer.finish().expect("the test fixture is created before this assertion reads it");
        }

        let target = root.join("out");
        extract_archive_to_directory(&package, &target).expect("the test fixture is created before this assertion reads it");

        assert!(target.join("outer.txt").exists());
        assert!(target.join("modloader").join("inner.txt").exists());
        assert!(!target.join("inner.zip").exists());
        remove_dir_if_exists(&root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn wrap_archives_use_native_zip_backend_case_insensitively()
    {
        assert_eq!(
            archive_backend(Path::new("package.WRAP")),
            ArchiveBackend::NativeZip
        );
        assert_eq!(
            archive_backend(Path::new("package.7z")),
            ArchiveBackend::SevenZip
        );
    }

    #[test]
    fn missing_7zip_message_explains_native_and_external_backends()
    {
        let message = missing_7zip_error_for_package(Path::new("mod.rar")).to_string();

        assert!(message.contains(".zip and .wrap"));
        assert!(message.contains(".7z and .rar"));
        assert!(message.contains("SA_MOD_MANAGER_7Z"));
    }

    #[test]
    fn parallel_zip_extraction_extracts_every_file_correctly()
    {
        let root = test_root("zip_parallel");
        let package = root.join("many.zip");
        // Enough files across nested dirs to fan out over multiple workers.
        let entries: Vec<(String, String)> = (0..PARALLEL_ZIP_TEST_FILE_COUNT)
            .map(|idx| {
                let name = if idx % PARALLEL_ZIP_TEST_DIRECTORY_MODULUS == 0 {
                    // literal: allow test fixture value is the specimen under judgment
                    format!("data/file-{idx:02}.txt")
                } else {
                    format!("cleo/nested/file-{idx:02}.txt")
                };
                (name, format!("payload-{idx}"))
            })
            .collect();
        let entry_refs: Vec<(&str, &str)> = entries
            .iter()
            .map(|(name, content)| (name.as_str(), content.as_str()))
            .collect();
        write_zip_package(&package, &entry_refs);

        let target = root.join("out");
        extract_archive_to_directory(&package, &target).expect("the test fixture is created before this assertion reads it");

        for (name, content) in &entries
        {
            assert_eq!(fs::read_to_string(target.join(name)).expect("the test fixture is created before this assertion reads it"), *content);
        }
        remove_dir_if_exists(&root).expect("the test fixture is created before this assertion reads it");
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
        remove_dir_if_exists(&root).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        fs::create_dir_all(&root).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        return root;
    }

    fn remove_dir_if_exists(path: &Path) -> Result<(), AppError>
    {
        if path.exists()
        {
            fs::remove_dir_all(path)?;
        }
        return Ok(());
    }

