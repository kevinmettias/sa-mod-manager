macro_rules! readme_insight {
    ($kind:expr, $title:expr, $detail:expr, $meta:expr $(,)?) => {
        build_readme_insight(
            $kind,
            ReadmeInsightText {
                title: $title,
                detail: $detail,
            },
            $meta,
        )
    };
}

use crate::prelude::*;
pub(crate) fn analyze_package(package: &Path, game_root: &Path) -> Result<PackageReport, AppError>
{
    let kind = package_kind_or_error(package)?;
    let entries = package_entries(package)?;
    let mut report = empty_package_report(package, game_root, kind, entries);
    classify_entries(&mut report);
    collect_readme_documents(&mut report)?;
    collect_readme_instructions(&mut report);
    collect_wrap_manifest_roots(&mut report)?;
    collect_readme_insights(&mut report);
    return Ok(report);
}

fn package_kind_or_error(package: &Path) -> Result<PackageKind, AppError>
{
    return PackageKind::from_path(package).ok_or_else(|| {
        AppError::Usage(format!(
            "unsupported package type; expected folder, .zip, .7z, .rar, or .wrap: {}",
            package.display()
        ))
    });
}

fn package_entries(package: &Path) -> Result<Vec<PackageEntry>, AppError>
{
    if package.is_dir()
    {
        return list_folder_entries(package);
    }
    return list_archive_entries(package);
}

fn list_folder_entries(root: &Path) -> Result<Vec<PackageEntry>, AppError>
{
    let mut entries = Vec::new();
    list_folder_entries_inner(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    return Ok(entries);
}

/// Cap on how many entries we enumerate from one archive listing. Matches the
/// extraction entry cap: a listing bigger than what we would ever extract is
/// hostile, and building the entry vector unbounded would let it dictate memory.
const MAX_LISTED_ENTRIES: usize = 500_000;
const README_SAMPLE_LIMIT: usize = 12;
const INJECTABLE_TABLE_MIN_NUMBERS: usize = 8;
const LANGUAGE_PACK_MIN_HITS: usize = 2;
const LANGUAGE_PACK_BASE_CONFIDENCE: f32 = 0.62;
const LANGUAGE_PACK_HIT_CONFIDENCE: f32 = 0.06;
const LANGUAGE_PACK_MAX_CONFIDENCE: f32 = 0.86;
const SAVED_OVERRIDE_CONFIDENCE: f32 = 0.80;
const EXPLICIT_CLEO_CONFIDENCE: f32 = 0.88;
const CLEO_TEXT_CONFIDENCE: f32 = 0.74;
const ROOT_HINT_CONFIDENCE: f32 = 0.66;
const WRAP_HINT_CONFIDENCE: f32 = 0.62;
const GTA3_IMG_CONFIDENCE: f32 = 0.82;
const COPY_SCORE_BASE: f32 = 0.35;
const COPY_SCORE_VERB_BONUS: f32 = 0.25;
const COPY_SCORE_TARGET_BONUS: f32 = 0.25;
const COPY_SCORE_SOURCE_BONUS: f32 = 0.12;
const COPY_SCORE_CONTENT_BONUS: f32 = 0.06;
const COPY_SCORE_CONTEXT_BONUS: f32 = 0.06;
const COPY_SCORE_AMBIGUITY_PENALTY: f32 = 0.05;
const COPY_SCORE_MAX: f32 = 0.97;
const CONFIDENCE_PERCENT_SCALE: f32 = 100.0;
const MAX_TARGET_PATH_DEPTH: usize = 3;
const MIN_SOURCE_NAME_LEN: usize = 2;
const SHORT_LINE_WORD_LIMIT: usize = 3;
const MANIFEST_README_MAX_BYTES: usize = 64 * 1024;
const REVIEW_CONFIDENCE_ASSERTION: f32 = 0.85;
const BASIC_COPY_PROPOSAL_CONFIDENCE: f32 = 0.78;
const STRONG_COPY_PROPOSAL_CONFIDENCE: f32 = 0.90;
const MIN_PLAN_NOTE_COUNT: usize = 2;
fn list_archive_entries(package: &Path) -> Result<Vec<PackageEntry>, AppError>
{
    if let Some(entries) = list_archive_entries_native(package)?
    {
        return Ok(entries);
    }
    let mut entries = Vec::new();
    let mut fields = ArchiveEntryFields::default();

    // Parse the `7z l -slt` output as it streams, so we never buffer the whole
    // listing (which is O(entries)) â€” only one line plus the entry list we build,
    // and that list is bounded by MAX_LISTED_ENTRIES.
    stream_archive_listing(package, |line| {
        parse_archive_listing_line(line, &mut fields, &mut entries);
        if entries.len() > MAX_LISTED_ENTRIES
        {
            return Err(list_archive_failed_error_detail(
                package,
                b"archive lists more entries than the manager will enumerate",
            ));
        }
        Ok(())
    })?;

    fields.flush_into(&mut entries);

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    return Ok(entries);
}

fn stream_archive_listing(
    package: &Path,
    mut on_line: impl FnMut(&str) -> Result<(), AppError>,
) -> Result<(), AppError>
{
    use std::io::BufRead;

    let seven_zip = find_seven_zip().ok_or_else(|| missing_7zip_error_for_package(package))?;
    let mut child = Command::new(seven_zip)
        .env("LC_ALL", "C") // prefer stable tool output regardless of system locale
        .arg("l")
        .arg("-slt")
        .arg(package)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    if let Some(stdout) = child.stdout.take()
    {
        for line in std::io::BufReader::new(stdout).lines()
        {
            on_line(&line?)?;
        }
    }
    // `wait_with_output` drains the remaining (small) stderr so a genuine 7-Zip
    // failure reports its own reason instead of a bare generic message.
    let output = child.wait_with_output()?;
    if !output.status.success()
    {
        return Err(list_archive_failed_error_detail(package, &output.stderr));
    }
    return Ok(());
}

fn parse_archive_listing_line(
    line: &str,
    fields: &mut ArchiveEntryFields,
    entries: &mut Vec<PackageEntry>,
)
{
    if let Some(value) = strip_listing_key(line, "Path = ")
    {
        fields.flush_into(entries);
        fields.path = Some(value.trim().replace('\\', "/"));
        fields.size = 0;
        fields.is_dir = false;
    }
    else if let Some(value) = strip_listing_key(line, "Size = ")
    {
        fields.size = value.trim().parse().unwrap_or(0);
    }
    else if let Some(value) = strip_listing_key(line, "Folder = ")
    {
        // 7-Zip marks directory entries with `Folder = +`, independent of the
        // localized attribute string.
        if value.trim() == "+"
        {
            fields.is_dir = true;
        }
    }
    else if let Some(value) = strip_listing_key(line, "Attributes = ")
    {
        if value.contains('D')
        {
            fields.is_dir = true;
        }
    }
}

/// Strip a 7-Zip `-slt` property key case-insensitively, tolerant of encoding
/// quirks, so listing parsing does not depend on the exact key casing.
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

fn empty_package_report(
    package: &Path,
    game_root: &Path,
    kind: PackageKind,
    entries: Vec<PackageEntry>,
) -> PackageReport
{
    return PackageReport {
        package: package.to_path_buf(),
        game_root: game_root.to_path_buf(),
        kind,
        entries,
        readmes: Vec::new(),
        readme_documents: Vec::new(),
        readme_instructions: Vec::new(),
        readme_insights: Vec::new(),
        manifest_roots: Vec::new(),
        component_paths: BTreeSet::new(),
        install_candidates: Vec::new(),
        option_groups: Vec::new(),
        context_hints: BTreeSet::new(),
        risks: BTreeSet::new(),
    };
}

fn collect_readme_documents(report: &mut PackageReport) -> Result<(), AppError>
{
    const MAX_README_BYTES: usize = 16 * 1024;
    let mut documents = Vec::new();
    let mut injectable = Vec::new();
    for readme in report.readmes.iter().take(README_SAMPLE_LIMIT)
    {
        if let Some(text) = read_package_text_file(&report.package, readme, MAX_README_BYTES)?
        {
            add_readme_context_hints(&text, &mut report.context_hints);
            // ModLoader's std.data claims `.txt` and scans readmes for pasteable
            // data lines (handling/weapon/carcols/â€¦), so a readme full of data can
            // silently inject it once the mod is under `modloader/`. Flag it.
            if has_readme_injectable_table(&text)
            {
                injectable.push(readme.clone());
            }
            documents.push(ReadmeDocument {
                path: readme.clone(),
                text,
            });
        }
    }
    for readme in injectable
    {
        report.risks.insert(format!(
            "readme `{readme}` contains data-table lines; ModLoader's std.data scans .txt files \
             and may inject them as game data â€” review before installing as ModLoader content"
        ));
    }
    report.readme_documents = documents;
    return Ok(());
}

/// Whether a readme's text carries lines that look like GTA data-table rows (the
/// kind ModLoader's std.data pastes from `.txt`). Conservative: a non-comment line
/// with many numeric fields is the signature of handling.cfg / weapon.dat rows and
/// almost never appears in prose, keeping false positives low.
fn has_readme_injectable_table(text: &str) -> bool
{
    return text.lines().any(|raw| {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with(';')
            || line.starts_with("//")
        {
            return false;
        }
        let numeric = line
            .split(|character: char| character.is_whitespace() || character == ',')
            .filter(|token| {
                let t = token.trim();
                !t.is_empty() && t.parse::<f64>().is_ok()
            })
            .count();
        numeric >= INJECTABLE_TABLE_MIN_NUMBERS
    });
}

fn collect_readme_instructions(report: &mut PackageReport)
{
    let source_candidates = package_source_candidates(report);
    let mut instructions = Vec::new();
    for document in &report.readme_documents
    {
        for context in readme_line_contexts(&document.text)
        {
            if let Some(instruction) =
                readme_instruction_from_line(document, &context, &source_candidates, report)
            {
                add_readme_instruction_hint(&instruction, &mut report.context_hints);
                instructions.push(instruction);
            }
        }
    }
    report.readme_instructions = instructions;
}

fn collect_readme_insights(report: &mut PackageReport)
{
    let mut insights = Vec::new();
    add_instruction_insights(report, &mut insights);
    add_option_set_insights(report, &mut insights);
    add_layout_insights(report, &mut insights);
    add_rule_pack_insights(report, &mut insights);
    add_dry_run_insights(report, &mut insights);
    add_override_insights(report, &mut insights);
    add_language_insights(report, &mut insights);
    report.readme_insights = insights;
}

fn add_instruction_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>)
{
    for instruction in &report.readme_instructions
    {
        let kind = match instruction.action {
            ReadmeAction::Copy => ReadmeInsightKind::Recipe,
            ReadmeAction::Requires => ReadmeInsightKind::Dependency,
            ReadmeAction::Conflict | ReadmeAction::DoNotInstall => ReadmeInsightKind::Conflict,
            ReadmeAction::Optional => ReadmeInsightKind::OptionSet,
            ReadmeAction::LoadAfter => ReadmeInsightKind::Conflict,
        };
        let source = instruction.source.as_deref().unwrap_or("unknown source");
        let target = instruction.target.as_deref().unwrap_or("unknown target");
        let detail = match instruction.action {
            ReadmeAction::Copy => format!("recipe step: copy {source} to {target}"),
            ReadmeAction::Requires => format!("dependency requirement: {target}"),
            ReadmeAction::Optional => format!("optional source or compatibility choice: {source}"),
            ReadmeAction::Conflict => "compatibility warning from readme".to_string(),
            ReadmeAction::LoadAfter => "load-order or priority warning from readme".to_string(),
            ReadmeAction::DoNotInstall => "explicit do-not-install warning".to_string(),
        };
        let insight = readme_insight!(
            kind,
            instruction.action.to_string(),
            detail,
            ReadmeInsightMeta {
                source_readme: &instruction.source_readme,
                line_number: instruction.line_number,
                evidence: &instruction.text,
                confidence: instruction.confidence,
                rule_id: &format!("readme.{}", instruction.action),
            },
        );
        insights.push(insight);
    }
}

fn add_option_set_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>)
{
    for option in &report.option_groups
    {
        let insight = readme_insight!(
            ReadmeInsightKind::OptionSet,
            "package option".to_string(),
            format!("detected optional or mutually exclusive package folder: {option}"),
            ReadmeInsightMeta {
                source_readme: "package layout",
                line_number: 0,
                evidence: option,
                confidence: BASIC_COPY_PROPOSAL_CONFIDENCE,
                rule_id: "layout.option-folder",
            },
        );
        insights.push(insight);
    }
}

fn add_layout_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>)
{
    for layout in detected_layout_templates(report)
    {
        let insight = readme_insight!(
            ReadmeInsightKind::Layout,
            "layout template".to_string(),
            layout,
            ReadmeInsightMeta {
                source_readme: "package layout",
                line_number: 0,
                evidence: "",
                confidence: LANGUAGE_PACK_MAX_CONFIDENCE,
                rule_id: "layout.template",
            },
        );
        insights.push(insight);
    }
}

fn add_rule_pack_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>)
{
    if !report.readme_instructions.is_empty()
    {
        let insight = readme_insight!(
            ReadmeInsightKind::Rule,
            "deterministic parser rules".to_string(),
            "used command verbs, fuzzy GTA SA targets, package contents, and confidence thresholds"
                .to_string(),
            ReadmeInsightMeta {
                source_readme: "parser rules",
                line_number: 0,
                evidence: "rules are local and human-editable in source/config-ready tables",
                confidence: STRONG_COPY_PROPOSAL_CONFIDENCE,
                rule_id: "rules.deterministic-readme-v1",
            },
        );
        insights.push(insight);
    }
}

fn add_dry_run_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>)
{
    for instruction in &report.readme_instructions
    {
        if !matches!(instruction.action, ReadmeAction::Copy)
        {
            continue;
        }
        let source = instruction.source.as_deref().unwrap_or("unknown source");
        let target = instruction.target.as_deref().unwrap_or("unknown target");
        let state = if instruction.confidence >= crate::settings::readme_auto_confidence() {
            "will auto-propose"
        } else if instruction.confidence >= crate::settings::readme_review_confidence() {
            "needs review before use"
        } else {
            "warning only"
        };
        let insight = readme_insight!(
            ReadmeInsightKind::DryRun,
            "install preview".to_string(),
            format!("{state}: {source} -> {target}"),
            ReadmeInsightMeta {
                source_readme: &instruction.source_readme,
                line_number: instruction.line_number,
                evidence: &instruction.text,
                confidence: instruction.confidence,
                rule_id: "dry-run.readme-copy",
            },
        );
        insights.push(insight);
    }
}

fn add_override_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>)
{
    let needs_override = report.readme_instructions.iter().any(|instruction| {
        matches!(instruction.action, ReadmeAction::Copy)
            && (instruction.confidence < crate::settings::readme_auto_confidence()
                || instruction.source.is_none()
                || instruction.target.is_none())
    });
    if needs_override || !report.option_groups.is_empty()
    {
        let insight = readme_insight!(             ReadmeInsightKind::Override,             "saved override recommended".to_string(),             "user decisions for ambiguous sources, targets, or options should be saved in mod.json install roots/options"                 .to_string(),             ReadmeInsightMeta {                 source_readme: "parser policy",                 line_number: 0,                 evidence: "",                 confidence: SAVED_OVERRIDE_CONFIDENCE,                 rule_id: "override.user-decision",             },         );
        insights.push(insight);
    }
}
