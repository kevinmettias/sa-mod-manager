use crate::prelude::*;
use std::io::Read;
use std::process::Stdio;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub(crate) fn extract_archive_to_named_staging(
    package: &Path,
    game_root: &Path,
    package_id: &str,
) -> Result<PathBuf, AppError> {
    let target = state_directory(game_root).join("staging").join(package_id); // literal: allow external interface text or file-format spelling
    // Reused, per-package staging path: clear any prior extraction first so stale
    // files from an earlier (possibly different) version of this package cannot
    // linger and mix into the fresh contents. Import uses unique dirs instead, so
    // only this named-staging path needs the reset.
    if target.exists() {
        fs::remove_dir_all(&target)
            .with_context(|| format!("clear staging directory {}", target.display()))?;
    }
    extract_archive_to_directory(package, &target)?;
    Ok(target)
}

pub(crate) fn extract_archive_to_directory(package: &Path, target: &Path) -> Result<(), AppError> {
    let mut budget = ExtractBudget::new();
    extract_archive_raw(package, target, &mut budget)?;
    // Unpack archives nested inside the package so their contents can be
    // classified and installed instead of being treated as opaque files.
    // Bounded by MAX_NESTED_DEPTH and the shared extraction budget.
    extract_nested_archives(target, &mut budget)
}

fn extract_archive_raw(
    package: &Path,
    target: &Path,
    budget: &mut ExtractBudget,
) -> Result<(), AppError> {
    match archive_backend(package) {
        ArchiveBackend::NativeZip => extract_zip_to_directory(package, target, budget),
        ArchiveBackend::SevenZip => extract_with_seven_zip(package, target, budget),
    }
}

fn extract_with_seven_zip(
    package: &Path,
    target: &Path,
    budget: &ExtractBudget,
) -> Result<(), AppError> {
    let seven_zip = find_seven_zip().ok_or_else(|| missing_7zip_error_for_package(package))?;
    // The external extractor gives no per-entry hook, so the containment and
    // zip-bomb guarantees the native path enforces inline are made here from the
    // archive listing *before* `7z x` writes anything: reject path-traversal
    // entries and charge declared sizes against the shared budget so a classic
    // decompression bomb is refused before it ever reaches disk.
    seven_zip_preflight(&seven_zip, package, budget)?;
    fs::create_dir_all(target)?;
    let output = Command::new(&seven_zip)
        .env("LC_ALL", "C") // prefer stable, English tool messages regardless of system locale
        .arg("x") // literal: allow external interface text or file-format spelling
        .arg("-y") // literal: allow external interface text or file-format spelling
        .arg(format!("-o{}", target.display()))
        .arg(package)
        .output()?;
    if !output.status.success() {
        return Err(seven_zip_extract_error(package, &output.stderr));
    }
    Ok(())
}

/// List a `.7z`/`.rar` before extraction, enforcing the same guarantees the
/// native zip path enforces per entry: reject any entry whose path escapes the
/// target, and charge the declared uncompressed sizes and entry count against
/// the shared budget so an over-limit archive (the classic bomb) is refused
/// before `7z x` writes a single byte.
fn seven_zip_preflight(
    seven_zip: &Path,
    package: &Path,
    budget: &ExtractBudget,
) -> Result<(), AppError> {
    use std::io::BufRead;

    let mut child = Command::new(seven_zip)
        .env("LC_ALL", "C") // prefer stable tool output regardless of system locale
        .arg("l") // literal: allow external interface text or file-format spelling
        .arg("-slt") // literal: allow external interface text or file-format spelling
        .arg(package)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut declared_bytes: u64 = 0;
    let mut declared_entries: usize = 0;
    // 7-Zip prints an archive-level block first (whose `Path` is the archive's
    // own, often absolute, path); real entries follow the `----------` divider.
    let mut past_header = false;
    if let Some(stdout) = child.stdout.take() {
        for line in io::BufReader::new(stdout).lines() {
            let line = line?;
            if line.starts_with("----------") {
                past_header = true;
                continue;
            }
            if !past_header {
                continue;
            }
            if let Some(value) = strip_listing_key(&line, "Path = ") {
                declared_entries = declared_entries.saturating_add(1);
                if declared_entries > budget.remaining_entries() {
                    return Err(too_many_entries_error(package));
                }
                ensure_contained_entry(value.trim(), package)?;
            } else if let Some(value) = strip_listing_key(&line, "Size = ")
                && let Ok(size) = value.trim().parse::<u64>()
            {
                declared_bytes = declared_bytes.saturating_add(size);
            }
        }
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(list_archive_failed_error_detail(package, &output.stderr));
    }

    if declared_entries > budget.remaining_entries() {
        return Err(too_many_entries_error(package));
    }
    if declared_bytes > budget.remaining_bytes() {
        return Err(archive_too_large_error(package));
    }
    // Charge the declared totals so nested archives draw down the same budget,
    // mirroring the native path's per-entry accounting.
    budget.charge(declared_bytes, declared_entries);
    Ok(())
}

/// Reject a listed archive entry whose path would extract outside the target —
/// absolute, drive-qualified (`C:\…`), or containing a `..` component. This is
/// the 7-Zip-path equivalent of the native backend's `enclosed_name()` guard.
fn ensure_contained_entry(entry_path: &str, package: &Path) -> Result<(), AppError> {
    let normalized = entry_path.replace('\\', "/");
    let drive_qualified = normalized
        .as_bytes()
        .get(1)
        .is_some_and(|&byte| byte == b':');
    let looks_absolute =
        normalized.starts_with('/') || Path::new(&normalized).is_absolute() || drive_qualified;
    let has_parent_escape = normalized.split('/').any(|component| component == "..");
    if looks_absolute || has_parent_escape {
        return Err(AppError::Tool(format!(
            "{} contains an unsafe entry path '{entry_path}' that would extract outside the package; refusing to extract",
            package.display()
        )));
    }
    Ok(())
}

/// Strip a 7-Zip `-slt` property key case-insensitively (tolerant of casing and
/// leading-byte quirks) so listing parsing does not hinge on exact key casing.
fn strip_listing_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let head = line.get(..key.len())?;
    if head.eq_ignore_ascii_case(key) {
        Some(&line[key.len()..])
    } else {
        None
    }
}

fn extract_nested_archives(root: &Path, budget: &mut ExtractBudget) -> Result<(), AppError> {
    // Each pass unpacks one layer of nesting (merging inner archives into their
    // parent directory) and removes the consumed archive; a following pass then
    // finds anything the previous layer revealed.
    for _ in 0..MAX_NESTED_DEPTH {
        let inner = find_inner_archives(root)?;
        if inner.is_empty() {
            break;
        }
        for archive in inner {
            let parent = archive
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.to_path_buf());
            extract_archive_raw(&archive, &parent, budget)?;
            fs::remove_file(&archive)?;
        }
    }
    Ok(())
}

fn find_inner_archives(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut archives = Vec::new();
    for file in collect_files_recursive(root)? {
        if let Some(ext) = package_extension(&file) {
            if NESTED_ARCHIVE_EXTENSIONS.contains(&ext.as_str()) {
                archives.push(file);
            }
        }
    }
    Ok(archives)
}

pub(crate) fn list_archive_entries_native(
    package: &Path,
) -> Result<Option<Vec<PackageEntry>>, AppError> {
    if archive_backend(package) != ArchiveBackend::NativeZip {
        return Ok(None);
    }
    let file = fs::File::open(package)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| {
        AppError::Tool(format!("failed to read zip {}: {err}", package.display()))
    })?;
    // A zip central directory can claim any number of entries; bound how many we
    // enumerate so a hostile archive cannot grow this vector without limit.
    if archive.len() > MAX_EXTRACT_ENTRIES {
        return Err(too_many_entries_error(package));
    }
    let mut entries = Vec::new();
    for idx in 0..archive.len() {
        let file = archive
            .by_index(idx)
            .map_err(|err| zip_entry_error(package, &err))?;
        let path = file.name().replace('\\', "/");
        if path.is_empty() {
            continue;
        }
        entries.push(PackageEntry {
            path,
            size: file.size(),
            is_dir: file.is_dir(),
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Some(entries))
}

pub(crate) fn read_package_text_file(
    package: &Path,
    entry_path: &str,
    max_bytes: usize,
) -> Result<Option<String>, AppError> {
    if package.is_dir() {
        return read_folder_text_file(package, entry_path, max_bytes);
    }
    if archive_backend(package) == ArchiveBackend::NativeZip {
        return read_zip_text_file(package, entry_path, max_bytes);
    }
    read_external_archive_text_file(package, entry_path, max_bytes)
}

pub(crate) fn missing_7zip_error_for_package(package: &Path) -> AppError {
    AppError::Tool(seven_zip_missing_message(&package.display().to_string()))
}

/// Build a "7-Zip failed to list" error, keeping 7-Zip's own reason (from its
/// stderr) so a corrupt archive is diagnosable instead of collapsing to a
/// generic line. Pass an empty slice when no stderr was captured.
pub(crate) fn list_archive_failed_error_detail(package: &Path, stderr: &[u8]) -> AppError {
    AppError::Tool(format!(
        "7-Zip failed to list {}{}",
        package.display(),
        stderr_snippet(&String::from_utf8_lossy(stderr))
    ))
}

pub(crate) fn copy_tree_contents(source: &Path, target: &Path) -> Result<(), AppError> {
    fs::create_dir_all(target)?;
    let files = collect_files_recursive(source)?;
    for file in files {
        let rel = file.strip_prefix(source).unwrap_or(&file);
        let dest = target.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&file, &dest)?;
    }
    Ok(())
}

pub(crate) fn collect_files_recursive(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut files = Vec::new();
    collect_files_recursive_inner(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn seven_zip_extract_error(package: &Path, stderr: &[u8]) -> AppError {
    let text = String::from_utf8_lossy(stderr);
    let lower = text.to_ascii_lowercase();
    if lower.contains("wrong password") || lower.contains("password") || lower.contains("encrypted")
    {
        return AppError::Tool(format!(
            "7-Zip could not extract {} because it is password-protected/encrypted, which is not supported",
            package.display()
        ));
    }
    // Keep 7-Zip's own reason instead of collapsing every failure to a generic
    // line, so a genuinely corrupt archive is diagnosable.
    AppError::Tool(format!(
        "7-Zip failed to extract {}{}",
        package.display(),
        stderr_snippet(&text)
    ))
}

/// A compact, single-line tail of a tool's stderr for use in an error message:
/// the last non-empty line, length-capped, so we keep the diagnostic without
/// dumping a multi-line banner.
fn stderr_snippet(text: &str) -> String {
    match text.lines().rev().find(|line| !line.trim().is_empty()) {
        Some(line) => {
            let capped: String = line.trim().chars().take(200).collect();
            format!(": {capped}")
        }
        None => String::new(),
    }
}

fn zip_entry_error(package: &Path, err: &zip::result::ZipError) -> AppError {
    let text = err.to_string();
    if text.to_ascii_lowercase().contains("password") {
        return AppError::Tool(format!(
            "{} contains an encrypted entry, which is not supported: {text}",
            package.display()
        ));
    }
    AppError::Tool(format!("failed to read zip entry: {text}"))
}

/// Upper bounds on a single package's extraction, shared across nested archives
/// so an archive-in-archive cannot multiply past these limits.
const MAX_EXTRACT_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_EXTRACT_ENTRIES: usize = 500_000;
/// Cap on concurrent zip-extraction workers; disk I/O saturates before high
/// thread counts, so we stay modest.
const MAX_EXTRACT_WORKERS: usize = 8;
const MAX_NESTED_DEPTH: usize = 3;
const NESTED_ARCHIVE_EXTENSIONS: [&str; 3] = ["zip", "7z", "rar"];

/// Atomic so it can be shared (`&self`) across parallel extraction workers; the
/// cap is enforced globally even though per-entry byte limits read approximately.
struct ExtractBudget {
    remaining_bytes: AtomicU64,
    remaining_entries: AtomicUsize,
}

impl ExtractBudget {
    fn new() -> Self {
        Self::with_limits(MAX_EXTRACT_BYTES, MAX_EXTRACT_ENTRIES)
    }

    fn with_limits(bytes: u64, entries: usize) -> Self {
        Self {
            remaining_bytes: AtomicU64::new(bytes),
            remaining_entries: AtomicUsize::new(entries),
        }
    }

    fn take_entry(&self, package: &Path) -> Result<(), AppError> {
        self.remaining_entries
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_sub(1)
            })
            .map(|_| ())
            .map_err(|_| too_many_entries_error(package))
    }

    fn take_bytes(&self, count: u64, package: &Path) -> Result<(), AppError> {
        self.remaining_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_sub(count)
            })
            .map(|_| ())
            .map_err(|_| archive_too_large_error(package))
    }

    fn remaining_bytes(&self) -> u64 {
        self.remaining_bytes.load(Ordering::Relaxed)
    }

    fn remaining_entries(&self) -> usize {
        self.remaining_entries.load(Ordering::Relaxed)
    }

    /// Draw down the budget by an already-validated amount (used by the 7-Zip
    /// path after its listing has been checked against the remaining budget).
    /// Saturating, since the caller has already refused anything over budget.
    fn charge(&self, bytes: u64, entries: usize) {
        self.remaining_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(bytes))
            })
            .ok();
        self.remaining_entries
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(entries))
            })
            .ok();
    }
}

fn archive_too_large_error(package: &Path) -> AppError {
    AppError::Tool(format!(
        "{} expands past the {} GiB extraction limit; refusing to continue",
        package.display(),
        MAX_EXTRACT_BYTES / (1024 * 1024 * 1024)
    ))
}

fn too_many_entries_error(package: &Path) -> AppError {
    AppError::Tool(format!(
        "{} exceeds the {}-entry extraction limit; refusing to continue",
        package.display(),
        MAX_EXTRACT_ENTRIES
    ))
}

fn is_symlink_mode(mode: Option<u32>) -> bool {
    mode.map(|value| value & 0o170000 == 0o120000).unwrap_or(false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchiveBackend {
    NativeZip,
    SevenZip,
}

fn archive_backend(package: &Path) -> ArchiveBackend {
    if native_zip_supported(package) {
        ArchiveBackend::NativeZip
    } else {
        ArchiveBackend::SevenZip
    }
}

fn native_zip_supported(package: &Path) -> bool {
    package_extension(package)
        .map(|ext| ext == "zip" || ext == "wrap")
        .unwrap_or(false)
}

fn package_extension(package: &Path) -> Option<String> {
    package
        .extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.to_ascii_lowercase())
}

fn seven_zip_missing_message(package: &str) -> String {
    format!(
        "7-Zip was not found for {package}. Native .zip and .wrap packages work without 7-Zip; .7z and .rar need 7-Zip, 7zz, or 7za on PATH, or set SA_MOD_MANAGER_7Z to a portable 7-Zip executable."
    )
}

fn extract_zip_to_directory(
    package: &Path,
    target: &Path,
    budget: &ExtractBudget,
) -> Result<(), AppError> {
    fs::create_dir_all(target)?;
    let count = open_zip(package)?.len();

    let workers = extraction_worker_count(count);
    if workers <= 1 {
        let mut archive = open_zip(package)?;
        for idx in 0..count {
            extract_zip_entry(&mut archive, idx, package, target, budget)?;
        }
        return Ok(());
    }

    // Each worker opens its own archive handle (`ZipArchive` isn't `Sync`) and
    // pulls entry indices from a shared atomic dispenser. deflate decompression
    // is CPU-bound, so this scales extraction across cores.
    let next = AtomicUsize::new(0);
    let first_error: Mutex<Option<AppError>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                extract_zip_worker(package, target, budget, count, &next, &first_error);
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

fn open_zip(package: &Path) -> Result<zip::ZipArchive<fs::File>, AppError> {
    let file = fs::File::open(package)?;
    zip::ZipArchive::new(file)
        .map_err(|err| AppError::Tool(format!("failed to read zip {}: {err}", package.display())))
}

fn extraction_worker_count(entry_count: usize) -> usize {
    if entry_count <= 1 {
        return 1;
    }
    let cpus = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    cpus.clamp(1, MAX_EXTRACT_WORKERS).min(entry_count)
}

fn extract_zip_worker(
    package: &Path,
    target: &Path,
    budget: &ExtractBudget,
    count: usize,
    next: &AtomicUsize,
    first_error: &Mutex<Option<AppError>>,
) {
    let mut archive = match open_zip(package) {
        Ok(archive) => archive,
        Err(err) => {
            record_first_error(first_error, err);
            return;
        }
    };
    loop {
        if first_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
        {
            break;
        }
        let idx = next.fetch_add(1, Ordering::Relaxed);
        if idx >= count {
            break;
        }
        if let Err(err) = extract_zip_entry(&mut archive, idx, package, target, budget) {
            record_first_error(first_error, err);
            break;
        }
    }
}

fn record_first_error(slot: &Mutex<Option<AppError>>, err: AppError) {
    let mut guard = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none() {
        *guard = Some(err);
    }
}

fn extract_zip_entry(
    archive: &mut zip::ZipArchive<fs::File>,
    idx: usize,
    package: &Path,
    target: &Path,
    budget: &ExtractBudget,
) -> Result<(), AppError> {
    budget.take_entry(package)?;
    let mut entry = archive
        .by_index(idx)
        .map_err(|err| zip_entry_error(package, &err))?;
    let raw_name = entry.name().to_string();
    let Some(enclosed) = entry.enclosed_name() else {
        // A traversal/absolute entry is dropped rather than extracted; surface
        // it so a partially-skipped archive isn't silently reported as complete.
        log_warn!("skipped unsafe zip entry path: {raw_name}");
        return Ok(());
    };
    // Skip symlink entries: materializing one would let a later entry write
    // through the link to a path outside the extraction target (zip-slip).
    if is_symlink_mode(entry.unix_mode()) {
        log_warn!("skipped symlink zip entry: {raw_name}");
        return Ok(());
    }
    let dest = target.join(enclosed);
    if entry.is_dir() {
        fs::create_dir_all(dest)?;
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = fs::File::create(dest)?;
    // Bound the *actual* decompressed bytes (not the header-declared size) so a
    // zip bomb cannot exhaust the disk: read at most one byte past the budget,
    // then charge what was written, which fails once the limit is crossed.
    let limit = budget.remaining_bytes().saturating_add(1);
    let written = io::copy(&mut entry.by_ref().take(limit), &mut out)?;
    budget.take_bytes(written, package)?;
    Ok(())
}

fn read_folder_text_file(
    package: &Path,
    entry_path: &str,
    max_bytes: usize,
) -> Result<Option<String>, AppError> {
    let rel = path_from_package_root(entry_path)?;
    let path = package.join(rel);
    if !path.exists() || !path.is_file() {
        return Ok(None);
    }
    // Read only up to the cap rather than the whole file, which may be large.
    let file = fs::File::open(&path)?;
    let mut bytes = Vec::new();
    file.take(max_bytes as u64).read_to_end(&mut bytes)?;
    Ok(Some(text_from_bytes(&bytes, max_bytes)))
}

fn read_zip_text_file(
    package: &Path,
    entry_path: &str,
    max_bytes: usize,
) -> Result<Option<String>, AppError> {
    let file = fs::File::open(package)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| {
        AppError::Tool(format!("failed to read zip {}: {err}", package.display()))
    })?;
    let mut entry = match archive.by_name(entry_path) {
        Ok(entry) => entry,
        Err(_) => return Ok(None),
    };
    if entry.is_dir() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    let limit = max_bytes.saturating_add(1) as u64;
    entry.by_ref().take(limit).read_to_end(&mut bytes)?;
    Ok(Some(text_from_bytes(&bytes, max_bytes)))
}

fn read_external_archive_text_file(
    package: &Path,
    entry_path: &str,
    max_bytes: usize,
) -> Result<Option<String>, AppError> {
    let seven_zip = find_seven_zip().ok_or_else(|| missing_7zip_error_for_package(package))?;
    let mut child = Command::new(seven_zip)
        .arg("e")
        .arg("-so")
        .arg(package)
        .arg(entry_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    // Read only up to the cap; closing the pipe and killing 7-Zip afterward means
    // we never buffer a whole (possibly multi-MB) extracted file just to keep a
    // small README window.
    let mut buffer = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout
            .by_ref()
            .take(max_bytes as u64)
            .read_to_end(&mut buffer)?;
    }
    let _ = child.kill();
    let _ = child.wait();

    if buffer.is_empty() {
        Ok(None)
    } else {
        Ok(Some(text_from_bytes(&buffer, max_bytes)))
    }
}

fn text_from_bytes(bytes: &[u8], max_bytes: usize) -> String {
    let end = bytes.len().min(max_bytes);
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

fn collect_files_recursive_inner(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), AppError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_files_recursive_inner(&path, files)?;
        } else if metadata.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    #[test]
    fn zip_archives_are_listed_read_and_extracted_without_external_tools() {
        let root = test_root("native_zip");
        let package = root.join("package.zip");
        write_zip_package(
            &package,
            &[
                ("README.txt", "Install with Mod Loader."),
                ("modloader/Test/file.txt", "payload"),
            ],
        );

        let entries = list_archive_entries_native(&package).unwrap().unwrap();
        let readme = read_package_text_file(&package, "README.txt", 1024)
            .unwrap()
            .unwrap();
        let target = root.join("extract");
        extract_archive_to_directory(&package, &target).unwrap();

        assert!(entries.iter().any(|entry| entry.path == "README.txt"));
        assert!(readme.contains("Mod Loader"));
        assert_eq!(
            fs::read_to_string(target.join("modloader").join("Test").join("file.txt")).unwrap(),
            "payload"
        );
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn extraction_enforces_total_byte_budget() {
        let root = test_root("zip_byte_budget");
        let package = root.join("big.zip");
        let payload = "x".repeat(64);
        write_zip_package(&package, &[("data/file.bin", payload.as_str())]);

        let budget = ExtractBudget::with_limits(16, 100);
        let err = extract_zip_to_directory(&package, &root.join("out"), &budget)
            .unwrap_err()
            .to_string();

        assert!(err.contains("extraction limit"));
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn extraction_enforces_entry_count_budget() {
        let root = test_root("zip_entry_budget");
        let package = root.join("many.zip");
        write_zip_package(&package, &[("a.txt", "a"), ("b.txt", "b")]);

        let budget = ExtractBudget::with_limits(1 << 20, 1);
        let err = extract_zip_to_directory(&package, &root.join("out"), &budget)
            .unwrap_err()
            .to_string();

        assert!(err.contains("extraction limit"));
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn symlink_modes_are_detected_and_regular_modes_are_not() {
        // Extraction skips any entry whose unix mode marks it a symlink (S_IFLNK),
        // the bits real unix archivers set on link entries.
        assert!(is_symlink_mode(Some(0o120777)));
        assert!(!is_symlink_mode(Some(0o100644))); // regular file
        assert!(!is_symlink_mode(Some(0o040755))); // directory
        assert!(!is_symlink_mode(None)); // no unix mode recorded
    }

    #[test]
    fn seven_zip_entry_containment_rejects_escapes_and_allows_safe_paths() {
        let package = Path::new("mod.7z");
        // Safe relative paths pass.
        for safe in [
            "modloader/Test/file.txt",
            "CLEO/script.cs",
            "readme.txt",
            "a/b/c.dat",
        ] {
            assert!(ensure_contained_entry(safe, package).is_ok(), "{safe}");
        }
        // Traversal, absolute, and drive-qualified paths are refused.
        for unsafe_path in [
            "../escape.txt",
            "modloader/../../escape.txt",
            "/etc/passwd",
            r"C:\Windows\system32\evil.dll",
            r"..\..\outside.txt",
        ] {
            let err = ensure_contained_entry(unsafe_path, package)
                .unwrap_err()
                .to_string();
            assert!(err.contains("unsafe entry path"), "{unsafe_path}: {err}");
        }
    }

    #[test]
    fn nested_archives_are_recursively_extracted_and_removed() {
        let root = test_root("zip_nested");
        let mut inner_bytes = Vec::new();
        {
            let mut inner = zip::ZipWriter::new(io::Cursor::new(&mut inner_bytes));
            inner
                .start_file("modloader/inner.txt", SimpleFileOptions::default())
                .unwrap();
            inner.write_all(b"nested payload").unwrap();
            inner.finish().unwrap();
        }

        let package = root.join("outer.zip");
        {
            let file = fs::File::create(&package).unwrap();
            let mut outer = zip::ZipWriter::new(file);
            outer
                .start_file("outer.txt", SimpleFileOptions::default())
                .unwrap();
            outer.write_all(b"outer payload").unwrap();
            outer
                .start_file("inner.zip", SimpleFileOptions::default())
                .unwrap();
            outer.write_all(&inner_bytes).unwrap();
            outer.finish().unwrap();
        }

        let target = root.join("out");
        extract_archive_to_directory(&package, &target).unwrap();

        assert!(target.join("outer.txt").exists());
        assert!(target.join("modloader").join("inner.txt").exists());
        assert!(!target.join("inner.zip").exists());
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn wrap_archives_use_native_zip_backend_case_insensitively() {
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
    fn missing_7zip_message_explains_native_and_external_backends() {
        let message = missing_7zip_error_for_package(Path::new("mod.rar")).to_string();

        assert!(message.contains(".zip and .wrap"));
        assert!(message.contains(".7z and .rar"));
        assert!(message.contains("SA_MOD_MANAGER_7Z"));
    }

    #[test]
    fn parallel_zip_extraction_extracts_every_file_correctly() {
        let root = test_root("zip_parallel");
        let package = root.join("many.zip");
        // Enough files across nested dirs to fan out over multiple workers.
        let entries: Vec<(String, String)> = (0..40)
            .map(|idx| {
                let name = if idx % 3 == 0 {
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
        extract_archive_to_directory(&package, &target).unwrap();

        for (name, content) in &entries {
            assert_eq!(fs::read_to_string(target.join(name)).unwrap(), *content);
        }
        remove_dir_if_exists(&root).unwrap();
    }

    fn write_zip_package(path: &Path, entries: &[(&str, &str)]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, text) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        remove_dir_if_exists(&root).unwrap();
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}
