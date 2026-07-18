use crate::prelude::*;

pub(crate) fn write_mod_config_json(
    report: &PackageReport,
    plan: &InstallPlan,
) -> Result<(), AppError> {
    write_mod_config_json_with_source(report, plan, None)
}

pub(crate) fn write_mod_config_json_with_source(
    report: &PackageReport,
    plan: &InstallPlan,
    source_root: Option<&Path>,
) -> Result<(), AppError> {
    ensure_state(&plan.game_root)?;
    let dir = state_directory(&plan.game_root)
        .join("mods") // literal: allow external interface text or file-format spelling
        .join(&plan.package_id);
    fs::create_dir_all(&dir)?;
    let config_path = dir.join("mod.json"); // literal: allow external interface text or file-format spelling
    let mut file = fs::File::create(&config_path)?;

    write_mod_config_header(&mut file, report, plan, source_root)?;
    write_mod_config_install_roots(&mut file, plan)?;
    writeln!(file, "  ],")?;
    write_mod_config_options(&mut file, report)?;
    writeln!(file, "  ],")?;
    write_mod_config_notes(&mut file, report)?;
    writeln!(file, "  ]")?;
    writeln!(file, "}}")?;

    println!("mod json: {}", config_path.display());
    Ok(())
}

pub(crate) fn update_mod_config_install_root(
    config_path: &Path,
    root_index: usize,
    root: &ModInstallRootJson,
) -> Result<(), AppError> {
    let text = fs::read_to_string(config_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&text).map_err(|err| {
        AppError::Usage(format!("invalid mod json {}: {err}", config_path.display()))
    })?;
    let roots = json
        .get_mut("install_roots")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            AppError::Usage(format!(
                "mod json missing install_roots array: {}",
                config_path.display()
            ))
        })?;
    if root_index >= roots.len() {
        return Err(AppError::Usage(format!(
            "install root index {root_index} out of range for {}",
            config_path.display()
        )));
    }
    roots[root_index] = serde_json::to_value(root)
        .map_err(|err| AppError::Tool(format!("failed to serialize install root: {err}")))?;
    let formatted = serde_json::to_string_pretty(&json)
        .map_err(|err| AppError::Tool(format!("failed to write mod json: {err}")))?;
    fs::write(config_path, format!("{formatted}\n"))?;
    Ok(())
}

/// Append an install root to an existing mod config, preserving every other
/// field. Returns `false` without writing when a root with the same
/// source/target/kind is already present, so accepting the same readme proposal
/// twice is a harmless no-op rather than a duplicate.
pub(crate) fn append_mod_config_install_root(
    config_path: &Path,
    root: &ModInstallRootJson,
) -> Result<bool, AppError> {
    let text = fs::read_to_string(config_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&text).map_err(|err| {
        AppError::Usage(format!("invalid mod json {}: {err}", config_path.display()))
    })?;
    let roots = json
        .get_mut("install_roots")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            AppError::Usage(format!(
                "mod json missing install_roots array: {}",
                config_path.display()
            ))
        })?;
    if roots.iter().any(|existing| install_root_matches(existing, root)) {
        return Ok(false);
    }
    let value = serde_json::to_value(root)
        .map_err(|err| AppError::Tool(format!("failed to serialize install root: {err}")))?;
    roots.push(value);
    let formatted = serde_json::to_string_pretty(&json)
        .map_err(|err| AppError::Tool(format!("failed to write mod json: {err}")))?;
    fs::write(config_path, format!("{formatted}\n"))?;
    Ok(true)
}

fn install_root_matches(existing: &serde_json::Value, root: &ModInstallRootJson) -> bool {
    let field = |key: &str| existing.get(key).and_then(serde_json::Value::as_str);
    field("source") == Some(root.source.as_str())
        && field("target") == Some(root.target.as_str())
        && field("kind") == Some(root.kind.as_str())
}

fn write_mod_config_header(
    file: &mut fs::File,
    report: &PackageReport,
    plan: &InstallPlan,
    source_root: Option<&Path>,
) -> Result<(), AppError> {
    writeln!(file, "{{")?;
    writeln!(file, "  \"version\": 1,")?;
    writeln!(file, "  \"id\": \"{}\",", json_escape(&plan.package_id))?;
    writeln!(
        file,
        "  \"name\": \"{}\",",
        json_escape(&plan.package_id.replace('_', " "))
    )?;
    writeln!(
        file,
        "  \"package\": \"{}\",",
        json_escape(&report.package.display().to_string())
    )?;
    if let Some(source_root) = source_root {
        writeln!(
            file,
            "  \"source_root\": \"{}\",",
            json_escape(&source_root.display().to_string())
        )?;
    }
    writeln!(file, "  \"enabled\": true,")?;
    writeln!(file, "  \"default_load_order\": 100,")?;
    writeln!(file, "  \"install_roots\": [")?;
    Ok(())
}

fn write_mod_config_install_roots(file: &mut fs::File, plan: &InstallPlan) -> Result<(), AppError> {
    for (idx, op) in plan.operations.iter().enumerate() {
        write_mod_config_install_root(file, plan, op, idx)?;
    }
    Ok(())
}

fn write_mod_config_install_root(
    file: &mut fs::File,
    plan: &InstallPlan,
    op: &InstallOperation,
    idx: usize,
) -> Result<(), AppError> {
    writeln!(file, "    {{")?;
    writeln!(
        file,
        "      \"source\": \"{}\",",
        json_escape(&op.source_root)
    )?;
    let target = target_template(&op.target_kind, &plan.package_id);
    writeln!(file, "      \"target\": \"{}\",", json_escape(&target))?;
    writeln!(file, "      \"kind\": \"{}\",", op.target_kind)?;
    writeln!(file, "      \"enabled\": true,")?;
    writeln!(file, "      \"optional\": {}", op.optional)?;
    write!(file, "    }}")?;
    let total_operations = plan.operations.len();
    write_json_comma_or_newline(file, idx, total_operations)
}

fn write_json_comma_or_newline(
    file: &mut fs::File,
    idx: usize,
    total: usize,
) -> Result<(), AppError> {
    if idx + 1 != total {
        writeln!(file, ",")?;
    } else {
        writeln!(file)?;
    }
    Ok(())
}

fn write_mod_config_options(file: &mut fs::File, report: &PackageReport) -> Result<(), AppError> {
    writeln!(file, "  \"available_options\": [")?;
    for (idx, option) in report.option_groups.iter().enumerate() {
        write!(file, "    \"{}\"", json_escape(option))?;
        if idx + 1 != report.option_groups.len() {
            writeln!(file, ",")?;
        } else {
            writeln!(file)?;
        }
    }
    Ok(())
}

fn write_mod_config_notes(file: &mut fs::File, report: &PackageReport) -> Result<(), AppError> {
    writeln!(file, "  \"notes\": [")?;
    let mut notes = Vec::new();
    let context_hints = report.context_hints.iter().cloned();
    notes.extend(context_hints);
    let risks = report.risks.iter().cloned();
    notes.extend(risks);
    let readme_insights = report.readme_insights.iter().map(|insight| {
        format!(
            "readme {} {:.0}% [{}] {} ({}:{}): {} | evidence: {}",
            insight.kind,
            insight.confidence * 100.0,
            insight.rule_id,
            insight.title,
            insight.source_readme,
            insight.line_number,
            insight.detail,
            insight.evidence
        )
    });
    notes.extend(readme_insights);
    notes.sort();
    notes.dedup();
    for (idx, note) in notes.iter().enumerate() {
        write!(file, "    \"{}\"", json_escape(note))?;
        if idx + 1 != notes.len() {
            writeln!(file, ",")?;
        } else {
            writeln!(file)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_mod_config_install_root_preserves_config_and_writes_optional() {
        let root = test_root("update_mod_config_root");
        let config_path = root.join("mod.json");
        fs::write(
            &config_path,
            r#"{
  "version": 1,
  "id": "test_mod",
  "package": "package.wrap",
  "enabled": true,
  "install_roots": [
    {
      "source": "old",
      "target": "modloader/old",
      "kind": "modloader",
      "enabled": true,
      "optional": false
    }
  ],
  "notes": ["keep me"]
}
"#,
        )
        .unwrap();
        let updated = ModInstallRootJson {
            source: "files/CLEO".to_string(),
            target: "CLEO".to_string(),
            kind: "cleo".to_string(),
            enabled: false,
            optional: true,
        };

        update_mod_config_install_root(&config_path, 0, &updated).unwrap();

        let parsed = read_mod_config_json(&config_path).unwrap();
        assert_eq!(parsed.install_roots[0].source, "files/CLEO");
        assert_eq!(parsed.install_roots[0].target, "CLEO");
        assert_eq!(parsed.install_roots[0].kind, "cleo");
        assert!(!parsed.install_roots[0].enabled);
        assert!(parsed.install_roots[0].optional);
        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("keep me"));
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn append_install_root_adds_once_and_dedups() {
        let root = test_root("append_mod_config_root");
        let config_path = root.join("mod.json");
        fs::write(
            &config_path,
            r#"{
  "version": 1,
  "id": "test_mod",
  "package": "package.wrap",
  "enabled": true,
  "install_roots": [
    {
      "source": "old",
      "target": "modloader/old",
      "kind": "modloader",
      "enabled": true,
      "optional": false
    }
  ],
  "notes": ["keep me"]
}
"#,
        )
        .unwrap();
        let added = ModInstallRootJson {
            source: "files/CLEO".to_string(),
            target: "CLEO".to_string(),
            kind: "cleo".to_string(),
            enabled: true,
            optional: false,
        };

        // First append writes and reports it added.
        assert!(append_mod_config_install_root(&config_path, &added).unwrap());
        let parsed = read_mod_config_json(&config_path).unwrap();
        assert_eq!(parsed.install_roots.len(), 2);
        assert_eq!(parsed.install_roots[1].source, "files/CLEO");
        // Other fields and the original root survive.
        assert_eq!(parsed.install_roots[0].source, "old");
        assert!(fs::read_to_string(&config_path).unwrap().contains("keep me"));

        // Re-appending the same source/target/kind is a no-op.
        assert!(!append_mod_config_install_root(&config_path, &added).unwrap());
        let reparsed = read_mod_config_json(&config_path).unwrap();
        assert_eq!(reparsed.install_roots.len(), 2);
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

    fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}
