use crate::prelude::*;

pub(crate) fn build_install_plan(report: &PackageReport, options: &CommandOptions) -> InstallPlan {
    let package_id = package_id(&report.package);
    let mut operations = Vec::new();
    let mut warnings = build_plan_warnings(report);
    let mut collections = PlanBuildCollections {
        operations: &mut operations,
        warnings: &mut warnings,
    };
    if !report.manifest_roots.is_empty() {
        add_manifest_operations(report, options, &mut collections);
    } else if add_readme_operations(report, options, &mut collections) == 0 {
        add_candidate_operations(report, options, &package_id, &mut collections);
    }

    warnings.sort();
    warnings.dedup();

    let parts = CompletedPlanParts {
        package_id,
        operations,
        warnings,
    };
    make_install_plan(report, options, parts)
}

fn add_readme_operations(
    report: &PackageReport,
    options: &CommandOptions,
    collections: &mut PlanBuildCollections,
) -> usize {
    let mut count = 0;
    for instruction in &report.readme_instructions {
        if push_readme_operation(instruction, report, options, collections) {
            count += 1;
        }
    }
    count
}

fn push_readme_operation(
    instruction: &ReadmeInstruction,
    report: &PackageReport,
    options: &CommandOptions,
    collections: &mut PlanBuildCollections,
) -> bool {
    if !matches!(instruction.action, ReadmeAction::Copy) || instruction.confidence < 0.85 {
        return false;
    }
    let source = match &instruction.source {
        Some(value) => normalize_path(value),
        None => return false,
    };
    let target = match &instruction.target {
        Some(value) => normalize_path(value),
        None => return false,
    };
    if options.excludes.contains(&source) {
        return false;
    }
    let target_rel = match path_from_package_root(&target) {
        Ok(path) => path,
        Err(err) => {
            collections.warnings.push(format!(
                "readme target skipped for {source}: {err} ({}, line {})",
                instruction.source_readme, instruction.line_number
            ));
            return false;
        }
    };
    let stats = manifest_source_stats(report, &source);
    let target_kind = readme_target_kind(&target);
    let mut notes = vec![format!(
        "readme evidence {:.0}%: {} line {}: {}",
        instruction.confidence * 100.0,
        instruction.source_readme,
        instruction.line_number,
        instruction.text
    )];
    if !instruction.confidence_reasons.is_empty() {
        notes.push(format!(
            "readme confidence reasons: {}",
            instruction.confidence_reasons.join("; ")
        ));
    }
    collections.operations.push(InstallOperation {
        source_root: source,
        target_kind,
        target_root: options.game_root.join(target_rel),
        file_count: stats.0,
        total_bytes: stats.1,
        notes,
        optional: false,
    });
    true
}

fn readme_target_kind(target: &str) -> TargetKind {
    match normalize_path(target).to_ascii_lowercase().as_str() {
        "cleo" => TargetKind::Cleo,
        "modloader" => TargetKind::ModLoader,
        "." => TargetKind::DirectManaged,
        _ => TargetKind::DirectManaged,
    }
}

fn add_manifest_operations(
    report: &PackageReport,
    options: &CommandOptions,
    collections: &mut PlanBuildCollections,
) {
    for root in &report.manifest_roots {
        push_manifest_operation(root, report, options, collections);
    }
}

fn push_manifest_operation(
    root: &ManifestInstallRoot,
    report: &PackageReport,
    options: &CommandOptions,
    collections: &mut PlanBuildCollections,
) {
    let source = normalize_path(&root.source);
    if !root.enabled || options.excludes.contains(&source) {
        return;
    }
    if root.optional && !options.includes.contains(&source) {
        collections
            .warnings
            .push(format!("optional manifest source skipped: {source}"));
        return;
    }

    let target_rel = match path_from_package_root(&root.target) {
        Ok(path) => path,
        Err(err) => {
            collections
                .warnings
                .push(format!("manifest target skipped for {source}: {err}"));
            return;
        }
    };
    let stats = manifest_source_stats(report, &source);
    let target_root = options.game_root.join(target_rel);
    collections.operations.push(InstallOperation {
        source_root: source,
        target_kind: root.kind,
        target_root,
        file_count: stats.0,
        total_bytes: stats.1,
        notes: root.notes.clone(),
        optional: root.optional,
    });
}

fn manifest_source_stats(report: &PackageReport, source: &str) -> (usize, u64) {
    let source = normalize_path(source);
    let mut file_count = 0;
    let mut total_bytes = 0;
    for entry in &report.entries {
        if entry.is_dir {
            continue;
        }
        let path = normalize_path(&entry.path);
        if source == "." || path == source || path.starts_with(&(source.clone() + "/")) {
            file_count += 1;
            total_bytes += entry.size;
        }
    }
    (file_count, total_bytes)
}

fn build_plan_warnings(report: &PackageReport) -> Vec<String> {
    let mut warnings = report.risks.iter().cloned().collect::<Vec<_>>();
    let context_hints = report.context_hints.iter().cloned();
    warnings.extend(context_hints);
    warnings
}

fn add_candidate_operations(
    report: &PackageReport,
    options: &CommandOptions,
    package_id: &str,
    collections: &mut PlanBuildCollections,
) {
    let mut context = PlanBuildContext {
        report,
        options,
        package_id,
        operations: collections.operations,
        warnings: collections.warnings,
    };
    for candidate in &report.install_candidates {
        push_plan_operation(candidate, &mut context);
    }
}

fn push_plan_operation(candidate: &InstallCandidate, context: &mut PlanBuildContext) {
    let source = normalize_path(&candidate.source_root);
    if context.options.excludes.contains(&source) {
        return;
    }
    let optional = is_optional_source(&source, &context.report.option_groups);
    if optional && !context.options.includes.contains(&source) {
        context
            .warnings
            .push(format!("optional source skipped: {source}"));
        return;
    }

    let target_kind = target_kind(candidate);
    let target_root = target_root_for(&target_kind, &context.options.game_root, context.package_id);
    context.operations.push(InstallOperation {
        source_root: source,
        target_kind,
        target_root,
        file_count: candidate.file_count,
        total_bytes: candidate.total_bytes,
        notes: candidate.notes.iter().cloned().collect(),
        optional,
    });
}

fn is_optional_source(source: &str, option_groups: &[String]) -> bool {
    let source_lower = source.to_ascii_lowercase();
    option_groups.iter().any(|option| {
        let option = normalize_path(option).to_ascii_lowercase();
        source_lower == option
            || source_lower.starts_with(&(option.clone() + "/")) // literal: allow external interface text or file-format spelling
            || option.starts_with(&(source_lower.clone() + "/")) // literal: allow external interface text or file-format spelling
    }) || source_lower.contains("optional") // literal: allow external interface text or file-format spelling
        || source_lower.contains("bonus") // literal: allow external interface text or file-format spelling
        || source_lower.contains("recommended") // literal: allow external interface text or file-format spelling
        || source_lower.contains("settings") // literal: allow external interface text or file-format spelling
}

fn target_kind(candidate: &InstallCandidate) -> TargetKind {
    if candidate.components.contains(&Component::ModLoader)
        /* literal: allow external interface text or file-format spelling */ || candidate.target_strategy == "game root"
    // literal: allow external interface text or file-format spelling
    {
        TargetKind::Bootstrap
    } else if candidate.components.contains(&Component::Cleo)
        || candidate.components.contains(&Component::CleoText)
    {
        TargetKind::Cleo
    } else if candidate.components.contains(&Component::Asi) {
        TargetKind::Asi
    } else if candidate.components.contains(&Component::ModLoaderContent)
        /* literal: allow external interface text or file-format spelling */ || candidate.target_strategy.contains("modloader")
    // literal: allow external interface text or file-format spelling
    {
        TargetKind::ModLoader
    } else {
        TargetKind::DirectManaged
    }
}

fn target_root_for(kind: &TargetKind, game_root: &Path, package_id: &str) -> PathBuf {
    match kind {
        TargetKind::ModLoader => game_root
            .join("modloader") // literal: allow external interface text or file-format spelling
            .join(format!("100_{package_id}")),
        TargetKind::Cleo => game_root.join("CLEO"), // literal: allow external interface text or file-format spelling
        TargetKind::Asi => game_root.to_path_buf(),
        TargetKind::Bootstrap => game_root.to_path_buf(),
        TargetKind::DirectManaged => game_root.to_path_buf(),
    }
}

fn make_install_plan(
    report: &PackageReport,
    options: &CommandOptions,
    parts: CompletedPlanParts,
) -> InstallPlan {
    InstallPlan {
        package_id: parts.package_id,
        package: report.package.clone(),
        game_root: options.game_root.clone(),
        profile: options.profile.clone(),
        operations: parts.operations,
        skipped_options: skipped_options(report, options),
        warnings: parts.warnings,
    }
}

fn skipped_options(report: &PackageReport, options: &CommandOptions) -> Vec<String> {
    report
        .option_groups
        .iter()
        .filter(|option| !options.includes.contains(&normalize_path(option)))
        .cloned()
        .collect()
}

pub(crate) fn sort_operations_for_apply(plan: &mut InstallPlan) {
    plan.operations.sort_by(|a, b| {
        a.optional
            .cmp(&b.optional)
            .then_with(|| target_order(&a.target_kind).cmp(&target_order(&b.target_kind)))
            .then_with(|| a.source_root.cmp(&b.source_root))
    });
}

fn target_order(kind: &TargetKind) -> u8 {
    match kind {
        TargetKind::Bootstrap => 0,
        TargetKind::ModLoader => 1,
        TargetKind::Cleo => 2,
        TargetKind::Asi => 3,
        TargetKind::DirectManaged => 4,
    }
}
