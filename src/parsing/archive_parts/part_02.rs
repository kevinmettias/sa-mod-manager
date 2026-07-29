
impl ExtractBudget
{
    fn take_entry(&self, package: &Path) -> Result<(), AppError>
    {
        return self.remaining_entries
            // atomic-ordering: allow: the budget counter is the shared state; it publishes no payload
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_sub(1)
            })
            .map(|_| ())
            .map_err(|err| too_many_entries_error(package).context(format!("update archive entry budget: {err:?}")));
    }

    fn take_bytes(&self, count: u64, package: &Path) -> Result<(), AppError>
    {
        return self.remaining_bytes
            // atomic-ordering: allow: the byte budget is only an atomic allowance counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_sub(count)
            })
            .map(|_| ())
            .map_err(|err| archive_too_large_error(package).context(format!("update archive byte budget: {err:?}")));
    }

    fn remaining_bytes(&self) -> u64
    {
        // atomic-ordering: allow: diagnostics read the counter without synchronizing other state
        return self.remaining_bytes.load(Ordering::Relaxed);
    }

    fn remaining_entries(&self) -> usize
    {
        // atomic-ordering: allow: diagnostics read the counter without synchronizing other state
        return self.remaining_entries.load(Ordering::Relaxed);
    }

    /// Draw down the budget by an already-validated amount (used by the 7-Zip
    /// path after its listing has been checked against the remaining budget).
    /// Saturating, since the caller has already refused anything over budget.
    fn charge(&self, bytes: u64, entries: usize)
    {
        self.remaining_bytes
            // atomic-ordering: allow: charging adjusts only the budget counter, with no payload
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(bytes))
            })
            .ok();
        self.remaining_entries
            // atomic-ordering: allow: charging adjusts only the entry counter, with no payload
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(entries))
            })
            .ok();
    }

    fn new() -> Self
    {
        return Self::with_limits(MAX_EXTRACT_BYTES, MAX_EXTRACT_ENTRIES);
    }

    fn with_limits(bytes: u64, entries: usize) -> Self
    {
        return Self {
            remaining_bytes: AtomicU64::new(bytes),
            remaining_entries: AtomicUsize::new(entries),
        };
    }
}

fn archive_too_large_error(package: &Path) -> AppError
{
    return AppError::Tool(format!(
        "{} expands past the {} GiB extraction limit; refusing to continue",
        package.display(),
        MAX_EXTRACT_BYTES / (1024 * 1024 * 1024) // literal: allow external format or runtime boundary value means itself here
    ));
}

fn too_many_entries_error(package: &Path) -> AppError
{
    return AppError::Tool(format!(
        "{} exceeds the {}-entry extraction limit; refusing to continue",
        package.display(),
        MAX_EXTRACT_ENTRIES
    ));
}

fn archive_backend(package: &Path) -> ArchiveBackend
{
    return if native_zip_supported(package)
    {
        ArchiveBackend::NativeZip
    }
    else
    {
        ArchiveBackend::SevenZip
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchiveBackend
{
    NativeZip,
    SevenZip,
}

fn native_zip_supported(package: &Path) -> bool
{
    return package_extension(package)
        .map(|ext| ext == "zip" || ext == "wrap")
        .unwrap_or(false);
}

fn package_extension(package: &Path) -> Option<String>
{
    return package
        .extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.to_ascii_lowercase());
}

fn seven_zip_missing_message(package: &str) -> String
{
    return format!(
        "7-Zip was not found for {package}. Native .zip and .wrap packages work without 7-Zip; .7z and .rar need 7-Zip, 7zz, or 7za on PATH, or set SA_MOD_MANAGER_7Z to a portable 7-Zip executable."
    );
}

fn extract_zip_to_directory(
    package: &Path,
    target: &Path,
    budget: &ExtractBudget,
) -> Result<(), AppError>
{
    fs::create_dir_all(target)?;
    let count = open_zip(package)?.len();

    let context = ZipExtractContext {
        package,
        target,
        budget,
    };
    let workers = extraction_worker_count(count);
    if workers <= 1
    {
        let mut archive = open_zip(package)?;
        for idx in 0..count
        {
            extract_zip_entry(&mut archive, idx, context)?;
        }
        return Ok(());
    }

    // Each worker opens its own archive handle (`ZipArchive` isn't `Sync`) and
    // pulls entry indices from a shared atomic dispenser. deflate decompression
    // is CPU-bound, so this scales extraction across cores.
    let state = ZipExtractWorkerState {
        count,
        next: AtomicUsize::new(0),
        first_error: Mutex::new(None),
    };
    std::thread::scope(|scope| {
        for _ in 0..workers
        {
            scope.spawn(|| {
                extract_zip_worker(context, &state);
            });
        }
    });
    return match state
        .first_error
        .into_inner()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    {
        Some(err) => Err(err),
        None => Ok(()),
    };
}

fn extraction_worker_count(entry_count: usize) -> usize
{
    if entry_count <= 1
    {
        return 1;
    }
    let cpus = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    return cpus.clamp(1, MAX_EXTRACT_WORKERS).min(entry_count);
}

fn extract_zip_worker(context: ZipExtractContext<'_>, state: &ZipExtractWorkerState)
{
    let mut archive = match open_zip(context.package) {
        Ok(archive) => archive,
        Err(err) => {
            record_first_error(&state.first_error, err);
            return;
        }
    };
    // unbounded-loop: allow: worker stops when another worker records an error or the shared index reaches the zip entry count
    loop
    {
        if state
            .first_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
        {
            break;
        }
        // atomic-ordering: allow: worker index allocation needs uniqueness, not memory synchronization
        let idx = state.next.fetch_add(1, Ordering::Relaxed);
        if idx >= state.count
        {
            break;
        }
        if let Err(err) = extract_zip_entry(&mut archive, idx, context)
        {
            record_first_error(&state.first_error, err);
            break;
        }
    }
}

fn record_first_error(slot: &Mutex<Option<AppError>>, err: AppError)
{
    let mut guard = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none()
    {
        *guard = Some(err);
    }
}

#[derive(Clone, Copy)]
struct ZipExtractContext<'a>
{
    package: &'a Path,
    target: &'a Path,
    budget: &'a ExtractBudget,
}

struct ZipExtractWorkerState
{
    count: usize,
    next: AtomicUsize,
    first_error: Mutex<Option<AppError>>,
}
fn read_folder_text_file(
    package: &Path,
    entry_path: &str,
    max_bytes: usize,
) -> Result<Option<String>, AppError>
{
    let rel = path_from_package_root(entry_path)?;
    let path = package.join(rel);
    if !path.exists() || !path.is_file()
    {
        return Ok(None);
    }
    // Read only up to the cap rather than the whole file, which may be large.
    let file = fs::File::open(&path)?;
    let mut bytes = Vec::new();
    file.take(max_bytes as u64).read_to_end(&mut bytes)?;
    return Ok(Some(text_from_bytes(&bytes, max_bytes)));
}

fn read_zip_text_file(
    package: &Path,
    entry_path: &str,
    max_bytes: usize,
) -> Result<Option<String>, AppError>
{
    let file = fs::File::open(package)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| {
        AppError::Tool(format!("failed to read zip {}: {err}", package.display()))
    })?;
    let mut entry = match archive.by_name(entry_path) {
        Ok(entry) => entry,
        Err(_) => return Ok(None),
    };
    if entry.is_dir()
    {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    let limit = max_bytes.saturating_add(1) as u64;
    entry.by_ref().take(limit).read_to_end(&mut bytes)?;
    return Ok(Some(text_from_bytes(&bytes, max_bytes)));
}

fn read_external_archive_text_file(
    package: &Path,
    entry_path: &str,
    max_bytes: usize,
) -> Result<Option<String>, AppError>
{
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
    if let Some(mut stdout) = child.stdout.take()
    {
        stdout
            .by_ref()
            .take(max_bytes as u64)
            .read_to_end(&mut buffer)?;
    }
    if let Err(err) = child.kill()
    {
        log_warn!("could not stop 7-Zip preview process: {err}");
    }
    if let Err(err) = child.wait()
    {
        log_warn!("could not wait for 7-Zip preview process: {err}");
    }

    return if buffer.is_empty()
    {
        Ok(None)
    }
    else
    {
        Ok(Some(text_from_bytes(&buffer, max_bytes)))
    };
}

fn is_symlink_mode(mode: Option<u32>) -> bool
{
    return mode.map(|value| value & 0o170000 == 0o120000) // literal: allow external format or runtime boundary value means itself here
        .unwrap_or(false);
}

fn open_zip(package: &Path) -> Result<zip::ZipArchive<fs::File>, AppError>
{
    let file = fs::File::open(package)?;
    return zip::ZipArchive::new(file)
        .map_err(|err| AppError::Tool(format!("failed to read zip {}: {err}", package.display())));
}

fn extract_zip_entry(
    archive: &mut zip::ZipArchive<fs::File>,
    idx: usize,
    context: ZipExtractContext<'_>,
) -> Result<(), AppError>
{
    context.budget.take_entry(context.package)?;
    let mut entry = archive
        .by_index(idx)
        .map_err(|err| zip_entry_error(context.package, &err))?;
    let raw_name = entry.name().to_string();
    let Some(enclosed) = entry.enclosed_name() else {
        // A traversal/absolute entry is dropped rather than extracted; surface
        // it so a partially-skipped archive isn't silently reported as complete.
        log_warn!("skipped unsafe zip entry path: {raw_name}");
        return Ok(());
    };
    // Skip symlink entries: materializing one would let a later entry write
    // through the link to a path outside the extraction target (zip-slip).
    if is_symlink_mode(entry.unix_mode())
    {
        log_warn!("skipped symlink zip entry: {raw_name}");
        return Ok(());
    }
    let dest = context.target.join(enclosed);
    if entry.is_dir()
    {
        fs::create_dir_all(dest)?;
        return Ok(());
    }
    if let Some(parent) = dest.parent()
    {
        fs::create_dir_all(parent)?;
    }
    let mut out = fs::File::create(dest)?;
    // Bound the *actual* decompressed bytes (not the header-declared size) so a
    // zip bomb cannot exhaust the disk: read at most one byte past the budget,
    // then charge what was written, which fails once the limit is crossed.
    let limit = context.budget.remaining_bytes().saturating_add(1);
    let written = io::copy(&mut entry.by_ref().take(limit), &mut out)?;
    context.budget.take_bytes(written, context.package)?;
    return Ok(());
}

fn text_from_bytes(bytes: &[u8], max_bytes: usize) -> String
{
    let end = bytes.len().min(max_bytes);
    return String::from_utf8_lossy(&bytes[..end]).to_string();
}

fn collect_files_recursive_inner(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), AppError>
{
    for entry in fs::read_dir(dir)?
    {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir()
        {
            collect_files_recursive_inner(&path, files)?;
        }
        else if metadata.is_file()
        {
            files.push(path);
        }
    }
    return Ok(());
}

#[cfg(test)]
mod tests
{
    include!("part_02_tests_01.rs");
}


