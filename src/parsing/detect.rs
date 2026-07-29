use crate::prelude::*;

pub(crate) fn detect_install_candidate(path: &str, size: u64) -> Option<InstallCandidate>
{
    let lower = path.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty()
    {
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
    return detect_modloader_candidate(&detection)
        .or_else(|| detect_cleo_plugin_candidate(&detection))
        .or_else(|| detect_cleo_modules_candidate(&detection))
        .or_else(|| detect_cleo_saves_candidate(&detection))
        .or_else(|| detect_cleo_text_candidate(&detection))
        .or_else(|| detect_cleo_candidate(&detection))
        .or_else(|| detect_asi_candidate(&detection))
        .or_else(|| detect_plugin_config_candidate(&detection))
        .or_else(|| detect_game_directory_candidate(&detection))
        .or_else(|| detect_img_candidate(&detection))
        .or_else(|| detect_script_candidate(&detection));
}

fn detect_modloader_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    if detection.parts[0] == "modloader"
    {
        return Some(modloader_content_candidate(detection));
    }
    if detection.lower.ends_with(".asi") && detection.lower.contains("modloader")
    {
        return Some(modloader_runtime_candidate(detection));
    }
    return None;
}

fn modloader_content_candidate(detection: &CandidateDetection) -> InstallCandidate
{
    let mut components = BTreeSet::new();
    components.insert(Component::ModLoaderContent);
    let notes = BTreeSet::new();
    let source_root = modloader_content_source_root(detection);
    let metadata = CandidateMetadata { components, notes };
    return install_candidate_from_detection(
        DetectedInstallTarget {
            source_root: &source_root,
            target: "modloader content",
        },
        detection.size,
        metadata,
    );
}

fn modloader_content_source_root(detection: &CandidateDetection) -> String
{
    if detection.original_parts.len() > 1
    {
        return format!(
            "{}/{}",
            detection.original_parts[0], detection.original_parts[1]
        );
    }
    return detection
        .original_parts
        .first()
        .copied()
        .unwrap_or(detection.path)
        .to_string();
}

fn modloader_runtime_candidate(detection: &CandidateDetection) -> InstallCandidate
{
    let mut components = BTreeSet::new();
    components.insert(Component::ModLoader);
    let notes = BTreeSet::new();
    let source_root = detection
        .original_parts
        .first()
        .copied()
        .unwrap_or(detection.path);
    let metadata = CandidateMetadata { components, notes };
    return install_candidate_from_detection(
        DetectedInstallTarget {
            source_root,
            target: "game root",
        },
        detection.size,
        metadata,
    );
}

fn detect_cleo_plugin_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let lower = detection.lower;
    let parts = detection.parts;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    if lower.ends_with(".cleo") || parts.contains(&"cleo_plugins")
    {
        components.insert(Component::CleoPlugin);
        let note = "CLEO5 plugin module; loads from CLEO/cleo_plugins".to_string();
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: source_root,
                target: "CLEO/cleo_plugins",
            },
            size,
            metadata,
        ));
    }
    return None;
}

fn detect_cleo_modules_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let parts = detection.parts;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    if parts.contains(&"cleo_modules")
    {
        components.insert(Component::CleoModules);
        let note =
            "CLEO script module; loads from CLEO/cleo_modules (the modules: path)".to_string();
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: source_root,
                target: "CLEO/cleo_modules",
            },
            size,
            metadata,
        ));
    }
    return None;
}

fn detect_cleo_saves_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let parts = detection.parts;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    if parts.contains(&"cleo_saves")
    {
        components.insert(Component::CleoSaves);
        let note = "CLEO save data is runtime-generated user data; shipping it in a mod can overwrite the player's own saves".to_string();
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: source_root,
                target: "CLEO/cleo_saves",
            },
            size,
            metadata,
        ));
    }
    return None;
}

fn detect_cleo_text_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let lower = detection.lower;
    let parts = detection.parts;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let notes = BTreeSet::new();
    if lower.ends_with(".fxt") || parts.contains(&"cleo_text")
    {
        components.insert(Component::CleoText);
        let source_root = top_install_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: source_root,
                target: "CLEO/cleo_text",
            },
            size,
            metadata,
        ));
    }
    return None;
}

fn detect_cleo_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let lower = detection.lower;
    let parts = detection.parts;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let notes = BTreeSet::new();
    if parts[0] == "cleo" || has_cleo_script_extension(lower)
    {
        components.insert(Component::Cleo);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: source_root,
                target: "CLEO or game root",
            },
            size,
            metadata,
        ));
    }
    return None;
}

fn detect_asi_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let lower = detection.lower;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    if lower.ends_with(".asi")
    {
        components.insert(Component::Asi);
        // Mod Loader's std.asi loads .asi/.cleo/.dll (and CLEO scripts) straight
        // from a mod folder, so an ASI can live in the game root beside a loader,
        // or self-contained inside a modloader/<mod> sandbox.
        let note = "ASI loads from the game root (with an ASI loader) or directly from a modloader/<mod> folder via Mod Loader's std.asi".to_string();
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: source_root,
                target: "game root or modloader/<mod>",
            },
            size,
            metadata,
        ));
    }
    return None;
}

fn detect_plugin_config_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let lower = detection.lower;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    if lower.ends_with(".ini") && looks_like_plugin_config(&lower)
    {
        components.insert(Component::Asi);
        let note = "Config file should stay beside its matching ASI/plugin".to_string();
        notes.insert(note);
        let source_root = component_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: source_root,
                target: "same target as plugin",
            },
            size,
            metadata,
        ));
    }
    return None;
}

fn looks_like_plugin_config(lower: &str) -> bool
{
    return lower.contains("streaming")
        || lower.contains("skygfx")
        || lower.contains("mixsets")
        || lower.contains("silentpatch")
        || lower.contains("crashinfo")
        || lower.contains("limit")
        || lower.contains("ola");
}

fn detect_game_directory_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let parts = detection.parts;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let notes = BTreeSet::new();
    if has_known_game_directory(&parts)
    {
        components.insert(Component::ModLoaderContent);
        let source_root = root_before_known_game_directory(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: &source_root,
                target: "modloader/<mod>",
            },
            size,
            metadata,
        ));
    }
    return None;
}

fn has_known_game_directory(parts: &[&str]) -> bool
{
    return parts.iter().any(|part| {
        matches!(
            *part,
            "data" | "models" | "text" | "anim" | "audio" | "movies" | "scripts" | "txd"
        )
    });
}

fn root_before_known_game_directory(path: &str) -> String
{
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    for (idx, part) in parts.iter().enumerate()
    {
        let lower = part.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "data" | "models" | "text" | "anim" | "audio" | "movies" | "scripts" | "txd"
        )
        {
            if idx == 0
            {
                return ".".to_string();
            }
            return parts[..idx].join("/");
        }
    }
    return top_install_root(path).to_string();
}

fn detect_img_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let lower = detection.lower;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    if lower.contains("gta3.img/") || lower.ends_with(".dff") || lower.ends_with(".txd")
    {
        components.insert(Component::ImgReplacement);
        // Mod Loader's std.stream injects loose DFF/TXD from the mod folder into
        // the correct archive automatically, so the default is the sandbox root;
        // a `gta3.img/` subfolder is only needed to pin a specific archive.
        let note = "Loose DFF/TXD load from the Mod Loader mod folder â€” std.stream routes them into the right archive; no gta3.img subfolder needed".to_string();
        notes.insert(note);
        let source_root = top_install_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: source_root,
                target: "modloader/<mod>",
            },
            size,
            metadata,
        ));
    }
    return None;
}

fn detect_script_candidate(detection: &CandidateDetection) -> Option<InstallCandidate>
{
    let path = detection.path;
    let lower = detection.lower;
    let size = detection.size;
    let mut components = BTreeSet::new();
    let mut notes = BTreeSet::new();
    if lower.ends_with(".scm") || lower.ends_with("script.img")
    {
        components.insert(Component::ScriptData);
        let note = "Main script replacements conflict heavily with mission/story mods".to_string();
        notes.insert(note);
        let source_root = top_install_root(path);
        let metadata = CandidateMetadata { components, notes };
        return Some(install_candidate_from_detection(
            DetectedInstallTarget {
                source_root: source_root,
                target: "modloader/<mod>/data/script",
            },
            size,
            metadata,
        ));
    }

    return None;
}

pub(crate) fn detect_option_group(path: &str) -> Option<String>
{
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    for (idx, part) in parts.iter().enumerate()
    {
        let lower = part.to_ascii_lowercase();
        let is_option = lower.contains("optional")
            || lower.contains("settings")
            || lower.contains("recommended")
            || lower.contains("bonus")
            || lower.contains("compatibility")
            || lower == "en"
            || lower == "pt";

        if is_option
        {
            let part_count = parts.len();
            let depth =
                if lower == "optionals" || lower == "(optionals)" || lower == "en" || lower == "pt"
                {
                    (idx + 2).min(part_count) // literal: allow domain threshold is documented by the surrounding code
                }
                else
                {
                    (idx + 1).min(part_count)
                };
            if depth > 0
            {
                let option_group = parts[..depth].join("/");
                return Some(option_group);
            }
        }
    }
    return None;
}

struct DetectedInstallTarget<'a>
{
    source_root: &'a str,
    target: &'a str,
}

fn install_candidate_from_detection(
    install_target: DetectedInstallTarget<'_>,
    size: u64,
    metadata: CandidateMetadata,
) -> InstallCandidate
{
    let source_root = install_target.source_root;
    let target = install_target.target;
    return InstallCandidate {
        source_root: source_root.trim_matches('/').to_string(),
        target_strategy: target.to_string(),
        file_count: 1,
        total_bytes: size,
        components: metadata.components,
        notes: metadata.notes,
    };
}

fn top_install_root(path: &str) -> &str
{
    return path.split('/').find(|p| !p.is_empty()).unwrap_or(path);
}

fn component_root(path: &str) -> &str
{
    let trimmed = path.trim_matches('/');
    if let Some((parent, _name)) = trimmed.rsplit_once('/')
    {
        if parent.is_empty()
        {
            return trimmed;
        }
        return parent;
    }
    return trimmed;
}

#[cfg(test)]
include!("detect_tests_01.rs");


