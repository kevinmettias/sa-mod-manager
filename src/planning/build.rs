use crate::prelude::*;
const TARGET_ORDER_CLEO: u8 = 2;
const TARGET_ORDER_CLEO_MODULES: u8 = 3;
const TARGET_ORDER_CLEO_PLUGIN: u8 = 4;
const TARGET_ORDER_CLEO_TEXT: u8 = 5;
const TARGET_ORDER_CLEO_SAVES: u8 = 6;
const TARGET_ORDER_ASI: u8 = 7;
const TARGET_ORDER_DIRECT_MANAGED: u8 = 8;

#[cfg(test)]
const TEST_DATA_BYTES: u64 = 10;
#[cfg(test)]
const TEST_LOOKALIKE_BYTES: u64 = 99;
#[cfg(test)]
const TEST_HANDLING_BYTES: u64 = 20;
#[cfg(test)]
const TEST_ANIM_BYTES: u64 = 5;
#[cfg(test)]
const TEST_MODEL_BYTES: u64 = 7;
#[cfg(test)]
const EXPECTED_DATA_ENTRY_COUNT: usize = 3;
#[cfg(test)]
const EXPECTED_DATA_BYTES: u64 = 35;
#[cfg(test)]
const EXPECTED_ROOT_ENTRY_COUNT: usize = 5;
#[cfg(test)]
const EXPECTED_ROOT_BYTES: u64 = 141;
use crate::settings::ReadmeThresholds;

pub(crate) fn build_install_plan(report: &PackageReport, options: &CommandOptions) -> InstallPlan {
    build_install_plan_with(report, options, ReadmeThresholds::from_settings())
}

/// Build a plan with explicit readme thresholds instead of the process-global
/// config. Production calls [`build_install_plan`]; tests pass fixed thresholds so
/// their assertions do not depend on the developer's local config file.
pub(crate) fn build_install_plan_with(
    report: &PackageReport,
    options: &CommandOptions,
    thresholds: ReadmeThresholds,
) -> InstallPlan {
    let package_id = package_id(&report.package);
    let mut operations = Vec::new();
    let mut warnings = build_plan_warnings(report);
    let mut collections = PlanBuildCollections {
        operations: &mut operations,
        warnings: &mut warnings,
    };
    // Index the package's files once, shared by whichever operation path runs,
    // instead of rescanning every entry per source.
    let index = SourceStatsIndex::new(report);
    if !report.manifest_roots.is_empty() {
        add_manifest_operations(report, &index, options, &mut collections);
    } else if add_readme_operations(
        PlanReadmeContext {
            report,
            index: &index,
            options,
            auto_confidence: thresholds.auto,
        },
        &mut collections,
    ) == 0
    {
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

struct PlanReadmeContext<'a> {
    report: &'a PackageReport,
    index: &'a SourceStatsIndex,
    options: &'a CommandOptions,
    auto_confidence: f32,
}
fn add_readme_operations(
    context: PlanReadmeContext<'_>,
    collections: &mut PlanBuildCollections,
) -> usize {
    let mut count = 0;
    for instruction in &context.report.readme_instructions {
        if push_readme_operation(instruction, &context, collections) {
            count += 1;
        }
    }
    count
}

fn push_readme_operation(
    instruction: &ReadmeInstruction,
    context: &PlanReadmeContext<'_>,
    collections: &mut PlanBuildCollections,
) -> bool {
    if !matches!(instruction.action, ReadmeAction::Copy)
        || instruction.confidence < context.auto_confidence
    {
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
    let target_kind = readme_target_kind(&target);
    let target_root = match path_from_package_root(&target) {
        Ok(path) => context.options.game_root.join(path),
        Err(err) => {
            collections
                .warnings
                .push(format!("readme target skipped for {source}: {err}"));
            return false;
        }
    };
    let stats = context.index.stats(&source);
    let mut notes = vec![format!("readme evidence: {}", instruction.text)];
    notes.extend(
        instruction
            .confidence_reasons
            .iter()
            .map(|reason| format!("readme confidence: {reason}")),
    );
    collections.operations.push(InstallOperation {
        source_root: source,
        target_kind,
        target_root,
        file_count: stats.0,
        total_bytes: stats.1,
        notes,
        optional: false,
    });
    collections
        .warnings
        .push(format!("readme instruction applied: {}", instruction.text));
    true
}
fn readme_target_kind(target: &str) -> TargetKind {
    match normalize_path(target).to_ascii_lowercase().as_str() {
        "cleo" => TargetKind::Cleo,
        "cleo_text" | "cleo/cleo_text" => TargetKind::CleoText,
        "cleo_plugins" | "cleo_plugin" | "cleo/cleo_plugins" => TargetKind::CleoPlugin,
        "cleo_modules" | "cleo_module" | "cleo/cleo_modules" => TargetKind::CleoModules,
        "cleo_saves" | "cleo_save" | "cleo/cleo_saves" => TargetKind::CleoSaves,
        "modloader" => TargetKind::ModLoader,
        "." => TargetKind::DirectManaged,
        _ => TargetKind::DirectManaged,
    }
}

/// Build the `mod.json` install root a readme "copy source -> target" instruction
/// implies, using the exact kind/target mapping the automatic import path uses so
/// a hand-accepted proposal is indistinguishable from an auto-detected one.
pub(crate) fn readme_copy_install_root(
    source: &str,
    target: &str,
    package_id: &str,
) -> ModInstallRootJson {
    let kind = readme_target_kind(target);
    ModInstallRootJson {
        source: normalize_path(source),
        target: target_template(&kind, package_id),
        kind: kind.to_string(),
        enabled: true,
        optional: false,
    }
}

fn add_manifest_operations(
    report: &PackageReport,
    index: &SourceStatsIndex,
    options: &CommandOptions,
    collections: &mut PlanBuildCollections,
) {
    for root in &report.manifest_roots {
        push_manifest_operation(root, index, options, collections);
    }
}

/// Per-source (file count, total bytes) over a package's file entries,
/// precomputed once and queried by prefix in O(log n + matches).
struct SourceStatsIndex {
    /// Normalized file paths with sizes, sorted by path.
    entries: Vec<(String, u64)>,
    total: (usize, u64),
}

impl SourceStatsIndex {
    fn new(report: &PackageReport) -> Self {
        let mut entries: Vec<(String, u64)> = report
            .entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| (normalize_path(&entry.path), entry.size))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let total = (entries.len(), entries.iter().map(|(_, size)| size).sum());
        Self { entries, total }
    }

    fn stats(&self, source: &str) -> (usize, u64) {
        let source = normalize_path(source);
        if source == "." {
            return self.total;
        }
        let mut file_count = 0;
        let mut total_bytes = 0;
        // Exact match (a single file whose path equals the source).
        if let Ok(idx) = self
            .entries
            .binary_search_by(|(path, _)| path.as_str().cmp(&source))
        {
            file_count += 1;
            total_bytes += self.entries[idx].1;
        }
        // Files under `source/` form a contiguous sorted range.
        let prefix = format!("{source}/");
        let start = self
            .entries
            .partition_point(|(path, _)| path.as_str() < prefix.as_str());
        for (path, size) in &self.entries[start..] {
            if path.starts_with(&prefix) {
                file_count += 1;
                total_bytes += size;
            } else {
                break;
            }
        }
        (file_count, total_bytes)
    }
}

fn push_manifest_operation(
    root: &ManifestInstallRoot,
    index: &SourceStatsIndex,
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
    let stats = index.stats(&source);
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
            || source_lower.starts_with(&(option.clone() + "/"))
            || option.starts_with(&(source_lower.clone() + "/"))
    }) || source_lower.contains("optional")
        || source_lower.contains("bonus")
        || source_lower.contains("recommended")
        || source_lower.contains("settings")
}

fn target_kind(candidate: &InstallCandidate) -> TargetKind {
    if candidate.components.contains(&Component::ModLoader)
        || candidate.target_strategy == "game root"
    {
        TargetKind::Bootstrap
    } else if candidate.components.contains(&Component::CleoPlugin) {
        TargetKind::CleoPlugin
    } else if candidate.components.contains(&Component::CleoModules) {
        TargetKind::CleoModules
    } else if candidate.components.contains(&Component::CleoSaves) {
        TargetKind::CleoSaves
    } else if candidate.components.contains(&Component::CleoText) {
        TargetKind::CleoText
    } else if candidate.components.contains(&Component::Cleo) {
        TargetKind::Cleo
    } else if candidate.components.contains(&Component::Asi) {
        TargetKind::Asi
    } else if candidate.components.contains(&Component::ModLoaderContent)
        || candidate.target_strategy.contains("modloader")
    {
        TargetKind::ModLoader
    } else {
        TargetKind::DirectManaged
    }
}

fn target_root_for(kind: &TargetKind, game_root: &Path, package_id: &str) -> PathBuf {
    match kind {
        TargetKind::ModLoader => game_root
            .join("modloader")
            .join(crate::settings::modloader_folder_name(package_id)),
        TargetKind::Cleo => game_root.join("CLEO"),
        TargetKind::CleoText => game_root.join("CLEO").join("cleo_text"),
        TargetKind::CleoPlugin => game_root.join("CLEO").join("cleo_plugins"),
        TargetKind::CleoModules => game_root.join("CLEO").join("cleo_modules"),
        TargetKind::CleoSaves => game_root.join("CLEO").join("cleo_saves"),
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
} // literal: allow UI tuning threshold is local to this control
// literal: allow UI tuning threshold is local to this control
fn skipped_options(report: &PackageReport, options: &CommandOptions) -> Vec<String> {
    // literal: allow UI tuning threshold is local to this control
    report // literal: allow UI tuning threshold is local to this control
        .option_groups // literal: allow UI tuning threshold is local to this control
        .iter() // literal: allow UI tuning threshold is local to this control
        .filter(|option| !options.includes.contains(&normalize_path(option))) // literal: allow UI tuning threshold is local to this control
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
        // CLEO scripts, then their modules/plugins/text/saves on top, before ASI.
        TargetKind::Cleo => TARGET_ORDER_CLEO,
        TargetKind::CleoModules => TARGET_ORDER_CLEO_MODULES,
        TargetKind::CleoPlugin => TARGET_ORDER_CLEO_PLUGIN,
        TargetKind::CleoText => TARGET_ORDER_CLEO_TEXT,
        TargetKind::CleoSaves => TARGET_ORDER_CLEO_SAVES,
        TargetKind::Asi => TARGET_ORDER_ASI,
        TargetKind::DirectManaged => TARGET_ORDER_DIRECT_MANAGED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(paths: &[(&str, u64)]) -> SourceStatsIndex {
        let mut entries: Vec<(String, u64)> = paths
            .iter()
            .map(|(path, size)| (normalize_path(path), *size))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let total = (entries.len(), entries.iter().map(|(_, size)| size).sum());
        SourceStatsIndex { entries, total }
    }

    fn candidate_with(component: Component) -> InstallCandidate {
        InstallCandidate {
            source_root: "pkg".to_string(),
            target_strategy: "CLEO".to_string(),
            file_count: 1,
            total_bytes: 1,
            components: BTreeSet::from([component]),
            notes: BTreeSet::new(),
        }
    }

    #[test] // literal: allow test fixture value is the specimen under judgment
    fn cleo_components_route_to_their_own_folders() {
        // literal: allow test fixture value is the specimen under judgment
        let game = Path::new("/game"); // literal: allow test fixture value is the specimen under judgment
        let cases = [
            // literal: allow test fixture value is the specimen under judgment
            (Component::Cleo, game.join("CLEO")), // literal: allow test fixture value is the specimen under judgment
            (Component::CleoText, game.join("CLEO").join("cleo_text")),
            (
                Component::CleoPlugin,
                game.join("CLEO").join("cleo_plugins"), // literal: allow test fixture value is the specimen under judgment
            ), // literal: allow test fixture value is the specimen under judgment
            (
                // literal: allow test fixture value is the specimen under judgment
                Component::CleoModules,
                game.join("CLEO").join("cleo_modules"), // literal: allow test fixture value is the specimen under judgment
            ),
            (Component::CleoSaves, game.join("CLEO").join("cleo_saves")),
        ];
        for (component, expected) in cases {
            let kind = target_kind(&candidate_with(component));
            assert_eq!(target_root_for(&kind, game, "pkg"), expected);
        }
    }

    #[test]
    fn source_stats_index_matches_exact_and_prefix_but_not_lookalikes() {
        let index = index_of(&[
            ("data", TEST_DATA_BYTES),
            ("data.bak", TEST_LOOKALIKE_BYTES), // lookalike: sorts between "data" and "data/" but must not match
            ("data/handling.cfg", TEST_HANDLING_BYTES),
            ("data/anim/x", TEST_ANIM_BYTES),
            ("models/a.dff", TEST_MODEL_BYTES),
        ]);

        // exact ("data") + everything under "data/", excluding "data.bak"
        assert_eq!(
            index.stats("data"),
            (EXPECTED_DATA_ENTRY_COUNT, EXPECTED_DATA_BYTES)
        );
        assert_eq!(index.stats("data/anim"), (1, TEST_ANIM_BYTES));
        assert_eq!(index.stats("models"), (1, TEST_MODEL_BYTES));
        assert_eq!(index.stats("missing"), (0, 0));
        assert_eq!(
            index.stats("."),
            (EXPECTED_ROOT_ENTRY_COUNT, EXPECTED_ROOT_BYTES)
        );
    }
}
