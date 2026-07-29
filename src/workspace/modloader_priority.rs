use crate::prelude::*;

/// Per-profile ModLoader priority overrides (folder name â†’ priority), so two
/// manager profiles can share the same mods but order them differently inside
/// ModLoader. A priority of `0` means the mod is disabled *in ModLoader* â€” its
/// files are still installed, but ModLoader ignores it and it appears disabled in
/// the in-game Mod Configuration menu. Stored under
/// `.sa-mod-manager/modloader_priority/<profile>.json`; applied to
/// `modloader/modloader.ini`'s active-profile `[Profiles.<profile>.Priority]`
/// section on demand.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ModLoaderPriorityFile
{
    version: u32,
    priorities: BTreeMap<String, i32>,
}

pub(crate) fn read_modloader_overrides(state_root: &Path, profile: &str) -> BTreeMap<String, i32>
{
    let path = overrides_path(state_root, profile);
    let Ok(text) = read_capped(&path, MAX_CONTROL_FILE_BYTES) else {
        return BTreeMap::new();
    };
    return serde_json::from_str::<ModLoaderPriorityFile>(&text)
        .map(|file| file.priorities)
        .unwrap_or_default();
}

pub(crate) fn write_modloader_overrides(
    state_root: &Path,
    profile: &str,
    priorities: &BTreeMap<String, i32>,
) -> Result<(), AppError>
{
    let path = overrides_path(state_root, profile);
    if let Some(parent) = path.parent()
    {
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
    return Ok(());
}

/// The active ModLoader profile from `[Folder.Config] Profile = â€¦` (defaults to
/// `Default`, ModLoader's own default), i.e. the profile whose Priority section
/// the game actually reads.
pub(crate) fn active_profile(ini: &str) -> String
{
    return ini_value(IniQuery {
        ini: ini,
        section: "folder.config",
        key: "profile",
    })
    .unwrap_or_else(|| "Default".to_string());
}

/// `PriorityLimit` from `[Folder.Config]` (default 100, floored at 2).
pub(crate) fn priority_limit(ini: &str) -> i32
{
    return ini_value(IniQuery {
        ini: ini,
        section: "folder.config",
        key: "prioritylimit",
    })
    .and_then(|value| value.parse::<i32>().ok())
    .unwrap_or(100) // literal: allow external format or runtime boundary value means itself here
    .max(2); // literal: allow external format or runtime boundary value means itself here;
}

pub(crate) fn write_priority_section(write: PrioritySectionWrite<'_>) -> String
{
    let existing = write.existing;
    let profile = write.profile;
    let limit = write.limit;
    let overrides = write.overrides;
    let limit = limit.max(2); // literal: allow external format or runtime boundary value means itself here
    let target = format!("Profiles.{profile}.Priority");
    // Remaining folders to place, keyed lowercase â†’ (display name, priority).
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
    for raw in existing.lines()
    {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']')
        {
            if in_target
            {
                flush_pending(&mut out, &mut pending);
            }
            let name = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
            in_target = name.eq_ignore_ascii_case(&target);
            if in_target
            {
                found = true;
            }
            out.push(raw.to_string());
            continue;
        }
        if in_target && has_replaced_pending_priority_line(trimmed, &mut pending, &mut out)
        {
            continue;
        }
        out.push(raw.to_string());
    }
    if in_target
    {
        flush_pending(&mut out, &mut pending);
    }
    if !found && !pending.is_empty()
    {
        if out.last().is_some_and(|line| !line.trim().is_empty())
        {
            out.push(String::new());
        }
        out.push(format!("[{target}]"));
        flush_pending(&mut out, &mut pending);
    }
    let mut text = out.join("\n");
    if !text.ends_with('\n')
    {
        text.push('\n');
    }
    return text;
}

struct IniQuery<'a>
{
    ini: &'a str,
    section: &'a str,
    key: &'a str,
}

fn flush_pending(out: &mut Vec<String>, pending: &mut BTreeMap<String, (String, i32)>)
{
    for (display, priority) in pending.values()
    {
        out.push(format!("{display}={priority}"));
    }
    pending.clear();
}

/// Return `existing` with each `overrides` folder set to its priority in the
/// active profile's `[Profiles.<profile>.Priority]` section â€” updating a matching
/// line in place (preserving its folder-name case), appending new folders to the
/// section, and creating the section if absent. Every other line (comments, other
/// folders, other sections) is preserved. Priorities are clamped to `[0, limit]`,
/// where `0` disables the mod in ModLoader.
pub(crate) struct PrioritySectionWrite<'a>
{
    pub(crate) existing: &'a str,
    pub(crate) profile: &'a str,
    pub(crate) limit: i32,
    pub(crate) overrides: &'a BTreeMap<String, i32>,
}

fn has_replaced_pending_priority_line(
    trimmed: &str,
    pending: &mut BTreeMap<String, (String, i32)>,
    out: &mut Vec<String>,
) -> bool
{
    let code = trimmed.split(';').next().unwrap_or("").trim();
    let Some((key, _)) = code.split_once('=') else {
        return false;
    };
    let key_lower = key.trim().to_ascii_lowercase();
    let Some((display, priority)) = pending.remove(&key_lower) else {
        return false;
    };
    out.push(format!("{display}={priority}"));
    return true;
}

/// Apply a profile's priority overrides to `modloader/modloader.ini`, writing
/// them into the active ModLoader profile's Priority section.
pub(crate) fn apply_modloader_priorities(
    game_root: &Path,
    overrides: &BTreeMap<String, i32>,
) -> Result<(), AppError>
{
    let path = game_root.join("modloader").join("modloader.ini");
    let existing = read_capped(&path, MAX_CONTROL_FILE_BYTES).map_err(|err| {
        AppError::Usage(format!(
            "modloader.ini not found at {} â€” install/run ModLoader first",
            path.display()
        ))
        .context(format!("read ModLoader priority file: {err}"))
    })?;
    let profile = active_profile(&existing);
    let limit = priority_limit(&existing);
    let updated = write_priority_section(PrioritySectionWrite {
        existing: &existing,
        profile: &profile,
        limit: limit,
        overrides: overrides,
    });
    fs::write(&path, updated).with_context(|| format!("write {}", path.display()))?;
    return Ok(());
}
fn overrides_path(state_root: &Path, profile: &str) -> PathBuf
{
    return state_root
        .join("modloader_priority")
        .join(format!("{}.json", safe_name(profile)));
}

fn ini_value(query: IniQuery<'_>) -> Option<String>
{
    let ini = query.ini;
    let section = query.section;
    let key = query.key;
    let mut in_section = false;
    for raw in ini.lines()
    {
        let line = raw.split(';').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[').and_then(|section_text| section_text.strip_suffix(']'))
        {
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
    return None;
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn overrides_round_trip()
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-mlprio-{}-{}",
            std::process::id(),
            unix_now()
        ));
        fs::create_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
        let mut priorities = BTreeMap::new();
        priorities.insert("ImVehFt".to_string(), 80); // literal: allow test fixture value is the specimen under judgment
        priorities.insert("OldMod".to_string(), 0);
        write_modloader_overrides(&root, "default", &priorities)
            .expect("the test fixture is created before this assertion reads it");
        let read = read_modloader_overrides(&root, "default");
        assert_eq!(read.get("ImVehFt"), Some(&80)); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(read.get("OldMod"), Some(&0));
        fs::remove_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn write_section_updates_appends_and_allows_zero()
    {
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
        overrides.insert("ImVehFt".to_string(), 90); // update in place // literal: allow test fixture value is the specimen under judgment
        overrides.insert("NewMod".to_string(), 30); // append // literal: allow test fixture value is the specimen under judgment
        overrides.insert("OldMod".to_string(), 0); // disabled (priority 0)
        let out = write_priority_section(PrioritySectionWrite {
            existing: ini,
            profile: "Default",
            limit: 100, // literal: allow test fixture value is the specimen under judgment
            overrides: &overrides,
        }); // literal: allow test fixture value is the specimen under judgment

        assert!(out.contains("; comment kept"));
        assert!(out.contains("ImVehFt=90"));
        assert!(out.contains("HD_Roads=40")); // untouched preserved
        assert!(out.contains("NewMod=30"));
        assert!(out.contains("OldMod=0"));
        // The Folder.Config section is preserved.
        assert!(out.contains("PriorityLimit = 100"));
    }

    #[test]
    fn write_section_creates_when_absent()
    {
        let ini = "[Folder.Config]\nProfile = Default\n";
        let mut overrides = BTreeMap::new();
        overrides.insert("Mod".to_string(), 60); // literal: allow test fixture value is the specimen under judgment
        let out = write_priority_section(PrioritySectionWrite {
            existing: ini,
            profile: "Default",
            limit: 100, // literal: allow test fixture value is the specimen under judgment
            overrides: &overrides,
        }); // literal: allow test fixture value is the specimen under judgment
        assert!(out.contains("[Profiles.Default.Priority]"));
        assert!(out.contains("Mod=60"));
    }
}
