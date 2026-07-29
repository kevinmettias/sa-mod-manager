fn normalize_readme_line(line: &str) -> String
{
    return line.replace('\\', "/")
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

fn infer_source_from_line(line: &str, candidates: &[String]) -> Option<String>
{
    let mut matches = candidates
        .iter()
        .filter(|candidate| source_candidate_matches_line(SourceCandidateLine { candidate: candidate, line: line }))
        .cloned()
        .collect::<Vec<_>>();
    matches.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    return matches.into_iter().next()
}

fn source_candidate_matches_line(context: SourceCandidateLine<'_>) -> bool
{
    let candidate = context.candidate;
    let line = context.line;
    let lower = candidate.to_ascii_lowercase().replace('\\', "/");
    let name = file_name(&lower);
    if name.len() < MIN_SOURCE_NAME_LEN
    {
        return false;
    }
    return line_contains_path(LinePath { line: line, path: &lower })
        || line_contains_token(LineNeedle { line: line, needle: name })
        || line.contains(&format!("folder {name}"))
        || line.contains(&format!("directory {name}"))
        || line.contains(&format!("contents of {name}"))
        || line.contains(&format!("files from {name}"))
}

struct SourceCandidateLine<'a>
{
    candidate: &'a str,
    line: &'a str,
}

fn infer_target_from_line(
    line: &str,
    source: Option<&str>,
    report: &PackageReport,
) -> Option<String>
{
    if mentions_gta3_img(line)
    {
        return Some(format!(
            "modloader/{}/gta3.img",
            safe_name(&package_id(&report.package))
        ));
    }
    if mentions_cleo(line)
    {
        return Some("CLEO".to_string());
    }
    if mentions_modloader(line)
    {
        return Some("modloader".to_string());
    }
    if mentions_game_data_folder(GameDataFolderMention { line: line, folder: "data" })
    {
        return Some("data".to_string());
    }
    if mentions_game_data_folder(GameDataFolderMention { line: line, folder: "models" })
    {
        return Some("models".to_string());
    }
    if mentions_game_data_folder(GameDataFolderMention { line: line, folder: "text" })
    {
        return Some("text".to_string());
    }
    if mentions_game_data_folder(GameDataFolderMention { line: line, folder: "anim" })
    {
        return Some("anim".to_string());
    }
    if mentions_game_data_folder(GameDataFolderMention { line: line, folder: "audio" })
    {
        return Some("audio".to_string());
    }
    if mentions_game_root(line)
    {
        if let Some(source) = source
        {
            let source_name = file_name(&source.to_ascii_lowercase()).to_string();
            if matches!(
                source_name.as_str(),
                "cleo" | "data" | "models" | "text" | "anim" | "audio" | "modloader"
            )
            {
                return Some(source_name);
            }
        }
        return Some(".".to_string());
    }
    return None
}

fn mentions_gta3_img(line: &str) -> bool
{
    return line.contains("gta3 img")
        || line.contains("gta3img")
        || line.contains("gta3 image")
        || line.contains("img archive")
        || line.contains("models/gta3")
        || line.contains("models gta3")
}

fn infer_source_from_target_and_contents(
    target: Option<&str>,
    report: &PackageReport,
) -> Option<String>
{
    let target = target?.to_ascii_lowercase();
    if target == "cleo_plugins" || target == "cleo_plugin" || target.ends_with("/cleo_plugins")
    {
        return source_with_extensions(report, &["cleo"])
            .or_else(|| source_named_like(report, &["cleo_plugins", "cleo_plugin"]));
    }
    if target == "cleo_text" || target.ends_with("/cleo_text")
    {
        return source_with_extensions(report, &["fxt"])
            .or_else(|| source_named_like(report, &["cleo_text"]));
    }
    if target == "cleo_modules" || target.ends_with("/cleo_modules")
    {
        return source_named_like(report, &["cleo_modules"]);
    }
    if target == "cleo_saves" || target.ends_with("/cleo_saves")
    {
        return source_named_like(report, &["cleo_saves"]);
    }
    if target == "cleo"
    {
        return source_with_extensions(report, &["cs", "cs4", "cs3", "fxt"])
            .or_else(|| source_named_like(report, &["cleo"]));
    }
    if target == "modloader"
    {
        return source_with_game_mirror(report)
            .or_else(|| source_with_extensions(report, &["dff", "txd"]))
            .or_else(|| source_named_like(report, &["modloader"]));
    }
    if target.ends_with("/gta3.img")
    {
        return source_with_extensions(report, &["dff", "txd"])
            .or_else(|| source_named_like(report, &["gta3.img", "models"]));
    }
    if matches!(
        target.as_str(),
        "data" | "models" | "text" | "anim" | "audio"
    )
    {
        return source_named_like(report, &[&target]);
    }
    if target == "."
    {
        return source_with_extensions(report, &["asi", "dll", "ini"]);
    }
    return None
}

fn source_with_extensions(report: &PackageReport, extensions: &[&str]) -> Option<String>
{
    let mut roots = BTreeSet::new();
    for entry in &report.entries
    {
        if entry.is_dir
        {
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
    return shortest_source(roots)
}

fn source_named_like(report: &PackageReport, names: &[&str]) -> Option<String>
{
    return package_source_candidates(report)
        .into_iter()
        .find(|candidate| {
            let name = file_name(&candidate.to_ascii_lowercase()).to_string();
            names.iter().any(|expected| &name == expected)
        })
}

fn package_source_candidates(report: &PackageReport) -> Vec<String>
{
    let mut candidates = BTreeSet::new();
    for entry in &report.entries
    {
        let path = normalize_path(&entry.path);
        let parts = path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        for depth in 1..=parts.len().min(MAX_TARGET_PATH_DEPTH)
        {
            candidates.insert(parts[..depth].join("/"));
        }
    }
    return candidates.into_iter().collect()
}

fn source_with_game_mirror(report: &PackageReport) -> Option<String>
{
    let mut roots = BTreeSet::new();
    for entry in &report.entries
    {
        let path = normalize_path(&entry.path);
        let parts = path.split('/').collect::<Vec<_>>();
        if parts.iter().any(|part| {
            matches!(
                part.to_ascii_lowercase().as_str(),
                "data" | "models" | "text" | "anim" | "audio"
            )
        })
        {
            roots.insert(top_level_or_parent_source(&path));
        }
    }
    return shortest_source(roots)
}

fn requirement_target_from_line(line: &str) -> Option<String>
{
    if mentions_cleo(line)
    {
        return Some("CLEO".to_string());
    }
    if mentions_modloader(line)
    {
        return Some("Mod Loader".to_string());
    }
    if line.contains("asi loader")
    {
        return Some("ASI Loader".to_string());
    }
    if line.contains("silentpatch") || line.contains("silent patch")
    {
        return Some("SilentPatch".to_string());
    }
    if line.contains("1 0 exe")
        || line.contains("original exe")
        || line.contains("hoodlum exe")
        || line.contains("us 1 0")
    {
        return Some("GTA SA 1.0 executable".to_string());
    }
    return None;
}

fn copy_instruction_score(input: CopyInstructionScoreInput<'_>) -> ConfidenceScore
{
    let mut confidence: f32 = COPY_SCORE_BASE;
    let mut reasons = Vec::new();
    if input.source.is_some()
    {
        confidence += COPY_SCORE_VERB_BONUS;
        reasons.push("source matched readme/package contents".to_string());
    }
    if input.target.is_some()
    {
        confidence += COPY_SCORE_TARGET_BONUS;
        reasons.push("target matched known GTA SA location".to_string());
    }
    if input.install_verb
    {
        confidence += COPY_SCORE_SOURCE_BONUS;
        reasons.push("install/copy verb present".to_string());
    }
    if input.install_section
    {
        confidence += COPY_SCORE_CONTENT_BONUS;
        reasons.push("inside install/manual section".to_string());
    }
    if mentions_game_root(input.line) || mentions_cleo(input.line) || mentions_modloader(input.line)
    {
        confidence += COPY_SCORE_CONTEXT_BONUS;
        reasons.push("target wording is explicit".to_string());
    }
    if input.line.contains("contents")
    {
        confidence -= COPY_SCORE_AMBIGUITY_PENALTY;
        reasons.push("source wording says contents; review destination shape".to_string());
    }
    if input.source.is_none()
    {
        reasons.push("source not found".to_string());
    }
    if input.target.is_none()
    {
        reasons.push("target not found".to_string());
    }
    return ConfidenceScore {
        value: confidence.clamp(0.0, COPY_SCORE_MAX),
        reasons,
    }
}

fn add_readme_instruction_hint(instruction: &ReadmeInstruction, hints: &mut BTreeSet<String>)
{
    let source = instruction.source.as_deref().unwrap_or("unknown source");
    let target = instruction.target.as_deref().unwrap_or("unknown target");
    hints.insert(format!(
        "readme instruction {} {} -> {} confidence {:.0}% ({}, line {})",
        instruction.action,
        source,
        target,
        instruction.confidence * CONFIDENCE_PERCENT_SCALE,
        instruction.source_readme,
        instruction.line_number
    ));
}

fn contains_install_verb(line: &str) -> bool
{
    return [
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
    .any(|word| line_contains_token(LineNeedle { line: line, needle: word }))
}

fn is_readme_section_heading(line: &str) -> bool
{
    return is_install_section_heading(line)
        || line_contains_token(LineNeedle { line: line, needle: "optional" })
        || line_contains_token(LineNeedle { line: line, needle: "requirements" })
        || line_contains_token(LineNeedle { line: line, needle: "requirement" })
        || line_contains_token(LineNeedle { line: line, needle: "compatibility" })
}

fn is_install_section_heading(line: &str) -> bool
{
    let words = line.split_whitespace().count();
    return words <= SHORT_LINE_WORD_LIMIT
        && (line_contains_token(LineNeedle { line: line, needle: "install" })
            || line_contains_token(LineNeedle { line: line, needle: "installation" })
            || line_contains_token(LineNeedle { line: line, needle: "manual" }))
}

fn contains_requirement(line: &str) -> bool
{
    return line.contains("requires")
        || line.contains("require ")
        || line.contains("requirement")
        || line.contains("dependencies")
        || line.contains("dependency")
        || line.contains("needed")
        || line.contains("need ")
        || line.contains("must have")
        || line.contains("you need")
}

fn contains_optional(line: &str) -> bool
{
    return line_contains_token(LineNeedle { line: line, needle: "optional" })
        || line_contains_token(LineNeedle { line: line, needle: "bonus" })
        || line.contains("for rosa")
        || line.contains("compatibility patch")
        || line.contains("if you want")
}

fn contains_conflict(line: &str) -> bool
{
    return line.contains("conflict")
        || line.contains("incompatible")
        || line.contains("do not use with")
        || line.contains("dont use with")
}

fn contains_load_after(line: &str) -> bool
{
    return line.contains("load after") || line.contains("priority after")
}

fn contains_do_not_install(line: &str) -> bool
{
    return line.contains("do not install")
        || line.contains("dont install")
        || line.contains("do not copy")
        || line.contains("dont copy")
}

fn top_level_or_parent_source(path: &str) -> String
{
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() <= 1
    {
        return ".".to_string();
    }
    return parts[..parts.len().saturating_sub(1).min(1)].join("/");
}

fn shortest_source(roots: BTreeSet<String>) -> Option<String>
{
    return roots
        .into_iter()
        .min_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)))
}

fn mentions_cleo(line: &str) -> bool
{
    return line_contains_token(LineNeedle { line: line, needle: "cleo" })
        || line.contains("cleo folder")
        || line.contains("cleo directory")
        || line.contains("cleo dir")
}

fn mentions_modloader(line: &str) -> bool
{
    return line.contains("modloader") || line.contains("mod loader") || line.contains("mod-loader");
}






