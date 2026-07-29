#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn acquire_writes_lock_and_release_removes_it()
    {
        let game_root = test_root("lock_roundtrip");
        let journal = journal_path(&game_root, "tx-1");

        acquire_install_lock(&game_root, "tx-1", &journal)
            .expect("the test fixture is created before this assertion reads it");
        let lock = read_install_lock(&game_root)
            .expect("the test fixture is created before this assertion reads it")
            .expect("the test fixture is created before this assertion reads it");
        assert_eq!(lock.txid, "tx-1");
        assert_eq!(lock.journal, journal);
        assert_eq!(lock.pid, Some(std::process::id()));

        release_install_lock(&game_root)
            .expect("the test fixture is created before this assertion reads it");
        assert!(
            read_install_lock(&game_root)
                .expect("the test fixture is created before this assertion reads it")
                .is_none()
        );
        remove_dir_if_exists(&game_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn acquire_blocks_second_install_while_owner_is_running()
    {
        let game_root = test_root("lock_blocks_running");
        // A lock owned by this very process is, by definition, still running.
        let journal_path_1 = journal_path(&game_root, "tx-live");
        write_install_lock(&game_root, "tx-live", &journal_path_1, std::process::id())
            .expect("the test fixture is created before this assertion reads it");

        let journal_path_2 = journal_path(&game_root, "tx-new");
        let err = acquire_install_lock(&game_root, "tx-new", &journal_path_2)
            .unwrap_err()
            .to_string();

        assert!(err.contains("already in progress"));
        remove_dir_if_exists(&game_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn acquire_reports_interrupted_install_when_owner_is_gone()
    {
        let game_root = test_root("lock_interrupted");
        let journal_path_3 = journal_path(&game_root, "tx-dead");
        write_install_lock(&game_root, "tx-dead", &journal_path_3, u32::MAX)
            .expect("the test fixture is created before this assertion reads it");

        let journal_path_4 = journal_path(&game_root, "tx-new");
        let err = acquire_install_lock(&game_root, "tx-new", &journal_path_4)
            .unwrap_err()
            .to_string();

        assert!(err.contains("interrupted"));
        assert!(err.contains("recover"));
        remove_dir_if_exists(&game_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn interrupted_install_only_reports_dead_owners()
    {
        let game_root = test_root("lock_detect");
        assert!(
            interrupted_install(&game_root)
                .expect("the test fixture is created before this assertion reads it")
                .is_none()
        );

        let journal_path_5 = journal_path(&game_root, "tx-live");

        write_install_lock(&game_root, "tx-live", &journal_path_5, std::process::id())
            .expect("the test fixture is created before this assertion reads it");
        assert!(
            interrupted_install(&game_root)
                .expect("the test fixture is created before this assertion reads it")
                .is_none()
        );

        let journal_path_6 = journal_path(&game_root, "tx-dead");

        write_install_lock(&game_root, "tx-dead", &journal_path_6, u32::MAX)
            .expect("the test fixture is created before this assertion reads it");
        let detected = interrupted_install(&game_root)
            .expect("the test fixture is created before this assertion reads it")
            .expect("the test fixture is created before this assertion reads it");
        assert_eq!(detected.txid, "tx-dead");
        remove_dir_if_exists(&game_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    // On Windows the lock also fingerprints the owner's process creation time,
    // so a live PID that has been recycled by an unrelated process is correctly
    // treated as a dead owner instead of wedging recovery.
    #[cfg(windows)]
    #[test]
    fn acquire_reports_interrupted_when_pid_is_reused_by_another_process()
    {
        let game_root = test_root("lock_pid_reuse");
        let journal = journal_path(&game_root, "tx-reused");
        let path = install_lock_path(&game_root);
        ensure_lock_parent(&path)
            .expect("the test fixture is created before this assertion reads it");
        // Owned by *this* live PID, but stamped with a creation-time fingerprint
        // that cannot be ours (1 == 100ns after 1601) -> the PID was recycled.
        fs::write(
            &path,
            format!(
                "version=1\ntxid=tx-reused\njournal={}\npid={}\npid_start=1\ncreated_unix=0\n",
                escape_value(&journal.display().to_string()),
                std::process::id(),
            ),
        )
        .expect("the test fixture is created before this assertion reads it");
        assert_ne!(crate::game_launch::current_process_start_ticks(), Some(1));

        let detected = interrupted_install(&game_root)
            .expect("the test fixture is created before this assertion reads it");
        assert_eq!(
            detected
                .expect("the test fixture is created before this assertion reads it")
                .txid,
            "tx-reused"
        );

        let journal_path_7 = journal_path(&game_root, "tx-new");
        let err = acquire_install_lock(&game_root, "tx-new", &journal_path_7)
            .unwrap_err()
            .to_string();
        assert!(err.contains("interrupted"));
        remove_dir_if_exists(&game_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn recover_rolls_back_interrupted_install_and_clears_lock()
    {
        let game_root = test_root("lock_recover");
        let dest = game_root.join("CLEO").join("newmod.cs");
        let journal = journal_path(&game_root, "tx-recover");
        write_file(&dest, "materialized");
        write_file(
            &journal,
            &format!(
                "version=1\nnew={}\n",
                escape_value(&dest.display().to_string())
            ),
        );
        write_install_lock(&game_root, "tx-recover", &journal, u32::MAX)
            .expect("the test fixture is created before this assertion reads it");

        let recovered = recover_interrupted_install(&game_root)
            .expect("the test fixture is created before this assertion reads it");

        assert_eq!(recovered.as_deref(), Some("tx-recover"));
        assert!(!dest.exists());
        assert!(
            read_install_lock(&game_root)
                .expect("the test fixture is created before this assertion reads it")
                .is_none()
        );
        remove_dir_if_exists(&game_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn recover_refuses_while_install_is_running()
    {
        let game_root = test_root("lock_recover_running");
        let journal = journal_path(&game_root, "tx-live");
        write_file(&journal, "version=1\n");
        write_install_lock(&game_root, "tx-live", &journal, std::process::id())
            .expect("the test fixture is created before this assertion reads it");

        let err = recover_interrupted_install(&game_root)
            .unwrap_err()
            .to_string();

        assert!(err.contains("in progress"));
        assert!(
            read_install_lock(&game_root)
                .expect("the test fixture is created before this assertion reads it")
                .is_some()
        );
        remove_dir_if_exists(&game_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn corrupt_pid_in_lock_is_reported_not_silently_ignored()
    {
        let game_root = test_root("lock_corrupt_pid");
        let path = install_lock_path(&game_root);
        ensure_lock_parent(&path)
            .expect("the test fixture is created before this assertion reads it");
        // A present-but-garbage pid must surface as corruption rather than
        // collapsing to "no pid" (which would read as a recoverable dead lock).
        fs::write(&path, "version=1\ntxid=tx\njournal=j\npid=not-a-number\n")
            .expect("the test fixture is created before this assertion reads it");

        let err = match read_install_lock(&game_root) {
            Ok(_) => panic!("expected a corruption error for a non-numeric pid"),
            Err(err) => err.to_string(),
        };
        assert!(err.contains("corrupt"), "{err}");
        assert!(err.contains("pid"), "{err}");
        remove_dir_if_exists(&game_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn recover_returns_none_without_a_lock()
    {
        let game_root = test_root("lock_recover_none");
        assert!(
            recover_interrupted_install(&game_root)
                .expect("the test fixture is created before this assertion reads it")
                .is_none()
        );
        remove_dir_if_exists(&game_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    fn journal_path(game_root: &Path, txid: &str) -> PathBuf
    {
        return state_directory(game_root)
            .join("journals")
            .join(format!("{txid}.journal"));
    }

    fn write_file(path: &Path, text: &str)
    {
        if let Some(parent) = path.parent()
        {
            fs::create_dir_all(parent)
                .expect("the test fixture is created before this assertion reads it");
        }
        fs::write(path, text).expect("the test fixture is created before this assertion reads it");
    }

    fn test_root(name: &str) -> PathBuf
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        remove_dir_if_exists(&root)
            .expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(state_directory(&root).join("journals"))
            .expect("the test fixture is created before this assertion reads it");
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
}
