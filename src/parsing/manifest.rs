use crate::prelude::*;

pub(crate) fn write_package_manifest(
    report: &PackageReport,
    plan: &InstallPlan,
) -> Result<(), AppError> {
    ensure_state(&plan.game_root)?;
    let root = state_directory(&plan.game_root);
    let manifest_path = root
        .join("packages")
        .join(format!("{}.manifest", plan.package_id));
    let plan_path = package_plan_path(&root, plan);
    write_manifest_file(&manifest_path, report, plan)?;
    write_plan_file(&plan_path, plan)?;

    Ok(())
}

fn package_plan_path(root: &Path, plan: &InstallPlan) -> PathBuf {
    root.join("plans").join(format!(
        "{}-{}-{}.plan",
        safe_name(&plan.profile),
        plan.package_id,
        unix_now()
    ))
}

fn write_manifest_file(
    manifest_path: &Path,
    report: &PackageReport,
    plan: &InstallPlan,
) -> Result<(), AppError> {
    let mut manifest = fs::File::create(manifest_path)?;
    writeln!(manifest, "version=1")?;
    writeln!(manifest, "id={}", escape_value(&plan.package_id))?;
    writeln!(
        manifest,
        "package={}",
        escape_value(&report.package.display().to_string())
    )?;
    writeln!(manifest, "kind={}", report.kind)?;
    writeln!(manifest, "entries={}", report.entries.len())?;
    write_manifest_lists(&mut manifest, report, plan)?;
    Ok(())
}

fn write_manifest_lists(
    manifest: &mut fs::File,
    report: &PackageReport,
    plan: &InstallPlan,
) -> Result<(), AppError> {
    for component in &report.components {
        writeln!(
            manifest,
            "component={}",
            escape_value(&component.to_string())
        )?;
    }
    for readme in &report.readmes {
        writeln!(manifest, "readme={}", escape_value(readme))?;
    }
    for document in &report.readme_documents {
        writeln!(
            manifest,
            "readme_text={}|{}",
            escape_value(&document.path),
            escape_value(&document.text)
        )?;
    }
    for instruction in &report.readme_instructions {
        writeln!(
            manifest,
            "readme_instruction={}|{}|{}|{}|{}|{}|{}",
            escape_value(&instruction.source_readme),
            instruction.line_number,
            instruction.action,
            escape_value(instruction.source.as_deref().unwrap_or("")),
            escape_value(instruction.target.as_deref().unwrap_or("")),
            instruction.confidence,
            escape_value(&instruction.text)
        )?;
    }
    for root in &report.manifest_roots {
        writeln!(
            manifest,
            "manifest_root={}|{}|{}|{}|{}",
            escape_value(&root.source),
            escape_value(&root.target),
            root.kind,
            root.optional,
            root.enabled
        )?;
    }
    for option in &report.option_groups {
        writeln!(manifest, "option={}", escape_value(option))?;
    }
    for warning in &plan.warnings {
        writeln!(manifest, "warning={}", escape_value(warning))?;
    }
    Ok(())
}

fn write_plan_file(plan_path: &Path, plan: &InstallPlan) -> Result<(), AppError> {
    let mut plan_file = fs::File::create(plan_path)?;
    writeln!(plan_file, "version=1")?;
    writeln!(plan_file, "package_id={}", escape_value(&plan.package_id))?;
    writeln!(
        plan_file,
        "package={}",
        escape_value(&plan.package.display().to_string())
    )?;
    writeln!(plan_file, "profile={}", escape_value(&plan.profile))?;
    writeln!(plan_file, "created_unix={}", unix_now())?;
    write_plan_lists(&mut plan_file, plan)?;
    Ok(())
}

fn write_plan_lists(plan_file: &mut fs::File, plan: &InstallPlan) -> Result<(), AppError> {
    for operation in &plan.operations {
        writeln!(
            plan_file,
            "op={}|{}|{}|{}|{}",
            escape_value(&operation.source_root),
            operation.target_kind,
            escape_value(&operation.target_root.display().to_string()),
            operation.file_count,
            operation.total_bytes
        )?;
    }
    for option in &plan.skipped_options {
        writeln!(plan_file, "skipped_option={}", escape_value(option))?;
    }
    Ok(())
}
