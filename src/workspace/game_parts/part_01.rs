use super::cleo_deps::{analyze_script, load_opcode_db};
use super::cleo_diagnostics::{
    parse_cleo_config, parse_cleo_log, parse_text_keys, plugin_blacklist,
};
use super::game_version::detect_game_version;
use crate::prelude::*;

pub(crate) fn inspect_game(game_root: &Path) -> Result<(), AppError>
{
    println!("game: {}", game_root.display());
    println!("exists: {}", game_root.exists());
    println!();
    print_interrupted_install_notice(game_root)?;
    print_game_infrastructure(game_root);
    return print_game_script_inventory(game_root);
}

fn print_interrupted_install_notice(game_root: &Path) -> Result<(), AppError>
{
    if let Some(lock) = interrupted_install(game_root)?
    {
        println!(
            "WARNING: install {} was interrupted; run `recover` to roll it back",
            lock.txid
        );
        println!();
    }
    return Ok(());
}

fn print_game_infrastructure(game_root: &Path)
{
    let checks = [
        ("classic exe", "gta_sa.exe"),
        ("steam exe", "gta-sa.exe"),
        ("modloader asi", "modloader.asi"),
        ("modloader dir", "modloader"),
        ("cleo asi", "CLEO.asi"),
        ("cleo dir", "CLEO"),
        ("silent asi loader candidate", "vorbisFile.dll"),
    ];

    for (label, rel) in checks
    {
        let path = game_root.join(rel);
        println!(
            "{label:28} {}",
            if path.exists() { "present" } else { "missing" }
        );
    }
    println!("{:28} {}", "cleo runtime", detect_cleo_runtime(game_root));
    print_game_version(game_root);
}

/// Report the executable version CLEO would detect, and warn if it is one CLEO
/// cannot load. CLEO5 supports US 1.0, EU 1.0/1.01 and Steam.
fn print_game_version(game_root: &Path)
{
    let exe = ["gta_sa.exe", "gta-sa.exe"]
        .into_iter()
        .map(|name| game_root.join(name))
        .find(|path| path.exists());
    let Some(exe) = exe else {
        return;
    };
    let version = detect_game_version(&exe);
    println!("{:28} {version}", "game version");
    if !version.is_cleo_supported()
    {
        // The check reads the exe on disk. A modern Steam/retail build genuinely
        // needs a downgrade for CLEO; but a Steam-DRM-packed exe can also read as
        // unrecognized here while still unwrapping to a supported build at
        // runtime â€” so this is a strong hint, not a verdict.
        println!(
            "  NOTE: CLEO's signature check does not match this exe on disk â€” likely the wrong \
             version (CLEO needs 1.0, EU, or the older Steam build), or a packed/Steam exe whose \
             on-disk bytes differ from runtime. CLEO may refuse to load."
        );
    }
}

/// Which CLEO runtime, if any, is installed. CLEO5 renamed the plugin folder
/// (`cleo_modules` â†’ `cleo_plugins`) and added a `.cleo_config.ini`, so the
/// on-disk layout distinguishes the two even though both hijack the same loader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CleoRuntime
{
    Absent,
    /// Installed, but the version could not be determined from the layout.
    Present,
    Cleo4,
    Cleo5,
}

impl fmt::Display for CleoRuntime
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        return match self
        {
            CleoRuntime::Absent => write!(f, "not installed"),
            CleoRuntime::Present => write!(f, "installed (version unknown)"),
            CleoRuntime::Cleo4 => write!(f, "CLEO 4"),
            CleoRuntime::Cleo5 => write!(f, "CLEO 5"),
        };
    }
}

fn print_game_script_inventory(game_root: &Path) -> Result<(), AppError>
{
    let asi_files = list_matching(game_root, |path| has_extension_equal_to(path, "asi"))?;
    let cleo_scripts = collect_cleo_scripts(game_root);
    let cleo_plugins = collect_cleo_plugins(game_root);

    println!();
    print_root_asi_files(asi_files);
    print_cleo_scripts(&cleo_scripts);
    print_cleo_subfolder_scripts(game_root);
    print_cleo_plugins(game_root, &cleo_plugins);
    print_cleo_blacklisted_plugins(game_root, &cleo_plugins);
    print_cleo_modules(game_root);
    print_cleo_saves(game_root);
    print_cleo_bundled_plugin_checklist(game_root, &cleo_plugins);
    print_cleo_content_validation(&cleo_scripts, &cleo_plugins);
    print_text_key_conflicts(game_root);
    print_cleo_script_dependencies(game_root, &cleo_scripts, &cleo_plugins);
    print_cleo_config(game_root);
    print_cleo_log_diagnostics(game_root);

    return Ok(());
}

/// Warn about `.cs`/`.cs4`/`.cs3` scripts sitting in an arbitrary subfolder of
/// `CLEO/`. CLEO scans only the CLEO root non-recursively, so such scripts never
/// load â€” a common packaging mistake. Scripts under the known module/config
/// folders are intentional and excluded.
fn print_cleo_subfolder_scripts(game_root: &Path)
{
    let cleo_dir = game_root.join("CLEO");
    let Ok(all) = collect_files_recursive(&cleo_dir) else {
        return;
    };
    let stray: Vec<PathBuf> = all
        .into_iter()
        .filter(|path| {
            CLEO_SCRIPT_EXTENSIONS
                .iter()
                .any(|ext| has_extension_equal_to(path, ext))
                && is_script_in_stray_subfolder(&cleo_dir, path)
        })
        .collect();
    if stray.is_empty()
    {
        return;
    }
    println!(
        "  WARNING: {} CLEO script(s) are in CLEO subfolders and will NOT load (CLEO scans CLEO/ non-recursively):",
        stray.len()
    );
    for path in stray
    {
        println!("    {}", path.display());
    }
}

/// True when `path` is a `.cs*` file below `cleo_dir` whose first path segment
/// under CLEO is a real (non-module, non-config) subfolder.
fn is_script_in_stray_subfolder(cleo_dir: &Path, path: &Path) -> bool
{
    let Ok(relative) = path.strip_prefix(cleo_dir) else {
        return false;
    };
    let mut segments = relative.components();
    let Some(first) = segments.next() else {
        return false;
    };
    // A file directly in CLEO/ has no further component after the first â€” that is
    // the file name itself, so it is a top-level script (fine).
    if segments.next().is_none()
    {
        return false;
    }
    let first = first.as_os_str().to_string_lossy().to_ascii_lowercase();
    return !CLEO_NON_SCRIPT_SUBDIRS.contains(&first.as_str());
}

/// A GXT text key defined by more than one `.fxt` file (last loaded wins).
#[derive(Clone)]
pub(crate) struct TextKeyConflict
{
    pub(crate) key: String,
    pub(crate) files: Vec<String>,
}

/// A CLEO script with something worth flagging: bundled plugins it needs that are
/// not installed, and/or elevated capabilities it exercises.
#[derive(Clone)]
pub(crate) struct CleoScriptIssue
{
    pub(crate) script: String,
    pub(crate) missing_plugins: Vec<String>,
    pub(crate) capabilities: Vec<String>,
}

/// The CLEO health findings for an installed game folder, computed once (during a
/// content scan) so the UI can render them without touching disk per frame.
#[derive(Clone, Default)]
pub(crate) struct CleoDiagnostics
{
    pub(crate) blacklisted_plugins: Vec<String>,
    pub(crate) fxt_conflicts: Vec<TextKeyConflict>,
    pub(crate) script_issues: Vec<CleoScriptIssue>,
}

impl CleoDiagnostics
{
    pub(crate) fn is_empty(&self) -> bool
    {
        return self.blacklisted_plugins.is_empty()
            && self.fxt_conflicts.is_empty()
            && self.script_issues.is_empty();
    }
}

/// Warn about installed `.cleo` plugins that CLEO5 blacklists â€” the superseded
/// legacy CLEO4 modules, plus any names in the config's `PluginBlacklist`. These
/// are present on disk but will not be loaded.
fn print_cleo_blacklisted_plugins(game_root: &Path, cleo_plugins: &[PathBuf])
{
    let blocked = blacklisted_plugin_paths(game_root, cleo_plugins);
    if blocked.is_empty()
    {
        return;
    }
    println!(
        "  WARNING: {} installed plugin(s) are blacklisted by CLEO5 (legacy, superseded by SA.* â€” will not load):",
        blocked.len()
    );
    for path in blocked
    {
        println!("    {}", path.display());
    }
}

/// Detect GXT text keys defined by more than one `.fxt` file in `cleo_text/`.
/// Because duplicate keys resolve last-loaded-wins, two mods defining the same
/// key is a real (file-name-invisible) conflict.
fn print_text_key_conflicts(game_root: &Path)
{
    let conflicts = collect_text_key_conflicts(game_root);
    if conflicts.is_empty()
    {
        return;
    }
    println!(
        "CLEO text key conflicts: {} key(s) defined in more than one .fxt (last loaded wins):",
        conflicts.len()
    );
    for conflict in &conflicts
    {
        println!("  {}: {}", conflict.key, conflict.files.join(", "));
    }
}

/// Surface the mod-management-relevant settings from `CLEO/.cleo_config.ini`.
fn print_cleo_config(game_root: &Path)
{
    let path = game_root.join("CLEO").join(".cleo_config.ini");
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    let config = parse_cleo_config(&text);
    println!("CLEO config ({} keys):", config.key_count);
    if config.highlights.is_empty()
    {
        println!("  (no notable settings)");
        return;
    }
    for highlight in &config.highlights
    {
        println!(
            "  {} = {}  ({})",
            highlight.key, highlight.value, highlight.note
        );
    }
}

pub(crate) fn detect_cleo_runtime(game_root: &Path) -> CleoRuntime
{
    let cleo_dir = game_root.join("CLEO");
    let installed = cleo_dir.is_dir()
        || game_root.join("CLEO.asi").exists()
        || game_root.join("cleo.asi").exists();
    if !installed
    {
        return CleoRuntime::Absent;
    }
    // CLEO5-only markers: the config file, the renamed plugin folder, the opcode_bytes DB.
    let has_cleo5_config = cleo_dir.join(".cleo_config.ini").exists();
    let has_cleo5_plugin_folder = cleo_dir.join("cleo_plugins").is_dir();
    let has_cleo5_opcode_db = cleo_dir.join(".config").join("sa.json").exists();
    if has_cleo5_config || has_cleo5_plugin_folder || has_cleo5_opcode_db
    {
        return CleoRuntime::Cleo5;
    }
    // With none of the CLEO5 markers present, an install that still has a
    // cleo_modules folder (or a bare CLEO.asi) is treated as CLEO4-era.
    if cleo_dir.join("cleo_modules").is_dir()
    {
        return CleoRuntime::Cleo4;
    }
    return CleoRuntime::Present;
}

/// Gather the CLEO diagnostics for an installed game folder: blacklisted plugins,
/// FXT text-key conflicts, and per-script missing-plugin / capability issues.
pub(crate) fn collect_cleo_diagnostics(game_root: &Path) -> CleoDiagnostics
{
    let plugins = collect_cleo_plugins(game_root);
    let blacklisted_plugins = blacklisted_plugin_paths(game_root, &plugins)
        .iter()
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()).map(String::from))
        .collect();
    return CleoDiagnostics {
        blacklisted_plugins,
        fxt_conflicts: collect_text_key_conflicts(game_root),
        script_issues: collect_script_issues(game_root),
    };
}

/// Per-script issues (missing bundled plugins, elevated capabilities), keyed off
/// the installed `sa.json` opcode_bytes DB. Empty when not CLEO5 or the DB is absent.
fn collect_script_issues(game_root: &Path) -> Vec<CleoScriptIssue>
{
    if detect_cleo_runtime(game_root) != CleoRuntime::Cleo5
    {
        return Vec::new();
    }
    let sa_json = game_root.join("CLEO").join(".config").join("sa.json");
    let Some(db) = load_opcode_db(&sa_json) else {
        return Vec::new();
    };
    let installed: BTreeSet<String> = collect_cleo_plugins(game_root)
        .iter()
        .map(|path| plugin_stem_lower(path))
        .collect();
    let mut issues = Vec::new();
    for script in collect_cleo_scripts(game_root)
    {
        let Ok(bytes) = fs::read(&script) else {
            continue;
        };
        let deps = analyze_script(&bytes, &db);
        let missing: Vec<String> = deps
            .plugins
            .iter()
            .filter(|plugin| !installed.contains(&plugin.to_ascii_lowercase()))
            .cloned()
            .collect();
        let capabilities: Vec<String> = deps
            .capabilities
            .iter()
            .map(|cap| cap.to_string())
            .collect();
        if missing.is_empty() && capabilities.is_empty()
        {
            continue;
        }
        let name = script
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<script>")
            .to_string();
        issues.push(CleoScriptIssue {
            script: name,
            missing_plugins: missing,
            capabilities,
        });
    }
    return issues;
}

/// The folders directly under `CLEO/` that legitimately hold `.cs*` files which
/// are not top-level scripts (modules, or the CLEO config folder).
const CLEO_NON_SCRIPT_SUBDIRS: [&str; 5] = [
    "cleo_modules",
    "cleo_plugins",
    "cleo_text",
    "cleo_saves",
    ".config",
];

/// Read `CLEO/.cleo_config.ini` text, if present â€” shared by the blacklist check
/// and the config viewer.
fn read_cleo_config_text(game_root: &Path) -> Option<String>
{
    return fs::read_to_string(game_root.join("CLEO").join(".cleo_config.ini")).ok();
}

/// The installed `.cleo` plugins CLEO5 blacklists â€” superseded legacy CLEO4
/// modules plus any names in the config's `PluginBlacklist`.
fn blacklisted_plugin_paths(game_root: &Path, cleo_plugins: &[PathBuf]) -> Vec<PathBuf>
{
    let config = read_cleo_config_text(game_root);
    let blacklist = plugin_blacklist(config.as_deref());
    return cleo_plugins
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| blacklist.contains(&name.to_ascii_lowercase()))
                .unwrap_or(false)
        })
        .cloned()
        .collect();
}

/// GXT keys defined by more than one `.fxt` file in `cleo_text/`.
fn collect_text_key_conflicts(game_root: &Path) -> Vec<TextKeyConflict>
{
    let text_dir = game_root.join("CLEO").join("cleo_text");
    let files = list_matching(&text_dir, |path| has_extension_equal_to(path, "fxt")).unwrap_or_default();
    let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in &files
    {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<fxt>")
            .to_string();
        for key in parse_text_keys(&text)
        {
            let providers = by_key.entry(key).or_default();
            if providers.last() != Some(&name)
            {
                providers.push(name.clone());
            }
        }
    }
    return by_key
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(key, files)| TextKeyConflict { key, files })
        .collect();
}
