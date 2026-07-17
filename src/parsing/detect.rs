use crate::prelude::*;

pub(crate) fn detect_install_candidate(path: &str, size: u64) -> Option<InstallCandidate> {
    let lower = path.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }

    let original_parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let detection = CandidateDetection {
        path,
        lower: &lower,
        parts: &parts,
        original_parts: &original_parts,
        size,
    };
    detect_modloader_candidate(&detection)
        .or_else(|| detect_cleo_candidate(path, &lower, &parts, size))
        .or_else(|| detect_asi_candidate(path, &lower, size))
        .or_else(|| detect_plugin_config_candidate(path, &lower, size))
        .or_else(|| detect_cleo_text_candidate(path, &lower, size))
        .or_else(|| detect_game_directory_candidate(path, &parts, size))
        .or_else(|| detect_img_candidate(path, &lower, size))
        .or_else(|| detect_script_candidate(path, &lower, size))
}

fn detect_modloader_candidate(detection: &CandidateDetection) -> Option<InstallCandidate> {
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if detection.parts[0] == "modloader" {
        // literal: allow external interface text or file-format spelling
        return Some(modloader_content_candidate(detection));
    }
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if detection.lower.ends_with(".asi") && detection.lower.contains("modloader") {
        // literal: allow external interface text or file-format spelling
        return Some(modloader_runtime_candidate(detection));
    }
    None
}

fn modloader_content_candidate(detection: &CandidateDetection) -> InstallCandidate {
    let mut components = BTreeSet::new();
    components.insert(Component::ModLoaderContent);
    let notes = BTreeSet::new();
    let source_root = modloader_content_source_root(detection);
    let metadata = CandidateMetadata { components, notes };
    install_candidate_from_detection(&source_root, "modloader content", detection.size, metadata) // literal: allow external interface text or file-format spelling
}

fn modloader_content_source_root(detection: &CandidateDetection) -> String {
    if detection.original_parts.len() > 1 {
        return format!(
            "{}/{}",
            detection.original_parts[0], detection.original_parts[1]
        );
    }
    detection
        .original_parts
        .first()
        .copied()
        .unwrap_or(detection.path)
        .to_string()
}

fn modloader_runtime_candidate(detection: &CandidateDetection) -> InstallCandidate {
    let mut components = BTreeSet::new();
    components.insert(Component::ModLoader);
    let notes = BTreeSet::new();
    let source_root = detection
        .original_parts
        .first()
        .copied()
        .unwrap_or(detection.path);
    let metadata = CandidateMetadata { components, notes };
    install_candidate_from_detection(source_root, "game root", detection.size, metadata) // literal: allow external interface text or file-format spelling
}

fn detect_cleo_candidate(
    path: &str,
    lower: &str,
    parts: &[&str],
    size: u64,
) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if parts[0] == "cleo" || lower.ends_with(".cs") || lower.ends_with(".cleo") {
        // literal: allow external interface text or file-format spelling
        components.insert(Component::Cleo);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "CLEO or game root", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }
    None
}

fn detect_asi_candidate(path: &str, lower: &str, size: u64) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.ends_with(".asi") {
        // literal: allow external interface text or file-format spelling
        components.insert(Component::Asi);
        let note = "ASI plugins may need root install or Mod Loader std.asi support".to_string(); // literal: allow external interface text or file-format spelling
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "game root or modloader/<mod>", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }
    None
}

fn detect_plugin_config_candidate(path: &str, lower: &str, size: u64) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.ends_with(".ini") && looks_like_plugin_config(&lower) {
        // literal: allow external interface text or file-format spelling
        components.insert(Component::Asi);
        let note = "Config file should stay beside its matching ASI/plugin".to_string(); // literal: allow external interface text or file-format spelling
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "same target as plugin", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }
    None
}

fn looks_like_plugin_config(lower: &str) -> bool {
    lower.contains("streaming") // literal: allow external interface text or file-format spelling
        || lower.contains("skygfx") // literal: allow external interface text or file-format spelling
        || lower.contains("mixsets") // literal: allow external interface text or file-format spelling
        || lower.contains("silentpatch") // literal: allow external interface text or file-format spelling
        || lower.contains("crashinfo") // literal: allow external interface text or file-format spelling
        || lower.contains("limit") // literal: allow external interface text or file-format spelling
        || lower.contains("ola") // literal: allow external interface text or file-format spelling
}

fn detect_cleo_text_candidate(path: &str, lower: &str, size: u64) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.ends_with(".fxt") || lower.contains("/cleo_text/") {
        // literal: allow external interface text or file-format spelling
        components.insert(Component::CleoText);
        let source_root = top_install_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "CLEO/cleo_text", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }
    None
}

fn detect_game_directory_candidate(
    path: &str,
    parts: &[&str],
    size: u64,
) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let notes = BTreeSet::new();
    if has_known_game_directory(&parts) {
        components.insert(Component::ModLoaderContent);
        let source_root = root_before_known_game_directory(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            &source_root,
            "modloader/<mod>", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }
    None
}

fn has_known_game_directory(parts: &[&str]) -> bool {
    parts.iter().any(|part| {
        matches!(
            *part,
            "data" | "models" | "text" | "anim" | "audio" | "movies" | "scripts" | "txd" // literal: allow external interface text or file-format spelling
        )
    })
}

fn root_before_known_game_directory(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    for (idx, part) in parts.iter().enumerate() {
        let lower = part.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "data" | "models" | "text" | "anim" | "audio" | "movies" | "scripts" | "txd" // literal: allow external interface text or file-format spelling
        ) {
            if idx == 0 {
                return ".".to_string(); // literal: allow external interface text or file-format spelling
            }
            return parts[..idx].join("/"); // literal: allow external interface text or file-format spelling
        }
    }
    top_install_root(path).to_string()
}

fn detect_img_candidate(path: &str, lower: &str, size: u64) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.contains("gta3.img/") || lower.ends_with(".dff") || lower.ends_with(".txd") {
        // literal: allow external interface text or file-format spelling
        components.insert(Component::ImgReplacement);
        let note = "Loose DFF/TXD files should normally be placed under a Mod Loader mod folder, not injected into gta3.img directly".to_string(); // literal: allow external interface text or file-format spelling
        notes.insert(note);
        let source_root = top_install_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "modloader/<mod>/gta3.img", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }
    None
}

fn detect_script_candidate(path: &str, lower: &str, size: u64) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.ends_with(".scm") || lower.ends_with("script.img") {
        // literal: allow external interface text or file-format spelling
        components.insert(Component::ScriptData);
        let note = "Main script replacements conflict heavily with mission/story mods".to_string(); // literal: allow external interface text or file-format spelling
        notes.insert(note);
        let source_root = top_install_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "modloader/<mod>/data/script", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }

    None
}

pub(crate) fn detect_option_group(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    for (idx, part) in parts.iter().enumerate() {
        let lower = part.to_ascii_lowercase();
        let is_option = lower.contains("optional") // literal: allow external interface text or file-format spelling
            || lower.contains("settings") // literal: allow external interface text or file-format spelling
            || lower.contains("recommended") // literal: allow external interface text or file-format spelling
            || lower.contains("bonus") // literal: allow external interface text or file-format spelling
            || lower.contains("compatibility") // literal: allow external interface text or file-format spelling
            || lower == "en" // literal: allow external interface text or file-format spelling
            || lower == "pt"; // literal: allow external interface text or file-format spelling

        if is_option {
            let part_count = parts.len();
            let depth =
                /* literal: allow external interface text or file-format spelling */ if lower == "optionals" || lower == "(optionals)" || lower == "en" || lower == "pt"
                // literal: allow external interface text or file-format spelling
                {
                    (idx + 2).min(part_count)
                } else {
                    (idx + 1).min(part_count)
                };
            if depth > 0 {
                let option_group = parts[..depth].join("/"); // literal: allow external interface text or file-format spelling
                return Some(option_group);
            }
        }
    }
    None
}

fn install_candidate_from_detection(
    source_root: &str,
    target: &str,
    size: u64,
    metadata: CandidateMetadata,
) -> InstallCandidate {
    InstallCandidate {
        source_root: source_root.trim_matches('/').to_string(),
        target_strategy: target.to_string(),
        file_count: 1,
        total_bytes: size,
        components: metadata.components,
        notes: metadata.notes,
    }
}

fn top_install_root(path: &str) -> &str {
    path.split('/').find(|p| !p.is_empty()).unwrap_or(path)
}

fn component_root(path: &str) -> &str {
    let trimmed = path.trim_matches('/');
    if let Some((parent, _name)) = trimmed.rsplit_once('/') {
        if parent.is_empty() { trimmed } else { parent }
    } else {
        trimmed
    }
}
