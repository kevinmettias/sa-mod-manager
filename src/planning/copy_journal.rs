use crate::prelude::*;
use std::io::Read;

/// Reused per-file read buffer size for streaming hashes. Bounds `file_hash`
/// memory to this regardless of file size (game `.img` archives are multi-GB).
const HASH_BUFFER_BYTES: usize = 64 * 1024;

pub(super) fn apply_copy_tree_with_journal(
    source_abs: &Path,
    target_root: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    let files = collect_files_recursive(source_abs)?;
    for file in files {
        apply_copy_file_with_journal(source_abs, target_root, &file, context)?;
    }
    Ok(())
}

fn apply_copy_file_with_journal(
    source_abs: &Path,
    target_root: &Path,
    file: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    let rel = file.strip_prefix(source_abs).unwrap_or(file);
    let dest = target_root.join(rel);
    ensure_destination_allowed(context.game_root, &dest)?;
    journal_destination_state(&dest, context)?;
    // Force the backup/new record to durable storage *before* the destructive
    // copy below. Otherwise a crash or power loss between the copy landing on
    // disk and the journal line being flushed would leave an overwritten game
    // file with no recoverable journal entry, silently breaking rollback.
    sync_journal(context.journal)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    fs::copy(file, &dest)
        .with_context(|| format!("copy {} -> {}", file.display(), dest.display()))?;
    let copied_hash = file_hash(&dest)?;
    write_copy_line(context.journal, file, &dest, &copied_hash)?;
    Ok(())
}

fn journal_destination_state(
    dest: &Path,
    context: &mut CopyJournalContext,
) -> Result<(), AppError> {
    if dest.exists() {
        let backup = context
            .backup_root
            .join(backup_relative_for_destination(context.game_root, dest)?);
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create backup directory {}", parent.display()))?;
        }
        fs::copy(dest, &backup)
            .with_context(|| format!("back up {} -> {}", dest.display(), backup.display()))?;
        // The backup is the only copy of the original file once we overwrite the
        // destination, so its contents must be durable before that overwrite.
        sync_file_contents(&backup)?;
        let backup_hash = file_hash(&backup)?;
        write_backup_line(context.journal, dest, &backup, &backup_hash)?;
    } else {
        write_new_line(context.journal, dest)?;
    }
    Ok(())
}

fn write_backup_line(
    journal: &mut fs::File,
    dest: &Path,
    backup: &Path,
    backup_hash: &str,
) -> Result<(), AppError> {
    writeln!(
        journal,
        "backup={}|{}|{}",
        escape_value(&dest.display().to_string()),
        escape_value(&backup.display().to_string()),
        backup_hash
    )?;
    Ok(())
}

fn write_new_line(journal: &mut fs::File, dest: &Path) -> Result<(), AppError> {
    writeln!(journal, "new={}", escape_value(&dest.display().to_string()))?;
    Ok(())
}

fn write_copy_line(
    journal: &mut fs::File,
    file: &Path,
    dest: &Path,
    copied_hash: &str,
) -> Result<(), AppError> {
    writeln!(
        journal,
        "copy={}|{}|{}",
        escape_value(&file.display().to_string()),
        escape_value(&dest.display().to_string()),
        copied_hash
    )?;
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
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
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
}
