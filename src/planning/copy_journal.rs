use crate::prelude::*;
use std::io::Read;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Reused per-file read buffer size for streaming copy/hash. Bounds memory to
/// this regardless of file size (game `.img` archives are multi-GB).
const HASH_BUFFER_BYTES: usize = 64 * 1024;
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
/// Cap on concurrent copy workers: file I/O saturates well before high thread
/// counts, so staying modest avoids disk thrashing.
const MAX_COPY_WORKERS: usize = 8;
#[cfg(test)]
const PARALLEL_COPY_TEST_FILE_COUNT: usize = 40;
#[cfg(test)]
const PARALLEL_COPY_TEST_NESTING_MODULUS: usize = 2;

/// A journal file shared across copy workers. Line writes and durability syncs
/// are serialized under the lock; the heavy byte-copying runs outside it.
type SharedJournal<'a> = Mutex<&'a mut fs::File>;

#[derive(Clone, Copy)]
struct CopyFileContext<'a, 'j> {
    source_abs: &'a Path,
    target_root: &'a Path,
    game_root: &'a Path,
    backup_root: &'a Path,
    journal: &'a SharedJournal<'j>,
}

struct CopyWorkerState<'a> {
    files: &'a [PathBuf],
    next: &'a AtomicUsize,
    first_error: &'a Mutex<Option<AppError>>,
}
pub(super) fn apply_copy_tree_with_journal(
    source_abs: &Path,
    target_root: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    let files = collect_files_recursive(source_abs)?;
    let game_root = context.game_root;
    let backup_root = context.backup_root;
    // Validate and create every distinct destination directory once, up front, so
    // the per-file hot loop below no longer canonicalizes the game root or
    // create_dir_all's a shared parent for each file. Validating here also fails
    // fast before any bytes are written if a destination would escape the root.
    prepare_destination_dirs(source_abs, target_root, &files, game_root)?;
    let journal: SharedJournal = Mutex::new(&mut *context.journal);
    let copy_context = CopyFileContext {
        source_abs,
        target_root,
        game_root,
        backup_root,
        journal: &journal,
    };

    let workers = worker_count(files.len());
    if workers <= 1 {
        for file in &files {
            apply_copy_file(file, copy_context)?;
        }
        return Ok(());
    }

    // Files within one source tree have distinct destinations, so copying them
    // concurrently cannot race, and rollback (reverse-order, per distinct path)
    // stays correct regardless of the order their journal lines interleave.
    let next = AtomicUsize::new(0);
    let first_error: Mutex<Option<AppError>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                copy_worker(
                    CopyWorkerState {
                        files: &files,
                        next: &next,
                        first_error: &first_error,
                    },
                    copy_context,
                );
            });
        }
    });
    match first_error
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Write manager-generated `content` to `dest` as a fully journaled operation,
/// so an ephemeral run's rollback restores the prior file (or removes it when it
/// was new) exactly as it does for copied mod files. Used for `modloader.ini`,
/// whose contents the manager synthesizes rather than copies from a source tree.
///
/// The existing file is backed up (or recorded `new`) and the journal is fsynced
/// *before* the destructive write, matching [`apply_copy_file`]'s crash-safety.
pub(super) fn apply_generated_file_with_journal(
    content: &str,
    dest: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    let game_root = context.game_root;
    let backup_root = context.backup_root;
    let journal: SharedJournal = Mutex::new(&mut *context.journal);
    journal_destination_state(dest, game_root, backup_root, &journal)?;
    sync_shared_journal(&journal)?;
    let written_hash = write_and_hash(content.as_bytes(), dest)?;
    // The synthetic "source" field is only journal metadata (rollback keys on the
    // destination); naming the dest keeps the line self-describing.
    write_copy_line(&journal, dest, dest, &written_hash)?;
    Ok(())
}

/// Atomically write `content` to `dest` (temp + rename) and return its FNV hash,
/// mirroring [`copy_and_hash`] but for in-memory bytes rather than a source file.
fn write_and_hash(content: &[u8], dest: &Path) -> Result<String, AppError> {
    let temp = temp_sibling(dest);
    let write_result = (|| -> Result<u64, AppError> {
        let mut output =
            fs::File::create(&temp).with_context(|| format!("create {}", temp.display()))?;
        output
            .write_all(content)
            .with_context(|| format!("write {}", temp.display()))?;
        output
            .sync_all()
            .with_context(|| format!("sync {}", temp.display()))?;
        let mut hash = FNV_OFFSET;
        for &byte in content {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        Ok(hash)
    })();
    let hash = match write_result {
        Ok(hash) => hash,
        Err(err) => {
            let _ = fs::remove_file(&temp);
            return Err(err);
        }
    };
    if let Err(err) = fs::rename(&temp, dest) {
        let _ = fs::remove_file(&temp);
        return Err(
            AppError::from(err).context(format!("replace {} with generated file", dest.display()))
        );
    }
    Ok(format!("fnv64:{hash:016x}"))
}

fn worker_count(file_count: usize) -> usize {
    if file_count <= 1 {
        return 1;
    }
    let cpus = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    cpus.clamp(1, MAX_COPY_WORKERS).min(file_count)
}

fn copy_worker(state: CopyWorkerState<'_>, context: CopyFileContext<'_, '_>) {
    loop {
        if state
            .first_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
        {
            break;
        }
        let idx = state.next.fetch_add(1, Ordering::Relaxed);
        let Some(file) = state.files.get(idx) else {
            break;
        };
        if let Err(err) = apply_copy_file(file, context) {
            let mut slot = state
                .first_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if slot.is_none() {
                *slot = Some(err);
            }
            break;
        }
    }
}

fn apply_copy_file(file: &Path, context: CopyFileContext<'_, '_>) -> Result<(), AppError> {
    let rel = file.strip_prefix(context.source_abs).unwrap_or(file);
    // The destination directory was created and validated by
    // `prepare_destination_dirs` before this loop, so the containment check and
    // directory creation are intentionally not repeated per file here.
    let dest = context.target_root.join(rel);
    journal_destination_state(
        &dest,
        context.game_root,
        context.backup_root,
        context.journal,
    )?;
    // Force the backup/new record to durable storage *before* the destructive
    // copy below, so a crash can never leave an overwritten game file with no
    // recoverable journal entry.
    sync_shared_journal(context.journal)?;
    // Copy source -> dest while hashing in one pass. The copied content equals
    // the source, so its hash is the source's — avoiding a re-read of dest.
    let copied_hash = copy_and_hash(file, &dest)?;
    write_copy_line(context.journal, file, &dest, &copied_hash)?;
    Ok(())
}

/// Create and validate each distinct destination directory once. Canonicalizing
/// the game root and each parent here — rather than per file — keeps the copy
/// loop off the filesystem-heavy `canonicalize`/`create_dir_all` path while
/// preserving the same containment guarantee: no destination escapes the root.
fn prepare_destination_dirs(
    source_abs: &Path,
    target_root: &Path,
    files: &[PathBuf],
    game_root: &Path,
) -> Result<(), AppError> {
    let canonical_game_root = game_root.canonicalize()?;
    let mut prepared = std::collections::BTreeSet::new();
    for file in files {
        let rel = file.strip_prefix(source_abs).unwrap_or(file);
        let dest = target_root.join(rel);
        let Some(parent) = dest.parent() else {
            continue;
        };
        if !prepared.insert(parent.to_path_buf()) {
            continue;
        }
        // Validate against the nearest already-existing ancestor *before* creating
        // anything, so a rejected (escaping) target leaves no stray directory
        // behind outside the game root. The to-be-created segments are plain
        // directories (never symlinks), so resolving the existing anchor is
        // sufficient to catch an escape.
        let anchor = nearest_existing_ancestor(parent);
        let canonical_anchor = anchor
            .canonicalize()
            .with_context(|| format!("resolve {}", anchor.display()))?;
        if !canonical_anchor.starts_with(&canonical_game_root) {
            return Err(AppError::Usage(format!(
                "destination escapes game root: {}",
                dest.display()
            )));
        }
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    Ok(())
}

/// The nearest ancestor of `path` (inclusive) that currently exists, walking up
/// until one is found; the filesystem root always exists, so this terminates.
fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    while !current.exists() && current.pop() {}
    current
}

fn journal_destination_state(
    dest: &Path,
    game_root: &Path,
    backup_root: &Path,
    journal: &SharedJournal,
) -> Result<(), AppError> {
    if dest.exists() {
        let relative_backup = backup_relative_for_destination(game_root, dest)?;
        let backup = backup_root.join(relative_backup);
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create backup directory {}", parent.display()))?;
        }
        // Back up the original by copying dest -> backup while hashing in one
        // pass, avoiding a separate re-read of the backup just to hash it.
        // `copy_and_hash` fsyncs the staged bytes before renaming them into place,
        // so the backup's contents are already durable here — no reopen+fsync of
        // the backup is needed before the destination is later overwritten.
        let backup_hash = copy_and_hash(dest, &backup)?;
        write_backup_line(journal, dest, &backup, &backup_hash)?;
    } else {
        write_new_line(journal, dest)?;
    }
    Ok(())
}

/// Stream `source` to `dest` through one reused buffer, returning the FNV hash
/// of the bytes written. A single read of `source` replaces the previous
/// copy-then-reread-to-hash, roughly halving disk reads per file.
///
/// The bytes are staged into a sibling temp file and then atomically renamed
/// over `dest`, so an observer (or a crash) never sees a half-written mix: the
/// visible destination is only ever the complete previous file or the complete
/// new one. Destinations within a copy tree are distinct, so the temp name
/// derived from `dest` cannot collide across concurrent workers.
fn copy_and_hash(source: &Path, dest: &Path) -> Result<String, AppError> {
    let temp = temp_sibling(dest);
    let hash = match stream_copy_to_temp(source, &temp) {
        Ok(hash) => hash,
        Err(err) => {
            let _ = fs::remove_file(&temp);
            return Err(err);
        }
    };
    // `fs::rename` replaces an existing destination on both Windows and Unix,
    // making the swap atomic. The staged bytes were fsynced before this point,
    // so the rename can never expose a renamed-but-empty destination.
    if let Err(err) = fs::rename(&temp, dest) {
        let _ = fs::remove_file(&temp);
        return Err(
            AppError::from(err).context(format!("replace {} with staged copy", dest.display()))
        );
    }
    Ok(format!("fnv64:{hash:016x}"))
}

fn stream_copy_to_temp(source: &Path, temp: &Path) -> Result<u64, AppError> {
    let mut input = fs::File::open(source).with_context(|| format!("open {}", source.display()))?;
    let mut output =
        fs::File::create(temp).with_context(|| format!("create {}", temp.display()))?;
    let mut buffer = [0u8; HASH_BUFFER_BYTES];
    let mut hash = FNV_OFFSET;
    loop {
        let read = input
            .read(&mut buffer)
            .with_context(|| format!("read {}", source.display()))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .with_context(|| format!("write {}", temp.display()))?;
        for &byte in &buffer[..read] {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    // Force the staged bytes durable before the caller renames them into place.
    output
        .sync_all()
        .with_context(|| format!("sync {}", temp.display()))?;
    Ok(hash)
}

/// A staging path alongside `dest` (same directory, hence same filesystem, so
/// the follow-up rename stays atomic).
fn temp_sibling(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(".sa-tmp");
    PathBuf::from(name)
}

fn write_backup_line(
    journal: &SharedJournal,
    dest: &Path,
    backup: &Path,
    backup_hash: &str,
) -> Result<(), AppError> {
    let mut guard = journal
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    writeln!(
        &mut **guard,
        "backup={}|{}|{}",
        escape_value(&dest.display().to_string()),
        escape_value(&backup.display().to_string()),
        backup_hash
    )
    .with_context(|| format!("write backup journal entry for {}", dest.display()))?;
    Ok(())
}

fn write_new_line(journal: &SharedJournal, dest: &Path) -> Result<(), AppError> {
    let mut guard = journal
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    writeln!(
        &mut **guard,
        "new={}",
        escape_value(&dest.display().to_string())
    )
    .with_context(|| format!("write new-file journal entry for {}", dest.display()))?;
    Ok(())
}

fn write_copy_line(
    journal: &SharedJournal,
    file: &Path,
    dest: &Path,
    copied_hash: &str,
) -> Result<(), AppError> {
    let mut guard = journal
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    writeln!(
        &mut **guard,
        "copy={}|{}|{}",
        escape_value(&file.display().to_string()),
        escape_value(&dest.display().to_string()),
        copied_hash
    )
    .with_context(|| format!("write copy journal entry for {}", dest.display()))?;
    Ok(())
}

/// Durably flush the shared journal (under its lock).
fn sync_shared_journal(journal: &SharedJournal) -> Result<(), AppError> {
    let mut guard = journal
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.flush().with_context(|| "flush journal")?;
    guard.sync_all().with_context(|| "sync journal to disk")?;
    Ok(())
}

/// Flush the journal file's buffered contents through to durable storage.
///
/// On Windows this maps to `FlushFileBuffers`; the journal is opened for
/// writing so the call is permitted.
pub(super) fn sync_journal(journal: &mut fs::File) -> Result<(), AppError> {
    journal.flush().with_context(|| "flush journal")?;
    journal.sync_all().with_context(|| "sync journal to disk")?;
    Ok(())
}

pub(super) fn file_hash(path: &Path) -> Result<String, AppError> {
    // Stream the file through one reused fixed buffer instead of reading it all
    // into a Vec. Folding FNV chunk-by-chunk yields the same hash as folding the
    // whole file, so existing journal hashes still verify.
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buffer = [0u8; HASH_BUFFER_BYTES];
    let mut hash = FNV_OFFSET;
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("read {}", path.display()))?;
        if read == 0 {
            break;
        }
        for &byte in &buffer[..read] {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    Ok(format!("fnv64:{hash:016x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_hash_streams_large_files_without_changing_the_result() {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-hash-{}-{}",
            std::process::id(),
            unix_now()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("big.img");
        // Larger than HASH_BUFFER_BYTES so hashing spans several read chunks.
        let data: Vec<u8> = (0..(HASH_BUFFER_BYTES * 3 + 123)) // literal: allow test fixture value is the specimen under judgment
            .map(|i| (i % 251) as u8) // literal: allow test fixture value is the specimen under judgment
            .collect();
        fs::write(&path, &data).unwrap();

        assert_eq!(file_hash(&path).unwrap(), reference_fnv64(&data));

        // An empty file hashes to the FNV offset basis, unchanged by streaming.
        let empty = root.join("empty.bin");
        fs::write(&empty, b"").unwrap();
        assert_eq!(file_hash(&empty).unwrap(), reference_fnv64(b""));

        fs::remove_dir_all(&root).unwrap();
    }

    fn reference_fnv64(bytes: &[u8]) -> String {
        let mut hash = 0xcbf29ce484222325u64; // literal: allow test fixture value is the specimen under judgment
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3); // literal: allow test fixture value is the specimen under judgment
        }
        format!("fnv64:{hash:016x}")
    }

    #[test]
    fn copy_tree_copies_every_file_and_journals_it() {
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
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir_all(journal_path.parent().unwrap()).unwrap();
        fs::create_dir_all(&backup_root).unwrap();
        for idx in 0..PARALLEL_COPY_TEST_FILE_COUNT {
            // literal: allow test fixture value is the specimen under judgment
            let rel = if idx % PARALLEL_COPY_TEST_NESTING_MODULUS == 0 {
                // literal: allow test fixture value is the specimen under judgment
                format!("file-{idx:02}.txt")
            } else {
                format!("nested/file-{idx:02}.txt")
            };
            fs::write(source.join(rel), format!("payload-{idx}")).unwrap();
        }

        let mut journal = fs::File::create(&journal_path).unwrap();
        let mut context = CopyJournalContext {
            game_root: &game_root,
            backup_root: &backup_root,
            journal: &mut journal,
        };
        apply_copy_tree_with_journal(&source, &target, &mut context).unwrap();
        drop(journal);

        for idx in 0..PARALLEL_COPY_TEST_FILE_COUNT {
            // literal: allow test fixture value is the specimen under judgment
            let rel = if idx % PARALLEL_COPY_TEST_NESTING_MODULUS == 0 {
                // literal: allow test fixture value is the specimen under judgment
                format!("file-{idx:02}.txt")
            } else {
                format!("nested/file-{idx:02}.txt")
            };
            assert_eq!(
                fs::read_to_string(target.join(rel)).unwrap(),
                format!("payload-{idx}")
            );
        }
        let journal_text = fs::read_to_string(&journal_path).unwrap();
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
        fs::remove_dir_all(&game_root).unwrap();
    }

    #[test]
    fn prepare_destination_dirs_creates_valid_dirs_and_rejects_escapes() {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-prep-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let game_root = root.join("game");
        let source = root.join("source");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir_all(&game_root).unwrap();
        fs::write(source.join("a.txt"), "a").unwrap();
        fs::write(source.join("nested/b.txt"), "b").unwrap();
        let files = collect_files_recursive(&source).unwrap();

        // A target under the game root is created and validated.
        let target = game_root.join("modloader");
        prepare_destination_dirs(&source, &target, &files, &game_root).unwrap();
        assert!(target.join("nested").is_dir());

        // A target outside the game root is rejected before anything is created,
        // leaving no stray directory behind.
        let escaping = root.join("outside");
        let err = prepare_destination_dirs(&source, &escaping, &files, &game_root)
            .unwrap_err()
            .to_string();
        assert!(err.contains("escapes game root"), "{err}");
        assert!(!escaping.exists(), "rejected target must not be created");

        fs::remove_dir_all(&root).unwrap();
    }
}
