
fn add_language_insights(report: &PackageReport, insights: &mut Vec<ReadmeInsight>)
{
    for document in &report.readme_documents
    {
        let normalized = normalize_readme_line(&document.text);
        for (language, confidence) in detected_language_packs(&normalized)
        {
            let insight = readme_insight!(
                ReadmeInsightKind::Language,
                format!("{language} keyword pack"),
                format!(
                    "detected {language} install keywords; deterministic multilingual rules were applied"
                ),
                ReadmeInsightMeta {
                    source_readme: &document.path,
                    line_number: 0,
                    evidence: "",
                    confidence,
                    rule_id: &format!("language.{language}"),
                },
            );
            insights.push(insight);
        }
    }
}

struct ReadmeInsightText
{
    title: String,
    detail: String,
}

struct ReadmeInsightMeta<'a>
{
    source_readme: &'a str,
    line_number: usize,
    evidence: &'a str,
    confidence: f32,
    rule_id: &'a str,
}

fn detected_language_packs(text: &str) -> Vec<(&'static str, f32)>
{
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
    return packs
        .iter()
        .filter_map(|(language, words)| {
            let hits = words.iter().filter(|word| text.contains(**word)).count();
            if hits >= LANGUAGE_PACK_MIN_HITS
            {
                Some((
                    *language,
                    (LANGUAGE_PACK_BASE_CONFIDENCE + (hits as f32 * LANGUAGE_PACK_HIT_CONFIDENCE))
                        .min(LANGUAGE_PACK_MAX_CONFIDENCE),
                ))
            }
            else
            {
                None
            }
        })
        .collect();
}
fn build_readme_insight(
    kind: ReadmeInsightKind,
    text: ReadmeInsightText,
    meta: ReadmeInsightMeta<'_>,
) -> ReadmeInsight
{
    return ReadmeInsight {
        kind,
        title: text.title,
        detail: text.detail,
        source_readme: meta.source_readme.to_string(),
        line_number: meta.line_number,
        evidence: meta.evidence.to_string(),
        confidence: meta.confidence,
        rule_id: meta.rule_id.to_string(),
    };
}

fn detected_layout_templates(report: &PackageReport) -> Vec<String>
{
    let mut layouts = Vec::new();
    let has = |component| report.component_paths.contains(&component);
    if has(Component::ModLoaderContent)
        && (has(Component::Data)
            || has(Component::Models)
            || has(Component::Text)
            || has(Component::Anim)
            || has(Component::Audio))
    {
        layouts.push(
            "modloader mirror: package has game-folder structure suitable for modloader"
                .to_string(),
        );
    }
    if has(Component::Cleo) || has(Component::CleoText) || has(Component::CleoPlugin)
    {
        layouts.push("CLEO package: .cs/.cs4/.cs3 â†’ CLEO/, .cleo plugins â†’ CLEO/cleo_plugins/, .fxt â†’ CLEO/cleo_text/".to_string());
    }
    if has(Component::Asi)
    {
        layouts.push("ASI root package: .asi/.dll/.ini payload likely belongs in the game root or ASI-supported modloader folder".to_string());
    }
    if has(Component::ImgReplacement)
    {
        layouts.push("gta3.img payload: loose .dff/.txd/.col files should be isolated under modloader/<mod>/gta3.img".to_string());
    }
    if has(Component::Text)
    {
        layouts
            .push("language/text package: .gxt or text assets map to the text folder".to_string());
    }
    if report.entries.iter().any(|entry| {
        let path = normalize_path(&entry.path).to_ascii_lowercase();
        path.contains("/data/")
            || path.contains("/models/")
            || path.contains("/text/")
            || path.contains("/anim/")
            || path.contains("/audio/")
    })
    {
        layouts.push("root mirror: archive contains direct game-folder names".to_string());
    }
    // std.stream layout requirements ModLoader enforces by folder name.
    if report
        .entries
        .iter()
        .any(|entry| is_streaming_nodes(&normalize_path(&entry.path)))
    {
        layouts.push("streaming nodes: nodesN.dat load only inside an *.img folder â€” place under modloader/<mod>/gta3.img/".to_string());
    }
    if report.entries.iter().any(|entry| {
        let lower = normalize_path(&entry.path).to_ascii_lowercase();
        lower
            .split('/')
            .any(|seg| seg == "player.img" || seg == "player_img")
    })
    {
        layouts.push("clothing: new clothes must sit in a folder named player.img â€” modloader/<mod>/player.img/".to_string());
    }
    layouts.sort();
    layouts.dedup();
    return layouts;
}

/// A `nodes<N>.dat` path-streaming file (nodes0.dat â€¦ nodes63.dat), which
/// ModLoader's std.stream only loads from inside an `*.img` folder.
fn is_streaming_nodes(path: &str) -> bool
{
    let name = file_name(&path.to_ascii_lowercase()).to_string();
    let Some(stem) = name.strip_suffix(".dat") else {
        return false;
    };
    let Some(digits) = stem.strip_prefix("nodes") else {
        return false;
    };
    return !digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit());
}

struct ReadmeLineContext
{
    line_number: usize,
    original: String,
    normalized: String,
    paragraph: String,
    section: Option<String>,
}

struct ConfidenceScore
{
    value: f32,
    reasons: Vec<String>,
}

struct ReadmeInstructionContext<'a>
{
    document: &'a ReadmeDocument,
    line_number: usize,
    text: &'a str,
    normalized_text: &'a str,
}

struct ReadmeInstructionDraft
{
    action: ReadmeAction,
    source: Option<String>,
    target: Option<String>,
    confidence: ConfidenceScore,
}

struct CopyInstructionScoreInput<'a>
{
    source: &'a Option<String>,
    target: &'a Option<String>,
    line: &'a str,
    install_verb: bool,
    install_section: bool,
}

fn readme_line_contexts(text: &str) -> Vec<ReadmeLineContext>
{
    let mut section = None;
    let mut contexts = Vec::new();
    let lines = text.lines().collect::<Vec<_>>();
    for (idx, line) in lines.iter().enumerate()
    {
        let trimmed = line.trim();
        if trimmed.is_empty()
        {
            continue;
        }
        let normalized = normalize_readme_line(trimmed);
        if is_readme_section_heading(&normalized)
        {
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
    return contexts;
}

fn readme_paragraph_context(lines: &[&str], index: usize) -> String
{
    let mut start = index;
    while start > 0 && has_readme_line_paragraph_text(lines[start - 1])
    {
        start -= 1;
    }
    let mut end = index;
    while end + 1 < lines.len() && has_readme_line_paragraph_text(lines[end + 1])
    {
        end += 1;
    }
    return lines[start..=end]
        .iter()
        .map(|line| normalize_readme_line(line.trim()))
        .filter(|line| !line.is_empty() && !is_readme_section_heading(line))
        .collect::<Vec<_>>()
        .join(" ");
}

fn has_readme_line_paragraph_text(line: &str) -> bool
{
    return !line.trim().is_empty();
}

fn readme_instruction_from_line(
    document: &ReadmeDocument,
    context: &ReadmeLineContext,
    source_candidates: &[String],
    report: &PackageReport,
) -> Option<ReadmeInstruction>
{
    let trimmed = context.original.trim();
    if trimmed.is_empty()
    {
        return None;
    }
    let lower = context.normalized.as_str();
    let evidence_context = combined_readme_context(ReadmeContextParts { line: lower, paragraph: &context.paragraph, section: &context.section });
    let instruction_context = ReadmeInstructionContext {
        document,
        line_number: context.line_number,
        text: trimmed,
        normalized_text: &evidence_context,
    };
    if has_skip_install_marker(lower)
    {
        return Some(readme_instruction(
            &instruction_context,
            ReadmeInstructionDraft {
                action: ReadmeAction::DoNotInstall,
                source: None,
                target: None,
                confidence: ConfidenceScore {
                    value: EXPLICIT_CLEO_CONFIDENCE,
                    reasons: vec!["explicit do-not-install wording".to_string()],
                },
            },
        ));
    }
    if has_conflict_marker(lower)
    {
        return Some(readme_instruction(
            &instruction_context,
            ReadmeInstructionDraft {
                action: ReadmeAction::Conflict,
                source: None,
                target: None,
                confidence: ConfidenceScore {
                    value: CLEO_TEXT_CONFIDENCE,
                    reasons: vec!["compatibility/conflict wording".to_string()],
                },
            },
        ));
    }
    if has_load_after_marker(lower)
    {
        return Some(readme_instruction(
            &instruction_context,
            ReadmeInstructionDraft {
                action: ReadmeAction::LoadAfter,
                source: None,
                target: None,
                confidence: ConfidenceScore {
                    value: ROOT_HINT_CONFIDENCE,
                    reasons: vec!["load-order wording".to_string()],
                },
            },
        ));
    }
    if has_optional_marker(lower)
    {
        let optional_source = infer_source_from_line(&evidence_context, source_candidates);
        return Some(readme_instruction(
            &instruction_context,
            ReadmeInstructionDraft {
                action: ReadmeAction::Optional,
                source: optional_source,
                target: None,
                confidence: ConfidenceScore {
                    value: WRAP_HINT_CONFIDENCE,
                    reasons: vec!["optional/bonus wording".to_string()],
                },
            },
        ));
    }
    if has_requirement(lower)
    {
        return Some(readme_instruction(
            &instruction_context,
            ReadmeInstructionDraft {
                action: ReadmeAction::Requires,
                source: None,
                target: requirement_target_from_line(&evidence_context),
                confidence: ConfidenceScore {
                    value: GTA3_IMG_CONFIDENCE,
                    reasons: vec!["requirement wording".to_string()],
                },
            },
        ));
    }
    let install_verb = has_install_verb(lower);
    let install_section = context
        .section
        .as_deref()
        .map(is_install_section_heading)
        .unwrap_or(false);
    if !install_verb
    {
        return None;
    }
    let mut source = infer_source_from_line(&evidence_context, source_candidates);
    let target = infer_target_from_line(&evidence_context, source.as_deref(), report);
    if source.is_none()
    {
        source = infer_source_from_target_and_contents(target.as_deref(), report);
    }
    let confidence = copy_instruction_score(CopyInstructionScoreInput {
        source: &source,
        target: &target,
        line: &evidence_context,
        install_verb,
        install_section,
    });
    return Some(readme_instruction(
        &instruction_context,
        ReadmeInstructionDraft {
            action: ReadmeAction::Copy,
            source,
            target,
            confidence,
        },
    ));
}
struct ReadmeContextParts<'a>
{
    line: &'a str,
    paragraph: &'a str,
    section: &'a Option<String>,
}

fn combined_readme_context(context: ReadmeContextParts<'_>) -> String
{
    let line = context.line;
    let paragraph = context.paragraph;
    let section = context.section;
    let mut parts = Vec::new();
    if let Some(section) = section
    {
        parts.push(section.as_str());
    }
    if !paragraph.is_empty()
    {
        parts.push(paragraph);
    }
    if !parts.iter().any(|part| *part == line)
    {
        parts.push(line);
    }
    return parts.join(" ");
}

fn readme_instruction(
    context: &ReadmeInstructionContext<'_>,
    draft: ReadmeInstructionDraft,
) -> ReadmeInstruction
{
    return ReadmeInstruction {
        source_readme: context.document.path.clone(),
        line_number: context.line_number,
        action: draft.action,
        source: draft.source,
        target: draft.target,
        confidence: draft.confidence.value,
        text: context.text.to_string(),
        normalized_text: context.normalized_text.to_string(),
        confidence_reasons: draft.confidence.reasons,
    };
}
