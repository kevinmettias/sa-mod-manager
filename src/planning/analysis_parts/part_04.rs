
fn mentions_game_root(line: &str) -> bool
{
    return line.contains("game root")
        || line.contains("root folder")
        || line.contains("root directory")
        || line.contains("main folder")
        || line.contains("main directory")
        || line.contains("install folder")
        || line.contains("installation folder")
        || line.contains("install directory")
        || line.contains("installation directory")
        || line.contains("game folder")
        || line.contains("game directory")
        || line.contains("gta folder")
        || line.contains("gta directory")
        || line.contains("gta sa folder")
        || line.contains("gta sa directory")
        || line.contains("gta san andreas folder")
        || line.contains("gta san andreas directory")
        || line.contains("folder with the game")
        || line.contains("folder where gta sa exe")
        || line.contains("directory where gta sa exe")
        || line.contains("folder with gta sa exe")
        || line.contains("gta sa exe")
        || line.contains("carpeta del juego")
        || line.contains("directorio del juego")
        || line.contains("pasta do jogo")
        || line.contains("diretorio do jogo")
        || line.contains("folderu gry")
        || line.contains("katalogu gry")
        || line.contains("papku s igroy")
        || line.contains("papka igry");
}

struct GameDataFolderMention<'a>
{
    line: &'a str,
    folder: &'a str,
}

fn mentions_game_data_folder(mention: GameDataFolderMention<'_>) -> bool
{
    let line = mention.line;
    let folder = mention.folder;
    return line_contains_token(LineNeedle { line: line, needle: folder })
        && (line.contains("folder") || line.contains("directory") || line.contains("copy"));
}

struct LineNeedle<'a>
{
    line: &'a str,
    needle: &'a str,
}

fn line_contains_path(line_path: LinePath<'_>) -> bool
{
    let line = line_path.line;
    let path = line_path.path;
    if line_contains_token(LineNeedle { line: line, needle: path })
    {
        return true;
    }
    let path_words = path.replace('/', " ");
    return line.contains(&path_words);
}

struct LinePath<'a>
{
    line: &'a str,
    path: &'a str,
}

fn add_readme_context_hints(text: &str, hints: &mut BTreeSet<String>)
{
    let lower = text.to_ascii_lowercase();
    if lower.contains("cleo")
    {
        hints.insert("readme mentions CLEO install requirements".to_string());
    }
    if lower.contains("modloader") || lower.contains("mod loader")
    {
        hints.insert("readme mentions Mod Loader install support".to_string());
    }
    if lower.contains("asi loader") || lower.contains(".asi")
    {
        hints.insert("readme mentions ASI/plugin install requirements".to_string());
    }
    if lower.contains("copy") && lower.contains("root")
    {
        hints.insert("readme mentions copying files to the game root".to_string());
    }
}

fn collect_wrap_manifest_roots(report: &mut PackageReport) -> Result<(), AppError>
{
    let Some(manifest_path) = wrap_manifest_path(report) else {
        if matches!(report.kind, PackageKind::Wrap)
        {
            return Err(AppError::Usage(
                ".wrap package must contain wrap.json, manifest.json, or package.wrap.json"
                    .to_string(),
            ));
        }
        return Ok(());
    };
    let Some(text) =
        read_package_text_file(&report.package, &manifest_path, MANIFEST_README_MAX_BYTES)?
    else {
        return Ok(());
    };
    let manifest: WrapManifestFile = serde_json::from_str(&text)
        .map_err(|err| AppError::Usage(format!("invalid .wrap manifest {manifest_path}: {err}")))?;
    validate_wrap_manifest(report, &manifest_path, &manifest)?;
    let roots = manifest
        .install_roots
        .into_iter()
        .map(manifest_install_root_from_json)
        .collect();
    report.manifest_roots = roots;
    if !report.manifest_roots.is_empty()
    {
        report
            .context_hints
            .insert(format!("using explicit .wrap manifest: {manifest_path}"));
    }
    return Ok(());
}

fn wrap_manifest_path(report: &PackageReport) -> Option<String>
{
    let known = ["wrap.json", "manifest.json", "package.wrap.json"];
    return report.entries.iter().find_map(|entry| {
        if entry.is_dir
        {
            return None;
        }
        let normalized = normalize_path(&entry.path);
        let lower = normalized.to_ascii_lowercase();
        if known
            .iter()
            .any(|name| lower == *name || lower.ends_with(&format!("/{name}")))
        {
            Some(normalized)
        }
        else
        {
            None
        }
    });
}

fn validate_wrap_manifest(
    report: &PackageReport,
    manifest_path: &str,
    manifest: &WrapManifestFile,
) -> Result<(), AppError>
{
    if manifest.install_roots.is_empty()
    {
        return Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} must define at least one install_roots entry"
        )));
    }
    for (idx, root) in manifest.install_roots.iter().enumerate()
    {
        validate_wrap_install_root(report, manifest_path, idx, root)?;
    }
    return Ok(());
}

fn validate_wrap_install_root(
    report: &PackageReport,
    manifest_path: &str,
    idx: usize,
    root: &WrapInstallRootFile,
) -> Result<(), AppError>
{
    let source = normalize_path(&root.source);
    let target = normalize_path(&root.target);
    validate_wrap_relative_path(WrapRelativePathField { manifest_path, idx, field: "source", value: &source })?;
    validate_wrap_relative_path(WrapRelativePathField { manifest_path, idx, field: "target", value: &target })?;
    validate_wrap_kind(manifest_path, idx, &root.kind)?;
    path_from_package_root(&target).map_err(|err| {
        AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].target is invalid: {err}"
        ))
    })?;
    if !wrap_source_exists(report, &source)
    {
        return Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].source does not exist in package: {source}"
        )));
    }
    return Ok(());
}

struct WrapRelativePathField<'a>
{
    manifest_path: &'a str,
    idx: usize,
    field: &'a str,
    value: &'a str,
}

fn validate_wrap_relative_path(path_field: WrapRelativePathField<'_>) -> Result<(), AppError>
{
    let manifest_path = path_field.manifest_path;
    let idx = path_field.idx;
    let field = path_field.field;
    let value = path_field.value;
    if value.trim().is_empty()
    {
        return Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].{field} must not be empty"
        )));
    }
    let path = Path::new(value);
    if path.is_absolute() || value.starts_with('/') || contains_parent_segment(value)
    {
        return Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].{field} must be a relative package path: {value}"
        )));
    }
    return Ok(());
}

fn contains_parent_segment(value: &str) -> bool
{
    return value.split('/').any(|part| part == "..");
}

fn validate_wrap_kind(manifest_path: &str, idx: usize, kind: &str) -> Result<(), AppError>
{
    let normalized = kind.to_ascii_lowercase();
    return if matches!(
        normalized.as_str(),
        "modloader"
            | "cleo"
            | "cleo_text"
            | "cleo_plugins"
            | "cleo_plugin"
            | "cleo_modules"
            | "cleo_module"
            | "cleo_saves"
            | "cleo_save"
            | "asi"
            | "plugin"
            | "bootstrap"
            | "runtime"
            | "direct"
            | "directmanaged"
            | "direct_managed"
    )
    {
        Ok(())
    }
    else
    {
        Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].kind is unsupported: {kind}"
        )))
    };
}

fn wrap_source_exists(report: &PackageReport, source: &str) -> bool
{
    if source == "."
    {
        return true;
    }
    return report.entries.iter().any(|entry| {
        let path = normalize_path(&entry.path);
        path == source || path.starts_with(&format!("{source}/"))
    });
}

fn default_manifest_kind() -> String
{
    return "modloader".to_string();
}

#[derive(Deserialize)]
struct WrapManifestFile
{
    #[serde(default, alias = "install")]
    install_roots: Vec<WrapInstallRootFile>,
}

#[derive(Deserialize)]
struct WrapInstallRootFile
{
    source: String,
    target: String,
    #[serde(default = "default_manifest_kind")]
    kind: String,
    #[serde(default)]
    optional: bool,
    #[serde(default = "default_manifest_enabled")]
    enabled: bool,
    #[serde(default)]
    notes: Vec<String>,
}

fn default_manifest_enabled() -> bool
{
    return true;
}

fn manifest_install_root_from_json(root: WrapInstallRootFile) -> ManifestInstallRoot
{
    return ManifestInstallRoot {
        source: normalize_path(&root.source),
        target: normalize_path(&root.target),
        kind: target_kind_from_manifest(&root.kind),
        optional: root.optional,
        enabled: root.enabled,
        notes: root.notes,
    };
}

fn target_kind_from_manifest(kind: &str) -> TargetKind
{
    return match kind.to_ascii_lowercase().as_str()
    {
        "cleo" => TargetKind::Cleo,
        "cleo_text" => TargetKind::CleoText,
        "cleo_plugins" | "cleo_plugin" => TargetKind::CleoPlugin,
        "cleo_modules" | "cleo_module" => TargetKind::CleoModules,
        "cleo_saves" | "cleo_save" => TargetKind::CleoSaves,
        "asi" | "plugin" => TargetKind::Asi,
        "bootstrap" | "runtime" => TargetKind::Bootstrap,
        "direct" | "directmanaged" | "direct_managed" => TargetKind::DirectManaged,
        _ => TargetKind::ModLoader,
    };
}

fn classify_entries(report: &mut PackageReport)
{
    let mut roots: BTreeMap<String, InstallCandidate> = BTreeMap::new();
    let mut state = EntryClassificationState::default();

    for entry in &report.entries
    {
        classify_package_entry(entry, &mut roots, &mut state);
    }

    report.readmes = state.readmes;
    report.components = state.components;
    report.context_hints = state.context_hints;
    report.risks = state.risks;
    report.install_candidates = roots.into_values().collect();
    report.option_groups = state.option_roots.into_iter().collect();
}

fn classify_package_entry(
    entry: &PackageEntry,
    roots: &mut BTreeMap<String, InstallCandidate>,
    state: &mut EntryClassificationState,
)
{
    let path = normalize_path(&entry.path);
    let lower = path.to_ascii_lowercase();
    let name = file_name(&lower);

    if !entry.is_dir && is_readme_name(name)
    {
        let readme_path = entry.path.clone();
        state.readmes.push(readme_path);
    }

    classify_component(&lower, &mut state.components);
    classify_context(&lower, &mut state.context_hints);
    classify_risk(&lower, &mut state.risks);
    classify_install_candidate(entry, &path, roots, state);
}

fn classify_install_candidate(
    entry: &PackageEntry,
    path: &str,
    roots: &mut BTreeMap<String, InstallCandidate>,
    state: &mut EntryClassificationState,
)
{
    if entry.is_dir
    {
        if let Some(option) = detect_option_group(path)
        {
            state.option_roots.insert(option);
        }
        return;
    }

    if let Some(candidate) = detect_install_candidate(path, entry.size)
    {
        merge_install_candidate(roots, candidate);
    }
}

fn merge_install_candidate(
    roots: &mut BTreeMap<String, InstallCandidate>,
    candidate: InstallCandidate,
)
{
    let key = candidate.source_root.clone();
    roots
        .entry(key)
        .and_modify(|existing| {
            existing.file_count += candidate.file_count;
            existing.total_bytes += candidate.total_bytes;
            let components = candidate.components.clone();
            existing.components.extend(components);
            let notes = candidate.notes.clone();
            existing.notes.extend(notes);
        })
        .or_insert(candidate);
}

fn line_contains_token(needle: LineNeedle<'_>) -> bool
{
    let line = needle.line;
    let token = needle.needle;
    return line.split_whitespace().any(|part| part == token) || line.contains(&format!(" {token} "));
}

fn list_folder_entries_inner(
    root: &Path,
    dir: &Path,
    entries: &mut Vec<PackageEntry>,
) -> Result<(), AppError>
{
    for entry in fs::read_dir(dir)?
    {
        let entry = entry?;
        let path = push_folder_entry(root, &entry, entries)?;
        let metadata = entry.metadata()?;
        if metadata.is_dir()
        {
            list_folder_entries_inner(root, &path, entries)?;
        }
    }
    return Ok(());
}

fn push_folder_entry(
    root: &Path,
    entry: &fs::DirEntry,
    entries: &mut Vec<PackageEntry>,
) -> Result<PathBuf, AppError>
{
    let path = entry.path();
    let metadata = entry.metadata()?;
    let package_entry = package_entry_from_folder(root, &path, &metadata);
    entries.push(package_entry);
    return Ok(path);
}

fn package_entry_from_folder(root: &Path, path: &Path, metadata: &fs::Metadata) -> PackageEntry
{
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let size = if metadata.is_file() {
        metadata.len()
    } else {
        0
    };
    return PackageEntry {
        path: rel,
        size,
        is_dir: metadata.is_dir(),
    };
}

#[cfg(test)]
mod tests
{
    include!("part_04_tests_01.rs");
    include!("part_04_tests_02.rs");
}



