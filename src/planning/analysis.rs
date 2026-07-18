use crate::prelude::*;

pub(crate) fn analyze_package(package: &Path, game_root: &Path) -> Result<PackageReport, AppError> {
    let kind = package_kind_or_error(package)?;
    let entries = package_entries(package)?;
    let mut report = empty_package_report(package, game_root, kind, entries);
    classify_entries(&mut report);
    collect_readme_documents(&mut report)?;
    collect_readme_instructions(&mut report);
    collect_wrap_manifest_roots(&mut report)?;
    collect_readme_insights(&mut report);
    Ok(report)
}

fn package_kind_or_error(package: &Path) -> Result<PackageKind, AppError> {
    PackageKind::from_path(package).ok_or_else(|| {
        AppError::Usage(format!(
            "unsupported package type; expected folder, .zip, .7z, .rar, or .wrap: {}",
            package.display()
        ))
    })
}

fn package_entries(package: &Path) -> Result<Vec<PackageEntry>, AppError> {
    if package.is_dir() {
        return list_folder_entries(package);
    }
    list_archive_entries(package)
}

fn list_folder_entries(root: &Path) -> Result<Vec<PackageEntry>, AppError> {
    let mut entries = Vec::new();
    list_folder_entries_inner(root, root, &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn list_archive_entries(package: &Path) -> Result<Vec<PackageEntry>, AppError> {
    if let Some(entries) = list_archive_entries_native(package)? {
        return Ok(entries);
    }
    let mut entries = Vec::new();
    let mut fields = ArchiveEntryFields::default();

    // Parse the `7z l -slt` output as it streams, so we never buffer the whole
    // listing (which is O(entries)) — only one line plus the entry list we build.
    stream_archive_listing(package, |line| {
        parse_archive_listing_line(line, &mut fields, &mut entries);
    })?;

    fields.flush_into(&mut entries);

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn stream_archive_listing(
    package: &Path,
    mut on_line: impl FnMut(&str),
) -> Result<(), AppError> {
    use std::io::BufRead;

    let seven_zip = find_seven_zip().ok_or_else(|| missing_7zip_error_for_package(package))?;
    let mut child = Command::new(seven_zip)
        .env("LC_ALL", "C") // prefer stable tool output regardless of system locale
        .arg("l") // literal: allow external interface text or file-format spelling
        .arg("-slt") // literal: allow external interface text or file-format spelling
        .arg(package)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    if let Some(stdout) = child.stdout.take() {
        for line in std::io::BufReader::new(stdout).lines() {
            on_line(&line?);
        }
    }
    if !child.wait()?.success() {
        return Err(list_archive_failed_error(package));
    }
    Ok(())
}

fn parse_archive_listing_line(
    line: &str,
    fields: &mut ArchiveEntryFields,
    entries: &mut Vec<PackageEntry>,
) {
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if let Some(value) = strip_listing_key(line, "Path = ") {
        // literal: allow external interface text or file-format spelling
        fields.flush_into(entries);
        fields.path = Some(value.trim().replace('\\', "/")); // literal: allow external interface text or file-format spelling
        fields.size = 0;
        fields.is_dir = false;
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    } else if let Some(value) = strip_listing_key(line, "Size = ") {
        // literal: allow external interface text or file-format spelling
        fields.size = value.trim().parse().unwrap_or(0);
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    } else if let Some(value) = strip_listing_key(line, "Folder = ") {
        // 7-Zip marks directory entries with `Folder = +`, independent of the
        // localized attribute string.
        if value.trim() == "+" {
            fields.is_dir = true;
        }
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    } else if let Some(value) = strip_listing_key(line, "Attributes = ") {
        // literal: allow external interface text or file-format spelling
        if value.contains('D') {
            fields.is_dir = true;
        }
    }
}

/// Strip a 7-Zip `-slt` property key case-insensitively, tolerant of encoding
/// quirks, so listing parsing does not depend on the exact key casing.
fn strip_listing_key<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let head = line.get(..key.len())?;
    if head.eq_ignore_ascii_case(key) {
        Some(&line[key.len()..])
    } else {
        None
    }
}

fn empty_package_report(
    package: &Path,
    game_root: &Path,
    kind: PackageKind,
    entries: Vec<PackageEntry>,
) -> PackageReport {
    PackageReport {
        package: package.to_path_buf(),
        game_root: game_root.to_path_buf(),
        kind,
        entries,
        readmes: Vec::new(),
        readme_documents: Vec::new(),
        readme_instructions: Vec::new(),
        readme_insights: Vec::new(),
        manifest_roots: Vec::new(),
        components: BTreeSet::new(),
        install_candidates: Vec::new(),
        option_groups: Vec::new(),
        context_hints: BTreeSet::new(),
        risks: BTreeSet::new(),
    }
}

fn collect_readme_documents(report: &mut PackageReport) -> Result<(), AppError> {
    const MAX_README_BYTES: usize = 16 * 1024;
    let mut documents = Vec::new();
    for readme in report.readmes.iter().take(12) {
        if let Some(text) = read_package_text_file(&report.package, readme, MAX_README_BYTES)? {
            add_readme_context_hints(&text, &mut report.context_hints);
            documents.push(ReadmeDocument {
                path: readme.clone(),
                text,
            });
        }
    }
    report.readme_documents = documents;
    Ok(())
}

fn collect_readme_instructions(report: &mut PackageReport) {
    let source_candidates = package_source_candidates(report);
    let mut instructions = Vec::new();
    for document in &report.readme_documents {
        for context in readme_line_contexts(&document.text) {
            if let Some(instruction) = readme_instruction_from_line(
                document,
                context.line_number,
                &context.original,
                &context.normalized,
                &context.paragraph,
                &context.section,
                &source_candidates,
                report,
            ) {
                add_readme_instruction_hint(&instruction, &mut report.context_hints);
                instructions.push(instruction);
            }
        }
    }
    report.readme_instructions = instructions;
}

fn collect_readme_insights(report: &mut PackageReport) {
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

fn add_instruction_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>) {
    for instruction in &report.readme_instructions {
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
        insights.push(readme_insight(
            kind,
            instruction.action.to_string(),
            detail,
            &instruction.source_readme,
            instruction.line_number,
            &instruction.text,
            instruction.confidence,
            &format!("readme.{}", instruction.action),
        ));
    }
}

fn add_option_set_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>) {
    for option in &report.option_groups {
        insights.push(readme_insight(
            ReadmeInsightKind::OptionSet,
            "package option".to_string(),
            format!("detected optional or mutually exclusive package folder: {option}"),
            "package layout",
            0,
            option,
            0.78,
            "layout.option-folder",
        ));
    }
}

fn add_layout_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>) {
    for layout in detected_layout_templates(report) {
        insights.push(readme_insight(
            ReadmeInsightKind::Layout,
            "layout template".to_string(),
            layout,
            "package layout",
            0,
            "",
            0.86,
            "layout.template",
        ));
    }
}

fn add_rule_pack_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>) {
    if !report.readme_instructions.is_empty() {
        insights.push(readme_insight(
            ReadmeInsightKind::Rule,
            "deterministic parser rules".to_string(),
            "used command verbs, fuzzy GTA SA targets, package contents, and confidence thresholds"
                .to_string(),
            "parser rules",
            0,
            "rules are local and human-editable in source/config-ready tables",
            0.90,
            "rules.deterministic-readme-v1",
        ));
    }
}

fn add_dry_run_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>) {
    for instruction in &report.readme_instructions {
        if !matches!(instruction.action, ReadmeAction::Copy) {
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
        insights.push(readme_insight(
            ReadmeInsightKind::DryRun,
            "install preview".to_string(),
            format!("{state}: {source} -> {target}"),
            &instruction.source_readme,
            instruction.line_number,
            &instruction.text,
            instruction.confidence,
            "dry-run.readme-copy",
        ));
    }
}

fn add_override_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>) {
    let needs_override = report
        .readme_instructions
        .iter()
        .any(|instruction| {
            matches!(instruction.action, ReadmeAction::Copy)
                && (instruction.confidence < crate::settings::readme_auto_confidence()
                    || instruction.source.is_none()
                    || instruction.target.is_none())
        });
    if needs_override || !report.option_groups.is_empty() {
        insights.push(readme_insight(
            ReadmeInsightKind::Override,
            "saved override recommended".to_string(),
            "user decisions for ambiguous sources, targets, or options should be saved in mod.json install roots/options"
                .to_string(),
            "parser policy",
            0,
            "",
            0.80,
            "override.user-decision",
        ));
    }
}

fn add_language_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>) {
    for document in &report.readme_documents {
        let normalized = normalize_readme_line(&document.text);
        for (language, confidence) in detected_language_packs(&normalized) {
            insights.push(readme_insight(
                ReadmeInsightKind::Language,
                format!("{language} keyword pack"),
                format!("detected {language} install keywords; deterministic multilingual rules were applied"),
                &document.path,
                0,
                "",
                confidence,
                &format!("language.{language}"),
            ));
        }
    }
}

fn readme_insight(
    kind: ReadmeInsightKind,
    title: String,
    detail: String,
    source_readme: &str,
    line_number: usize,
    evidence: &str,
    confidence: f32,
    rule_id: &str,
) -> ReadmeInsight {
    ReadmeInsight {
        kind,
        title,
        detail,
        source_readme: source_readme.to_string(),
        line_number,
        evidence: evidence.to_string(),
        confidence,
        rule_id: rule_id.to_string(),
    }
}

fn detected_layout_templates(report: &PackageReport) -> Vec<String> {
    let mut layouts = Vec::new();
    let has = |component| report.components.contains(&component);
    if has(Component::ModLoaderContent)
        && (has(Component::Data)
            || has(Component::Models)
            || has(Component::Text)
            || has(Component::Anim)
            || has(Component::Audio))
    {
        layouts.push("modloader mirror: package has game-folder structure suitable for modloader".to_string());
    }
    if has(Component::Cleo) || has(Component::CleoText) {
        layouts.push("CLEO script package: .cs/.cleo/.fxt files should map to CLEO/CLEO_TEXT".to_string());
    }
    if has(Component::Asi) {
        layouts.push("ASI root package: .asi/.dll/.ini payload likely belongs in the game root or ASI-supported modloader folder".to_string());
    }
    if has(Component::ImgReplacement) {
        layouts.push("gta3.img payload: loose .dff/.txd/.col files should be isolated under modloader/<mod>/gta3.img".to_string());
    }
    if has(Component::Text) {
        layouts.push("language/text package: .gxt or text assets map to the text folder".to_string());
    }
    if report.entries.iter().any(|entry| {
        let path = normalize_path(&entry.path).to_ascii_lowercase();
        path.contains("/data/")
            || path.contains("/models/")
            || path.contains("/text/")
            || path.contains("/anim/")
            || path.contains("/audio/")
    }) {
        layouts.push("root mirror: archive contains direct game-folder names".to_string());
    }
    layouts.sort();
    layouts.dedup();
    layouts
}

fn detected_language_packs(text: &str) -> Vec<(&'static str, f32)> {
    let packs = [
        (
            "spanish",
            [
                "copiar",
                "copia",
                "carpeta del juego",
                "directorio del juego",
                "instalar",
            ],
        ),
        (
            "portuguese",
            [
                "copie",
                "copiar",
                "pasta do jogo",
                "diretorio do jogo",
                "instalar",
            ],
        ),
        (
            "polish",
            [
                "skopiuj",
                "folderu gry",
                "katalogu gry",
                "instalacja",
                "wymaga",
            ],
        ),
        (
            "russian-translit",
            [
                "skopiruyte",
                "papku s igroy",
                "papka igry",
                "ustanovka",
                "trebuetsya",
            ],
        ),
    ];
    packs
        .iter()
        .filter_map(|(language, words)| {
            let hits = words.iter().filter(|word| text.contains(**word)).count();
            if hits >= 2 {
                Some((*language, (0.62 + (hits as f32 * 0.06)).min(0.86)))
            } else {
                None
            }
        })
        .collect()
}

struct ReadmeLineContext {
    line_number: usize,
    original: String,
    normalized: String,
    paragraph: String,
    section: Option<String>,
}

struct ConfidenceScore {
    value: f32,
    reasons: Vec<String>,
}

fn readme_line_contexts(text: &str) -> Vec<ReadmeLineContext> {
    let mut section = None;
    let mut contexts = Vec::new();
    let lines = text.lines().collect::<Vec<_>>();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = normalize_readme_line(trimmed);
        if is_readme_section_heading(&normalized) {
            section = Some(normalized.clone());
            continue;
        }
        contexts.push(ReadmeLineContext {
            line_number: idx + 1,
            original: trimmed.to_string(),
            normalized,
            paragraph: readme_paragraph_context(&lines, idx),
            section: section.clone(),
        });
    }
    contexts
}

fn readme_paragraph_context(lines: &[&str], index: usize) -> String {
    let mut start = index;
    while start > 0 && !lines[start - 1].trim().is_empty() {
        start -= 1;
    }
    let mut end = index;
    while end + 1 < lines.len() && !lines[end + 1].trim().is_empty() {
        end += 1;
    }
    lines[start..=end]
        .iter()
        .map(|line| normalize_readme_line(line.trim()))
        .filter(|line| !line.is_empty() && !is_readme_section_heading(line))
        .collect::<Vec<_>>()
        .join(" ")
}

fn readme_instruction_from_line(
    document: &ReadmeDocument,
    line_number: usize,
    line: &str,
    normalized: &str,
    paragraph: &str,
    section: &Option<String>,
    source_candidates: &[String],
    report: &PackageReport,
) -> Option<ReadmeInstruction> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = normalized;
    let evidence_context = combined_readme_context(lower, paragraph, section);
    if contains_do_not_install(lower) {
        return Some(readme_instruction(
            document,
            line_number,
            ReadmeAction::DoNotInstall,
            None,
            None,
            ConfidenceScore {
                value: 0.88,
                reasons: vec!["explicit do-not-install wording".to_string()],
            },
            trimmed,
            &evidence_context,
        ));
    }
    if contains_conflict(lower) {
        return Some(readme_instruction(
            document,
            line_number,
            ReadmeAction::Conflict,
            None,
            None,
            ConfidenceScore {
                value: 0.74,
                reasons: vec!["compatibility/conflict wording".to_string()],
            },
            trimmed,
            &evidence_context,
        ));
    }
    if contains_load_after(lower) {
        return Some(readme_instruction(
            document,
            line_number,
            ReadmeAction::LoadAfter,
            None,
            None,
            ConfidenceScore {
                value: 0.66,
                reasons: vec!["load-order wording".to_string()],
            },
            trimmed,
            &evidence_context,
        ));
    }
    if contains_optional(lower) {
        return Some(readme_instruction(
            document,
            line_number,
            ReadmeAction::Optional,
            infer_source_from_line(&evidence_context, source_candidates),
            None,
            ConfidenceScore {
                value: 0.62,
                reasons: vec!["optional/bonus wording".to_string()],
            },
            trimmed,
            &evidence_context,
        ));
    }
    if contains_requirement(lower) {
        return Some(readme_instruction(
            document,
            line_number,
            ReadmeAction::Requires,
            None,
            requirement_target_from_line(&evidence_context),
            ConfidenceScore {
                value: 0.82,
                reasons: vec!["requirement wording".to_string()],
            },
            trimmed,
            &evidence_context,
        ));
    }
    let install_verb = contains_install_verb(lower);
    let install_section = section
        .as_deref()
        .map(is_install_section_heading)
        .unwrap_or(false);
    if !install_verb {
        return None;
    }
    let mut source = infer_source_from_line(&evidence_context, source_candidates);
    let target = infer_target_from_line(&evidence_context, source.as_deref(), report);
    if source.is_none() {
        source = infer_source_from_target_and_contents(target.as_deref(), report);
    }
    let confidence = copy_instruction_score(
        &source,
        &target,
        &evidence_context,
        install_verb,
        install_section,
    );
    Some(readme_instruction(
        document,
        line_number,
        ReadmeAction::Copy,
        source,
        target,
        confidence,
        trimmed,
        &evidence_context,
    ))
}

fn combined_readme_context(line: &str, paragraph: &str, section: &Option<String>) -> String {
    let mut parts = Vec::new();
    if let Some(section) = section {
        parts.push(section.as_str());
    }
    if !paragraph.is_empty() {
        parts.push(paragraph);
    }
    if !parts.iter().any(|part| *part == line) {
        parts.push(line);
    }
    parts.join(" ")
}

fn readme_instruction(
    document: &ReadmeDocument,
    line_number: usize,
    action: ReadmeAction,
    source: Option<String>,
    target: Option<String>,
    confidence: ConfidenceScore,
    text: &str,
    normalized_text: &str,
) -> ReadmeInstruction {
    ReadmeInstruction {
        source_readme: document.path.clone(),
        line_number,
        action,
        source,
        target,
        confidence: confidence.value,
        text: text.to_string(),
        normalized_text: normalized_text.to_string(),
        confidence_reasons: confidence.reasons,
    }
}

fn normalize_readme_line(line: &str) -> String {
    line.replace('\\', "/")
        .replace(
            [
                '"', '\'', '`', '\u{201c}', '\u{201d}', '\u{2018}', '\u{2019}',
            ],
            "",
        )
        .replace(['*', '\u{2022}', '\u{25e6}', '\u{25aa}', '\u{25ab}'], " ")
        .replace(['-', '\u{2013}', '\u{2014}', '\u{2192}', '\u{21d2}'], " ")
        .replace(
            [
                ':', ';', ',', '.', '(', ')', '[', ']', '{', '}', '<', '>', '_',
            ],
            " ",
        )
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn package_source_candidates(report: &PackageReport) -> Vec<String> {
    let mut candidates = BTreeSet::new();
    for entry in &report.entries {
        let path = normalize_path(&entry.path);
        let parts = path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        for depth in 1..=parts.len().min(3) {
            candidates.insert(parts[..depth].join("/"));
        }
    }
    candidates.into_iter().collect()
}

fn infer_source_from_line(line: &str, candidates: &[String]) -> Option<String> {
    let mut matches = candidates
        .iter()
        .filter(|candidate| source_candidate_matches_line(candidate, line))
        .cloned()
        .collect::<Vec<_>>();
    matches.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    matches.into_iter().next()
}

fn source_candidate_matches_line(candidate: &str, line: &str) -> bool {
    let lower = candidate.to_ascii_lowercase().replace('\\', "/");
    let name = file_name(&lower);
    if name.len() < 2 {
        return false;
    }
    line_contains_path(line, &lower)
        || line_contains_token(line, name)
        || line.contains(&format!("folder {name}"))
        || line.contains(&format!("directory {name}"))
        || line.contains(&format!("contents of {name}"))
        || line.contains(&format!("files from {name}"))
}

fn infer_target_from_line(
    line: &str,
    source: Option<&str>,
    report: &PackageReport,
) -> Option<String> {
    if mentions_gta3_img(line) {
        return Some(format!(
            "modloader/{}/gta3.img",
            safe_name(&package_id(&report.package))
        ));
    }
    if mentions_cleo(line) {
        return Some("CLEO".to_string());
    }
    if mentions_modloader(line) {
        return Some("modloader".to_string());
    }
    if mentions_game_data_folder(line, "data") {
        return Some("data".to_string());
    }
    if mentions_game_data_folder(line, "models") {
        return Some("models".to_string());
    }
    if mentions_game_data_folder(line, "text") {
        return Some("text".to_string());
    }
    if mentions_game_data_folder(line, "anim") {
        return Some("anim".to_string());
    }
    if mentions_game_data_folder(line, "audio") {
        return Some("audio".to_string());
    }
    if mentions_game_root(line) {
        if let Some(source) = source {
            let source_name = file_name(&source.to_ascii_lowercase()).to_string();
            if matches!(
                source_name.as_str(),
                "cleo" | "data" | "models" | "text" | "anim" | "audio" | "modloader"
            ) {
                return Some(source_name);
            }
        }
        return Some(".".to_string());
    }
    None
}

fn infer_source_from_target_and_contents(
    target: Option<&str>,
    report: &PackageReport,
) -> Option<String> {
    let target = target?.to_ascii_lowercase();
    if target == "cleo" {
        return source_with_extensions(report, &["cs", "cleo", "fxt"])
            .or_else(|| source_named_like(report, &["cleo", "cleo_text"]));
    }
    if target == "modloader" {
        return source_with_game_mirror(report)
            .or_else(|| source_with_extensions(report, &["dff", "txd"]))
            .or_else(|| source_named_like(report, &["modloader"]));
    }
    if target.ends_with("/gta3.img") {
        return source_with_extensions(report, &["dff", "txd"])
            .or_else(|| source_named_like(report, &["gta3.img", "models"]));
    }
    if matches!(
        target.as_str(),
        "data" | "models" | "text" | "anim" | "audio"
    ) {
        return source_named_like(report, &[&target]);
    }
    if target == "." {
        return source_with_extensions(report, &["asi", "dll", "ini"]);
    }
    None
}

fn source_named_like(report: &PackageReport, names: &[&str]) -> Option<String> {
    package_source_candidates(report)
        .into_iter()
        .find(|candidate| {
            let name = file_name(&candidate.to_ascii_lowercase()).to_string();
            names.iter().any(|expected| &name == expected)
        })
}

fn source_with_extensions(report: &PackageReport, extensions: &[&str]) -> Option<String> {
    let mut roots = BTreeSet::new();
    for entry in &report.entries {
        if entry.is_dir {
            continue;
        }
        let path = normalize_path(&entry.path);
        let lower = path.to_ascii_lowercase();
        if !extensions
            .iter()
            .any(|extension| lower.ends_with(&format!(".{extension}")))
        {
            continue;
        }
        roots.insert(top_level_or_parent_source(&path));
    }
    shortest_source(roots)
}

fn source_with_game_mirror(report: &PackageReport) -> Option<String> {
    let mut roots = BTreeSet::new();
    for entry in &report.entries {
        let path = normalize_path(&entry.path);
        let parts = path.split('/').collect::<Vec<_>>();
        if parts.iter().any(|part| {
            matches!(
                part.to_ascii_lowercase().as_str(),
                "data" | "models" | "text" | "anim" | "audio"
            )
        }) {
            roots.insert(top_level_or_parent_source(&path));
        }
    }
    shortest_source(roots)
}

fn top_level_or_parent_source(path: &str) -> String {
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() <= 1 {
        ".".to_string()
    } else {
        parts[..parts.len().saturating_sub(1).min(1)].join("/")
    }
}

fn shortest_source(roots: BTreeSet<String>) -> Option<String> {
    roots
        .into_iter()
        .min_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)))
}

fn requirement_target_from_line(line: &str) -> Option<String> {
    if mentions_cleo(line) {
        Some("CLEO".to_string())
    } else if mentions_modloader(line) {
        Some("Mod Loader".to_string())
    } else if line.contains("asi loader") {
        Some("ASI Loader".to_string())
    } else if line.contains("silentpatch") || line.contains("silent patch") {
        Some("SilentPatch".to_string())
    } else if line.contains("1 0 exe")
        || line.contains("original exe")
        || line.contains("hoodlum exe")
        || line.contains("us 1 0")
    {
        Some("GTA SA 1.0 executable".to_string())
    } else {
        None
    }
}

fn copy_instruction_score(
    source: &Option<String>,
    target: &Option<String>,
    line: &str,
    install_verb: bool,
    install_section: bool,
) -> ConfidenceScore {
    let mut confidence: f32 = 0.35;
    let mut reasons = Vec::new();
    if source.is_some() {
        confidence += 0.25;
        reasons.push("source matched readme/package contents".to_string());
    }
    if target.is_some() {
        confidence += 0.25;
        reasons.push("target matched known GTA SA location".to_string());
    }
    if install_verb {
        confidence += 0.12;
        reasons.push("install/copy verb present".to_string());
    }
    if install_section {
        confidence += 0.06;
        reasons.push("inside install/manual section".to_string());
    }
    if mentions_game_root(line) || mentions_cleo(line) || mentions_modloader(line) {
        confidence += 0.06;
        reasons.push("target wording is explicit".to_string());
    }
    if line.contains("contents") {
        confidence -= 0.05;
        reasons.push("source wording says contents; review destination shape".to_string());
    }
    if source.is_none() {
        reasons.push("source not found".to_string());
    }
    if target.is_none() {
        reasons.push("target not found".to_string());
    }
    ConfidenceScore {
        value: confidence.clamp(0.0, 0.97),
        reasons,
    }
}

fn add_readme_instruction_hint(instruction: &ReadmeInstruction, hints: &mut BTreeSet<String>) {
    let source = instruction.source.as_deref().unwrap_or("unknown source");
    let target = instruction.target.as_deref().unwrap_or("unknown target");
    hints.insert(format!(
        "readme instruction {} {} -> {} confidence {:.0}% ({}, line {})",
        instruction.action,
        source,
        target,
        instruction.confidence * 100.0,
        instruction.source_readme,
        instruction.line_number
    ));
}

fn contains_install_verb(line: &str) -> bool {
    [
        "copy",
        "copied",
        "copying",
        "put",
        "paste",
        "place",
        "placed",
        "move",
        "moved",
        "extract",
        "extracted",
        "unpack",
        "unpacked",
        "unzip",
        "unzipped",
        "drag",
        "drop",
        "transfer",
        "install",
        "installed",
        "copiar",
        "copie",
        "skopiuj",
        "skopiruyte",
        "instalar",
        "instalacja",
        "ustanovka",
    ]
    .iter()
    .any(|word| line_contains_token(line, word))
}

fn is_readme_section_heading(line: &str) -> bool {
    is_install_section_heading(line)
        || line_contains_token(line, "optional")
        || line_contains_token(line, "requirements")
        || line_contains_token(line, "requirement")
        || line_contains_token(line, "compatibility")
}

fn is_install_section_heading(line: &str) -> bool {
    let words = line.split_whitespace().count();
    words <= 3
        && (line_contains_token(line, "install")
            || line_contains_token(line, "installation")
            || line_contains_token(line, "manual"))
}

fn contains_requirement(line: &str) -> bool {
    line.contains("requires")
        || line.contains("require ")
        || line.contains("requirement")
        || line.contains("dependencies")
        || line.contains("dependency")
        || line.contains("needed")
        || line.contains("need ")
        || line.contains("must have")
        || line.contains("you need")
}

fn contains_optional(line: &str) -> bool {
    line_contains_token(line, "optional")
        || line_contains_token(line, "bonus")
        || line.contains("for rosa")
        || line.contains("compatibility patch")
        || line.contains("if you want")
}

fn contains_conflict(line: &str) -> bool {
    line.contains("conflict")
        || line.contains("incompatible")
        || line.contains("do not use with")
        || line.contains("dont use with")
}

fn contains_load_after(line: &str) -> bool {
    line.contains("load after") || line.contains("priority after")
}

fn contains_do_not_install(line: &str) -> bool {
    line.contains("do not install")
        || line.contains("dont install")
        || line.contains("do not copy")
        || line.contains("dont copy")
}

fn mentions_cleo(line: &str) -> bool {
    line_contains_token(line, "cleo")
        || line.contains("cleo folder")
        || line.contains("cleo directory")
        || line.contains("cleo dir")
}

fn mentions_modloader(line: &str) -> bool {
    line.contains("modloader") || line.contains("mod loader") || line.contains("mod-loader")
}

fn mentions_gta3_img(line: &str) -> bool {
    line.contains("gta3 img")
        || line.contains("gta3img")
        || line.contains("gta3 image")
        || line.contains("img archive")
        || line.contains("models/gta3")
        || line.contains("models gta3")
}

fn mentions_game_root(line: &str) -> bool {
    line.contains("game root")
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
        || line.contains("papka igry")
}

fn mentions_game_data_folder(line: &str, folder: &str) -> bool {
    line_contains_token(line, folder)
        && (line.contains("folder") || line.contains("directory") || line.contains("copy"))
}

fn line_contains_token(line: &str, token: &str) -> bool {
    line.split_whitespace().any(|part| part == token) || line.contains(&format!(" {token} "))
}

fn line_contains_path(line: &str, path: &str) -> bool {
    if line_contains_token(line, path) {
        return true;
    }
    let path_words = path.replace('/', " ");
    line.contains(&path_words)
}

fn add_readme_context_hints(text: &str, hints: &mut BTreeSet<String>) {
    let lower = text.to_ascii_lowercase();
    if lower.contains("cleo") {
        hints.insert("readme mentions CLEO install requirements".to_string());
    }
    if lower.contains("modloader") || lower.contains("mod loader") {
        hints.insert("readme mentions Mod Loader install support".to_string());
    }
    if lower.contains("asi loader") || lower.contains(".asi") {
        hints.insert("readme mentions ASI/plugin install requirements".to_string());
    }
    if lower.contains("copy") && lower.contains("root") {
        hints.insert("readme mentions copying files to the game root".to_string());
    }
}

fn collect_wrap_manifest_roots(report: &mut PackageReport) -> Result<(), AppError> {
    let Some(manifest_path) = wrap_manifest_path(report) else {
        if matches!(report.kind, PackageKind::Wrap) {
            return Err(AppError::Usage(
                ".wrap package must contain wrap.json, manifest.json, or package.wrap.json"
                    .to_string(),
            ));
        }
        return Ok(());
    };
    let Some(text) = read_package_text_file(&report.package, &manifest_path, 64 * 1024)? else {
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
    if !report.manifest_roots.is_empty() {
        report
            .context_hints
            .insert(format!("using explicit .wrap manifest: {manifest_path}"));
    }
    Ok(())
}

fn validate_wrap_manifest(
    report: &PackageReport,
    manifest_path: &str,
    manifest: &WrapManifestFile,
) -> Result<(), AppError> {
    if manifest.install_roots.is_empty() {
        return Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} must define at least one install_roots entry"
        )));
    }
    for (idx, root) in manifest.install_roots.iter().enumerate() {
        validate_wrap_install_root(report, manifest_path, idx, root)?;
    }
    Ok(())
}

fn validate_wrap_install_root(
    report: &PackageReport,
    manifest_path: &str,
    idx: usize,
    root: &WrapInstallRootFile,
) -> Result<(), AppError> {
    let source = normalize_path(&root.source);
    let target = normalize_path(&root.target);
    validate_wrap_relative_path(manifest_path, idx, "source", &source)?;
    validate_wrap_relative_path(manifest_path, idx, "target", &target)?;
    validate_wrap_kind(manifest_path, idx, &root.kind)?;
    path_from_package_root(&target).map_err(|err| {
        AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].target is invalid: {err}"
        ))
    })?;
    if !wrap_source_exists(report, &source) {
        return Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].source does not exist in package: {source}"
        )));
    }
    Ok(())
}

fn validate_wrap_relative_path(
    manifest_path: &str,
    idx: usize,
    field: &str,
    value: &str,
) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].{field} must not be empty"
        )));
    }
    let path = Path::new(value);
    if path.is_absolute() || value.starts_with('/') || value.split('/').any(|part| part == "..") {
        return Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].{field} must be a relative package path: {value}"
        )));
    }
    Ok(())
}

fn validate_wrap_kind(manifest_path: &str, idx: usize, kind: &str) -> Result<(), AppError> {
    let normalized = kind.to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "modloader"
            | "cleo"
            | "cleo_text"
            | "asi"
            | "plugin"
            | "bootstrap"
            | "runtime"
            | "direct"
            | "directmanaged"
            | "direct_managed"
    ) {
        Ok(())
    } else {
        Err(AppError::Usage(format!(
            ".wrap manifest {manifest_path} install_roots[{idx}].kind is unsupported: {kind}"
        )))
    }
}

fn wrap_source_exists(report: &PackageReport, source: &str) -> bool {
    if source == "." {
        return true;
    }
    report.entries.iter().any(|entry| {
        let path = normalize_path(&entry.path);
        path == source || path.starts_with(&format!("{source}/"))
    })
}

fn wrap_manifest_path(report: &PackageReport) -> Option<String> {
    let known = ["wrap.json", "manifest.json", "package.wrap.json"];
    report.entries.iter().find_map(|entry| {
        if entry.is_dir {
            return None;
        }
        let normalized = normalize_path(&entry.path);
        let lower = normalized.to_ascii_lowercase();
        if known
            .iter()
            .any(|name| lower == *name || lower.ends_with(&format!("/{name}")))
        {
            Some(normalized)
        } else {
            None
        }
    })
}

#[derive(Deserialize)]
struct WrapManifestFile {
    #[serde(default, alias = "install")]
    install_roots: Vec<WrapInstallRootFile>,
}

#[derive(Deserialize)]
struct WrapInstallRootFile {
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

fn default_manifest_kind() -> String {
    "modloader".to_string()
}

fn default_manifest_enabled() -> bool {
    true
}

fn manifest_install_root_from_json(root: WrapInstallRootFile) -> ManifestInstallRoot {
    ManifestInstallRoot {
        source: normalize_path(&root.source),
        target: normalize_path(&root.target),
        kind: target_kind_from_manifest(&root.kind),
        optional: root.optional,
        enabled: root.enabled,
        notes: root.notes,
    }
}

fn target_kind_from_manifest(kind: &str) -> TargetKind {
    match kind.to_ascii_lowercase().as_str() {
        "cleo" | "cleo_text" => TargetKind::Cleo,
        "asi" | "plugin" => TargetKind::Asi,
        "bootstrap" | "runtime" => TargetKind::Bootstrap,
        "direct" | "directmanaged" | "direct_managed" => TargetKind::DirectManaged,
        _ => TargetKind::ModLoader,
    }
}

fn classify_entries(report: &mut PackageReport) {
    let mut roots: BTreeMap<String, InstallCandidate> = BTreeMap::new();
    let mut state = EntryClassificationState::default();

    for entry in &report.entries {
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
) {
    let path = normalize_path(&entry.path);
    let lower = path.to_ascii_lowercase();
    let name = file_name(&lower);

    if !entry.is_dir && is_readme_name(name) {
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
) {
    if entry.is_dir {
        if let Some(option) = detect_option_group(path) {
            state.option_roots.insert(option);
        }
        return;
    }

    if let Some(candidate) = detect_install_candidate(path, entry.size) {
        merge_install_candidate(roots, candidate);
    }
}

fn merge_install_candidate(
    roots: &mut BTreeMap<String, InstallCandidate>,
    candidate: InstallCandidate,
) {
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

fn list_folder_entries_inner(
    root: &Path,
    dir: &Path,
    entries: &mut Vec<PackageEntry>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = push_folder_entry(root, &entry, entries)?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            list_folder_entries_inner(root, &path, entries)?;
        }
    }
    Ok(())
}

fn push_folder_entry(
    root: &Path,
    entry: &fs::DirEntry,
    entries: &mut Vec<PackageEntry>,
) -> Result<PathBuf, AppError> {
    let path = entry.path();
    let metadata = entry.metadata()?;
    let package_entry = package_entry_from_folder(root, &path, &metadata);
    entries.push(package_entry);
    Ok(path)
}

fn package_entry_from_folder(root: &Path, path: &Path, metadata: &fs::Metadata) -> PackageEntry {
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/"); // literal: allow external interface text or file-format spelling
    let size = if metadata.is_file() {
        metadata.len()
    } else {
        0
    };
    PackageEntry {
        path: rel,
        size,
        is_dir: metadata.is_dir(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    #[test]
    fn wrap_manifest_roots_are_parsed_and_preferred_over_heuristics() {
        let root = test_root("wrap_manifest");
        let package = root.join("explicit.wrap");
        write_zip_package(
            &package,
            &[
                (
                    "wrap.json",
                    r#"{
                        "install_roots": [
                            {
                                "source": "files/CLEO",
                                "target": "CLEO",
                                "kind": "cleo",
                                "notes": ["declared by manifest"]
                            }
                        ]
                    }"#,
                ),
                (
                    "README.txt",
                    "Copy the CLEO folder to your game root. Requires CLEO.",
                ),
                ("files/CLEO/gravityfix.cs", "script"),
            ],
        );

        let report = analyze_package(&package, &root).unwrap();
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = build_install_plan(&report, &options);

        assert_eq!(report.manifest_roots.len(), 1);
        assert_eq!(report.readme_documents.len(), 1);
        assert!(report.readme_documents[0].text.contains("Requires CLEO"));
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "files/CLEO");
        assert_eq!(plan.operations[0].target_root, root.join("CLEO"));
        assert_eq!(plan.operations[0].notes, vec!["declared by manifest"]);
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn wrap_without_manifest_is_rejected() {
        let root = test_root("wrap_missing_manifest");
        let package = root.join("missing.wrap");
        write_zip_package(&package, &[("CLEO/gravityfix.cs", "script")]);

        let err = analyze_package_error(&package, &root).to_string();

        assert!(err.contains("must contain wrap.json"));
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn wrap_manifest_validation_reports_bad_roots() {
        let root = test_root("wrap_bad_manifest");
        let package = root.join("bad.wrap");
        write_zip_package(
            &package,
            &[(
                "wrap.json",
                r#"{
                    "install_roots": [
                        {
                            "source": "../outside",
                            "target": "CLEO",
                            "kind": "cleo"
                        }
                    ]
                }"#,
            )],
        );

        let err = analyze_package_error(&package, &root).to_string();

        assert!(err.contains("install_roots[0].source"));
        assert!(err.contains("relative package path"));
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn wrap_manifest_validation_requires_existing_source() {
        let root = test_root("wrap_missing_source");
        let package = root.join("missing-source.wrap");
        write_zip_package(
            &package,
            &[(
                "wrap.json",
                r#"{
                    "install_roots": [
                        {
                            "source": "files/CLEO",
                            "target": "CLEO",
                            "kind": "cleo"
                        }
                    ]
                }"#,
            )],
        );

        let err = analyze_package_error(&package, &root).to_string();

        assert!(err.contains("source does not exist"));
        assert!(err.contains("files/CLEO"));
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn messy_readme_copy_instruction_drives_plan_when_confident() {
        let root = test_root("readme_instruction");
        let package = root.join("messy.zip");
        write_zip_package(
            &package,
            &[
                (
                    "InstallEN.txt",
                    "Manual install:\n- Transfer the contents of the folder CLEO into your GTA SA directory.",
                ),
                ("CLEO/gravityfix.cs", "script"),
            ],
        );

        let report = analyze_package(&package, &root).unwrap();
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        // Inject explicit thresholds so plan inclusion depends only on the code
        // under test, not on the developer's local config file.
        let plan = crate::planning::build::build_install_plan_with(
            &report,
            &options,
            crate::settings::ReadmeThresholds::default(),
        );

        assert_eq!(report.readme_instructions.len(), 1);
        assert_eq!(
            report.readme_instructions[0].source.as_deref(),
            Some("CLEO")
        );
        assert_eq!(
            report.readme_instructions[0].target.as_deref(),
            Some("CLEO")
        );
        assert!(report.readme_instructions[0].confidence >= 0.85);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "CLEO");
        assert_eq!(plan.operations[0].target_root, root.join("CLEO"));
        assert!(plan.operations[0].notes[0].contains("readme evidence"));
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn vague_readme_instruction_does_not_override_heuristics() {
        let root = test_root("vague_readme_instruction");
        let package = root.join("vague.zip");
        write_zip_package(
            &package,
            &[
                ("README.txt", "Install the files as usual."),
                ("CLEO/gravityfix.cs", "script"),
            ],
        );

        let report = analyze_package(&package, &root).unwrap();
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = crate::planning::build::build_install_plan_with(
            &report,
            &options,
            crate::settings::ReadmeThresholds::default(),
        );

        assert_eq!(report.readme_instructions.len(), 1);
        assert!(report.readme_instructions[0].confidence < 0.85);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "CLEO");
        assert!(
            !plan.operations[0]
                .notes
                .iter()
                .any(|note| note.contains("readme evidence"))
        );
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn readme_infers_missing_cleo_source_from_package_contents() {
        let root = test_root("readme_infer_cleo_source");
        let package = root.join("cleo-source.zip");
        write_zip_package(
            &package,
            &[
                ("README.txt", "Copy the files into the CLEO folder."),
                ("scripts/gravityfix.cs", "script"),
                ("scripts/gravityfix.fxt", "text"),
            ],
        );

        let report = analyze_package(&package, &root).unwrap();
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = build_install_plan(&report, &options);

        assert_eq!(report.readme_instructions.len(), 1);
        assert_eq!(
            report.readme_instructions[0].source.as_deref(),
            Some("scripts")
        );
        assert_eq!(
            report.readme_instructions[0].target.as_deref(),
            Some("CLEO")
        );
        assert!(report.readme_instructions[0].confidence >= 0.85);
        assert!(
            report.readme_instructions[0]
                .confidence_reasons
                .iter()
                .any(|reason| reason.contains("package contents"))
        );
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "scripts");
        assert_eq!(plan.operations[0].target_root, root.join("CLEO"));
        assert!(plan.operations[0].notes.len() >= 2);
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn readme_normalizes_weird_formatting_and_fuzzy_root_targets() {
        let root = test_root("readme_weird_formatting");
        let package = root.join("root-asi.zip");
        write_zip_package(
            &package,
            &[
                (
                    "README.txt",
                    "INSTALLATION:\n* DRAG-and-DROP the ASI files -> the folder where gta_sa.exe is.",
                ),
                ("plugin/limit_adjuster.asi", "asi"),
                ("plugin/limit_adjuster.ini", "ini"),
            ],
        );

        let report = analyze_package(&package, &root).unwrap();
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = build_install_plan(&report, &options);

        assert_eq!(report.readme_instructions.len(), 1);
        let instruction = &report.readme_instructions[0];
        assert_eq!(instruction.source.as_deref(), Some("plugin"));
        assert_eq!(instruction.target.as_deref(), Some("."));
        assert!(instruction.normalized_text.contains("drag and drop"));
        assert!(instruction.normalized_text.contains("gta sa exe"));
        assert!(instruction.confidence >= 0.85);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "plugin");
        assert_eq!(plan.operations[0].target_root, root);
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn readme_fuses_multiline_install_paragraph_context() {
        let root = test_root("readme_multiline_context");
        let package = root.join("multiline.zip");
        write_zip_package(
            &package,
            &[
                (
                    "README.txt",
                    "Installation:\nCopy the files.\nDestination: CLEO folder.",
                ),
                ("scripts/gravityfix.cs", "script"),
            ],
        );

        let report = analyze_package(&package, &root).unwrap();
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = build_install_plan(&report, &options);

        assert_eq!(report.readme_instructions.len(), 1);
        let copy = report
            .readme_instructions
            .iter()
            .find(|instruction| matches!(instruction.action, ReadmeAction::Copy))
            .unwrap();
        assert_eq!(copy.source.as_deref(), Some("scripts"));
        assert_eq!(copy.target.as_deref(), Some("CLEO"));
        assert!(copy.normalized_text.contains("destination cleo folder"));
        assert!(copy.confidence >= 0.85);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "scripts");
        assert_eq!(plan.operations[0].target_root, root.join("CLEO"));
        remove_dir_if_exists(&root).unwrap();
    }

    #[test]
    fn readme_maps_loose_model_files_to_modloader_gta3_img() {
        let root = test_root("readme_loose_models");
        let package = root.join("vehicle.zip");
        write_zip_package(
            &package,
            &[
                ("README.txt", "Copy the files into gta3.img."),
                ("models/infernus.dff", "model"),
                ("models/infernus.txd", "texture"),
            ],
        );

        let report = analyze_package(&package, &root).unwrap();
        let options = CommandOptions {
            game_root: root.clone(),
            profile: "default".to_string(),
            includes: BTreeSet::new(),
            excludes: BTreeSet::new(),
            write_manifest: false,
        };
        let plan = build_install_plan(&report, &options);

        assert_eq!(report.readme_instructions.len(), 1);
        let instruction = &report.readme_instructions[0];
        assert_eq!(instruction.source.as_deref(), Some("models"));
        assert_eq!(
            instruction.target.as_deref(),
            Some("modloader/vehicle/gta3.img")
        );
        assert!(instruction.confidence >= 0.85);
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].source_root, "models");
        assert_eq!(
            plan.operations[0].target_root,
            root.join("modloader").join("vehicle").join("gta3.img")
        );
        remove_dir_if_exists(&root).unwrap();
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

    fn analyze_package_error(package: &Path, root: &Path) -> AppError {
        match analyze_package(package, root) {
            Ok(_) => panic!("expected analyze_package to fail"),
            Err(err) => err,
        }
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
