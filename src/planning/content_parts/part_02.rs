
fn categorize_by_path(lower: &str) -> ContentCategory
{
    return if lower.ends_with(".asi")
    {
        ContentCategory::Asi
    }
    else if is_cleo_script(lower) || is_cleo_plugin(lower) || lower.ends_with(".fxt")
    {
        // CLEO scripts (.cs/.cs4/.cs3), plugins (.cleo) and text (.fxt) all belong
        // to the CLEO subsystem viewer.
        ContentCategory::Cleo
    }
    else if lower.ends_with(".img") || lower.ends_with(".dff") || lower.ends_with(".txd")
    {
        ContentCategory::Img
    }
    else if lower.ends_with(".gxt") || has_segment_needle(SegmentNeedle { lower: lower, needle: "text" })
    {
        ContentCategory::Text
    }
    else if lower.ends_with(".ifp") || has_segment_needle(SegmentNeedle { lower: lower, needle: "anim" })
    {
        ContentCategory::Anim
    }
    else if has_segment_needle(SegmentNeedle { lower: lower, needle: "audio" }) || lower.ends_with(".ogg")
    {
        ContentCategory::Audio
    }
    else if has_segment_needle(SegmentNeedle { lower: lower, needle: "models" })
    {
        ContentCategory::Models
    }
    else if lower.ends_with(".scm") || lower.ends_with(".cm")
    {
        ContentCategory::Script
    } else if has_segment_needle(SegmentNeedle { lower: lower, needle: "data" })
        || lower.ends_with(".dat")
        || lower.ends_with(".ide")
        || lower.ends_with(".ipl")
        || lower.ends_with(".cfg")
    {
        ContentCategory::Data
    }
    else
    {
        ContentCategory::Other
    };
}

/// True when `needle` appears as a whole path segment (bounded by `/`), so
/// `data/handling.cfg` matches "data" but `metadata.dat` does not.
struct SegmentNeedle<'a>
{
    lower: &'a str,
    needle: &'a str,
}

fn has_segment_needle(segment: SegmentNeedle<'_>) -> bool
{
    let lower = segment.lower;
    let needle = segment.needle;
    return lower.split('/').any(|segment| segment == needle);
}

/// Split entries (already filtered to one category) into labeled sections for a
/// tailored viewer. ModLoader groups by its sandboxed `modloader/<name>` folder
/// â€” the way ModLoader actually isolates mods; every other category groups by
/// the mod that ultimately wins those files. Groups and their rows keep a stable
/// (target-sorted) order for a predictable display.
pub(crate) fn group_entries<'a>(
    category: ContentCategory,
    entries: &[&'a ContentEntry],
) -> Vec<(String, Vec<&'a ContentEntry>)>
{
    let mut groups: BTreeMap<String, Vec<&'a ContentEntry>> = BTreeMap::new();
    for entry in entries
    {
        let key = if category == ContentCategory::ModLoader {
            modloader_folder(&entry.target)
        } else {
            entry.winner().to_string()
        };
        groups.entry(key).or_default().push(entry);
    }
    return groups.into_iter().collect();
}

/// The ModLoader mod folder a target lives in: the segment right after a
/// `modloader` path segment (e.g. `modloader/ImVehFt/x.dff` â†’ `ImVehFt`).
/// Falls back to `(root)` when a modloader target has no named subfolder.
fn modloader_folder(target: &str) -> String
{
    let mut segments = target.split('/');
    while let Some(segment) = segments.next()
    {
        if segment.eq_ignore_ascii_case("modloader")
        {
            return segments.next().unwrap_or("(root)").to_string();
        }
    }
    return "(root)".to_string();
}

// --- ModLoader priorities -------------------------------------------------

/// ModLoader's own per-folder load-order priorities, read from
/// `modloader/modloader.ini`. ModLoader loads folders in ascending priority so
/// higher-priority mods apply last and win â€” a system layered on top of this
/// manager's profile order, worth surfacing so the two are not confused.
#[derive(Clone, Default)]
pub(crate) struct ModLoaderPriorities
{
    pub(crate) default: i32,
    /// Lowercased folder name â†’ priority.
    pub(crate) by_folder: BTreeMap<String, i32>,
}

impl ModLoaderPriorities
{
    /// The effective priority for a modloader mod folder, falling back to the
    /// configured default when the folder has no explicit entry.
    pub(crate) fn for_folder(&self, folder: &str) -> i32
    {
        return self.by_folder
            .get(&folder.to_ascii_lowercase())
            .copied()
            .unwrap_or(self.default);
    }
}

/// Read and parse `<game_root>/modloader/modloader.ini`, if present.
pub(crate) fn read_modloader_priorities(game_root: &Path) -> Option<ModLoaderPriorities>
{
    let path = game_root.join("modloader").join("modloader.ini");
    let text = read_capped(&path, MAX_CONTROL_FILE_BYTES).ok()?;
    return Some(parse_modloader_priorities(&text));
}

/// Parse ModLoader's priority config. Tolerant of the format's variations: any
/// section whose name contains "priority" contributes `folder = number` entries;
/// a `DefaultPriority` key (anywhere) or a `default` key in the priority section
/// sets the fallback (ModLoader's built-in default is 50).
pub(crate) fn parse_modloader_priorities(text: &str) -> ModLoaderPriorities
{
    let mut priorities = ModLoaderPriorities {
        default: MODLOADER_BUILTIN_DEFAULT_PRIORITY,
        by_folder: BTreeMap::new(),
    };
    let mut in_priority_section = false;
    for raw in text.lines()
    {
        // Strip inline `;` comments and surrounding whitespace.
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty()
        {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|section_text| section_text.strip_suffix(']'))
        {
            in_priority_section = section.trim().to_ascii_lowercase().contains("priority");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if key.eq_ignore_ascii_case("defaultpriority")
        {
            if let Ok(number) = value.parse::<i32>()
            {
                priorities.default = number;
            }
            continue;
        }
        if in_priority_section
        {
            apply_modloader_priority_value(ModLoaderPriorityEntry { key: key, value: value }, &mut priorities);
        }
    }
    return priorities;
}

struct ModLoaderPriorityEntry<'a>
{
    key: &'a str,
    value: &'a str,
}

fn apply_modloader_priority_value(entry: ModLoaderPriorityEntry<'_>, priorities: &mut ModLoaderPriorities)
{
    let key = entry.key;
    let value = entry.value;
    let Ok(number) = value.parse::<i32>() else {
        return;
    };
    if key.eq_ignore_ascii_case("default")
    {
        priorities.default = number;
    }
    else
    {
        priorities
            .by_folder
            .insert(key.to_ascii_lowercase(), number);
    }
}
// --- ModLoader log (#3) ---------------------------------------------------

/// ModLoader caps `modloader.log` at 5 MiB; read with headroom above that and
/// refuse anything larger rather than loading an unbounded file.
const MAX_MODLOADER_LOG_BYTES: u64 = 8 * 1024 * 1024;

/// How many warning/error lines to keep â€” enough to be useful in the viewer
/// without letting a spammy log dictate memory.
const MAX_LOG_LINES: usize = 100;

/// A best-effort read of what ModLoader actually did on the last launch, scraped
/// from `modloader/modloader.log`. The log is free-form, plugin-authored text
/// with no stable schema, so this is a heuristic summary â€” the `modloader.ini`
/// the manager writes remains the source of truth for intended state; this is the
/// after-the-fact reality check (did a mod fail to load, was something ignored).
#[derive(Clone, Debug, Default)]
pub(crate) struct ModLoaderLogSummary
{
    /// The ModLoader version from the log's session banner, if found.
    pub(crate) version: Option<String>,
    /// Lines that read as errors/failures (capped).
    pub(crate) errors: Vec<String>,
    /// Lines that read as warnings/skips (capped).
    pub(crate) warnings: Vec<String>,
    /// True when more error/warning lines existed than were kept.
    pub(crate) truncated: bool,
}

impl ModLoaderLogSummary
{
    pub(crate) fn is_clean(&self) -> bool
    {
        return self.errors.is_empty() && self.warnings.is_empty();
    }
}

/// Read and summarize `<game_root>/modloader/modloader.log`, if present.
pub(crate) fn read_modloader_log(game_root: &Path) -> Option<ModLoaderLogSummary>
{
    let path = game_root.join("modloader").join("modloader.log");
    let text = read_capped(&path, MAX_MODLOADER_LOG_BYTES).ok()?;
    return Some(parse_modloader_log(&text));
}

/// Scrape a ModLoader log into a summary. Heuristic and deliberately
/// conservative: it pulls the version banner and the lines that read as
/// errors/warnings, and never claims more than the free-text log supports.
pub(crate) fn parse_modloader_log(text: &str) -> ModLoaderLogSummary
{
    let mut summary = ModLoaderLogSummary::default();
    for raw in text.lines()
    {
        let line = raw.trim().trim_matches('=').trim();
        if line.is_empty()
        {
            continue;
        }
        if summary.version.is_none()
            && let Some(version) = parse_modloader_log_version(line)
        {
            summary.version = Some(version);
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if is_log_error_line(&lower)
        {
            push_log_line(&mut summary.errors, line, &mut summary.truncated);
        }
        else if is_log_warning_line(&lower)
        {
            push_log_line(&mut summary.warnings, line, &mut summary.truncated);
        }
    }
    return summary;
}

/// Extract a `Mod Loader X.Y.Z` version from a banner line, if present.
fn parse_modloader_log_version(line: &str) -> Option<String>
{
    let lower = line.to_ascii_lowercase();
    let idx = lower.find("mod loader ")?;
    let after = line[idx + "mod loader ".len()..].trim_start();
    let token: String = after
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    return if token.chars().next().is_some_and(|character| character.is_ascii_digit())
    {
        Some(token)
    }
    else
    {
        None
    };
}

fn is_log_error_line(lower: &str) -> bool
{
    return [
        "error",
        "failed",
        "failure",
        "could not",
        "couldn't",
        "cannot",
        "unable to",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
}

fn push_log_line(bucket: &mut Vec<String>, line: &str, truncated: &mut bool)
{
    if bucket.len() >= MAX_LOG_LINES
    {
        *truncated = true;
        return;
    }
    bucket.push(line.to_string());
}

fn is_log_warning_line(lower: &str) -> bool
{
    return [
        "warning",
        "warn:",
        "ignoring",
        "ignored",
        "skipping",
        "skipped",
        "deprecated",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
}

// --- ModLoader cross-folder conflicts -------------------------------------

/// A game asset that two or more ModLoader mod folders both provide. Because
/// ModLoader sandboxes each mod in its own `modloader/<folder>/`, these files
/// never collide on disk â€” the manager's content index sees distinct targets â€”
/// yet at runtime ModLoader makes them fight over the same underlying resource,
/// resolved by folder *priority*, not by profile load order or copy sequence.
/// This is the conflict class the plain target-keyed index cannot see.
#[derive(Clone, Debug)]
pub(crate) struct ModLoaderConflict
{
    /// The shared virtual asset (e.g. `infernus.dff`, `data/handling.cfg`).
    pub(crate) asset: String,
    /// Every contending folder with its ModLoader priority, ordered so the
    /// runtime winner (highest priority, ties by folder name) is first.
    pub(crate) contenders: Vec<ModLoaderContender>,
    /// True when the top two contenders share a priority, so ModLoader's winner
    /// is decided by an unspecified tie-break â€” the user should set distinct
    /// priorities to make the outcome deterministic.
    pub(crate) ambiguous: bool,
    /// True when this is a data file ModLoader **merges** entry-by-entry
    /// (handling.cfg, *.ide, carcolsâ€¦). Then two mods only truly collide on
    /// overlapping entries (same vehicle/model id); everything else combines â€” so
    /// this is a soft conflict, not winner-take-all. False for override-only files
    /// (models, textures, .ipl, timecycâ€¦), where the highest-priority folder wins
    /// the whole file.
    pub(crate) mergeable: bool,
}

impl ModLoaderConflict
{
    /// The folder ModLoader will let win this asset at runtime.
    pub(crate) fn winner(&self) -> &str
    {
        return self.contenders
            .first()
            .map(|contender| contender.folder.as_str())
            .unwrap_or("");
    }
}

/// Whether a file is one ModLoader's std.data merges line-by-line, so two mods
/// providing it only collide on entries sharing a key rather than the whole file.
/// An allowlist taken from the std.data trait registrations (AddMerger); anything
/// not listed â€” including override-only data like timecyc/popcycle/fonts/clothes
/// and all `.ipl`/`.zon` â€” is treated as winner-take-all.
pub(crate) fn is_mergeable_game_resource_file(target: &str) -> bool
{
    let name = target
        .rsplit('/')
        .next()
        .unwrap_or(target)
        .to_ascii_lowercase();
    // Every IDE is merged by model id.
    if name.ends_with(".ide")
    {
        return true;
    }
    const MERGEABLE: &[&str] = &[
        "handling.cfg",
        "carcols.dat",
        "carmods.dat",
        "weapon.dat",
        "water.dat",
        "plants.dat",
        "melee.dat",
        "object.dat",
        "surface.dat",
        "surfinfo.dat",
        "surfaud.dat",
        "particle.cfg",
        "procobj.dat",
        "stream.ini",
        "ped.dat",
        "pedstats.dat",
        "statdisp.dat",
        "shopping.dat",
        "cargrp.dat",
        "pedgrp.dat",
        "ar_stats.dat",
        "animgrp.dat",
        "fistfite.dat",
        "gta.dat",
        "default.dat",
    ];
    return MERGEABLE.contains(&name.as_str());
}

/// ModLoader-reserved folder names inside `modloader/` â€” its own state, never a
/// user mod. Dot-prefixed entries are skipped by ModLoader's own scan, so a
/// folder like `.data` or `.profiles` must never be treated as a mod.
fn is_reserved_modloader_folder(folder: &str) -> bool
{
    return folder.starts_with('.') || folder == "(root)";
}

/// One folder contending for a shared ModLoader asset, with the priority
/// ModLoader will apply it at.
#[derive(Clone, Debug)]
pub(crate) struct ModLoaderContender
{
    pub(crate) folder: String,
    pub(crate) priority: i32,
}

/// Reduce a ModLoader target to the virtual asset ModLoader actually resolves it
/// to, so two mods in different sandbox folders can be compared. Strips the
/// `modloader/<folder>/` prefix and any `*.img` archive-container segment
/// (loose `infernus.dff` and `gta3.img/infernus.dff` inject the same resource).
/// Streamed model/texture/collision/anim assets are keyed by base name, since
/// ModLoader streams those by resource name regardless of subfolder; everything
/// else keeps its relative path to avoid false collisions on generic names.
pub(crate) fn modloader_virtual_asset(target: &str) -> Option<String>
{
    let mut segments = target.split('/');
    // Advance to the segment right after the `modloader` root segment.
    let mut found_root = false;
    let mut rest: Vec<&str> = Vec::new();
    for segment in segments.by_ref()
    {
        if !found_root
        {
            if segment.eq_ignore_ascii_case("modloader")
            {
                found_root = true;
            }
            continue;
        }
        rest.push(segment);
    }
    if !found_root || rest.len() < SANDBOXED_ASSET_MIN_SEGMENTS
    {
        // Not under a named sandbox folder (rest = [folder, file...]) â€” nothing
        // to compare across mods.
        return None;
    }
    // Drop the sandbox folder itself, then any archive-container segments.
    let tail: Vec<&str> = rest[1..]
        .iter()
        .copied()
        .filter(|segment| !segment.to_ascii_lowercase().ends_with(".img"))
        .collect();
    let name = tail.last()?.to_ascii_lowercase();
    if name.is_empty()
    {
        return None;
    }
    return if is_streamed_by_name(&name)
    {
        Some(name)
    }
    else
    {
        Some(tail.join("/").to_ascii_lowercase())
    };
}
