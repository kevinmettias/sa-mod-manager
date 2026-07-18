use crate::prelude::*;

/// The San-Andreas-native content buckets a materialized file can fall into.
/// These are the "viewers" the UI groups by: the loader subsystems (ModLoader,
/// CLEO, ASI) plus the direct game resources modders replace.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ContentCategory {
    ModLoader,
    Cleo,
    Asi,
    Img,
    Models,
    Text,
    Anim,
    Audio,
    Data,
    Script,
    Other,
}

impl ContentCategory {
    /// Every category in a stable display order, so the UI can render a fixed
    /// set of tabs/filters without discovering them from the data.
    pub(crate) fn all() -> [ContentCategory; 11] {
        [
            ContentCategory::ModLoader,
            ContentCategory::Cleo,
            ContentCategory::Asi,
            ContentCategory::Img,
            ContentCategory::Models,
            ContentCategory::Text,
            ContentCategory::Anim,
            ContentCategory::Audio,
            ContentCategory::Data,
            ContentCategory::Script,
            ContentCategory::Other,
        ]
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ContentCategory::ModLoader => "ModLoader",
            ContentCategory::Cleo => "CLEO",
            ContentCategory::Asi => "ASI",
            ContentCategory::Img => "IMG / DFF / TXD",
            ContentCategory::Models => "Models",
            ContentCategory::Text => "Text / GXT",
            ContentCategory::Anim => "Animation",
            ContentCategory::Audio => "Audio",
            ContentCategory::Data => "Data",
            ContentCategory::Script => "Scripts",
            ContentCategory::Other => "Other",
        }
    }

    /// One line describing what this subsystem's viewer shows, so each category
    /// reads as its own tailored viewer rather than a generic file list.
    pub(crate) fn description(self) -> &'static str {
        match self {
            ContentCategory::ModLoader => {
                "ModLoader sandboxes each mod in its own modloader/<name> folder, so these rarely \
                 collide — grouped by mod folder below."
            }
            ContentCategory::Cleo => {
                "CLEO scripts (.cs CLEO5, .cs4/.cs3 compat), plugins (.cleo → cleo_plugins), \
                 modules (→ cleo_modules), text (.fxt → cleo_text). Two scripts with the same file \
                 name are the usual conflict."
            }
            ContentCategory::Asi => {
                "ASI plugins load through an ASI loader (e.g. Ultimate ASI Loader). Duplicate .asi \
                 names or clashing loader DLLs are the thing to watch."
            }
            ContentCategory::Img => {
                "Loose DFF/TXD models and IMG archives. Same-named entries: the winning mod's \
                 version is what the game streams."
            }
            ContentCategory::Models => "Model files. Later mods overwrite same-named models.",
            ContentCategory::Text => {
                "GXT text tables. Replacing a whole GXT usually wins over partial edits."
            }
            ContentCategory::Anim => "Animation files (.ifp). Same-named packs overwrite.",
            ContentCategory::Audio => "Audio streams and SFX packs.",
            ContentCategory::Data => {
                "Direct data files (handling.cfg, gta.dat, *.ide, *.ipl). The most conflict-prone \
                 category — merges are often needed here."
            }
            ContentCategory::Script => "Mission/script files (main.scm).",
            ContentCategory::Other => "Files that did not match a known San Andreas category.",
        }
    }

    /// A compact chip label for the per-mod flags column, where space is tight.
    pub(crate) fn short_label(self) -> &'static str {
        match self {
            ContentCategory::ModLoader => "ML",
            ContentCategory::Cleo => "CS",
            ContentCategory::Asi => "ASI",
            ContentCategory::Img => "IMG",
            ContentCategory::Models => "MDL",
            ContentCategory::Text => "GXT",
            ContentCategory::Anim => "ANM",
            ContentCategory::Audio => "AUD",
            ContentCategory::Data => "DAT",
            ContentCategory::Script => "SCM",
            ContentCategory::Other => "•",
        }
    }
}

/// What one mod contributes, distilled from the content index: the categories it
/// touches, whether it overwrites lower-priority mods, and whether any of its
/// files are overwritten by higher-priority ones. This drives the flags column
/// in the load-order list (MO2's content/conflict icons).
#[derive(Clone, Default)]
pub(crate) struct ModContentFlags {
    pub(crate) categories: BTreeSet<ContentCategory>,
    pub(crate) overwrites_others: bool,
    pub(crate) overwritten: bool,
    pub(crate) file_count: usize,
}

/// Reduce a content index to per-mod flags keyed by mod id. Only mods that
/// appear in the index (enabled and readable) get an entry.
pub(crate) fn per_mod_flags(index: &ContentIndex) -> BTreeMap<String, ModContentFlags> {
    let mut map: BTreeMap<String, ModContentFlags> = BTreeMap::new();
    for entry in &index.entries {
        let is_conflict = entry.is_conflict();
        let last = entry.providers.len().saturating_sub(1);
        for (position, id) in entry.providers.iter().enumerate() {
            let flags = map.entry(id.clone()).or_default();
            flags.categories.insert(entry.category);
            flags.file_count += 1;
            if is_conflict {
                // The last provider wins the file and thus overwrites the rest.
                if position == last {
                    flags.overwrites_others = true;
                } else {
                    flags.overwritten = true;
                }
            }
        }
    }
    map
}

/// One enabled mod resolved to what it will actually copy: where its files live
/// on disk (`source_root`) and the effective install roots (profile overrides
/// already applied, disabled roots already dropped). Built by the UI from the
/// selected profile, kept here so the index logic is testable in isolation.
pub(crate) struct IndexedMod {
    pub(crate) id: String,
    pub(crate) source_root: PathBuf,
    pub(crate) roots: Vec<ModInstallRootJson>,
}

/// One materialized target path and every enabled mod that writes it, listed in
/// load order. `providers.last()` is therefore the winner — the mod whose copy
/// survives after all others are applied.
#[derive(Clone, Debug)]
pub(crate) struct ContentEntry {
    pub(crate) target: String,
    pub(crate) category: ContentCategory,
    pub(crate) providers: Vec<String>,
}

impl ContentEntry {
    /// The mod that wins this file after load order is applied.
    pub(crate) fn winner(&self) -> &str {
        self.providers.last().map(String::as_str).unwrap_or("")
    }

    /// Whether more than one enabled mod writes this exact target.
    pub(crate) fn is_conflict(&self) -> bool {
        self.providers.len() > 1
    }
}

/// The full picture of what a profile materializes: every target file with its
/// providers, plus any mods whose files could not be read (e.g. an archive that
/// was never extracted to the library).
pub(crate) struct ContentIndex {
    pub(crate) entries: Vec<ContentEntry>,
    pub(crate) not_indexed: Vec<String>,
}

impl ContentIndex {
    pub(crate) fn conflict_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.is_conflict()).count()
    }

    pub(crate) fn category_count(&self, category: ContentCategory) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.category == category)
            .count()
    }
}

/// Resolve a mod's install roots for a profile the same way a run does: drop
/// roots the profile disables, apply any retarget override, and keep only the
/// enabled ones. Shared with the content viewer so its picture matches reality.
pub(crate) fn effective_install_roots(
    roots: &[ModInstallRootJson],
    overrides: &BTreeMap<String, ProfileRootOverride>,
) -> Vec<ModInstallRootJson> {
    roots
        .iter()
        .filter_map(|root| {
            let over = overrides.get(&root.source);
            let enabled = over.and_then(|over| over.enabled).unwrap_or(root.enabled);
            if !enabled {
                return None;
            }
            let mut resolved = root.clone();
            if let Some(target) = over.and_then(|over| over.target.clone()) {
                resolved.target = target;
            }
            Some(resolved)
        })
        .collect()
}

/// Build the content index for a set of enabled mods **already sorted by load
/// order** (lowest priority first). Files are mapped to their materialized
/// target exactly as [`crate::planning::materialize_profile_for_run`] would, so
/// the reported winner matches what a real run produces.
pub(crate) fn build_content_index(mods_in_load_order: &[IndexedMod]) -> ContentIndex {
    // target path -> (category, providers in load order). BTreeMap keeps the
    // output stable and sorted by target for a predictable viewer.
    let mut by_target: BTreeMap<String, (ContentCategory, Vec<String>)> = BTreeMap::new();
    let mut not_indexed = Vec::new();

    for indexed in mods_in_load_order {
        let mut read_any = false;
        let mut read_failed = false;
        for root in &indexed.roots {
            // A bootstrap root is blocked at run time and copies nothing, so it
            // contributes no materialized files here either.
            if root.kind.eq_ignore_ascii_case("bootstrap") {
                continue;
            }
            let Ok(relative_source) = path_from_package_root(&root.source) else {
                continue;
            };
            let source_abs = indexed.source_root.join(&relative_source);
            if !source_abs.exists() {
                continue;
            }
            match collect_files_recursive(&source_abs) {
                Ok(files) => {
                    read_any = true;
                    for file in files {
                        let Some(target) = target_path_for(&source_abs, &file, &root.target) else {
                            continue;
                        };
                        // CLEO save data is runtime user data, not mod content:
                        // keep it out of the index so it is never counted as a
                        // conflict or reported as overwritten by another mod.
                        if is_user_data_target(&target) {
                            continue;
                        }
                        let category = categorize(&root.kind, &target);
                        let entry = by_target
                            .entry(target)
                            .or_insert_with(|| (category, Vec::new()));
                        // The first mod to touch a target fixes its category; a
                        // later winner does not reclassify an existing file.
                        push_provider(&mut entry.1, &indexed.id);
                    }
                }
                Err(_) => read_failed = true,
            }
        }
        if read_failed && !read_any {
            not_indexed.push(indexed.id.clone());
        }
    }

    let entries = by_target
        .into_iter()
        .map(|(target, (category, providers))| ContentEntry {
            target,
            category,
            providers,
        })
        .collect();
    ContentIndex {
        entries,
        not_indexed,
    }
}

/// Whether a materialized target is CLEO runtime user data — per-script save
/// files under `cleo_saves/`. These are generated by the game at play time, not
/// authored mod content, so the content index skips them: they are never a
/// conflict and never reported as overwritten.
fn is_user_data_target(target: &str) -> bool {
    target
        .split('/')
        .any(|segment| segment.eq_ignore_ascii_case("cleo_saves"))
}

/// Append `id` as a provider unless it already wrote this target through another
/// root, so a mod that maps two sources onto one file is counted once.
fn push_provider(providers: &mut Vec<String>, id: &str) {
    if providers.last().map(String::as_str) != Some(id) {
        providers.push(id.to_string());
    }
}

/// The materialized target for one source file: `target_root` joined with the
/// file's path relative to the root's source, normalized to forward slashes so
/// two mods writing the same logical path compare equal across platforms.
fn target_path_for(source_abs: &Path, file: &Path, target_root: &str) -> Option<String> {
    let relative = file.strip_prefix(source_abs).ok()?;
    let relative_str = relative.to_string_lossy().replace('\\', "/");
    let base = normalize_path(target_root);
    let combined = if base.is_empty() || base == "." {
        relative_str
    } else {
        format!("{}/{relative_str}", base.trim_end_matches('/'))
    };
    Some(combined)
}

/// Sort a materialized file into a viewer bucket. The install root's `kind` wins
/// for loader subsystems (that is the modder's own declaration); direct/managed
/// roots are classified by the target path so `data/`, `models/`, `.gxt`, etc.
/// land in the right resource viewer.
fn categorize(kind: &str, target_lower: &str) -> ContentCategory {
    let lower = target_lower.to_ascii_lowercase();
    // Anything inside a `modloader/<mod>` sandbox is ModLoader content — including
    // `.asi`/`.cleo`/`.cs`, which ModLoader's std.asi loads straight from the mod
    // folder — so a self-contained mod stays grouped under its sandbox in the
    // viewer instead of scattering into the CLEO/ASI tabs.
    if segment_contains(&lower, "modloader") {
        return ContentCategory::ModLoader;
    }
    match kind.to_ascii_lowercase().as_str() {
        "modloader" => ContentCategory::ModLoader,
        "cleo" | "cleo_text" | "cleo_plugin" | "cleo_plugins" | "cleo_modules" | "cleo_saves" => {
            ContentCategory::Cleo
        }
        "asi" | "plugin" => ContentCategory::Asi,
        _ => categorize_by_path(&lower),
    }
}

fn categorize_by_path(lower: &str) -> ContentCategory {
    if lower.ends_with(".asi") {
        ContentCategory::Asi
    } else if is_cleo_script(lower) || is_cleo_plugin(lower) || lower.ends_with(".fxt") {
        // CLEO scripts (.cs/.cs4/.cs3), plugins (.cleo) and text (.fxt) all belong
        // to the CLEO subsystem viewer.
        ContentCategory::Cleo
    } else if lower.ends_with(".img") || lower.ends_with(".dff") || lower.ends_with(".txd") {
        ContentCategory::Img
    } else if lower.ends_with(".gxt") || segment_contains(lower, "text") {
        ContentCategory::Text
    } else if lower.ends_with(".ifp") || segment_contains(lower, "anim") {
        ContentCategory::Anim
    } else if segment_contains(lower, "audio") || lower.ends_with(".ogg") {
        ContentCategory::Audio
    } else if segment_contains(lower, "models") {
        ContentCategory::Models
    } else if lower.ends_with(".scm") || lower.ends_with(".cm") {
        ContentCategory::Script
    } else if segment_contains(lower, "data") || lower.ends_with(".dat") || lower.ends_with(".ide")
        || lower.ends_with(".ipl") || lower.ends_with(".cfg")
    {
        ContentCategory::Data
    } else {
        ContentCategory::Other
    }
}

/// True when `needle` appears as a whole path segment (bounded by `/`), so
/// `data/handling.cfg` matches "data" but `metadata.dat` does not.
fn segment_contains(lower: &str, needle: &str) -> bool {
    lower.split('/').any(|segment| segment == needle)
}

/// Split entries (already filtered to one category) into labeled sections for a
/// tailored viewer. ModLoader groups by its sandboxed `modloader/<name>` folder
/// — the way ModLoader actually isolates mods; every other category groups by
/// the mod that ultimately wins those files. Groups and their rows keep a stable
/// (target-sorted) order for a predictable display.
pub(crate) fn group_entries<'a>(
    category: ContentCategory,
    entries: &[&'a ContentEntry],
) -> Vec<(String, Vec<&'a ContentEntry>)> {
    let mut groups: BTreeMap<String, Vec<&'a ContentEntry>> = BTreeMap::new();
    for entry in entries {
        let key = if category == ContentCategory::ModLoader {
            modloader_folder(&entry.target)
        } else {
            entry.winner().to_string()
        };
        groups.entry(key).or_default().push(entry);
    }
    groups.into_iter().collect()
}

/// The ModLoader mod folder a target lives in: the segment right after a
/// `modloader` path segment (e.g. `modloader/ImVehFt/x.dff` → `ImVehFt`).
/// Falls back to `(root)` when a modloader target has no named subfolder.
fn modloader_folder(target: &str) -> String {
    let mut segments = target.split('/');
    while let Some(segment) = segments.next() {
        if segment.eq_ignore_ascii_case("modloader") {
            return segments.next().unwrap_or("(root)").to_string();
        }
    }
    "(root)".to_string()
}

// --- ModLoader priorities -------------------------------------------------

/// ModLoader's own per-folder load-order priorities, read from
/// `modloader/modloader.ini`. ModLoader loads folders in ascending priority so
/// higher-priority mods apply last and win — a system layered on top of this
/// manager's profile order, worth surfacing so the two are not confused.
#[derive(Clone, Default)]
pub(crate) struct ModLoaderPriorities {
    pub(crate) default: i32,
    /// Lowercased folder name → priority.
    pub(crate) by_folder: BTreeMap<String, i32>,
}

impl ModLoaderPriorities {
    /// The effective priority for a modloader mod folder, falling back to the
    /// configured default when the folder has no explicit entry.
    pub(crate) fn for_folder(&self, folder: &str) -> i32 {
        self.by_folder
            .get(&folder.to_ascii_lowercase())
            .copied()
            .unwrap_or(self.default)
    }
}

/// Read and parse `<game_root>/modloader/modloader.ini`, if present.
pub(crate) fn read_modloader_priorities(game_root: &Path) -> Option<ModLoaderPriorities> {
    let path = game_root.join("modloader").join("modloader.ini");
    let text = read_capped(&path, MAX_CONTROL_FILE_BYTES).ok()?;
    Some(parse_modloader_priorities(&text))
}

/// Parse ModLoader's priority config. Tolerant of the format's variations: any
/// section whose name contains "priority" contributes `folder = number` entries;
/// a `DefaultPriority` key (anywhere) or a `default` key in the priority section
/// sets the fallback (ModLoader's built-in default is 50).
pub(crate) fn parse_modloader_priorities(text: &str) -> ModLoaderPriorities {
    let mut priorities = ModLoaderPriorities {
        default: 50,
        by_folder: BTreeMap::new(),
    };
    let mut in_priority_section = false;
    for raw in text.lines() {
        // Strip inline `;` comments and surrounding whitespace.
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_priority_section = section.trim().to_ascii_lowercase().contains("priority");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if key.eq_ignore_ascii_case("defaultpriority") {
            if let Ok(number) = value.parse::<i32>() {
                priorities.default = number;
            }
            continue;
        }
        if in_priority_section {
            if let Ok(number) = value.parse::<i32>() {
                if key.eq_ignore_ascii_case("default") {
                    priorities.default = number;
                } else {
                    priorities.by_folder.insert(key.to_ascii_lowercase(), number);
                }
            }
        }
    }
    priorities
}

// --- ModLoader log (#3) ---------------------------------------------------

/// ModLoader caps `modloader.log` at 5 MiB; read with headroom above that and
/// refuse anything larger rather than loading an unbounded file.
const MAX_MODLOADER_LOG_BYTES: u64 = 8 * 1024 * 1024;

/// How many warning/error lines to keep — enough to be useful in the viewer
/// without letting a spammy log dictate memory.
const MAX_LOG_LINES: usize = 100;

/// A best-effort read of what ModLoader actually did on the last launch, scraped
/// from `modloader/modloader.log`. The log is free-form, plugin-authored text
/// with no stable schema, so this is a heuristic summary — the `modloader.ini`
/// the manager writes remains the source of truth for intended state; this is the
/// after-the-fact reality check (did a mod fail to load, was something ignored).
#[derive(Clone, Debug, Default)]
pub(crate) struct ModLoaderLogSummary {
    /// The ModLoader version from the log's session banner, if found.
    pub(crate) version: Option<String>,
    /// Lines that read as errors/failures (capped).
    pub(crate) errors: Vec<String>,
    /// Lines that read as warnings/skips (capped).
    pub(crate) warnings: Vec<String>,
    /// True when more error/warning lines existed than were kept.
    pub(crate) truncated: bool,
}

impl ModLoaderLogSummary {
    pub(crate) fn is_clean(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }
}

/// Read and summarize `<game_root>/modloader/modloader.log`, if present.
pub(crate) fn read_modloader_log(game_root: &Path) -> Option<ModLoaderLogSummary> {
    let path = game_root.join("modloader").join("modloader.log");
    let text = read_capped(&path, MAX_MODLOADER_LOG_BYTES).ok()?;
    Some(parse_modloader_log(&text))
}

/// Scrape a ModLoader log into a summary. Heuristic and deliberately
/// conservative: it pulls the version banner and the lines that read as
/// errors/warnings, and never claims more than the free-text log supports.
pub(crate) fn parse_modloader_log(text: &str) -> ModLoaderLogSummary {
    let mut summary = ModLoaderLogSummary::default();
    for raw in text.lines() {
        let line = raw.trim().trim_matches('=').trim();
        if line.is_empty() {
            continue;
        }
        if summary.version.is_none()
            && let Some(version) = parse_modloader_log_version(line)
        {
            summary.version = Some(version);
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if is_log_error_line(&lower) {
            push_log_line(&mut summary.errors, line, &mut summary.truncated);
        } else if is_log_warning_line(&lower) {
            push_log_line(&mut summary.warnings, line, &mut summary.truncated);
        }
    }
    summary
}

/// Extract a `Mod Loader X.Y.Z` version from a banner line, if present.
fn parse_modloader_log_version(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let idx = lower.find("mod loader ")?;
    let after = line[idx + "mod loader ".len()..].trim_start();
    let token: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Some(token)
    } else {
        None
    }
}

fn is_log_error_line(lower: &str) -> bool {
    ["error", "failed", "failure", "could not", "couldn't", "cannot", "unable to"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn is_log_warning_line(lower: &str) -> bool {
    ["warning", "warn:", "ignoring", "ignored", "skipping", "skipped", "deprecated"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn push_log_line(bucket: &mut Vec<String>, line: &str, truncated: &mut bool) {
    if bucket.len() >= MAX_LOG_LINES {
        *truncated = true;
        return;
    }
    bucket.push(line.to_string());
}

// --- ModLoader cross-folder conflicts -------------------------------------

/// A game asset that two or more ModLoader mod folders both provide. Because
/// ModLoader sandboxes each mod in its own `modloader/<folder>/`, these files
/// never collide on disk — the manager's content index sees distinct targets —
/// yet at runtime ModLoader makes them fight over the same underlying resource,
/// resolved by folder *priority*, not by profile load order or copy sequence.
/// This is the conflict class the plain target-keyed index cannot see.
#[derive(Clone, Debug)]
pub(crate) struct ModLoaderConflict {
    /// The shared virtual asset (e.g. `infernus.dff`, `data/handling.cfg`).
    pub(crate) asset: String,
    /// Every contending folder with its ModLoader priority, ordered so the
    /// runtime winner (highest priority, ties by folder name) is first.
    pub(crate) contenders: Vec<ModLoaderContender>,
    /// True when the top two contenders share a priority, so ModLoader's winner
    /// is decided by an unspecified tie-break — the user should set distinct
    /// priorities to make the outcome deterministic.
    pub(crate) ambiguous: bool,
}

impl ModLoaderConflict {
    /// The folder ModLoader will let win this asset at runtime.
    pub(crate) fn winner(&self) -> &str {
        self.contenders.first().map(|c| c.folder.as_str()).unwrap_or("")
    }
}

/// One folder contending for a shared ModLoader asset, with the priority
/// ModLoader will apply it at.
#[derive(Clone, Debug)]
pub(crate) struct ModLoaderContender {
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
pub(crate) fn modloader_virtual_asset(target: &str) -> Option<String> {
    let mut segments = target.split('/');
    // Advance to the segment right after the `modloader` root segment.
    let mut found_root = false;
    let mut rest: Vec<&str> = Vec::new();
    for segment in segments.by_ref() {
        if !found_root {
            if segment.eq_ignore_ascii_case("modloader") {
                found_root = true;
            }
            continue;
        }
        rest.push(segment);
    }
    if !found_root || rest.len() < 2 {
        // Not under a named sandbox folder (rest = [folder, file...]) — nothing
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
    if name.is_empty() {
        return None;
    }
    if is_streamed_by_name(&name) {
        Some(name)
    } else {
        Some(tail.join("/").to_ascii_lowercase())
    }
}

/// Whether a file name is a streamed asset ModLoader resolves by resource name
/// (so its subfolder is irrelevant to conflicts).
fn is_streamed_by_name(name: &str) -> bool {
    [".dff", ".txd", ".col", ".ifp"]
        .iter()
        .any(|ext| name.ends_with(ext))
}

/// Find the assets that two or more distinct ModLoader folders both provide,
/// with the runtime winner decided by ModLoader priority. `entries` should be
/// the `ContentCategory::ModLoader` slice; `priorities` is the parsed
/// `modloader.ini` (default priorities are assumed when absent).
pub(crate) fn modloader_conflicts(
    entries: &[&ContentEntry],
    priorities: &ModLoaderPriorities,
) -> Vec<ModLoaderConflict> {
    // asset -> set of contending folders.
    let mut by_asset: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in entries {
        let (Some(asset), folder) = (
            modloader_virtual_asset(&entry.target),
            modloader_folder(&entry.target),
        ) else {
            continue;
        };
        if folder == "(root)" {
            continue;
        }
        by_asset.entry(asset).or_default().insert(folder);
    }

    let mut conflicts = Vec::new();
    for (asset, folders) in by_asset {
        if folders.len() < 2 {
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
        conflicts.push(ModLoaderConflict {
            asset,
            contenders,
            ambiguous,
        });
    }
    conflicts
}

// --- ModLoader priority writer --------------------------------------------

/// ModLoader's default upper bound for folder priorities (`PriorityLimit`).
const MODLOADER_DEFAULT_PRIORITY_LIMIT: i32 = 100;

/// The sandbox folder an install-root target lands in: the segment right after
/// the `modloader` segment (e.g. `modloader/vehicle/gta3.img` → `vehicle`).
/// Returns `None` for a target that is not inside a named `modloader/<folder>`.
pub(crate) fn modloader_folder_from_target(target: &str) -> Option<String> {
    let mut segments = target.split('/').filter(|s| !s.is_empty());
    let mut found_root = false;
    for segment in segments.by_ref() {
        if segment.eq_ignore_ascii_case("modloader") {
            found_root = true;
            break;
        }
    }
    if !found_root {
        return None;
    }
    segments.next().map(|s| s.to_string())
}

/// The active ModLoader profile named in `[Folder.Config] Profile = …`,
/// defaulting to `Default` (ModLoader's own default) when unset. This is the
/// profile whose `Priority` section actually governs load order, so the manager
/// writes into it rather than assuming `Default`.
pub(crate) fn modloader_active_profile(ini: &str) -> String {
    ini_value_in_section(ini, "folder.config", "profile").unwrap_or_else(|| "Default".to_string())
}

/// The `PriorityLimit` from `[Folder.Config]`, defaulting to 100 and floored at
/// 2 so a spread of distinct priorities is always possible.
pub(crate) fn modloader_priority_limit(ini: &str) -> i32 {
    ini_value_in_section(ini, "folder.config", "prioritylimit")
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(MODLOADER_DEFAULT_PRIORITY_LIMIT)
        .max(2)
}

/// First `key = value` in the section whose header equals `section`
/// (case-insensitive), with inline `;` comments stripped.
fn ini_value_in_section(ini: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for raw in ini.lines() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if let Some(header) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_section = header.trim().eq_ignore_ascii_case(section);
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((found, value)) = line.split_once('=')
            && found.trim().eq_ignore_ascii_case(key)
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Map a mod's rank among the modloader mods (0 = applied first / lowest
/// priority) to a distinct ModLoader priority spread across `[1, limit]` and
/// centered on 50, so managed mods straddle ModLoader's default for unlisted
/// mods and preserve the profile's relative order. A lone mod gets ~50.
pub(crate) fn spread_priority(rank: usize, count: usize, limit: i32) -> i32 {
    let limit = limit.max(2);
    if count <= 1 {
        return (limit / 2).clamp(1, limit);
    }
    let numer = rank as i64 * (limit as i64 - 1);
    let denom = (count - 1) as i64;
    let value = 1 + ((numer + denom / 2) / denom) as i32;
    value.clamp(1, limit)
}

/// Produce a `modloader.ini` that reflects `folder_priorities` (folder name →
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
) -> Option<String> {
    let header = format!("[Profiles.{profile}.Priority]");
    // Remaining folders to place, lowercased key → (display name, priority).
    let mut pending: BTreeMap<String, (String, i32)> = folder_priorities
        .iter()
        .map(|(folder, priority)| {
            (
                folder.to_ascii_lowercase(),
                (folder.clone(), (*priority).clamp(1, limit.max(2))),
            )
        })
        .collect();

    let text = existing.unwrap_or("");
    if text.trim().is_empty() {
        return Some(fresh_priority_ini(profile, limit, &pending));
    }

    let mut out: Vec<String> = Vec::new();
    let mut in_target = false;
    let mut wrote_section = false;
    let mut changed = false;
    let lines: Vec<&str> = text.lines().collect();
    for line in lines {
        let trimmed = line.trim();
        let is_header = trimmed.starts_with('[') && trimmed.ends_with(']');
        if in_target && is_header {
            // Leaving the target section: flush any folders not already present.
            flush_pending(&mut out, &mut pending, &mut changed);
            in_target = false;
        }
        if is_header {
            in_target = trimmed.eq_ignore_ascii_case(&header);
            if in_target {
                wrote_section = true;
            }
            out.push(line.to_string());
            continue;
        }
        if in_target {
            // Update an existing `folder = value` entry in place.
            if let Some((key, _)) = trimmed.split_once('=') {
                let folder_key = key.trim().to_ascii_lowercase();
                if let Some((display, priority)) = pending.remove(&folder_key) {
                    let replacement = format!("{display} = {priority}");
                    if replacement != trimmed {
                        changed = true;
                    }
                    out.push(replacement);
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }
    // File ended while still inside the target section, or the section was never
    // seen: flush the remainder (adding the section header if needed).
    if in_target {
        flush_pending(&mut out, &mut pending, &mut changed);
    } else if !wrote_section && !pending.is_empty() {
        if out.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
            out.push(String::new());
        }
        out.push(header.clone());
        flush_pending(&mut out, &mut pending, &mut changed);
    }

    if !changed {
        return None;
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    Some(joined)
}

/// Append each still-pending folder as a `Folder = priority` line.
fn flush_pending(
    out: &mut Vec<String>,
    pending: &mut BTreeMap<String, (String, i32)>,
    changed: &mut bool,
) {
    if pending.is_empty() {
        return;
    }
    for (_, (display, priority)) in std::mem::take(pending) {
        out.push(format!("{display} = {priority}"));
        *changed = true;
    }
}

/// A minimal `modloader.ini` created when none exists, pointing the active
/// profile at `profile` and listing the folder priorities.
fn fresh_priority_ini(
    profile: &str,
    limit: i32,
    pending: &BTreeMap<String, (String, i32)>,
) -> String {
    let mut lines = vec![
        "; Generated by SA Mod Manager: ModLoader folder priority from profile load order."
            .to_string(),
        "[Folder.Config]".to_string(),
        format!("Profile = {profile}"),
        format!("PriorityLimit = {limit}"),
        String::new(),
        format!("[Profiles.{profile}.Priority]"),
    ];
    for (display, priority) in pending.values() {
        lines.push(format!("{display} = {priority}"));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

// --- ModLoader native profile (2b) ----------------------------------------

/// The name of the manager-owned ModLoader profile for a given manager profile.
/// Prefixed and sanitized so it never collides with a user's own profiles and is
/// safe inside `[Profiles.<name>.*]` headers and a `-modprof <name>` argument.
pub(crate) fn modloader_managed_profile_name(profile_name: &str) -> String {
    format!("SAMM_{}", safe_name(profile_name))
}

/// Render a `modloader.ini` that expresses the profile as a native ModLoader
/// profile: a `[Profiles.<profile>.Config]` (inheriting `parent` so the user's
/// own settings still apply), a `[Profiles.<profile>.Priority]` section from load
/// order, and a `[Profiles.<profile>.IgnoreMods]` section listing `ignore_folders`
/// — the mods the manager has disabled, so ModLoader skips their folders even
/// when they are physically present (a persistent `modloader/` install). All
/// other content is preserved, and `[Folder.Config]` is left alone — the profile
/// is activated per launch with `-modprof`. Returns `None` when nothing changes.
pub(crate) fn render_modloader_managed_profile(
    existing: Option<&str>,
    profile: &str,
    parent: &str,
    limit: i32,
    folder_priorities: &BTreeMap<String, i32>,
    ignore_folders: &[String],
) -> Option<String> {
    let base = existing.unwrap_or("");
    let (with_config, config_changed) = ensure_profile_config_section(base, profile, parent);
    // Layer the priority section on top of the (possibly config-augmented) text.
    // Passing a non-empty text avoids the "fresh file" path, so we never write a
    // `[Folder.Config] Profile =` line — activation is via -modprof only.
    let priority = render_modloader_priority_ini(Some(&with_config), profile, limit, folder_priorities);
    let after_priority = priority.clone().unwrap_or(with_config);
    // Regenerate our own IgnoreMods block (we fully own this profile's sections).
    let (after_ignore, ignore_changed) =
        set_profile_ignore_mods(&after_priority, profile, ignore_folders);
    if config_changed || priority.is_some() || ignore_changed {
        Some(after_ignore)
    } else {
        None
    }
}

/// Replace this profile's `[Profiles.<profile>.IgnoreMods]` block with one listing
/// `folders` (one bare glob per line, ModLoader's IgnoreMods format). The block is
/// dropped entirely when `folders` is empty. Returns the text and whether it
/// changed. Only this profile's own IgnoreMods section is touched.
fn set_profile_ignore_mods(text: &str, profile: &str, folders: &[String]) -> (String, bool) {
    let header = format!("[Profiles.{profile}.IgnoreMods]");
    // Drop any existing managed IgnoreMods block (header + its body up to the
    // next section), preserving everything else.
    let mut out: Vec<String> = Vec::new();
    let mut skipping = false;
    for line in text.lines() {
        let trimmed = line.trim();
        let is_header = trimmed.starts_with('[') && trimmed.ends_with(']');
        if skipping {
            if is_header {
                skipping = false;
            } else {
                continue;
            }
        }
        if is_header && trimmed.eq_ignore_ascii_case(&header) {
            skipping = true;
            continue;
        }
        out.push(line.to_string());
    }
    if !folders.is_empty() {
        if out.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
            out.push(String::new());
        }
        out.push(header);
        for folder in folders {
            out.push(folder.clone());
        }
    }
    let mut result = out.join("\n");
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    let changed = result.trim_end() != text.trim_end();
    (result, changed)
}

/// Ensure a `[Profiles.<profile>.Config]` section exists (creating it with
/// `Parents = parent` when absent, so ModLoader recognizes the profile). Returns
/// the text and whether it was changed.
fn ensure_profile_config_section(text: &str, profile: &str, parent: &str) -> (String, bool) {
    let header = format!("[Profiles.{profile}.Config]");
    let has_section = text
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case(&header));
    if has_section {
        return (text.to_string(), false);
    }
    if text.trim().is_empty() {
        return (format!("{header}\nParents = {parent}\n"), true);
    }
    let mut out = text.trim_end().to_string();
    out.push_str("\n\n");
    out.push_str(&header);
    out.push('\n');
    out.push_str(&format!("Parents = {parent}\n"));
    (out, true)
}

// --- CLEO viewer ----------------------------------------------------------

/// A CLEO script (.cs/.cs4/.cleo …) paired with the companion files that ship
/// alongside it (its `.ini`, `.fxt`, data files sharing the same name), so the
/// viewer can present a script as the unit a modder actually installs.
pub(crate) struct CleoScript<'a> {
    pub(crate) script: &'a ContentEntry,
    pub(crate) companions: Vec<&'a ContentEntry>,
}

/// The CLEO viewer's model: plugin modules (`.cleo`), scripts with their
/// companions, plus loose files in the CLEO folder that belong to no script.
pub(crate) struct CleoView<'a> {
    pub(crate) plugins: Vec<&'a ContentEntry>,
    pub(crate) scripts: Vec<CleoScript<'a>>,
    pub(crate) loose: Vec<&'a ContentEntry>,
}

/// Split CLEO-category entries into plugins, scripts-with-companions and loose
/// files. A companion is a non-script file sharing a script's directory and base
/// name (e.g. `cleo/speedo.cs` owns `cleo/speedo.ini`). Plugin modules (`.cleo`,
/// which CLEO5 loads from `cleo_plugins/`) are their own unit, not script
/// companions.
pub(crate) fn cleo_view<'a>(entries: &[&'a ContentEntry]) -> CleoView<'a> {
    let mut plugins: Vec<&ContentEntry> = Vec::new();
    let mut scripts: Vec<&ContentEntry> = Vec::new();
    let mut companions_by_key: BTreeMap<(String, String), Vec<&'a ContentEntry>> = BTreeMap::new();
    for &entry in entries {
        if is_cleo_plugin(&entry.target) {
            plugins.push(entry);
        } else if is_cleo_script(&entry.target) {
            scripts.push(entry);
        } else {
            let (dir, stem) = dir_and_stem(&entry.target);
            companions_by_key
                .entry((dir.to_string(), stem.to_string()))
                .or_default()
                .push(entry);
        }
    }
    plugins.sort_by(|a, b| a.target.cmp(&b.target));
    scripts.sort_by(|a, b| a.target.cmp(&b.target));

    let script_keys: BTreeSet<(String, String)> = scripts
        .iter()
        .map(|script| {
            let (dir, stem) = dir_and_stem(&script.target);
            (dir.to_string(), stem.to_string())
        })
        .collect();

    let script_rows = scripts
        .iter()
        .map(|&script| {
            let (dir, stem) = dir_and_stem(&script.target);
            let mut companions = companions_by_key
                .get(&(dir.to_string(), stem.to_string()))
                .cloned()
                .unwrap_or_default();
            companions.sort_by(|a, b| a.target.cmp(&b.target));
            CleoScript { script, companions }
        })
        .collect();
    let loose = companions_by_key
        .iter()
        .filter(|(key, _)| !script_keys.contains(*key))
        .flat_map(|(_, files)| files.iter().copied())
        .collect();
    CleoView {
        plugins,
        scripts: script_rows,
        loose,
    }
}

/// A CLEO script: `.cs` (CLEO5), `.cs4` (CLEO4 compat), `.cs3` (CLEO3 compat).
/// Uses the shared extension list so this never drifts from detection.
fn is_cleo_script(target: &str) -> bool {
    has_cleo_script_extension(&target.to_ascii_lowercase())
}

/// A CLEO5 plugin module (`.cleo`), loaded from `cleo/cleo_plugins/`.
fn is_cleo_plugin(target: &str) -> bool {
    target.to_ascii_lowercase().ends_with(".cleo")
}

/// A target's directory and file stem (name without its final extension), used
/// to pair CLEO companions with their script.
fn dir_and_stem(target: &str) -> (&str, &str) {
    let (dir, name) = match target.rfind('/') {
        Some(index) => (&target[..index], &target[index + 1..]),
        None => ("", target),
    };
    let stem = match name.rfind('.') {
        Some(index) => &name[..index],
        None => name,
    };
    (dir, stem)
}

// --- ASI viewer -----------------------------------------------------------

/// The ASI viewer's model: actual `.asi` plugins, the loader/proxy DLLs that
/// bootstrap them (Ultimate ASI Loader and friends), and anything else.
pub(crate) struct AsiView<'a> {
    pub(crate) plugins: Vec<&'a ContentEntry>,
    pub(crate) loaders: Vec<&'a ContentEntry>,
    pub(crate) other: Vec<&'a ContentEntry>,
}

/// Sort ASI-category entries into plugins, loader/proxy DLLs, and other files.
pub(crate) fn asi_view<'a>(entries: &[&'a ContentEntry]) -> AsiView<'a> {
    let mut plugins = Vec::new();
    let mut loaders = Vec::new();
    let mut other = Vec::new();
    for &entry in entries {
        if entry.target.to_ascii_lowercase().ends_with(".asi") {
            plugins.push(entry);
        } else if is_asi_loader(&entry.target) {
            loaders.push(entry);
        } else {
            other.push(entry);
        }
    }
    for bucket in [&mut plugins, &mut loaders, &mut other] {
        bucket.sort_by(|a, b| a.target.cmp(&b.target));
    }
    AsiView {
        plugins,
        loaders,
        other,
    }
}

/// Known ASI loader / proxy DLL file names. These hook the game's startup to
/// load `.asi` plugins; normally exactly one should be present.
fn is_asi_loader(target: &str) -> bool {
    const LOADERS: [&str; 8] = [
        "dinput8.dll",
        "dinput.dll",
        "vorbisfile.dll",
        "vorbishooked.dll",
        "dsound.dll",
        "d3d9.dll",
        "ddraw.dll",
        "dxwrapper.dll",
    ];
    let name = target.rsplit('/').next().unwrap_or(target).to_ascii_lowercase();
    LOADERS.contains(&name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(source: &str, target: &str, kind: &str) -> ModInstallRootJson {
        ModInstallRootJson {
            source: source.to_string(),
            target: target.to_string(),
            kind: kind.to_string(),
            enabled: true,
            optional: false,
        }
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn later_mod_wins_a_conflicting_target_and_is_flagged() {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-content-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let early = base.join("early");
        let late = base.join("late");
        write_file(&early.join("payload").join("handling.cfg"), "early");
        write_file(&late.join("payload").join("handling.cfg"), "late");

        let mods = vec![
            IndexedMod {
                id: "early".to_string(),
                source_root: early.clone(),
                roots: vec![root("payload", "data", "direct")],
            },
            IndexedMod {
                id: "late".to_string(),
                source_root: late.clone(),
                roots: vec![root("payload", "data", "direct")],
            },
        ];

        let index = build_content_index(&mods);
        assert_eq!(index.entries.len(), 1);
        let entry = &index.entries[0];
        assert_eq!(entry.target, "data/handling.cfg");
        assert_eq!(entry.category, ContentCategory::Data);
        assert_eq!(entry.providers, vec!["early", "late"]);
        assert_eq!(entry.winner(), "late");
        assert!(entry.is_conflict());
        assert_eq!(index.conflict_count(), 1);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn per_mod_flags_mark_winner_and_loser_of_a_conflict() {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-flags-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let early = base.join("early");
        let late = base.join("late");
        write_file(&early.join("p").join("handling.cfg"), "e");
        write_file(&early.join("p").join("solo.dat"), "e");
        write_file(&late.join("p").join("handling.cfg"), "l");

        let mods = vec![
            IndexedMod {
                id: "early".to_string(),
                source_root: early.clone(),
                roots: vec![root("p", "data", "direct")],
            },
            IndexedMod {
                id: "late".to_string(),
                source_root: late.clone(),
                roots: vec![root("p", "data", "direct")],
            },
        ];
        let index = build_content_index(&mods);
        let flags = per_mod_flags(&index);

        let early_flags = flags.get("early").unwrap();
        assert!(early_flags.overwritten, "early loses handling.cfg");
        assert!(!early_flags.overwrites_others);
        assert_eq!(early_flags.file_count, 2);

        let late_flags = flags.get("late").unwrap();
        assert!(late_flags.overwrites_others, "late wins handling.cfg");
        assert!(!late_flags.overwritten);
        assert!(late_flags.categories.contains(&ContentCategory::Data));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn cleo_saves_are_excluded_from_the_content_index() {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-saves-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let a = base.join("a");
        let b = base.join("b");
        // Two mods both ship the same CLEO save file — normally a conflict.
        write_file(&a.join("s").join("slot1.sav"), "a");
        write_file(&b.join("s").join("slot1.sav"), "b");

        let mods = vec![
            IndexedMod {
                id: "a".to_string(),
                source_root: a.clone(),
                roots: vec![root("s", "CLEO/cleo_saves", "cleo_saves")],
            },
            IndexedMod {
                id: "b".to_string(),
                source_root: b.clone(),
                roots: vec![root("s", "CLEO/cleo_saves", "cleo_saves")],
            },
        ];

        let index = build_content_index(&mods);
        assert!(
            index.entries.is_empty(),
            "CLEO save data is user data and must not be indexed"
        );
        assert_eq!(index.conflict_count(), 0, "save data is never a conflict");
        assert!(per_mod_flags(&index).is_empty(), "no mod is flagged for saves");
        fs::remove_dir_all(&base).unwrap();
    }

    fn entry(target: &str) -> ContentEntry {
        ContentEntry {
            target: target.to_string(),
            category: ContentCategory::Cleo,
            providers: vec!["m".to_string()],
        }
    }

    #[test]
    fn cleo_view_pairs_scripts_with_their_companions() {
        let script = entry("cleo/speedo.cs");
        let ini = entry("cleo/speedo.ini");
        let fxt = entry("cleo/speedo.fxt");
        let orphan = entry("cleo/shared_data.dat");
        let refs = vec![&script, &ini, &fxt, &orphan];

        let view = cleo_view(&refs);
        assert!(view.plugins.is_empty());
        assert_eq!(view.scripts.len(), 1);
        assert_eq!(view.scripts[0].script.target, "cleo/speedo.cs");
        let companions: Vec<&str> = view.scripts[0]
            .companions
            .iter()
            .map(|c| c.target.as_str())
            .collect();
        assert_eq!(companions, vec!["cleo/speedo.fxt", "cleo/speedo.ini"]);
        assert_eq!(view.loose.len(), 1);
        assert_eq!(view.loose[0].target, "cleo/shared_data.dat");
    }

    #[test]
    fn cleo_view_separates_plugins_from_scripts() {
        let script = entry("cleo/mod.cs");
        let compat = entry("cleo/legacy.cs4");
        let plugin = entry("cleo/cleo_plugins/SA.IniFiles.cleo");
        let refs = vec![&script, &compat, &plugin];

        let view = cleo_view(&refs);
        assert_eq!(view.plugins.len(), 1);
        assert_eq!(view.plugins[0].target, "cleo/cleo_plugins/SA.IniFiles.cleo");
        let scripts: Vec<&str> = view.scripts.iter().map(|s| s.script.target.as_str()).collect();
        assert_eq!(scripts, vec!["cleo/legacy.cs4", "cleo/mod.cs"]);
    }

    #[test]
    fn asi_view_separates_plugins_from_loader_dlls() {
        let plugin = entry("scripts/CLEO.asi");
        let loader = entry("dinput8.dll");
        let config = entry("scripts/CLEO.ini");
        let refs = vec![&plugin, &loader, &config];

        let view = asi_view(&refs);
        assert_eq!(view.plugins.len(), 1);
        assert_eq!(view.plugins[0].target, "scripts/CLEO.asi");
        assert_eq!(view.loaders.len(), 1);
        assert_eq!(view.loaders[0].target, "dinput8.dll");
        assert_eq!(view.other.len(), 1);
        assert_eq!(view.other[0].target, "scripts/CLEO.ini");
    }

    #[test]
    fn modloader_priorities_parse_section_and_default() {
        let ini = "\
[Config]
DefaultPriority = 60

[Config.Priority]
; comment line
ImVehFt = 100
HD_Roads=30   ; inline comment
";
        let priorities = parse_modloader_priorities(ini);
        assert_eq!(priorities.default, 60);
        assert_eq!(priorities.for_folder("ImVehFt"), 100);
        // Case-insensitive folder lookup.
        assert_eq!(priorities.for_folder("hd_roads"), 30);
        // Unknown folder falls back to the default.
        assert_eq!(priorities.for_folder("SomethingElse"), 60);
    }

    #[test]
    fn modloader_entries_group_by_their_sandboxed_folder() {
        let make = |target: &str| ContentEntry {
            target: target.to_string(),
            category: ContentCategory::ModLoader,
            providers: vec!["m".to_string()],
        };
        let a = make("modloader/ImVehFt/veh.dff");
        let b = make("modloader/ImVehFt/veh.txd");
        let c = make("modloader/HD_Roads/road.txd");
        let refs = vec![&a, &b, &c];

        let groups = group_entries(ContentCategory::ModLoader, &refs);
        let labels: Vec<&str> = groups.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(labels, vec!["HD_Roads", "ImVehFt"]);
        let imvehft = groups.iter().find(|(label, _)| label == "ImVehFt").unwrap();
        assert_eq!(imvehft.1.len(), 2);
    }

    #[test]
    fn non_modloader_entries_group_by_winning_mod() {
        let entry = ContentEntry {
            target: "data/handling.cfg".to_string(),
            category: ContentCategory::Data,
            providers: vec!["early".to_string(), "late".to_string()],
        };
        let refs = vec![&entry];
        let groups = group_entries(ContentCategory::Data, &refs);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "late");
    }

    #[test]
    fn kinds_and_paths_route_to_the_right_viewers() {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-content-cat-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let m = base.join("mod");
        write_file(&m.join("cleo").join("script.cs"), "x");
        write_file(&m.join("asi").join("plugin.asi"), "x");
        write_file(&m.join("ml").join("car.dff"), "x");
        write_file(&m.join("direct").join("text").join("american.gxt"), "x");

        let mods = vec![IndexedMod {
            id: "mod".to_string(),
            source_root: m.clone(),
            roots: vec![
                root("cleo", "cleo", "cleo"),
                root("asi", ".", "asi"),
                root("ml", "modloader/mymod", "modloader"),
                root("direct", ".", "direct"),
            ],
        }];

        let index = build_content_index(&mods);
        let category_of = |target: &str| {
            index
                .entries
                .iter()
                .find(|entry| entry.target == target)
                .map(|entry| entry.category)
        };
        assert_eq!(category_of("cleo/script.cs"), Some(ContentCategory::Cleo));
        assert_eq!(category_of("plugin.asi"), Some(ContentCategory::Asi));
        assert_eq!(
            category_of("modloader/mymod/car.dff"),
            Some(ContentCategory::ModLoader)
        );
        assert_eq!(
            category_of("text/american.gxt"),
            Some(ContentCategory::Text)
        );
        fs::remove_dir_all(&base).unwrap();
    }

    fn ml_entry(target: &str) -> ContentEntry {
        ContentEntry {
            target: target.to_string(),
            category: ContentCategory::ModLoader,
            providers: vec!["m".to_string()],
        }
    }

    #[test]
    fn virtual_asset_strips_sandbox_and_img_container() {
        // Streamed model resolves by base name, ignoring the img container.
        assert_eq!(
            modloader_virtual_asset("modloader/ModA/gta3.img/infernus.dff").as_deref(),
            Some("infernus.dff")
        );
        assert_eq!(
            modloader_virtual_asset("modloader/ModB/infernus.dff").as_deref(),
            Some("infernus.dff")
        );
        // Non-streamed data keeps its relative path (minus the sandbox folder).
        assert_eq!(
            modloader_virtual_asset("modloader/ModA/data/handling.cfg").as_deref(),
            Some("data/handling.cfg")
        );
        // A bare sandbox folder with no file is not comparable.
        assert_eq!(modloader_virtual_asset("modloader/ModA").as_deref(), None);
    }

    #[test]
    fn modloader_conflicts_detected_across_sandbox_folders_with_priority_winner() {
        // Two vehicle mods each replace infernus.dff in their own folder — no
        // literal target collision, but a real ModLoader runtime conflict.
        let a = ml_entry("modloader/ModA/infernus.dff");
        let b = ml_entry("modloader/ModB/gta3.img/infernus.dff");
        let refs = vec![&a, &b];

        let mut priorities = ModLoaderPriorities {
            default: 50,
            by_folder: BTreeMap::new(),
        };
        priorities.by_folder.insert("moda".to_string(), 30);
        priorities.by_folder.insert("modb".to_string(), 80);

        let conflicts = modloader_conflicts(&refs, &priorities);
        assert_eq!(conflicts.len(), 1);
        let conflict = &conflicts[0];
        assert_eq!(conflict.asset, "infernus.dff");
        // Higher priority wins.
        assert_eq!(conflict.winner(), "ModB");
        assert!(!conflict.ambiguous);
    }

    #[test]
    fn equal_priority_conflict_is_flagged_ambiguous() {
        let a = ml_entry("modloader/ModA/player.txd");
        let b = ml_entry("modloader/ModB/player.txd");
        let refs = vec![&a, &b];
        // Both fall back to the default priority.
        let priorities = ModLoaderPriorities {
            default: 50,
            by_folder: BTreeMap::new(),
        };

        let conflicts = modloader_conflicts(&refs, &priorities);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].ambiguous, "equal priorities are non-deterministic");
        // A single-folder asset is never a conflict.
        let solo = ml_entry("modloader/ModA/unique.dff");
        assert!(modloader_conflicts(&[&solo], &priorities).is_empty());
    }

    #[test]
    fn folder_from_target_extracts_named_sandbox() {
        assert_eq!(
            modloader_folder_from_target("modloader/vehicle/gta3.img").as_deref(),
            Some("vehicle")
        );
        // A target that is itself the sandbox folder yields that folder.
        assert_eq!(
            modloader_folder_from_target("modloader/HD_Roads").as_deref(),
            Some("HD_Roads")
        );
        // Bare `modloader` (no named subfolder) and non-modloader targets yield none.
        assert_eq!(modloader_folder_from_target("modloader").as_deref(), None);
        assert_eq!(modloader_folder_from_target("cleo").as_deref(), None);
    }

    #[test]
    fn spread_priority_is_ordered_distinct_and_centered() {
        // Two mods land at the extremes; three straddle the default.
        assert_eq!(spread_priority(0, 2, 100), 1);
        assert_eq!(spread_priority(1, 2, 100), 100);
        assert_eq!(spread_priority(0, 3, 100), 1);
        assert_eq!(spread_priority(1, 3, 100), 51);
        assert_eq!(spread_priority(2, 3, 100), 100);
        // A lone mod sits near the default.
        assert_eq!(spread_priority(0, 1, 100), 50);
    }

    #[test]
    fn render_priority_ini_from_scratch_sets_profile_and_section() {
        let mut priorities = BTreeMap::new();
        priorities.insert("HD_Roads".to_string(), 1);
        priorities.insert("ImVehFt".to_string(), 100);
        let ini = render_modloader_priority_ini(None, "Default", 100, &priorities).unwrap();

        assert!(ini.contains("[Folder.Config]"));
        assert!(ini.contains("Profile = Default"));
        assert!(ini.contains("[Profiles.Default.Priority]"));
        assert!(ini.contains("HD_Roads = 1"));
        assert!(ini.contains("ImVehFt = 100"));
        // Round-trips through our own reader.
        let parsed = parse_modloader_priorities(&ini);
        assert_eq!(parsed.for_folder("ImVehFt"), 100);
        assert_eq!(parsed.for_folder("HD_Roads"), 1);
    }

    #[test]
    fn render_priority_ini_preserves_unrelated_lines_and_updates_in_place() {
        let existing = "\
; user notes
[Folder.Config]
Profile = Default
PriorityLimit = 100

[Profiles.Default.Config]
IgnoreAllMods = false

[Profiles.Default.Priority]
ImVehFt = 20
UserMod = 90

[Profiles.Default.IgnoreMods]
_ignore
";
        let mut priorities = BTreeMap::new();
        // Update an existing managed folder and add a new one; UserMod untouched.
        priorities.insert("ImVehFt".to_string(), 100);
        priorities.insert("HD_Roads".to_string(), 1);
        let ini =
            render_modloader_priority_ini(Some(existing), "Default", 100, &priorities).unwrap();

        assert!(ini.contains("; user notes"));
        assert!(ini.contains("[Profiles.Default.Config]"));
        assert!(ini.contains("UserMod = 90"), "unmanaged folder preserved");
        assert!(ini.contains("ImVehFt = 100"), "managed folder updated in place");
        assert!(ini.contains("HD_Roads = 1"), "new managed folder added");
        assert!(ini.contains("[Profiles.Default.IgnoreMods]"));
        assert!(ini.contains("_ignore"));
        // The new folder is inserted inside the Priority section, before the
        // next section header.
        let priority_idx = ini.find("[Profiles.Default.Priority]").unwrap();
        let ignore_idx = ini.find("[Profiles.Default.IgnoreMods]").unwrap();
        let hd_idx = ini.find("HD_Roads = 1").unwrap();
        assert!(priority_idx < hd_idx && hd_idx < ignore_idx);
    }

    #[test]
    fn render_priority_ini_returns_none_when_already_current() {
        let existing = "\
[Folder.Config]
Profile = Default

[Profiles.Default.Priority]
ImVehFt = 100
";
        let mut priorities = BTreeMap::new();
        priorities.insert("ImVehFt".to_string(), 100);
        assert!(
            render_modloader_priority_ini(Some(existing), "Default", 100, &priorities).is_none(),
            "no change should skip the write"
        );
    }

    #[test]
    fn active_profile_and_limit_read_from_folder_config() {
        let ini = "\
[Folder.Config]
Profile = SAMP
PriorityLimit = 200
";
        assert_eq!(modloader_active_profile(ini), "SAMP");
        assert_eq!(modloader_priority_limit(ini), 200);
        // Defaults when absent.
        assert_eq!(modloader_active_profile(""), "Default");
        assert_eq!(modloader_priority_limit(""), 100);
    }

    #[test]
    fn managed_profile_name_is_prefixed_and_sanitized() {
        assert_eq!(modloader_managed_profile_name("vanilla-plus"), "SAMM_vanilla-plus");
        // Spaces and unsafe characters are normalized so the name is safe in an
        // INI header and a -modprof argument.
        let name = modloader_managed_profile_name("My Profile!");
        assert!(name.starts_with("SAMM_"));
        assert!(!name.contains(' '));
        assert!(!name.contains('!'));
    }

    #[test]
    fn managed_profile_from_scratch_writes_config_and_priority_without_folder_config() {
        let mut priorities = BTreeMap::new();
        priorities.insert("early_mod".to_string(), 1);
        priorities.insert("late_mod".to_string(), 100);
        let ini =
            render_modloader_managed_profile(None, "SAMM_default", "Default", 100, &priorities, &[])
                .unwrap();

        assert!(ini.contains("[Profiles.SAMM_default.Config]"));
        assert!(ini.contains("Parents = Default"));
        assert!(ini.contains("[Profiles.SAMM_default.Priority]"));
        assert!(ini.contains("early_mod = 1"));
        assert!(ini.contains("late_mod = 100"));
        // Activation is via -modprof, so the file's default profile is untouched.
        assert!(!ini.contains("[Folder.Config]"));
        // No disabled mods -> no IgnoreMods section.
        assert!(!ini.contains("IgnoreMods"));
    }

    #[test]
    fn managed_profile_writes_ignoremods_for_disabled_folders() {
        let mut priorities = BTreeMap::new();
        priorities.insert("enabled_mod".to_string(), 100);
        let ignore = vec!["off_mod".to_string(), "also_off".to_string()];
        let ini = render_modloader_managed_profile(
            None,
            "SAMM_default",
            "Default",
            100,
            &priorities,
            &ignore,
        )
        .unwrap();

        assert!(ini.contains("[Profiles.SAMM_default.IgnoreMods]"), "ini was:\n{ini}");
        assert!(ini.contains("off_mod"));
        assert!(ini.contains("also_off"));
        // Clearing the disabled set removes the IgnoreMods block again.
        let cleared =
            render_modloader_managed_profile(Some(&ini), "SAMM_default", "Default", 100, &priorities, &[])
                .unwrap();
        assert!(!cleared.contains("IgnoreMods"), "cleared ini was:\n{cleared}");
        assert!(!cleared.contains("off_mod"));
    }

    #[test]
    fn managed_profile_preserves_user_config_and_leaves_default_active() {
        let existing = "\
[Folder.Config]
Profile = Default
PriorityLimit = 100

[Profiles.Default.Priority]
UserMod = 70
";
        let mut priorities = BTreeMap::new();
        priorities.insert("my_mod".to_string(), 100);
        let ini = render_modloader_managed_profile(
            Some(existing),
            "SAMM_default",
            "Default",
            100,
            &priorities,
            &[],
        )
        .unwrap();

        // The user's own profile, active profile, and priorities are untouched.
        assert!(ini.contains("Profile = Default"));
        assert!(ini.contains("UserMod = 70"));
        // Our managed profile is added, inheriting the user's Default.
        assert!(ini.contains("[Profiles.SAMM_default.Config]"));
        assert!(ini.contains("Parents = Default"));
        assert!(ini.contains("[Profiles.SAMM_default.Priority]"));
        assert!(ini.contains("my_mod = 100"));
    }

    #[test]
    fn managed_profile_returns_none_when_already_current() {
        let existing = "\
[Profiles.SAMM_default.Config]
Parents = Default

[Profiles.SAMM_default.Priority]
my_mod = 100
";
        let mut priorities = BTreeMap::new();
        priorities.insert("my_mod".to_string(), 100);
        assert!(
            render_modloader_managed_profile(
                Some(existing),
                "SAMM_default",
                "Default",
                100,
                &priorities,
                &[]
            )
            .is_none()
        );
    }

    #[test]
    fn modloader_log_scrapes_version_errors_and_warnings() {
        let log = "\
========================== Mod Loader 0.3.7 ==========================
Using data from \"modloader/late_mod\"
Warning: ignoring file \"readme.txt\"
Failed to read \"modloader/broken/veh.dff\"
Everything loaded fine
Could not open archive gta3.img
";
        let summary = parse_modloader_log(log);
        assert_eq!(summary.version.as_deref(), Some("0.3.7"));
        assert_eq!(summary.errors.len(), 2, "errors: {:?}", summary.errors);
        assert!(summary.errors.iter().any(|l| l.contains("Failed to read")));
        assert!(summary.errors.iter().any(|l| l.contains("Could not open")));
        assert_eq!(summary.warnings.len(), 1);
        assert!(summary.warnings[0].contains("ignoring"));
        assert!(!summary.is_clean());
    }

    #[test]
    fn modloader_log_clean_run_has_no_findings() {
        let log = "\
========================== Mod Loader 0.3.7 ==========================
Using data from \"modloader/a\"
Using data from \"modloader/b\"
";
        let summary = parse_modloader_log(log);
        assert_eq!(summary.version.as_deref(), Some("0.3.7"));
        assert!(summary.is_clean());
        assert!(!summary.truncated);
    }
}
