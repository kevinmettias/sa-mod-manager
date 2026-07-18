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

/// A journal file shared across copy workers. Line writes and durability syncs
/// are serialized under the lock; the heavy byte-copying runs outside it.
type SharedJournal<'a> = Mutex<&'a mut fs::File>;

pub(super) fn apply_copy_tree_with_journal(
    source_abs: &Path,
    target_root: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    let files = collect_files_recursive(source_abs)?;
    let game_root = context.game_root;
    let backup_root = context.backup_root;
    let journal: SharedJournal = Mutex::new(&mut *context.journal);

    let workers = worker_count(files.len());
    if workers <= 1 {
        for file in &files {
            apply_copy_file(source_abs, target_root, file, game_root, backup_root, &journal)?;
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
                    &files,
                    &next,
                    &first_error,
                    source_abs,
                    target_root,
                    game_root,
                    backup_root,
                    &journal,
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

fn worker_count(file_count: usize) -> usize {
    if file_count <= 1 {
        return 1;
    }
    let cpus = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    cpus.clamp(1, MAX_COPY_WORKERS).min(file_count)
}

#[allow(clippy::too_many_arguments)]
fn copy_worker(
    files: &[PathBuf],
    next: &AtomicUsize,
    first_error: &Mutex<Option<AppError>>,
    source_abs: &Path,
    target_root: &Path,
    game_root: &Path,
    backup_root: &Path,
    journal: &SharedJournal,
) {
    loop {
        if first_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
        {
            break;
        }
        let idx = next.fetch_add(1, Ordering::Relaxed);
        let Some(file) = files.get(idx) else {
            break;
        };
        if let Err(err) =
            apply_copy_file(source_abs, target_root, file, game_root, backup_root, journal)
        {
            let mut slot = first_error
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if slot.is_none() {
                *slot = Some(err);
            }
            break;
        }
    }
}

fn apply_copy_file(
    source_abs: &Path,
    target_root: &Path,
    file: &Path,
    game_root: &Path,
    backup_root: &Path,
    journal: &SharedJournal,
) -> Result<(), AppError> {
    let rel = file.strip_prefix(source_abs).unwrap_or(file);
    let dest = target_root.join(rel);
    ensure_destination_allowed(game_root, &dest)?;
    journal_destination_state(&dest, game_root, backup_root, journal)?;
    // Force the backup/new record to durable storage *before* the destructive
    // copy below, so a crash can never leave an overwritten game file with no
    // recoverable journal entry.
    sync_shared_journal(journal)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    // Copy source -> dest while hashing in one pass. The copied content equals
    // the source, so its hash is the source's — avoiding a re-read of dest.
    let copied_hash = copy_and_hash(file, &dest)?;
    write_copy_line(journal, file, &dest, &copied_hash)?;
    Ok(())
}

fn journal_destination_state(
    dest: &Path,
    game_root: &Path,
    backup_root: &Path,
    journal: &SharedJournal,
) -> Result<(), AppError> {
    if dest.exists() {
        let backup = backup_root.join(backup_relative_for_destination(game_root, dest)?);
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create backup directory {}", parent.display()))?;
        }
        // Back up the original by copying dest -> backup while hashing in one
        // pass, avoiding a separate re-read of the backup just to hash it.
        let backup_hash = copy_and_hash(dest, &backup)?;
        // The backup is the only copy of the original once we overwrite the
        // destination, so its contents must be durable before that overwrite.
        sync_file_contents(&backup)?;
        write_backup_line(journal, dest, &backup, &backup_hash)?;
    } else {
        write_new_line(journal, dest)?;
    }
    Ok(())
}

/// Stream `source` to `dest` through one reused buffer, returning the FNV hash
/// of the bytes written. A single read of `source` replaces the previous
/// copy-then-reread-to-hash, roughly halving disk reads per file.
fn copy_and_hash(source: &Path, dest: &Path) -> Result<String, AppError> {
    let mut input =
        fs::File::open(source).with_context(|| format!("open {}", source.display()))?;
    let mut output =
        fs::File::create(dest).with_context(|| format!("create {}", dest.display()))?;
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
            .with_context(|| format!("write {}", dest.display()))?;
        for &byte in &buffer[..read] {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    Ok(format!("fnv64:{hash:016x}"))
}

fn write_backup_line(
    journal: &SharedJournal,
    dest: &Path,
    backup: &Path,
    backup_hash: &str,
) -> Result<(), AppError> {
    let mut guard = journal.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    writeln!(
        &mut **guard,
        "backup={}|{}|{}",
        escape_value(&dest.display().to_string()),
        escape_value(&backup.display().to_string()),
        backup_hash
    )?;
    Ok(())
}

fn write_new_line(journal: &SharedJournal, dest: &Path) -> Result<(), AppError> {
    let mut guard = journal.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    writeln!(
        &mut **guard,
        "new={}",
        escape_value(&dest.display().to_string())
    )?;
    Ok(())
}

fn write_copy_line(
    journal: &SharedJournal,
    file: &Path,
    dest: &Path,
    copied_hash: &str,
) -> Result<(), AppError> {
    let mut guard = journal.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    writeln!(
        &mut **guard,
        "copy={}|{}|{}",
        escape_value(&file.display().to_string()),
        escape_value(&dest.display().to_string()),
        copied_hash
    )?;
    Ok(())
}

/// Durably flush the shared journal (under its lock).
fn sync_shared_journal(journal: &SharedJournal) -> Result<(), AppError> {
    let mut guard = journal.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let file: &mut fs::File = &mut **guard;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

/// Flush the journal file's buffered contents through to durable storage.
///
/// On Windows this maps to `FlushFileBuffers`; the journal is opened for
/// writing so the call is permitted.
pub(super) fn sync_journal(journal: &mut fs::File) -> Result<(), AppError> {
    journal.flush()?;
    journal.sync_all()?;
    Ok(())
}

/// Force an already-written file's contents to durable storage.
///
/// Opens the file with write access because `File::sync_all` -> `FlushFileBuffers`
/// requires a writable handle on Windows; `write(true)` without `truncate`
/// leaves the existing contents intact.
fn sync_file_contents(path: &Path) -> Result<(), AppError> {
    let file = fs::OpenOptions::new().write(true).open(path)?;
    file.sync_all()?;
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
        let data: Vec<u8> = (0..(HASH_BUFFER_BYTES * 3 + 123))
            .map(|i| (i % 251) as u8)
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
        let mut hash = 0xcbf29ce484222325u64;
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
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
        let backup_root = game_root
            .join(".sa-mod-manager")
            .join("backups")
            .join("tx");
        let journal_path = game_root
            .join(".sa-mod-manager")
            .join("journals")
            .join("tx.journal");
        // Enough files to fan out across multiple copy workers.
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir_all(journal_path.parent().unwrap()).unwrap();
        fs::create_dir_all(&backup_root).unwrap();
        for idx in 0..40 {
            let rel = if idx % 2 == 0 {
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

        for idx in 0..40 {
            let rel = if idx % 2 == 0 {
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
        assert_eq!(journal_text.lines().filter(|l| l.starts_with("new=")).count(), 40);
        assert_eq!(
            journal_text.lines().filter(|l| l.starts_with("copy=")).count(),
            40
        );
        fs::remove_dir_all(&game_root).unwrap();
    }
}
