use crate::prelude::*;
use std::io::Read;
use std::process::Stdio;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub(crate) fn extract_archive_to_named_staging(
    package: &Path,
    game_root: &Path,
    package_id: &str,
) -> Result<PathBuf, AppError>
{
    let target = state_directory(game_root).join("staging").join(package_id);
    // Reused, per-package staging path: clear any prior extraction first so stale
    // files from an earlier (possibly different) version of this package cannot
    // linger and mix into the fresh contents. Import uses unique dirs instead, so
    // only this named-staging path needs the reset.
    if target.exists()
    {
        fs::remove_dir_all(&target)
            .with_context(|| format!("clear staging directory {}", target.display()))?;
    }
    extract_archive_to_directory(package, &target)?;
    return Ok(target);
}

pub(crate) fn extract_archive_to_directory(package: &Path, target: &Path) -> Result<(), AppError>
{
    let mut budget = ExtractBudget::new();
    extract_archive_raw(package, target, &mut budget)?;
    // Unpack archives nested inside the package so their contents can be
    // classified and installed instead of being treated as opaque files.
    // Bounded by MAX_NESTED_DEPTH and the shared extraction budget.
    return extract_nested_archives(target, &mut budget);
}

fn extract_nested_archives(root: &Path, budget: &mut ExtractBudget) -> Result<(), AppError>
{
    // Each pass unpacks one layer of nesting (merging inner archives into their
    // parent directory) and removes the consumed archive; a following pass then
    // finds anything the previous layer revealed.
    for _ in 0..MAX_NESTED_DEPTH
    {
        let inner = find_inner_archives(root)?;
        if inner.is_empty()
        {
            break;
        }
        for archive in inner
        {
            let parent = archive
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.to_path_buf());
            extract_archive_raw(&archive, &parent, budget)?;
            fs::remove_file(&archive)?;
        }
    }
    return Ok(());
}

fn find_inner_archives(root: &Path) -> Result<Vec<PathBuf>, AppError>
{
    let mut archives = Vec::new();
    for file in collect_files_recursive(root)?
    {
        if let Some(ext) = package_extension(&file)
        {
            if NESTED_ARCHIVE_EXTENSIONS.contains(&ext.as_str())
            {
                archives.push(file);
            }
        }
    }
    return Ok(archives);
}

pub(crate) fn list_archive_entries_native(
    package: &Path,
) -> Result<Option<Vec<PackageEntry>>, AppError>
{
    if archive_backend(package) != ArchiveBackend::NativeZip
    {
        return Ok(None);
    }
    let file = fs::File::open(package)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| {
        AppError::Tool(format!("failed to read zip {}: {err}", package.display()))
    })?;
    // A zip central directory can claim any number of entries; bound how many we
    // enumerate so a hostile archive cannot grow this vector without limit.
    if archive.len() > MAX_EXTRACT_ENTRIES
    {
        return Err(too_many_entries_error(package));
    }
    let mut entries = Vec::new();
    for idx in 0..archive.len()
    {
        let file = archive
            .by_index(idx)
            .map_err(|err| zip_entry_error(package, &err))?;
        let path = file.name().replace('\\', "/");
        if path.is_empty()
        {
            continue;
        }
        entries.push(PackageEntry {
            path,
            size: file.size(),
            is_dir: file.is_dir(),
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    return Ok(Some(entries));
}

/// Reject a listed archive entry whose path would extract outside the target —
/// absolute, drive-qualified (`C:\…`), or containing a `..` component. This is
/// the 7-Zip-path equivalent of the native backend's `enclosed_name()` guard.
struct SevenZipListingTotals
{
    bytes: u64,
    entries: usize,
}

fn zip_entry_error(package: &Path, err: &zip::result::ZipError) -> AppError
{
    let text = err.to_string();
    if text.to_ascii_lowercase().contains("password")
    {
        return AppError::Tool(format!(
            "{} contains an encrypted entry, which is not supported: {text}",
            package.display()
        ));
    }
    return AppError::Tool(format!("failed to read zip entry: {text}"));
}

pub(crate) fn read_package_text_file(
    package: &Path,
    entry_path: &str,
    max_bytes: usize,
) -> Result<Option<String>, AppError>
{
    if package.is_dir()
    {
        return read_folder_text_file(package, entry_path, max_bytes);
    }
    if archive_backend(package) == ArchiveBackend::NativeZip
    {
        return read_zip_text_file(package, entry_path, max_bytes);
    }
    return read_external_archive_text_file(package, entry_path, max_bytes);
}

pub(crate) fn missing_7zip_error_for_package(package: &Path) -> AppError
{
    return AppError::Tool(seven_zip_missing_message(&package.display().to_string()));
}

/// Build a "7-Zip failed to list" error, keeping 7-Zip's own reason (from its
/// stderr) so a corrupt archive is diagnosable instead of collapsing to a
/// generic line. Pass an empty slice when no stderr was captured.
pub(crate) fn list_archive_failed_error_detail(package: &Path, stderr: &[u8]) -> AppError
{
    return AppError::Tool(format!(
        "7-Zip failed to list {}{}",
        package.display(),
        stderr_snippet(&String::from_utf8_lossy(stderr))
    ));
}

pub(crate) fn copy_tree_contents(source: &Path, target: &Path) -> Result<(), AppError>
{
    fs::create_dir_all(target)?;
    let files = collect_files_recursive(source)?;
    for file in files
    {
        let rel = file.strip_prefix(source).unwrap_or(&file);
        let dest = target.join(rel);
        if let Some(parent) = dest.parent()
        {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&file, &dest)?;
    }
    return Ok(());
}

pub(crate) fn collect_files_recursive(root: &Path) -> Result<Vec<PathBuf>, AppError>
{
    let mut files = Vec::new();
    collect_files_recursive_inner(root, &mut files)?;
    files.sort();
    return Ok(files);
}

/// A compact, single-line tail of a tool's stderr for use in an error message:
/// the last non-empty line, length-capped, so we keep the diagnostic without
/// dumping a multi-line banner.
fn stderr_snippet(text: &str) -> String
{
    return match text.lines().rev().find(|line| !line.trim().is_empty())
    {
        Some(line) => {
            let capped: String = line.trim().chars().take(200).collect(); // literal: allow external format or runtime boundary value means itself here
            format!(": {capped}")
        }
        None => String::new(),
    };
}

fn extract_archive_raw(
    package: &Path,
    target: &Path,
    budget: &mut ExtractBudget,
) -> Result<(), AppError>
{
    return match archive_backend(package)
    {
        ArchiveBackend::NativeZip => extract_zip_to_directory(package, target, budget),
        ArchiveBackend::SevenZip => extract_with_seven_zip(package, target, budget),
    };
}

fn extract_with_seven_zip(
    package: &Path,
    target: &Path,
    budget: &ExtractBudget,
) -> Result<(), AppError>
{
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
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", target.display()))
        .arg(package)
        .output()?;
    if !output.status.success()
    {
        return Err(seven_zip_extract_error(package, &output.stderr));
    }
    return Ok(());
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
) -> Result<(), AppError>
{
    use std::io::BufRead;

    let mut child = Command::new(seven_zip)
        .env("LC_ALL", "C") // prefer stable tool output regardless of system locale
        .arg("l")
        .arg("-slt")
        .arg(package)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut declared = SevenZipListingTotals {
        bytes: 0,
        entries: 0,
    };
    // 7-Zip prints an archive-level block first (whose `Path` is the archive's
    // own, often absolute, path); real entries follow the `----------` divider.
    let mut past_header = false;
    if let Some(stdout) = child.stdout.take()
    {
        for line in io::BufReader::new(stdout).lines()
        {
            let line = line?;
            if line.starts_with("----------")
            {
                past_header = true;
                continue;
            }
            if !past_header
            {
                continue;
            }
            process_seven_zip_listing_line(&line, package, budget, &mut declared)?;
        }
    }
    let output = child.wait_with_output()?;
    if !output.status.success()
    {
        return Err(list_archive_failed_error_detail(package, &output.stderr));
    }

    if declared.entries > budget.remaining_entries()
    {
        return Err(too_many_entries_error(package));
    }
    if declared.bytes > budget.remaining_bytes()
    {
        return Err(archive_too_large_error(package));
    }
    // Charge the declared totals so nested archives draw down the same budget,
    // mirroring the native path's per-entry accounting.
    budget.charge(declared.bytes, declared.entries);
    return Ok(());
}

fn process_seven_zip_listing_line(
    line: &str,
    package: &Path,
    budget: &ExtractBudget,
    declared: &mut SevenZipListingTotals,
) -> Result<(), AppError>
{
    if let Some(value) = strip_listing_key(line, "Path = ")
    {
        declared.entries = declared.entries.saturating_add(1);
        if declared.entries > budget.remaining_entries()
        {
            return Err(too_many_entries_error(package));
        }
        ensure_contained_entry(value.trim(), package)?;
        return Ok(());
    }
    if let Some(value) = strip_listing_key(line, "Size = ")
        && let Ok(size) = value.trim().parse::<u64>()
    {
        declared.bytes = declared.bytes.saturating_add(size);
    }
    return Ok(());
}

fn ensure_contained_entry(entry_path: &str, package: &Path) -> Result<(), AppError>
{
    let normalized = entry_path.replace('\\', "/");
    let drive_qualified = normalized
        .as_bytes()
        .get(1)
        .is_some_and(|&byte| byte == b':');
    let looks_absolute =
        normalized.starts_with('/') || Path::new(&normalized).is_absolute() || drive_qualified;
    let has_parent_escape = normalized.split('/').any(|component| component == "..");
    if looks_absolute || has_parent_escape
    {
        return Err(AppError::Tool(format!(
            "{} contains an unsafe entry path '{entry_path}' that would extract outside the package; refusing to extract",
            package.display()
        )));
    }
    return Ok(());
}

/// Strip a 7-Zip `-slt` property key case-insensitively (tolerant of casing and
/// leading-byte quirks) so listing parsing does not hinge on exact key casing.
fn strip_listing_key<'a>(line: &'a str, key: &str) -> Option<&'a str>
{
    let head = line.get(..key.len())?;
    return if head.eq_ignore_ascii_case(key)
    {
        Some(&line[key.len()..])
    }
    else
    {
        None
    };
}

fn seven_zip_extract_error(package: &Path, stderr: &[u8]) -> AppError
{
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
    return AppError::Tool(format!(
        "7-Zip failed to extract {}{}",
        package.display(),
        stderr_snippet(&text)
    ));
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
#[cfg(test)]
const PARALLEL_ZIP_TEST_FILE_COUNT: usize = 40;
#[cfg(test)]
const PARALLEL_ZIP_TEST_DIRECTORY_MODULUS: usize = 3;

/// Atomic so it can be shared (`&self`) across parallel extraction workers; the
/// cap is enforced globally even though per-entry byte limits read approximately.
struct ExtractBudget
{
    remaining_bytes: AtomicU64,
    remaining_entries: AtomicUsize,
}
