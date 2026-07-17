use crate::prelude::*;

pub(crate) fn import_package(package: &Path, options: &CommandOptions) -> Result<(), AppError> {
    ensure_state(&options.game_root)?;
    let report = analyze_package(package, &options.game_root)?;
    let plan = build_install_plan(&report, options);
    let package_id = plan.package_id.clone();
    let library_root = state_directory(&options.game_root)
        .join("library") // literal: allow external interface text or file-format spelling
        .join(&package_id);
    let source_root = library_root.join("source"); // literal: allow external interface text or file-format spelling
    fs::create_dir_all(&library_root)?;

    let staged_source = stage_import_source(package, &library_root, &package_id)?;
    replace_import_source(&source_root, &staged_source, &package_id)?;

    write_import_manifest(package, &library_root, &report, &plan)?;
    write_mod_config_json_with_source(&report, &plan, Some(&source_root))?;
    println!("imported: {package_id}");
    println!("source : {}", source_root.display());
    Ok(())
}

fn stage_import_source(
    package: &Path,
    library_root: &Path,
    package_id: &str,
) -> Result<PathBuf, AppError> {
    let staged_source = unique_import_staging_path(library_root, package_id);
    fs::create_dir_all(&staged_source)?;

    let import_result = if package.is_dir() {
        copy_tree_contents(package, &staged_source)
    } else {
        extract_archive_to_directory(package, &staged_source)
    };

    if let Err(err) = import_result {
        remove_dir_if_exists(&staged_source)?;
        return Err(err);
    }

    Ok(staged_source)
}

fn unique_import_staging_path(library_root: &Path, package_id: &str) -> PathBuf {
    let base = format!(".pending-source-{}-{}", safe_name(package_id), unix_now());
    for suffix in 0..1000 {
        let name = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        let candidate = library_root.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    library_root.join(format!("{base}-fallback"))
}

fn replace_import_source(
    source_root: &Path,
    staged_source: &Path,
    package_id: &str,
) -> Result<(), AppError> {
    let replaced_source = replacement_backup_path(source_root, package_id);
    if replaced_source.exists() {
        remove_dir_if_exists(&replaced_source)?;
    }

    let had_previous_source = source_root.exists();
    if had_previous_source {
        fs::rename(source_root, &replaced_source)?;
    }

    match fs::rename(staged_source, source_root) {
        Ok(()) => {
            remove_dir_if_exists(&replaced_source)?;
            Ok(())
        }
        Err(err) => {
            restore_previous_source(source_root, &replaced_source, had_previous_source)?;
            Err(AppError::Io(err))
        }
    }
}

fn replacement_backup_path(source_root: &Path, package_id: &str) -> PathBuf {
    let parent = source_root.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(
        ".replaced-source-{}-{}",
        safe_name(package_id),
        unix_now()
    ))
}

fn restore_previous_source(
    source_root: &Path,
    replaced_source: &Path,
    had_previous_source: bool,
) -> Result<(), AppError> {
    if source_root.exists() {
        remove_dir_if_exists(source_root)?;
    }
    if had_previous_source && replaced_source.exists() {
        fs::rename(replaced_source, source_root)?;
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn write_import_manifest(
    package: &Path,
    library_root: &Path,
    report: &PackageReport,
    plan: &InstallPlan,
) -> Result<(), AppError> {
    fs::create_dir_all(library_root)?;
    let import_manifest = library_root.join("import.json"); // literal: allow external interface text or file-format spelling
    let mut file = fs::File::create(import_manifest)?;
    writeln!(file, "{{")?;
    writeln!(file, "  \"version\": 1,")?;
    writeln!(file, "  \"id\": \"{}\",", json_escape(&plan.package_id))?;
    writeln!(
        file,
        "  \"package\": \"{}\",",
        json_escape(&package.display().to_string())
    )?;
    writeln!(file, "  \"imported_unix\": {},", unix_now())?;
    writeln!(file, "  \"entry_count\": {},", report.entries.len())?;
    writeln!(file, "  \"operation_count\": {}", plan.operations.len())?;
    writeln!(file, "}}")?;
    Ok(())
}

pub(crate) fn target_template(kind: &TargetKind, package_id: &str) -> String {
    match kind {
        TargetKind::ModLoader => format!("modloader/100_{package_id}"),
        TargetKind::Cleo => "CLEO".to_string(), // literal: allow external interface text or file-format spelling
        TargetKind::Asi | TargetKind::Bootstrap => ".".to_string(), // literal: allow external interface text or file-format spelling
        TargetKind::DirectManaged => ".".to_string(), // literal: allow external interface text or file-format spelling
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_import_source_removes_stale_files() {
        let root = test_root("replace_removes_stale");
        let source = root.join("source");
        let staged = root.join(".pending-source-test");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&staged).unwrap();
        fs::write(source.join("old_only.txt"), "old").unwrap();
        fs::write(source.join("shared.txt"), "old").unwrap();
        fs::write(staged.join("shared.txt"), "new").unwrap();
        fs::write(staged.join("new_only.txt"), "new").unwrap();

        replace_import_source(&source, &staged, "test_mod").unwrap();

        assert!(!source.join("old_only.txt").exists());
        assert_eq!(
            fs::read_to_string(source.join("shared.txt")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(source.join("new_only.txt")).unwrap(),
            "new"
        );
        assert!(!staged.exists());
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn replace_import_source_restores_old_source_when_new_source_cannot_move() {
        let root = test_root("replace_restores_old");
        let source = root.join("source");
        let staged = root.join(".missing-pending-source-test");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("existing.txt"), "old").unwrap();

        let result = replace_import_source(&source, &staged, "test_mod");

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(source.join("existing.txt")).unwrap(),
            "old"
        );
        remove_dir_if_exists(&root).unwrap();
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
}
