use crate::prelude::*;
const CONTENT_CATEGORY_COUNT: usize = 11;
const MODLOADER_BUILTIN_DEFAULT_PRIORITY: i32 = 50;
const MODLOADER_PRIORITY_MIN: i32 = 1;
const MODLOADER_PRIORITY_PAIR_COUNT: usize = 2;
const MODLOADER_PRIORITY_TRIPLE_COUNT: usize = 3;
const MODLOADER_PRIORITY_PAIR_DIVISOR_I32: i32 = 2;
const MODLOADER_PRIORITY_PAIR_DIVISOR_I64: i64 = 2;
const MODLOADER_TEST_LIMIT: i32 = 100;
const MODLOADER_TEST_DEFAULT_PRIORITY: i32 = 60;
const MODLOADER_TEST_LOW_PRIORITY: i32 = 30;
const MODLOADER_TEST_HIGH_PRIORITY: i32 = 80;
const MODLOADER_TEST_MID_PRIORITY: i32 = 51;
const MODLOADER_TEST_EXTENDED_LIMIT: i32 = 200;
const SANDBOXED_ASSET_MIN_SEGMENTS: usize = 2;
const CONFLICT_MIN_CONTENDERS: usize = 2;
const EXPECTED_HANDLING_FILE_COUNT: usize = 2;
const EXPECTED_GROUP_ENTRY_COUNT: usize = 2;
const EXPECTED_LOG_ERROR_COUNT: usize = 2;

/// The San-Andreas-native content buckets a materialized file can fall into.
/// These are the "viewers" the UI groups by: the loader subsystems (ModLoader,
/// CLEO, ASI) plus the direct game resources modders replace.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ContentCategory
{
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

impl ContentCategory
{
    /// Every category in a stable display order, so the UI can render a fixed
    /// set of tabs/filters without discovering them from the data.
    pub(crate) fn all() -> [ContentCategory; CONTENT_CATEGORY_COUNT]
    {
        return [
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
        ];
    }

    pub(crate) fn label(self) -> &'static str
    {
        return match self
        {
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
        };
    }

    /// One line describing what this subsystem's viewer shows, so each category
    /// reads as its own tailored viewer rather than a generic file list.
    pub(crate) fn description(self) -> &'static str
    {
        return match self
        {
            ContentCategory::ModLoader =>
            {
                "ModLoader sandboxes each mod in its own modloader/<name> folder, so these rarely \
                 collide â€” grouped by mod folder below."
        }
            ContentCategory::Cleo =>
            {
                "CLEO scripts (.cs CLEO5, .cs4/.cs3 compat), plugins (.cleo â†’ cleo_plugins), \
                 modules (â†’ cleo_modules), text (.fxt â†’ cleo_text). Two scripts with the same file \
                 name are the usual conflict."
    }
            ContentCategory::Asi =>
            {
                "ASI plugins load through an ASI loader (e.g. Ultimate ASI Loader). Duplicate .asi \
                 names or clashing loader DLLs are the thing to watch."
    }
            ContentCategory::Img =>
            {
                "Loose DFF/TXD models and IMG archives. Same-named entries: the winning mod's \
                 version is what the game streams."
}
            ContentCategory::Models => "Model files. Later mods overwrite same-named models.",
            ContentCategory::Text =>
            {
                "GXT text tables. Replacing a whole GXT usually wins over partial edits."
            }
            ContentCategory::Anim => "Animation files (.ifp). Same-named packs overwrite.",
            ContentCategory::Audio => "Audio streams and SFX packs.",
            ContentCategory::Data =>
            {
                "Direct data files (handling.cfg, gta.dat, *.ide, *.ipl). The most conflict-prone \
                 category â€” merges are often needed here."
            }
            ContentCategory::Script => "Mission/script files (main.scm).",
            ContentCategory::Other => "Files that did not match a known San Andreas category.",
        };
    }

    /// A compact chip label for the per-mod flags column, where space is tight.
    pub(crate) fn short_label(self) -> &'static str
    {
        return match self
        {
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
            ContentCategory::Other => "â€¢",
        };
    }
}

/// What one mod contributes, distilled from the content index: the categories it
/// touches, whether it overwrites lower-priority mods, and whether any of its
/// files are overwritten by higher-priority ones. This drives the flags column
/// in the load-order list (MO2's content/conflict icons).
#[derive(Clone, Default)]
pub(crate) struct ModContentFlags
{
    pub(crate) categories: BTreeSet<ContentCategory>,
    pub(crate) overwrites_others: bool,
    pub(crate) overwritten: bool,
    pub(crate) file_count: usize,
}

/// Reduce a content index to per-mod flags keyed by mod id. Only mods that
/// appear in the index (enabled and readable) get an entry.
pub(crate) fn per_mod_flags(index: &ContentIndex) -> BTreeMap<String, ModContentFlags>
{
    let mut map: BTreeMap<String, ModContentFlags> = BTreeMap::new();
    for entry in &index.entries
    {

        let last = entry.providers.len().saturating_sub(1);
        for (position, id) in entry.providers.iter().enumerate()
        {
            let flags = map.entry(id.clone()).or_default();
            flags.categories.insert(entry.category);
            flags.file_count += 1;
            let conflict = ProviderConflict::from_positions(ProviderConflictPresence::from_count(entry.providers.len()), position, last);
            mark_provider_conflict(flags, conflict);
        }
    }
    return map;
}

fn mark_provider_conflict(flags: &mut ModContentFlags, conflict: ProviderConflict)
{
    match conflict
    {
        ProviderConflict::None => {}
        ProviderConflict::Winner => flags.overwrites_others = true,
        ProviderConflict::Loser => flags.overwritten = true,
    }
}

#[derive(Clone, Copy)]
struct ContentRootIndexContext<'a>
{
    indexed: &'a IndexedMod,
    root: &'a ModInstallRootJson,
    source_abs: &'a Path,
}

/// Resolve a mod's install roots for a profile the same way a run does: drop
/// roots the profile disables, apply any retarget override, and keep only the
/// enabled ones. Shared with the content viewer so its picture matches reality.
pub(crate) fn effective_install_roots(
    roots: &[ModInstallRootJson],
    overrides: &BTreeMap<String, ProfileRootOverride>,
) -> Vec<ModInstallRootJson>
{
    return roots
        .iter()
        .filter_map(|root| {
            let over = overrides.get(&root.source);
            let enabled = over.and_then(|over| over.enabled).unwrap_or(root.enabled);
            if !enabled
            {
                return None;
            }
            let mut resolved = root.clone();
            if let Some(target) = over.and_then(|over| over.target.clone())
            {
                resolved.target = target;
            }
            Some(resolved)
        })
        .collect();
}
enum ProviderConflictPresence
{
    SingleProvider,
    MultipleProviders,
}

impl ProviderConflictPresence
{
    fn from_count(provider_count: usize) -> Self
    {
        return if provider_count > 1
        {
            Self::MultipleProviders
        }
        else
        {
            Self::SingleProvider
        };
    }
}

enum ProviderConflict
{
    None,
    Winner,
    Loser,
}

impl ProviderConflict
{
    fn from_positions(presence: ProviderConflictPresence, position: usize, last: usize) -> Self
    {
        return match presence
        {
            ProviderConflictPresence::SingleProvider => Self::None,
            ProviderConflictPresence::MultipleProviders if position == last => Self::Winner,
            ProviderConflictPresence::MultipleProviders => Self::Loser,
        };
    }
}

/// Build the content index for a set of enabled mods **already sorted by load
/// order** (lowest priority first). Files are mapped to their materialized
/// target exactly as [`crate::planning::materialize_profile_for_run`] would, so
/// the reported winner matches what a real run produces.
pub(crate) fn build_content_index(mods_in_load_order: &[IndexedMod]) -> ContentIndex
{
    // target path -> (category, providers in load order). BTreeMap keeps the
    // output stable and sorted by target for a predictable viewer.
    let mut by_target: BTreeMap<String, (ContentCategory, Vec<String>)> = BTreeMap::new();
    let mut not_indexed = Vec::new();

    for indexed in mods_in_load_order
    {
        let mut read_any = false;
        let mut read_failed = false;
        for root in &indexed.roots
        {
            match index_content_root(indexed, root, &mut by_target)
            {
                Ok(root_read) => read_any |= root_read,
                Err(()) => read_failed = true,
            }
        }
        if read_failed && !read_any
        {
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
    return ContentIndex {
        entries,
        not_indexed,
    };
}
/// One enabled mod resolved to what it will actually copy: where its files live
/// on disk (`source_root`) and the effective install roots (profile overrides
/// already applied, disabled roots already dropped). Built by the UI from the
/// selected profile, kept here so the index logic is testable in isolation.
pub(crate) struct IndexedMod
{
    pub(crate) id: String,
    pub(crate) source_root: PathBuf,
    pub(crate) roots: Vec<ModInstallRootJson>,
}

/// One materialized target path and every enabled mod that writes it, listed in
/// load order. `providers.last()` is therefore the winner â€” the mod whose copy
/// survives after all others are applied.
#[derive(Clone, Debug)]
pub(crate) struct ContentEntry
{
    pub(crate) target: String,
    pub(crate) category: ContentCategory,
    pub(crate) providers: Vec<String>,
}

impl ContentEntry
{
    /// The mod that wins this file after load order is applied.
    pub(crate) fn winner(&self) -> &str
    {
        return self.providers.last().map(String::as_str).unwrap_or("");
    }

    /// Whether more than one enabled mod writes this exact target.
    pub(crate) fn is_conflict(&self) -> bool
    {
        return self.providers.len() > 1;
    }
}

/// The full picture of what a profile materializes: every target file with its
/// providers, plus any mods whose files could not be read (e.g. an archive that
/// was never extracted to the library).
pub(crate) struct ContentIndex
{
    pub(crate) entries: Vec<ContentEntry>,
    pub(crate) not_indexed: Vec<String>,
}

impl ContentIndex
{
    pub(crate) fn conflict_count(&self) -> usize
    {
        return self.entries
            .iter()
            .filter(|entry| entry.is_conflict())
            .count();
    }

    pub(crate) fn category_count(&self, category: ContentCategory) -> usize
    {
        return self.entries
            .iter()
            .filter(|entry| entry.category == category)
            .count();
    }
}

fn index_content_root(
    indexed: &IndexedMod,
    root: &ModInstallRootJson,
    by_target: &mut BTreeMap<String, (ContentCategory, Vec<String>)>,
) -> Result<bool, ()>
{
    if root.kind.eq_ignore_ascii_case("bootstrap")
    {
        return Ok(false);
    }
    let Ok(relative_source) = path_from_package_root(&root.source) else {
        return Ok(false);
    };
    let source_abs = indexed.source_root.join(&relative_source);
    if !source_abs.exists()
    {
        return Ok(false);
    }
    let files = match collect_files_recursive(&source_abs)
    {
        Ok(files) => files,
        Err(err) =>
        {
            log_warn!("could not scan content root {}: {err}", source_abs.display());
            return Err(());
        }
    };
    let context = ContentRootIndexContext {
        indexed,
        root,
        source_abs: &source_abs,
    };
    for file in files
    {
        index_content_file(context, &file, by_target);
    }
    return Ok(true);
}

fn index_content_file(
    context: ContentRootIndexContext<'_>,
    file: &Path,
    by_target: &mut BTreeMap<String, (ContentCategory, Vec<String>)>,
)
{
    let Some(target) = target_path_for(context.source_abs, file, &context.root.target) else {
        return;
    };
    if is_user_data_target(&target)
    {
        return;
    }
    let category = categorize(ContentCategoryTarget { kind: &context.root.kind, target_lower: &target });
    let entry = by_target
        .entry(target)
        .or_insert_with(|| (category, Vec::new()));
    push_provider(&mut entry.1, &context.indexed.id);
}

/// The materialized target for one source file: `target_root` joined with the
/// file's path relative to the root's source, normalized to forward slashes so
/// two mods writing the same logical path compare equal across platforms.
fn target_path_for(source_abs: &Path, file: &Path, target_root: &str) -> Option<String>
{
    let relative = file.strip_prefix(source_abs).ok()?;
    let relative_str = relative.to_string_lossy().replace('\\', "/");
    let base = normalize_path(target_root);
    let combined = if base.is_empty() || base == "."
    {
        relative_str
    }
    else
    {
        format!("{}/{relative_str}", base.trim_end_matches('/'))
    };
    return Some(combined);
}

/// Whether a materialized target is CLEO runtime user data â€” per-script save
/// files under `cleo_saves/`. These are generated by the game at play time, not
/// authored mod content, so the content index skips them: they are never a
/// conflict and never reported as overwritten.
fn is_user_data_target(target: &str) -> bool
{
    return target
        .split('/')
        .any(|segment| segment.eq_ignore_ascii_case("cleo_saves"));
}

fn categorize(target: ContentCategoryTarget<'_>) -> ContentCategory
{
    let kind = target.kind;
    let target_lower = target.target_lower;
    let lower = target_lower.to_ascii_lowercase();
    // Anything inside a `modloader/<mod>` sandbox is ModLoader content â€” including
    // `.asi`/`.cleo`/`.cs`, which ModLoader's std.asi loads straight from the mod
    // folder â€” so a self-contained mod stays grouped under its sandbox in the
    // viewer instead of scattering into the CLEO/ASI tabs.
    if segment_contains(SegmentNeedle { lower: &lower, needle: "modloader" })
    {
        return ContentCategory::ModLoader;
    }
    return match kind.to_ascii_lowercase().as_str()
    {
        "modloader" => ContentCategory::ModLoader,
        "cleo" | "cleo_text" | "cleo_plugin" | "cleo_plugins" | "cleo_modules" | "cleo_saves" => {
            ContentCategory::Cleo
    }
        "asi" | "plugin" => ContentCategory::Asi,
        _ => categorize_by_path(&lower),
    };
}

/// Sort a materialized file into a viewer bucket. The install root's `kind` wins
/// for loader subsystems (that is the modder's own declaration); direct/managed
/// roots are classified by the target path so `data/`, `models/`, `.gxt`, etc.
/// land in the right resource viewer.
struct ContentCategoryTarget<'a>
{
    kind: &'a str,
    target_lower: &'a str,
}

/// Append `id` as a provider unless it already wrote this target through another
/// root, so a mod that maps two sources onto one file is counted once.
fn push_provider(providers: &mut Vec<String>, id: &str)
{
    if providers.last().map(String::as_str) != Some(id)
    {
        providers.push(id.to_string());
    }
}
