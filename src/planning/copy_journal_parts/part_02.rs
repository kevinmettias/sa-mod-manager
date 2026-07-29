
/// Durably flush the shared journal (under its lock).
fn sync_shared_journal(journal: &SharedJournal) -> Result<(), AppError>
{
    let mut guard = journal
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.flush().with_context(|| "flush journal")?;
    guard.sync_all().with_context(|| "sync journal to disk")?;
    return Ok(());
}

/// Flush the journal file's buffered contents through to durable storage.
///
/// On Windows this maps to `FlushFileBuffers`; the journal is opened for
/// writing so the call is permitted.
pub(super) fn sync_journal(journal: &mut fs::File) -> Result<(), AppError>
{
    journal.flush().with_context(|| "flush journal")?;
    journal.sync_all().with_context(|| "sync journal to disk")?;
    return Ok(());
}

pub(super) fn file_hash(path: &Path) -> Result<String, AppError>
{
    // Stream the file through one reused fixed buffer instead of reading it all
    // into a Vec. Folding FNV chunk-by-chunk yields the same hash as folding the
    // whole file, so existing journal hashes still verify.
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buffer = [0u8; HASH_BUFFER_BYTES];
    let mut hash = FNV_OFFSET;
    // unbounded-loop: allow: streaming stops when File::read reports EOF with a zero-byte read
    loop
    {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("read {}", path.display()))?;
        if read == 0
        {
            break;
        }
        for &byte in &buffer[..read]
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    return Ok(format!("fnv64:{hash:016x}"));
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn file_hash_streams_large_files_without_changing_the_result()
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-hash-{}-{}",
            std::process::id(),
            unix_now()
        ));
        fs::create_dir_all(&root).expect("the test fixture is created before this assertion reads it");
        let path = root.join("big.img");
        // Larger than HASH_BUFFER_BYTES so hashing spans several read chunks.
        let data: Vec<u8> = (0..(HASH_BUFFER_BYTES * 3 + 123)) // literal: allow test fixture value is the specimen under judgment
            .map(|i| (i % 251) as u8) // literal: allow test fixture value is the specimen under judgment
            .collect();
        fs::write(&path, &data).expect("the test fixture is created before this assertion reads it");

        assert_eq!(file_hash(&path).expect("the test fixture is created before this assertion reads it"), reference_fnv64(&data));

        // An empty file hashes to the FNV offset basis, unchanged by streaming.
        let empty = root.join("empty.bin");
        fs::write(&empty, b"").expect("the test fixture is created before this assertion reads it");
        assert_eq!(file_hash(&empty).expect("the test fixture is created before this assertion reads it"), reference_fnv64(b""));

        fs::remove_dir_all(&root).expect("the test fixture is created before this assertion reads it");
    }

    fn reference_fnv64(bytes: &[u8]) -> String
    {
        let mut hash = 0xcbf29ce484222325u64; // literal: allow test fixture value is the specimen under judgment
        for &byte in bytes
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3); // literal: allow test fixture value is the specimen under judgment
        }
        return format!("fnv64:{hash:016x}");
    }

    #[test]
    fn copy_tree_copies_every_file_and_journals_it()
    {
        let game_root = env::temp_dir().join(format!(
            "sa-mod-manager-copytree-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let source = game_root.join("source");
        let target = game_root.join("modloader").join("dest");
        let backup_root = game_root.join(".sa-mod-manager").join("backups").join("tx");
        let journal_path = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        // Enough files to fan out across multiple copy workers.
        fs::create_dir_all(source.join("nested")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(journal_path.parent().expect("the test fixture is created before this assertion reads it")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(&backup_root).expect("the test fixture is created before this assertion reads it");
        for idx in 0..PARALLEL_COPY_TEST_FILE_COUNT
        {
            // literal: allow test fixture value is the specimen under judgment
            let rel = if idx % PARALLEL_COPY_TEST_NESTING_MODULUS == 0 {
                // literal: allow test fixture value is the specimen under judgment
                format!("file-{idx:02}.txt")
            } else {
                format!("nested/file-{idx:02}.txt")
            };
            fs::write(source.join(rel), format!("payload-{idx}")).expect("the test fixture is created before this assertion reads it");
        }

        let mut journal = fs::File::create(&journal_path).expect("the test fixture is created before this assertion reads it");
        let mut context = CopyJournalContext {
            game_root: &game_root,
            backup_root: &backup_root,
            journal: &mut journal,
        };
        apply_copy_tree_with_journal(&source, &target, &mut context).expect("the test fixture is created before this assertion reads it");
        drop(journal);

        for idx in 0..PARALLEL_COPY_TEST_FILE_COUNT
        {
            // literal: allow test fixture value is the specimen under judgment
            let rel = if idx % PARALLEL_COPY_TEST_NESTING_MODULUS == 0 {
                // literal: allow test fixture value is the specimen under judgment
                format!("file-{idx:02}.txt")
            } else {
                format!("nested/file-{idx:02}.txt")
            };
            assert_eq!(
                fs::read_to_string(target.join(rel)).expect("the test fixture is created before this assertion reads it"),
                format!("payload-{idx}")
            );
        }
        let journal_text = fs::read_to_string(&journal_path).expect("the test fixture is created before this assertion reads it");
        assert_eq!(
            journal_text
                .lines()
                .filter(|l| l.starts_with("new="))
                .count(),
            40 // literal: allow test fixture value is the specimen under judgment
        );
        assert_eq!(
            journal_text
                .lines()
                .filter(|l| l.starts_with("copy="))
                .count(),
            40 // literal: allow test fixture value is the specimen under judgment
        );
        fs::remove_dir_all(&game_root).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn prepare_destination_dirs_creates_valid_dirs_and_rejects_escapes()
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-prep-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let game_root = root.join("game");
        let source = root.join("source");
        fs::create_dir_all(source.join("nested")).expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(&game_root).expect("the test fixture is created before this assertion reads it");
        fs::write(source.join("a.txt"), "a").expect("the test fixture is created before this assertion reads it");
        fs::write(source.join("nested/b.txt"), "b").expect("the test fixture is created before this assertion reads it");
        let files = collect_files_recursive(&source).expect("the test fixture is created before this assertion reads it");

        // A target under the game root is created and validated.
        let target = game_root.join("modloader");
        prepare_destination_dirs(&source, &target, &files, &game_root).expect("the test fixture is created before this assertion reads it");
        assert!(target.join("nested").is_dir());

        // A target outside the game root is rejected before anything is created,
        // leaving no stray directory behind.
        let escaping = root.join("outside");
        let err = prepare_destination_dirs(&source, &escaping, &files, &game_root)
            .unwrap_err()
            .to_string();
        assert!(err.contains("escapes game root"), "{err}");
        assert!(!escaping.exists(), "rejected target must not be created");

        fs::remove_dir_all(&root).expect("the test fixture is created before this assertion reads it");
    }
}
