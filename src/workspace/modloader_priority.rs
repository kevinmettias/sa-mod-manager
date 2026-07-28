use crate::prelude::*;

/// Per-profile ModLoader priority overrides (folder name → priority), so two
/// manager profiles can share the same mods but order them differently inside
/// ModLoader. A priority of `0` means the mod is disabled *in ModLoader* — its
/// files are still installed, but ModLoader ignores it and it appears disabled in
/// the in-game Mod Configuration menu. Stored under
/// `.sa-mod-manager/modloader_priority/<profile>.json`; applied to
/// `modloader/modloader.ini`'s active-profile `[Profiles.<profile>.Priority]`
/// section on demand.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ModLoaderPriorityFile {
    version: u32,
    priorities: BTreeMap<String, i32>,
}

fn overrides_path(state_root: &Path, profile: &str) -> PathBuf {
    state_root
        .join("modloader_priority")
        .join(format!("{}.json", safe_name(profile)))
}

pub(crate) fn read_modloader_overrides(state_root: &Path, profile: &str) -> BTreeMap<String, i32> {
    let Ok(text) = read_capped(&overrides_path(state_root, profile), MAX_CONTROL_FILE_BYTES) else {
        return BTreeMap::new();
    };
    serde_json::from_str::<ModLoaderPriorityFile>(&text)
        .map(|file| file.priorities)
        .unwrap_or_default()
}

pub(crate) fn write_modloader_overrides(
    state_root: &Path,
    profile: &str,
    priorities: &BTreeMap<String, i32>,
) -> Result<(), AppError> {
    let path = overrides_path(state_root, profile);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create modloader-priority directory {}", parent.display()))?;
    }
    let file = ModLoaderPriorityFile {
        version: 1,
        priorities: priorities.clone(),
    };
    let mut text = serde_json::to_string_pretty(&file)
        .map_err(|err| AppError::Usage(format!("serialize modloader priorities: {err}")))?;
    text.push('\n');
    fs::write(&path, text)
        .with_context(|| format!("write modloader priorities {}", path.display()))?;
    Ok(())
}

/// The active ModLoader profile from `[Folder.Config] Profile = …` (defaults to
/// `Default`, ModLoader's own default), i.e. the profile whose Priority section
/// the game actually reads.
pub(crate) fn active_profile(ini: &str) -> String {
    ini_value(ini, "folder.config", "profile").unwrap_or_else(|| "Default".to_string())
}

/// `PriorityLimit` from `[Folder.Config]` (default 100, floored at 2).
pub(crate) fn priority_limit(ini: &str) -> i32 {
    ini_value(ini, "folder.config", "prioritylimit")
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(100)
        .max(2)
}

fn ini_value(ini: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for raw in ini.lines() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_section = header.trim().eq_ignore_ascii_case(section);
            continue;
        }
        if in_section
            && let Some((found, value)) = line.split_once('=')
            && found.trim().eq_ignore_ascii_case(key)
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Return `existing` with each `overrides` folder set to its priority in the
/// active profile's `[Profiles.<profile>.Priority]` section — updating a matching
/// line in place (preserving its folder-name case), appending new folders to the
/// section, and creating the section if absent. Every other line (comments, other
/// folders, other sections) is preserved. Priorities are clamped to `[0, limit]`,
/// where `0` disables the mod in ModLoader.
pub(crate) fn write_priority_section(
    existing: &str,
    profile: &str,
    limit: i32,
    overrides: &BTreeMap<String, i32>,
) -> String {
    let limit = limit.max(2);
    let target = format!("Profiles.{profile}.Priority");
    // Remaining folders to place, keyed lowercase → (display name, priority).
    let mut pending: BTreeMap<String, (String, i32)> = overrides
        .iter()
        .map(|(folder, priority)| {
            (
                folder.to_ascii_lowercase(),
                (folder.clone(), (*priority).clamp(0, limit)),
            )
        })
        .collect();

    let mut out: Vec<String> = Vec::new();
    let mut in_target = false;
    let mut found = false;
    for raw in existing.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_target {
                flush_pending(&mut out, &mut pending);
            }
            let name = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
            in_target = name.eq_ignore_ascii_case(&target);
            if in_target {
                found = true;
            }
            out.push(raw.to_string());
            continue;
        }
        if in_target {
            let code = trimmed.split(';').next().unwrap_or("").trim();
            if let Some((key, _)) = code.split_once('=') {
                let key_lower = key.trim().to_ascii_lowercase();
                if let Some((display, priority)) = pending.remove(&key_lower) {
                    out.push(format!("{display}={priority}"));
                    continue;
                }
            }
        }
        out.push(raw.to_string());
    }
    if in_target {
        flush_pending(&mut out, &mut pending);
    }
    if !found && !pending.is_empty() {
        if out.last().is_some_and(|line| !line.trim().is_empty()) {
            out.push(String::new());
        }
        out.push(format!("[{target}]"));
        flush_pending(&mut out, &mut pending);
    }
    let mut text = out.join("\n");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn flush_pending(out: &mut Vec<String>, pending: &mut BTreeMap<String, (String, i32)>) {
    for (display, priority) in pending.values() {
        out.push(format!("{display}={priority}"));
    }
    pending.clear();
}

/// Apply a profile's priority overrides to `modloader/modloader.ini`, writing
/// them into the active ModLoader profile's Priority section.
pub(crate) fn apply_modloader_priorities(
    game_root: &Path,
    overrides: &BTreeMap<String, i32>,
) -> Result<(), AppError> {
    let path = game_root.join("modloader").join("modloader.ini");
    let existing = read_capped(&path, MAX_CONTROL_FILE_BYTES).map_err(|_| {
        AppError::Usage(format!(
            "modloader.ini not found at {} — install/run ModLoader first",
            path.display()
        ))
    })?;
    let profile = active_profile(&existing);
    let limit = priority_limit(&existing);
    let updated = write_priority_section(&existing, &profile, limit, overrides);
    fs::write(&path, updated).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_round_trip() {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-mlprio-{}-{}",
            std::process::id(),
            unix_now()
        ));
        fs::create_dir_all(&root).unwrap();
        let mut priorities = BTreeMap::new();
        priorities.insert("ImVehFt".to_string(), 80);
        priorities.insert("OldMod".to_string(), 0);
        write_modloader_overrides(&root, "default", &priorities).unwrap();
        let read = read_modloader_overrides(&root, "default");
        assert_eq!(read.get("ImVehFt"), Some(&80));
        assert_eq!(read.get("OldMod"), Some(&0));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn write_section_updates_appends_and_allows_zero() {
        let ini = "\
[Folder.Config]
Profile = Default
PriorityLimit = 100

[Profiles.Default.Priority]
; comment kept
ImVehFt=50
HD_Roads=40
";
        let mut overrides = BTreeMap::new();
        overrides.insert("ImVehFt".to_string(), 90); // update in place
        overrides.insert("NewMod".to_string(), 30); // append
        overrides.insert("OldMod".to_string(), 0); // disabled (priority 0)
        let out = write_priority_section(ini, "Default", 100, &overrides);

        assert!(out.contains("; comment kept"));
        assert!(out.contains("ImVehFt=90"));
        assert!(out.contains("HD_Roads=40")); // untouched preserved
        assert!(out.contains("NewMod=30"));
        assert!(out.contains("OldMod=0"));
        // The Folder.Config section is preserved.
        assert!(out.contains("PriorityLimit = 100"));
    }

    #[test]
    fn write_section_creates_when_absent() {
        let ini = "[Folder.Config]\nProfile = Default\n";
        let mut overrides = BTreeMap::new();
        overrides.insert("Mod".to_string(), 60);
        let out = write_priority_section(ini, "Default", 100, &overrides);
        assert!(out.contains("[Profiles.Default.Priority]"));
        assert!(out.contains("Mod=60"));
    }
}
