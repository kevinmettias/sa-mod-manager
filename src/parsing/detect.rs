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
        .or_else(|| detect_cleo_plugin_candidate(path, &lower, &parts, size))
        .or_else(|| detect_cleo_modules_candidate(path, &parts, size))
        .or_else(|| detect_cleo_saves_candidate(path, &parts, size))
        .or_else(|| detect_cleo_text_candidate(path, &lower, &parts, size))
        .or_else(|| detect_cleo_candidate(path, &lower, &parts, size))
        .or_else(|| detect_asi_candidate(path, &lower, size))
        .or_else(|| detect_plugin_config_candidate(path, &lower, size))
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

fn detect_cleo_plugin_candidate(
    path: &str,
    lower: &str,
    parts: &[&str],
    size: u64,
) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.ends_with(".cleo") || parts.contains(&"cleo_plugins") {
        // literal: allow external interface text or file-format spelling
        components.insert(Component::CleoPlugin);
        let note = "CLEO5 plugin module; loads from CLEO/cleo_plugins".to_string(); // literal: allow external interface text or file-format spelling
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "CLEO/cleo_plugins", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }
    None
}

fn detect_cleo_modules_candidate(
    path: &str,
    parts: &[&str],
    size: u64,
) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if parts.contains(&"cleo_modules") {
        // literal: allow external interface text or file-format spelling
        components.insert(Component::CleoModules);
        let note = "CLEO script module; loads from CLEO/cleo_modules (the modules: path)".to_string(); // literal: allow external interface text or file-format spelling
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "CLEO/cleo_modules", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }
    None
}

fn detect_cleo_saves_candidate(
    path: &str,
    parts: &[&str],
    size: u64,
) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if parts.contains(&"cleo_saves") {
        // literal: allow external interface text or file-format spelling
        components.insert(Component::CleoSaves);
        let note = "CLEO save data is runtime-generated user data; shipping it in a mod can overwrite the player's own saves".to_string(); // literal: allow external interface text or file-format spelling
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "CLEO/cleo_saves", // literal: allow external interface text or file-format spelling
            size,
            metadata,
        ));
    }
    None
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
    if parts[0] == "cleo" || has_cleo_script_extension(lower) {
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
        // Mod Loader's std.asi loads .asi/.cleo/.dll (and CLEO scripts) straight
        // from a mod folder, so an ASI can live in the game root beside a loader,
        // or self-contained inside a modloader/<mod> sandbox.
        let note = "ASI loads from the game root (with an ASI loader) or directly from a modloader/<mod> folder via Mod Loader's std.asi".to_string(); // literal: allow external interface text or file-format spelling
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

fn detect_cleo_text_candidate(
    path: &str,
    lower: &str,
    parts: &[&str],
    size: u64,
) -> Option<InstallCandidate> {
    let mut components = BTreeSet::new();
    let notes = BTreeSet::new();
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if lower.ends_with(".fxt") || parts.contains(&"cleo_text") {
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
        // Mod Loader's std.stream injects loose DFF/TXD from the mod folder into
        // the correct archive automatically, so the default is the sandbox root;
        // a `gta3.img/` subfolder is only needed to pin a specific archive.
        let note = "Loose DFF/TXD load from the Mod Loader mod folder — std.stream routes them into the right archive; no gta3.img subfolder needed".to_string(); // literal: allow external interface text or file-format spelling
        notes.insert(note);
        let source_root = top_install_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            source_root,
            "modloader/<mod>", // literal: allow external interface text or file-format spelling
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

#[cfg(test)]
mod tests {
    use super::*;

    fn components(path: &str) -> BTreeSet<Component> {
        detect_install_candidate(path, 100)
            .unwrap_or_else(|| panic!("no candidate detected for {path}"))
            .components
    }

    #[test]
    fn cleo5_script_and_compat_scripts_route_to_cleo() {
        for path in ["cleo/speedo.cs", "cleo/legacy.cs4", "cleo/older.cs3"] {
            assert_eq!(
                components(path),
                BTreeSet::from([Component::Cleo]),
                "{path} should be a CLEO script"
            );
        }
    }

    #[test]
    fn cleo_modules_and_saves_route_to_their_own_components() {
        let modules = components("cleo/cleo_modules/shared.txt");
        assert!(
            modules.contains(&Component::CleoModules),
            "cleo_modules file should be a CLEO module, got {modules:?}"
        );
        assert!(!modules.contains(&Component::Cleo));

        let saves = components("cleo/cleo_saves/slot1.sav");
        assert!(
            saves.contains(&Component::CleoSaves),
            "cleo_saves file should be CLEO save data, got {saves:?}"
        );
        assert!(!saves.contains(&Component::Cleo));
    }

    #[test]
    fn cleo_plugin_routes_to_plugin_component_not_script() {
        // A .cleo plugin, whether loose or already under cleo_plugins/.
        for path in ["cleo/SA.IniFiles.cleo", "cleo/cleo_plugins/SA.Audio.cleo"] {
            let found = components(path);
            assert!(
                found.contains(&Component::CleoPlugin),
                "{path} should be a CLEO plugin, got {found:?}"
            );
            assert!(
                !found.contains(&Component::Cleo),
                "{path} must not also be a plain CLEO script"
            );
        }
    }

    #[test]
    fn fxt_and_cleo_text_folder_route_to_cleo_text() {
        for path in ["cleo/cleo_text/strings.fxt", "cleo_text/lang.fxt", "loose.fxt"] {
            assert_eq!(
                components(path),
                BTreeSet::from([Component::CleoText]),
                "{path} should be CLEO text"
            );
        }
    }
}
