use crate::prelude::*;

const MAX_IMPORT_STAGING_COLLISIONS: usize = 1000;

pub(crate) fn import_package(package: &Path, options: &CommandOptions) -> Result<(), AppError>
{
    ensure_gta_install(&options.game_root)?;
    ensure_state(&options.game_root)?;
    let report = analyze_package(package, &options.game_root)?;
    let plan = build_install_plan(&report, options);
    let package_id = plan.package_id.clone();
    let library_root = state_directory(&options.game_root)
        .join("library")
        .join(&package_id);
    let source_root = library_root.join("source");
    fs::create_dir_all(&library_root)?;

    let staged_source = stage_import_source(package, &library_root, &package_id)?;
    replace_import_source(&source_root, &staged_source, &package_id)?;

    write_import_manifest(package, &library_root, &report, &plan)?;
    write_mod_config_json_with_source(&report, &plan, Some(&source_root))?;
    println!("imported: {package_id}");
    println!("source : {}", source_root.display());
    return Ok(());
}

fn stage_import_source(
    package: &Path,
    library_root: &Path,
    package_id: &str,
) -> Result<PathBuf, AppError>
{
    let staged_source = unique_import_staging_path(library_root, package_id);
    fs::create_dir_all(&staged_source)?;

    let import_result = if package.is_dir() {
        copy_tree_contents(package, &staged_source)
    } else {
        extract_archive_to_directory(package, &staged_source)
    };

    if let Err(err) = import_result
    {
        remove_directory_if_exists(&staged_source)?;
        return Err(err);
    }

    return Ok(staged_source);
}

fn unique_import_staging_path(library_root: &Path, package_id: &str) -> PathBuf
{
    let base = format!(".pending-source-{}-{}", safe_name(package_id), unix_now());
    for suffix in 0..MAX_IMPORT_STAGING_COLLISIONS
    {
        // literal: allow domain threshold is documented by the surrounding code
        let name = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        let candidate = library_root.join(name);
        if !candidate.exists()
        {
            return candidate;
        }
    }
    return library_root.join(format!("{base}-fallback"));
}

fn replace_import_source(
    source_root: &Path,
    staged_source: &Path,
    package_id: &str,
) -> Result<(), AppError>
{
    let replaced_source = replacement_backup_path(source_root, package_id);
    if replaced_source.exists()
    {
        remove_directory_if_exists(&replaced_source)?;
    }

    let had_previous_source = source_root.exists();
    if had_previous_source
    {
        fs::rename(source_root, &replaced_source)?;
    }

    return match fs::rename(staged_source, source_root) {
        Ok(()) => {
            remove_directory_if_exists(&replaced_source)?;
            Ok(())
        }
        Err(err) => {
            let previous_source = PreviousSourceState::from_exists(had_previous_source);
            restore_previous_source(source_root, &replaced_source, previous_source)?;
            Err(AppError::Io(err))
        }
    };
}

fn replacement_backup_path(source_root: &Path, package_id: &str) -> PathBuf
{
    let parent = source_root.parent().unwrap_or_else(|| Path::new("."));
    return parent.join(format!(
        ".replaced-source-{}-{}",
        safe_name(package_id),
        unix_now()
    ));
}

enum PreviousSourceState
{
    Present,
    Absent,
}

impl PreviousSourceState
{
    fn from_exists(exists: bool) -> Self
    {
        return if exists { Self::Present } else { Self::Absent };
    }
}
fn restore_previous_source(
    source_root: &Path,
    replaced_source: &Path,
    previous_source: PreviousSourceState,
) -> Result<(), AppError>
{
    if source_root.exists()
    {
        remove_directory_if_exists(source_root)?;
    }
    if matches!(previous_source, PreviousSourceState::Present) && replaced_source.exists()
    {
        fs::rename(replaced_source, source_root)?;
    }
    return Ok(());
}

fn write_import_manifest(
    package: &Path,
    library_root: &Path,
    report: &PackageReport,
    plan: &InstallPlan,
) -> Result<(), AppError>
{
    fs::create_dir_all(library_root)?;
    let import_manifest = library_root.join("import.json");
    let manifest = ImportManifest {
        version: 1,
        id: plan.package_id.clone(),
        package: package.display().to_string(),
        imported_unix: unix_now(),
        entry_count: report.entries.len(),
        operation_count: plan.operations.len(),
    };
    let mut text = serde_json::to_string_pretty(&manifest)
        .map_err(|err| AppError::Usage(format!("serialize import json: {err}")))?;
    text.push('\n');
    fs::write(&import_manifest, text)
        .with_context(|| format!("write import manifest {}", import_manifest.display()))?;
    return Ok(());
}

/// The `import.json` manifest recorded for each imported package. Serialized and
/// parsed with `serde_json` (like `mod.json`/`profile.json`) so the on-disk form
/// survives reformatting or minifying instead of relying on one-field-per-line
/// prefix matching.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct ImportManifest
{
    pub(crate) version: u32,
    pub(crate) id: String,
    pub(crate) package: String,
    pub(crate) imported_unix: u64,
    pub(crate) entry_count: usize,
    pub(crate) operation_count: usize,
}

/// Read an import manifest, tolerating any valid JSON layout (pretty, minified,
/// reordered) rather than a fixed line format.
pub(crate) fn read_import_manifest(path: &Path) -> Result<ImportManifest, AppError>
{
    let text = read_capped(path, MAX_CONTROL_FILE_BYTES)
        .with_context(|| format!("read import manifest {}", path.display()))?;
    return serde_json::from_str(&text)
        .map_err(|err| AppError::Usage(format!("invalid import json {}: {err}", path.display())));
}

pub(crate) fn target_template(kind: &TargetKind, package_id: &str) -> String
{
    return match kind {
        TargetKind::ModLoader => {
            format!(
                "modloader/{}",
                crate::settings::modloader_folder_name(package_id)
            )
        }
        TargetKind::Cleo => "CLEO".to_string(),
        TargetKind::CleoText => "CLEO/cleo_text".to_string(),
        TargetKind::CleoPlugin => "CLEO/cleo_plugins".to_string(),
        TargetKind::CleoModules => "CLEO/cleo_modules".to_string(),
        TargetKind::CleoSaves => "CLEO/cleo_saves".to_string(),
        TargetKind::Asi | TargetKind::Bootstrap => ".".to_string(),
        TargetKind::DirectManaged => ".".to_string(),
    };
}

fn remove_directory_if_exists(path: &Path) -> Result<(), AppError>
{
    if path.exists()
    {
        fs::remove_dir_all(path)?;
    }
    return Ok(());
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn read_import_manifest_tolerates_minified_and_reordered_json()
    {
        let root = test_root("import_manifest_minified");
        // Minified, fields reordered, whitespace stripped â€” the old
        // one-field-per-line parser would have yielded zeros for all of these.
        let path = root.join("import.json");
        fs::write(
            &path,
            r#"{"operation_count":7,"id":"minified_mod","imported_unix":1234,"version":1,"entry_count":9,"package":"pkg.7z"}"#,
        )
        .expect("the test fixture is created before this assertion reads it");

        let manifest = read_import_manifest(&path)
            .expect("the test fixture is created before this assertion reads it");

        assert_eq!(manifest.id, "minified_mod");
        assert_eq!(manifest.imported_unix, 1234); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(manifest.entry_count, 9); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(manifest.operation_count, 7); // literal: allow test fixture value is the specimen under judgment
        remove_directory_if_exists(&root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn replace_import_source_removes_stale_files()
    {
        let root = test_root("replace_removes_stale");
        let source = root.join("source");
        let staged = root.join(".pending-source-test");
        fs::create_dir_all(&source)
            .expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(&staged)
            .expect("the test fixture is created before this assertion reads it");
        fs::write(source.join("old_only.txt"), "old")
            .expect("the test fixture is created before this assertion reads it");
        fs::write(source.join("shared.txt"), "old")
            .expect("the test fixture is created before this assertion reads it");
        fs::write(staged.join("shared.txt"), "new")
            .expect("the test fixture is created before this assertion reads it");
        fs::write(staged.join("new_only.txt"), "new")
            .expect("the test fixture is created before this assertion reads it");

        replace_import_source(&source, &staged, "test_mod")
            .expect("the test fixture is created before this assertion reads it");

        assert!(!source.join("old_only.txt").exists());
        assert_eq!(
            fs::read_to_string(source.join("shared.txt"))
                .expect("the test fixture is created before this assertion reads it"),
            "new"
        );
        assert_eq!(
            fs::read_to_string(source.join("new_only.txt"))
                .expect("the test fixture is created before this assertion reads it"),
            "new"
        );
        assert!(!staged.exists());
        remove_directory_if_exists(&root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn replace_import_source_restores_old_source_when_new_source_cannot_move()
    {
        let root = test_root("replace_restores_old");
        let source = root.join("source");
        let staged = root.join(".missing-pending-source-test");
        fs::create_dir_all(&source)
            .expect("the test fixture is created before this assertion reads it");
        fs::write(source.join("existing.txt"), "old")
            .expect("the test fixture is created before this assertion reads it");

        let result = replace_import_source(&source, &staged, "test_mod");

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(source.join("existing.txt"))
                .expect("the test fixture is created before this assertion reads it"),
            "old"
        );
        remove_directory_if_exists(&root)
            .expect("the test fixture is created before this assertion reads it");
    }

    fn test_root(name: &str) -> PathBuf
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        remove_directory_if_exists(&root)
            .expect("the test fixture is created before this assertion reads it");
        fs::create_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
        return root;
    }
}
