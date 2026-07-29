
/// Find the assets that two or more distinct ModLoader folders both provide,
/// with the runtime winner decided by ModLoader priority. `entries` should be
/// the `ContentCategory::ModLoader` slice; `priorities` is the parsed
/// `modloader.ini` (default priorities are assumed when absent).
pub(crate) fn modloader_conflicts(
    entries: &[&ContentEntry],
    priorities: &ModLoaderPriorities,
) -> Vec<ModLoaderConflict>
{
    // asset -> set of contending folders.
    let mut by_asset: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in entries
    {
        let (Some(asset), folder) = (
            modloader_virtual_asset(&entry.target),
            modloader_folder(&entry.target),
        ) else {
            continue;
        };
        if is_reserved_modloader_folder(&folder)
        {
            continue;
        }
        by_asset.entry(asset).or_default().insert(folder);
    }

    let mut conflicts = Vec::new();
    for (asset, folders) in by_asset
    {
        if folders.len() < CONFLICT_MIN_CONTENDERS
        {
            continue;
        }
        let mut contenders: Vec<ModLoaderContender> = folders
            .into_iter()
            .map(|folder| {
                let priority = priorities.for_folder(&folder);
                ModLoaderContender { folder, priority }
            })
            .collect();
        // Highest priority wins; ties fall back to folder name so the display is
        // stable even where ModLoader's own tie-break is unspecified.
        contenders.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.folder.cmp(&b.folder))
        });
        let ambiguous = contenders
            .get(1)
            .is_some_and(|second| second.priority == contenders[0].priority);
        let mergeable = is_mergeable_data_file(&asset);
        conflicts.push(ModLoaderConflict {
            asset,
            contenders,
            ambiguous,
            mergeable,
        });
    }
    return conflicts;
}

// --- ModLoader priority writer --------------------------------------------

/// ModLoader's default upper bound for folder priorities (`PriorityLimit`).
const MODLOADER_DEFAULT_PRIORITY_LIMIT: i32 = 100;

/// The sandbox folder an install-root target lands in: the segment right after
/// the `modloader` segment (e.g. `modloader/vehicle/gta3.img` â†’ `vehicle`).
/// Returns `None` for a target not inside a named `modloader/<folder>`, or one
/// under a reserved dot-folder (`.data`, `.profiles`) ModLoader would never treat
/// as a mod â€” so we never write priority/ignore entries for those.
pub(crate) fn modloader_folder_from_target(target: &str) -> Option<String>
{
    let mut segments = target.split('/').filter(|s| !s.is_empty());
    let mut found_root = false;
    for segment in segments.by_ref()
    {
        if segment.eq_ignore_ascii_case("modloader")
        {
            found_root = true;
            break;
        }
    }
    if !found_root
    {
        return None;
    }
    let folder = segments.next()?.to_string();
    if is_reserved_modloader_folder(&folder)
    {
        return None;
    }
    return Some(folder);
}

/// The active ModLoader profile named in `[Folder.Config] Profile = â€¦`,
/// defaulting to `Default` (ModLoader's own default) when unset. This is the
/// profile whose `Priority` section actually governs load order, so the manager
/// writes into it rather than assuming `Default`.
pub(crate) fn modloader_active_profile(ini: &str) -> String
{
    return ini_value_in_section(IniLookup { ini: ini, section: "folder.config", key: "profile" }).unwrap_or_else(|| "Default".to_string());
}

/// The `PriorityLimit` from `[Folder.Config]`, defaulting to 100 and floored at
/// 2 so a spread of distinct priorities is always possible.
pub(crate) fn modloader_priority_limit(ini: &str) -> i32
{
    return ini_value_in_section(IniLookup { ini: ini, section: "folder.config", key: "prioritylimit" })
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(MODLOADER_DEFAULT_PRIORITY_LIMIT)
        .max(MODLOADER_PRIORITY_PAIR_DIVISOR_I32);
}

/// First `key = value` in the section whose header equals `section`
/// (case-insensitive), with inline `;` comments stripped.
struct IniLookup<'a>
{
    ini: &'a str,
    section: &'a str,
    key: &'a str,
}

/// Map a mod's rank among the modloader mods (0 = applied first / lowest
/// priority) to a distinct ModLoader priority spread across `[1, limit]` and
/// centered on 50, so managed mods straddle ModLoader's default for unlisted
/// mods and preserve the profile's relative order. A lone mod gets ~50.
pub(crate) fn spread_priority(rank: usize, count: usize, limit: i32) -> i32
{
    let limit = limit.max(MODLOADER_PRIORITY_PAIR_DIVISOR_I32);
    if count <= 1
    {
        return (limit / MODLOADER_PRIORITY_PAIR_DIVISOR_I32).clamp(1, limit);
    }
    let numer = rank as i64 * (limit as i64 - 1);
    let denom = (count - 1) as i64;
    let value = 1 + ((numer + denom / MODLOADER_PRIORITY_PAIR_DIVISOR_I64) / denom) as i32;
    return value.clamp(1, limit);
}

/// Produce a `modloader.ini` that reflects `folder_priorities` (folder name â†’
/// priority) in the `[Profiles.<profile>.Priority]` section, preserving every
/// other line of `existing` (comments, other sections, unmanaged folders). When
/// there is no existing file, a minimal one is generated that also points
/// `[Folder.Config] Profile` at `profile`. Returns `None` when the section
/// already matches, so the caller can skip a needless write.
pub(crate) fn render_modloader_priority_ini(
    existing: Option<&str>,
    profile: &str,
    limit: i32,
    folder_priorities: &BTreeMap<String, i32>,
) -> Option<String>
{
    let header = format!("[Profiles.{profile}.Priority]");
    // Remaining folders to place, lowercased key â†’ (display name, priority).
    let mut pending: BTreeMap<String, (String, i32)> = folder_priorities
        .iter()
        .map(|(folder, priority)| {
            (
                folder.to_ascii_lowercase(),
                (
                    folder.clone(),
                    (*priority).clamp(
                        MODLOADER_PRIORITY_MIN,
                        limit.max(MODLOADER_PRIORITY_PAIR_DIVISOR_I32),
                    ),
                ),
            )
        })
        .collect();

    let text = existing.unwrap_or("");
    if text.trim().is_empty()
    {
        return Some(fresh_priority_ini(profile, limit, &pending));
    }

    let mut out: Vec<String> = Vec::new();
    let mut in_target = false;
    let mut wrote_section = false;
    let mut changed = false;
    let lines: Vec<&str> = text.lines().collect();
    for line in lines
    {
        let trimmed = line.trim();
        let is_header = trimmed.starts_with('[') && trimmed.ends_with(']');
        if in_target && is_header
        {
            // Leaving the target section: flush any folders not already present.
            flush_pending(&mut out, &mut pending, &mut changed);
            in_target = false;
        }
        if is_header
        {
            in_target = trimmed.eq_ignore_ascii_case(&header);
            if in_target
            {
                wrote_section = true;
            }
            out.push(line.to_string());
            continue;
        }
        if in_target
            && replace_rendered_priority_line(trimmed, &mut pending, &mut out, &mut changed)
        {
            continue;
        }
        out.push(line.to_string());
    }
    // File ended while still inside the target section, or the section was never
    // seen: flush the remainder (adding the section header if needed).
    if in_target
    {
        flush_pending(&mut out, &mut pending, &mut changed);
    }
    else if !wrote_section && !pending.is_empty()
    {
        if out.last().map(|l| !l.trim().is_empty()).unwrap_or(false)
        {
            out.push(String::new());
        }
        out.push(header.clone());
        flush_pending(&mut out, &mut pending, &mut changed);
    }

    if !changed
    {
        return None;
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    return Some(joined);
}

/// A minimal `modloader.ini` created when none exists, pointing the active
/// profile at `profile` and listing the folder priorities.
fn fresh_priority_ini(
    profile: &str,
    limit: i32,
    pending: &BTreeMap<String, (String, i32)>,
) -> String
{
    let mut lines = vec![
        "; Generated by SA Mod Manager: ModLoader folder priority from profile load order."
            .to_string(),
        "[Folder.Config]".to_string(),
        format!("Profile = {profile}"),
        format!("PriorityLimit = {limit}"),
        String::new(),
        format!("[Profiles.{profile}.Priority]"),
    ];
    for (display, priority) in pending.values()
    {
        lines.push(format!("{display} = {priority}"));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    return text;
}

fn flush_pending(
    out: &mut Vec<String>,
    pending: &mut BTreeMap<String, (String, i32)>,
    changed: &mut bool,
)
{
    if pending.is_empty()
    {
        return;
    }
    for (_, (display, priority)) in std::mem::take(pending)
    {
        out.push(format!("{display} = {priority}"));
        *changed = true;
    }
}
/// Append each still-pending folder as a `Folder = priority` line.
fn replace_rendered_priority_line(
    trimmed: &str,
    pending: &mut BTreeMap<String, (String, i32)>,
    out: &mut Vec<String>,
    changed: &mut bool,
) -> bool
{
    let Some((key, _)) = trimmed.split_once('=') else {
        return false;
    };
    let folder_key = key.trim().to_ascii_lowercase();
    let Some((display, priority)) = pending.remove(&folder_key) else {
        return false;
    };
    let replacement = format!("{display} = {priority}");
    if replacement != trimmed
    {
        *changed = true;
    }
    out.push(replacement);
    return true;
}

/// The name of the manager-owned ModLoader profile for a given manager profile.
/// Prefixed and sanitized so it never collides with a user's own profiles and is
/// safe inside `[Profiles.<name>.*]` headers and a `-modprof <name>` argument.
pub(crate) fn modloader_managed_profile_name(profile_name: &str) -> String
{
    return format!("SAMM_{}", safe_name(profile_name));
}

// --- ModLoader native profile (2b) ----------------------------------------

pub(crate) fn render_modloader_managed_profile(
    request: ModLoaderManagedProfileRender<'_>,
) -> Option<String>
{
    let base = request.existing.unwrap_or("");
    let config_result = ensure_profile_config_section(ProfileConfigSectionEdit { text: base, profile: request.profile, parent: request.parent });
    let with_config = config_result.text;
    let config_changed = config_result.changed;
    // Layer the priority section on top of the (possibly config-augmented) text.
    // Passing a non-empty text avoids the "fresh file" path, so we never write a
    // `[Folder.Config] Profile =` line â€” activation is via -modprof only.
    let priority = render_modloader_priority_ini(
        Some(&with_config),
        request.profile,
        request.limit,
        request.folder_priorities,
    );
    let after_priority = priority.clone().unwrap_or(with_config);
    // Regenerate our own IgnoreMods / IgnoreFiles blocks (we fully own this
    // profile's sections): disabled mods, then per-file exclusion globs.
    let mods_result = set_profile_list_section(ProfileListSectionEdit { text: &after_priority, profile: request.profile, section: "IgnoreMods", entries: request.ignore_folders });
    let after_ignore_mods = mods_result.text;
    let mods_changed = mods_result.changed;
    let files_result = set_profile_list_section(ProfileListSectionEdit { text: &after_ignore_mods, profile: request.profile, section: "IgnoreFiles", entries: request.ignore_files });
    let after_ignore_files = files_result.text;
    let files_changed = files_result.changed;
    return if config_changed || priority.is_some() || mods_changed || files_changed
    {
        Some(after_ignore_files)
    }
    else
    {
        None
    };
}

/// Render a `modloader.ini` that expresses the profile as a native ModLoader
/// profile: a `[Profiles.<profile>.Config]` (inheriting `parent` so the user's
/// own settings still apply), a `[Profiles.<profile>.Priority]` section from load
/// order, and a `[Profiles.<profile>.IgnoreMods]` section listing `ignore_folders`
/// â€” the mods the manager has disabled, so ModLoader skips their folders even
/// when they are physically present (a persistent `modloader/` install). All
/// other content is preserved, and `[Folder.Config]` is left alone â€” the profile
/// is activated per launch with `-modprof`. Returns `None` when nothing changes.
pub(crate) struct ModLoaderManagedProfileRender<'a>
{
    pub(crate) existing: Option<&'a str>,
    pub(crate) profile: &'a str,
    pub(crate) parent: &'a str,
    pub(crate) limit: i32,
    pub(crate) folder_priorities: &'a BTreeMap<String, i32>,
    pub(crate) ignore_folders: &'a [String],
    pub(crate) ignore_files: &'a [String],
}

fn ensure_profile_config_section(edit: ProfileConfigSectionEdit<'_>) -> RenderedProfileSection
{
    let text = edit.text;
    let profile = edit.profile;
    let parent = edit.parent;
    let header = format!("[Profiles.{profile}.Config]");
    let has_section = text
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case(&header));
    if has_section
    {
        return RenderedProfileSection { text: text.to_string(), changed: false };
    }
    if text.trim().is_empty()
    {
        return RenderedProfileSection { text: format!("{header}\nParents = {parent}\n"), changed: true };
    }
    let mut out = text.trim_end().to_string();
    out.push_str("\n\n");
    out.push_str(&header);
    out.push('\n');
    out.push_str(&format!("Parents = {parent}\n"));
    return RenderedProfileSection { text: out, changed: true };
}

/// Replace this profile's `[Profiles.<profile>.<section>]` list block (e.g.
/// `IgnoreMods`, `IgnoreFiles`) with one listing `entries`, one bare glob per line
/// in ModLoader's list format. The block is dropped entirely when `entries` is
/// empty. Returns the text and whether it changed. Only this profile's own section
/// is touched; everything else is preserved verbatim.
struct RenderedProfileSection
{
    text: String,
    changed: bool,
}

struct ProfileListSectionEdit<'a>
{
    text: &'a str,
    profile: &'a str,
    section: &'a str,
    entries: &'a [String],
}

fn set_profile_list_section(edit: ProfileListSectionEdit<'_>) -> RenderedProfileSection
{
    let text = edit.text;
    let profile = edit.profile;
    let section = edit.section;
    let entries = edit.entries;
    let header = format!("[Profiles.{profile}.{section}]");
    // Drop any existing managed block (header + its body up to the next section),
    // preserving everything else.
    let mut out: Vec<String> = Vec::new();
    let mut skipping = false;
    for line in text.lines()
    {
        let trimmed = line.trim();
        let is_header = trimmed.starts_with('[') && trimmed.ends_with(']');
        if skipping
        {
            if is_header
            {
                skipping = false;
            }
            else
            {
                continue;
            }
        }
        if is_header && trimmed.eq_ignore_ascii_case(&header)
        {
            skipping = true;
            continue;
        }
        out.push(line.to_string());
    }
    if !entries.is_empty()
    {
        if out.last().map(|l| !l.trim().is_empty()).unwrap_or(false)
        {
            out.push(String::new());
        }
        out.push(header);
        for entry in entries
        {
            out.push(entry.clone());
        }
    }
    let mut result = out.join("\n");
    if !result.is_empty() && !result.ends_with('\n')
    {
        result.push('\n');
    }
    let changed = result.trim_end() != text.trim_end();
    return RenderedProfileSection { text: result, changed };
}

/// Ensure a `[Profiles.<profile>.Config]` section exists (creating it with
/// `Parents = parent` when absent, so ModLoader recognizes the profile). Returns
/// the text and whether it was changed.
struct ProfileConfigSectionEdit<'a>
{
    text: &'a str,
    profile: &'a str,
    parent: &'a str,
}

fn ini_value_in_section(lookup: IniLookup<'_>) -> Option<String>
{
    let ini = lookup.ini;
    let section = lookup.section;
    let key = lookup.key;
    let mut in_section = false;
    for raw in ini.lines()
    {
        let line = raw.split(';').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
        {
            in_section = header.trim().eq_ignore_ascii_case(section);
            continue;
        }
        if !in_section
        {
            continue;
        }
        if let Some((found, value)) = line.split_once('=')
            && found.trim().eq_ignore_ascii_case(key)
        {
            return Some(value.trim().to_string());
        }
    }
    return None;
}
