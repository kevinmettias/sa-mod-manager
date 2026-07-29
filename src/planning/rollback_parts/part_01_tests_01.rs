    use super::{file_hash, rollback_journal};
    use crate::prelude::*;

    #[test]
    fn rollback_restores_overwritten_file_from_backup()
    {
        let game_root = test_root("rollback_restore_overwrite");
        let dest = game_root.join("data").join("handling.cfg");
        let backup = game_root
            .join(".sa-mod-manager")
            .join("backups")
            .join("tx")
            .join("data")
            .join("handling.cfg");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "modded");
        write_file(&backup, "vanilla");
        write_journal(
            &journal,
            &[
                backup_line(&dest, &backup),
                copy_line(&game_root.join("source").join("handling.cfg"), &dest),
            ],
        );

        rollback_journal(&journal, &game_root).expect("the test fixture is created before this assertion reads it");

        assert_eq!(fs::read_to_string(&dest).expect("the test fixture is created before this assertion reads it"), "vanilla");
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn rollback_removes_nested_new_file_and_empty_parent_dirs()
    {
        let game_root = test_root("rollback_nested_new");
        let dest = game_root
            .join("modloader")
            .join("test_mod")
            .join("deep")
            .join("folder")
            .join("file.txt");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "payload");
        write_journal(
            &journal,
            &[
                new_line(&dest),
                copy_line(&game_root.join("source").join("file.txt"), &dest),
            ],
        );

        rollback_journal(&journal, &game_root).expect("the test fixture is created before this assertion reads it");

        assert!(!dest.exists());
        assert!(!game_root.join("modloader").join("test_mod").exists());
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn rollback_fails_loudly_when_backup_is_missing_without_mutating()
    {
        let game_root = test_root("rollback_missing_backup");
        let dest = game_root.join("data").join("foo.dat");
        let backup = game_root
            .join(".sa-mod-manager")
            .join("backups")
            .join("tx")
            .join("data")
            .join("foo.dat");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        fs::create_dir_all(backup.parent().expect("the test fixture is created before this assertion reads it")).expect("the test fixture is created before this assertion reads it");
        write_file(&dest, "modded");
        write_journal(
            &journal,
            &[
                format!(
                    "backup={}|{}|fnv64:0000000000000000",
                    escape_value(&dest.display().to_string()),
                    escape_value(&backup.display().to_string())
                ),
                copy_line(&game_root.join("source").join("foo.dat"), &dest),
            ],
        );

        let err = rollback_journal(&journal, &game_root)
            .unwrap_err()
            .to_string();

        assert!(err.contains("missing backup"));
        assert_eq!(fs::read_to_string(&dest).expect("the test fixture is created before this assertion reads it"), "modded");
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn rollback_detects_direct_overwrite_conflict_before_restore()
    {
        let game_root = test_root("rollback_conflict");
        let dest = game_root.join("data").join("foo.dat");
        let backup = game_root
            .join(".sa-mod-manager")
            .join("backups")
            .join("tx")
            .join("data")
            .join("foo.dat");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "modded");
        write_file(&backup, "vanilla");
        let copied_hash = file_hash(&dest).expect("the test fixture is created before this assertion reads it");
        fs::write(&dest, "changed by later mod").expect("the test fixture is created before this assertion reads it");
        write_journal(
            &journal,
            &[
                backup_line(&dest, &backup),
                format!(
                    "copy={}|{}|{}",
                    escape_value(
                        &game_root
                            .join("source")
                            .join("foo.dat")
                            .display()
                            .to_string()
                    ),
                    escape_value(&dest.display().to_string()),
                    copied_hash
                ),
            ],
        );

        let err = rollback_journal(&journal, &game_root)
            .unwrap_err()
            .to_string();

        assert!(err.contains("rollback conflict"));
        assert_eq!(fs::read_to_string(&dest).expect("the test fixture is created before this assertion reads it"), "changed by later mod");
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn rollback_removes_file_created_then_overwritten_in_same_transaction()
    {
        let game_root = test_root("rollback_new_then_overwrite");
        let dest = game_root
            .join("modloader")
            .join("test_mod")
            .join("file.txt");
        let backup = game_root
            .join(".sa-mod-manager")
            .join("backups")
            .join("tx")
            .join("modloader")
            .join("test_mod")
            .join("file.txt");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "first mod version");
        write_file(&backup, "first mod version");
        let first_hash = file_hash(&dest).expect("the test fixture is created before this assertion reads it");
        fs::write(&dest, "second mod version").expect("the test fixture is created before this assertion reads it");
        let second_hash = file_hash(&dest).expect("the test fixture is created before this assertion reads it");
        write_journal(
            &journal,
            &[
                new_line(&dest),
                format!(
                    "copy={}|{}|{}",
                    escape_value(
                        &game_root
                            .join("source-one")
                            .join("file.txt")
                            .display()
                            .to_string()
                    ),
                    escape_value(&dest.display().to_string()),
                    first_hash
                ),
                backup_line(&dest, &backup),
                format!(
                    "copy={}|{}|{}",
                    escape_value(
                        &game_root
                            .join("source-two")
                            .join("file.txt")
                            .display()
                            .to_string()
                    ),
                    escape_value(&dest.display().to_string()),
                    second_hash
                ),
            ],
        );

        rollback_journal(&journal, &game_root).expect("the test fixture is created before this assertion reads it");

        assert!(!dest.exists());
        assert!(!game_root.join("modloader").join("test_mod").exists());
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn malformed_journal_entry_aborts_rollback_loudly()
    {
        let game_root = test_root("rollback_malformed");
        let dest = game_root.join("data").join("foo.dat");
        let journal = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        write_file(&dest, "modded");
        // A truncated `backup=` line (only one field) must fail the whole rollback
        // rather than being silently skipped into a partial restore.
        write_journal(
            &journal,
            &[format!(
                "backup={}",
                escape_value(&dest.display().to_string())
            )],
        );

        let err = rollback_journal(&journal, &game_root)
            .unwrap_err()
            .to_string();

        assert!(err.contains("corrupt"), "{err}");
        assert!(err.contains("backup"), "{err}");
        // Nothing was mutated: the destructive restore never ran.
        assert_eq!(fs::read_to_string(&dest).expect("the test fixture is created before this assertion reads it"), "modded");
        remove_directory_if_exists(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    fn backup_line(dest: &Path, backup: &Path) -> String
    {
        return format!(
            "backup={}|{}|{}",
            escape_value(&dest.display().to_string()),
            escape_value(&backup.display().to_string()),
            file_hash(backup).expect("the test fixture is created before this assertion reads it")
        );
    }

    fn new_line(dest: &Path) -> String
    {
        return format!("new={}", escape_value(&dest.display().to_string()));
    }

    fn copy_line(source: &Path, dest: &Path) -> String
    {
        return format!(
            "copy={}|{}|{}",
            escape_value(&source.display().to_string()),
            escape_value(&dest.display().to_string()),
            file_hash(dest).expect("the test fixture is created before this assertion reads it")
        );
    }

    fn write_journal(path: &Path, lines: &[String])
    {
        let mut text = "version=1\nmode=ephemeral-run\n".to_string();
        for line in lines
        {
            text.push_str(line);
            text.push('\n');
        }
        write_file(path, &text);
    }

    fn write_file(path: &Path, text: &str)
    {
        if let Some(parent) = path.parent()
        {
            fs::create_dir_all(parent).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        }
        fs::write(path, text).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
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
        fs::create_dir_all(root.join(".sa-mod-manager").join("journals")).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
        fs::create_dir_all(root.join(".sa-mod-manager").join("backups")).expect("the test fixture is created before this assertion reads it"); // error-type: allow included only from cfg(test) harness code
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
