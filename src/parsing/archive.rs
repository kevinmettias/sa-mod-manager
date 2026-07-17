use crate::prelude::*;
use std::io::Read;

pub(crate) fn extract_archive_to_named_staging(
    package: &Path,
    game_root: &Path,
    package_id: &str,
) -> Result<PathBuf, AppError> {
    let target = state_directory(game_root).join("staging").join(package_id); // literal: allow external interface text or file-format spelling
    extract_archive_to_directory(package, &target)?;
    Ok(target)
}

pub(crate) fn extract_archive_to_directory(package: &Path, target: &Path) -> Result<(), AppError> {
    match archive_backend(package) {
        ArchiveBackend::NativeZip => return extract_zip_to_directory(package, target),
        ArchiveBackend::SevenZip => {}
    }
    let seven_zip = find_seven_zip().ok_or_else(|| missing_7zip_error_for_package(package))?;
    fs::create_dir_all(target)?;
    let output = Command::new(seven_zip)
        .arg("x") // literal: allow external interface text or file-format spelling
        .arg("-y") // literal: allow external interface text or file-format spelling
        .arg(format!("-o{}", target.display()))
        .arg(package)
        .output()?;
    if !output.status.success() {
        return Err(extract_failed_error(package));
    }
    Ok(())
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
    let mut entries = Vec::new();
    for idx in 0..archive.len() {
        let file = archive
            .by_index(idx)
            .map_err(|err| AppError::Tool(format!("failed to read zip entry: {err}")))?;
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

pub(crate) fn list_archive_failed_error(package: &Path) -> AppError {
    let message = format!("7-Zip failed to list {}", package.display());
    AppError::Tool(message)
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

fn extract_failed_error(package: &Path) -> AppError {
    let message = format!("7-Zip failed to extract {}", package.display());
    AppError::Tool(message)
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

fn extract_zip_to_directory(package: &Path, target: &Path) -> Result<(), AppError> {
    fs::create_dir_all(target)?;
    let file = fs::File::open(package)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| {
        AppError::Tool(format!("failed to read zip {}: {err}", package.display()))
    })?;
    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|err| AppError::Tool(format!("failed to read zip entry: {err}")))?;
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        let dest = target.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(dest)?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(dest)?;
        io::copy(&mut entry, &mut out)?;
    }
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
    let bytes = fs::read(path)?;
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
    let output = Command::new(seven_zip)
        .arg("e")
        .arg("-so")
        .arg(package)
        .arg(entry_path)
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(text_from_bytes(&output.stdout, max_bytes)))
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
