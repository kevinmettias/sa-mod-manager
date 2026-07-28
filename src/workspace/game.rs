use super::cleo_deps::{analyze_script, load_opcode_db};
use super::cleo_diagnostics::{
    parse_cleo_config, parse_cleo_log, parse_fxt_keys, plugin_blacklist,
};
use super::game_version::detect_game_version;
use crate::prelude::*;

pub(crate) fn inspect_game(game_root: &Path) -> Result<(), AppError> {
    println!("game: {}", game_root.display());
    println!("exists: {}", game_root.exists());
    println!();
    print_interrupted_install_notice(game_root)?;
    print_game_infrastructure(game_root);
    print_game_script_inventory(game_root)
}

fn print_interrupted_install_notice(game_root: &Path) -> Result<(), AppError> {
    if let Some(lock) = interrupted_install(game_root)? {
        println!(
            "WARNING: install {} was interrupted; run `recover` to roll it back",
            lock.txid
        );
        println!();
    }
    Ok(())
}

fn print_game_infrastructure(game_root: &Path) {
    let checks = [
        ("classic exe", "gta_sa.exe"),
        ("steam exe", "gta-sa.exe"),
        ("modloader asi", "modloader.asi"),
        ("modloader dir", "modloader"),
        ("cleo asi", "CLEO.asi"),
        ("cleo dir", "CLEO"),
        ("silent asi loader candidate", "vorbisFile.dll"),
    ];

    for (label, rel) in checks {
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
fn print_game_version(game_root: &Path) {
    let exe = ["gta_sa.exe", "gta-sa.exe"]
        .into_iter()
        .map(|name| game_root.join(name))
        .find(|path| path.exists());
    let Some(exe) = exe else {
        return;
    };
    let version = detect_game_version(&exe);
    println!("{:28} {version}", "game version");
    if !version.is_cleo_supported() {
        // The check reads the exe on disk. A modern Steam/retail build genuinely
        // needs a downgrade for CLEO; but a Steam-DRM-packed exe can also read as
        // unrecognized here while still unwrapping to a supported build at
        // runtime — so this is a strong hint, not a verdict.
        println!(
            "  NOTE: CLEO's signature check does not match this exe on disk — likely the wrong \
             version (CLEO needs 1.0, EU, or the older Steam build), or a packed/Steam exe whose \
             on-disk bytes differ from runtime. CLEO may refuse to load."
        );
    }
}

/// Which CLEO runtime, if any, is installed. CLEO5 renamed the plugin folder
/// (`cleo_modules` → `cleo_plugins`) and added a `.cleo_config.ini`, so the
/// on-disk layout distinguishes the two even though both hijack the same loader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CleoRuntime {
    Absent,
    /// Installed, but the version could not be determined from the layout.
    Present,
    Cleo4,
    Cleo5,
}

impl fmt::Display for CleoRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CleoRuntime::Absent => write!(f, "not installed"),
            CleoRuntime::Present => write!(f, "installed (version unknown)"),
            CleoRuntime::Cleo4 => write!(f, "CLEO 4"),
            CleoRuntime::Cleo5 => write!(f, "CLEO 5"),
        }
    }
}

pub(crate) fn detect_cleo_runtime(game_root: &Path) -> CleoRuntime {
    let cleo_dir = game_root.join("CLEO");
    let installed = cleo_dir.is_dir()
        || game_root.join("CLEO.asi").exists()
        || game_root.join("cleo.asi").exists();
    if !installed {
        return CleoRuntime::Absent;
    }
    // CLEO5-only markers: the config file, the renamed plugin folder, the opcode DB.
    if cleo_dir.join(".cleo_config.ini").exists()
        || cleo_dir.join("cleo_plugins").is_dir()
        || cleo_dir.join(".config").join("sa.json").exists()
    {
        return CleoRuntime::Cleo5;
    }
    // With none of the CLEO5 markers present, an install that still has a
    // cleo_modules folder (or a bare CLEO.asi) is treated as CLEO4-era.
    if cleo_dir.join("cleo_modules").is_dir() {
        return CleoRuntime::Cleo4;
    }
    CleoRuntime::Present
}

fn print_game_script_inventory(game_root: &Path) -> Result<(), AppError> {
    let asi_files = list_matching(game_root, |path| extension_eq(path, "asi"))?;
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
    print_fxt_key_conflicts(game_root);
    print_cleo_script_dependencies(game_root, &cleo_scripts, &cleo_plugins);
    print_cleo_config(game_root);
    print_cleo_log_diagnostics(game_root);

    Ok(())
}

/// Read `CLEO/.cleo_config.ini` text, if present — shared by the blacklist check
/// and the config viewer.
fn read_cleo_config_text(game_root: &Path) -> Option<String> {
    fs::read_to_string(game_root.join("CLEO").join(".cleo_config.ini")).ok()
}

/// A GXT text key defined by more than one `.fxt` file (last loaded wins).
#[derive(Clone)]
pub(crate) struct FxtKeyConflict {
    pub(crate) key: String,
    pub(crate) files: Vec<String>,
}

/// A CLEO script with something worth flagging: bundled plugins it needs that are
/// not installed, and/or elevated capabilities it exercises.
#[derive(Clone)]
pub(crate) struct CleoScriptIssue {
    pub(crate) script: String,
    pub(crate) missing_plugins: Vec<String>,
    pub(crate) capabilities: Vec<String>,
}

/// The CLEO health findings for an installed game folder, computed once (during a
/// content scan) so the UI can render them without touching disk per frame.
#[derive(Clone, Default)]
pub(crate) struct CleoDiagnostics {
    pub(crate) blacklisted_plugins: Vec<String>,
    pub(crate) fxt_conflicts: Vec<FxtKeyConflict>,
    pub(crate) script_issues: Vec<CleoScriptIssue>,
}

impl CleoDiagnostics {
    pub(crate) fn is_empty(&self) -> bool {
        self.blacklisted_plugins.is_empty()
            && self.fxt_conflicts.is_empty()
            && self.script_issues.is_empty()
    }
}

/// Gather the CLEO diagnostics for an installed game folder: blacklisted plugins,
/// FXT text-key conflicts, and per-script missing-plugin / capability issues.
pub(crate) fn collect_cleo_diagnostics(game_root: &Path) -> CleoDiagnostics {
    let plugins = collect_cleo_plugins(game_root);
    let blacklisted_plugins = blacklisted_plugin_paths(game_root, &plugins)
        .iter()
        .filter_map(|path| path.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    CleoDiagnostics {
        blacklisted_plugins,
        fxt_conflicts: collect_fxt_conflicts(game_root),
        script_issues: collect_script_issues(game_root),
    }
}

/// The installed `.cleo` plugins CLEO5 blacklists — superseded legacy CLEO4
/// modules plus any names in the config's `PluginBlacklist`.
fn blacklisted_plugin_paths(game_root: &Path, cleo_plugins: &[PathBuf]) -> Vec<PathBuf> {
    let config = read_cleo_config_text(game_root);
    let blacklist = plugin_blacklist(config.as_deref());
    cleo_plugins
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| blacklist.contains(&name.to_ascii_lowercase()))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// GXT keys defined by more than one `.fxt` file in `cleo_text/`.
fn collect_fxt_conflicts(game_root: &Path) -> Vec<FxtKeyConflict> {
    let text_dir = game_root.join("CLEO").join("cleo_text");
    let files = list_matching(&text_dir, |path| extension_eq(path, "fxt")).unwrap_or_default();
    let mut by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<fxt>")
            .to_string();
        for key in parse_fxt_keys(&text) {
            let providers = by_key.entry(key).or_default();
            if providers.last() != Some(&name) {
                providers.push(name.clone());
            }
        }
    }
    by_key
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(key, files)| FxtKeyConflict { key, files })
        .collect()
}

/// Per-script issues (missing bundled plugins, elevated capabilities), keyed off
/// the installed `sa.json` opcode DB. Empty when not CLEO5 or the DB is absent.
fn collect_script_issues(game_root: &Path) -> Vec<CleoScriptIssue> {
    if detect_cleo_runtime(game_root) != CleoRuntime::Cleo5 {
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
    for script in collect_cleo_scripts(game_root) {
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
        if missing.is_empty() && capabilities.is_empty() {
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
    issues
}

/// Warn about installed `.cleo` plugins that CLEO5 blacklists — the superseded
/// legacy CLEO4 modules, plus any names in the config's `PluginBlacklist`. These
/// are present on disk but will not be loaded.
fn print_cleo_blacklisted_plugins(game_root: &Path, cleo_plugins: &[PathBuf]) {
    let blocked = blacklisted_plugin_paths(game_root, cleo_plugins);
    if blocked.is_empty() {
        return;
    }
    println!(
        "  WARNING: {} installed plugin(s) are blacklisted by CLEO5 (legacy, superseded by SA.* — will not load):",
        blocked.len()
    );
    for path in blocked {
        println!("    {}", path.display());
    }
}

/// Detect GXT text keys defined by more than one `.fxt` file in `cleo_text/`.
/// Because duplicate keys resolve last-loaded-wins, two mods defining the same
/// key is a real (file-name-invisible) conflict.
fn print_fxt_key_conflicts(game_root: &Path) {
    let conflicts = collect_fxt_conflicts(game_root);
    if conflicts.is_empty() {
        return;
    }
    println!(
        "CLEO text key conflicts: {} key(s) defined in more than one .fxt (last loaded wins):",
        conflicts.len()
    );
    for conflict in &conflicts {
        println!("  {}: {}", conflict.key, conflict.files.join(", "));
    }
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

/// Warn about `.cs`/`.cs4`/`.cs3` scripts sitting in an arbitrary subfolder of
/// `CLEO/`. CLEO scans only the CLEO root non-recursively, so such scripts never
/// load — a common packaging mistake. Scripts under the known module/config
/// folders are intentional and excluded.
fn print_cleo_subfolder_scripts(game_root: &Path) {
    let cleo_dir = game_root.join("CLEO");
    let Ok(all) = collect_files_recursive(&cleo_dir) else {
        return;
    };
    let stray: Vec<PathBuf> = all
        .into_iter()
        .filter(|path| {
            CLEO_SCRIPT_EXTENSIONS
                .iter()
                .any(|ext| extension_eq(path, ext))
                && script_is_in_stray_subfolder(&cleo_dir, path)
        })
        .collect();
    if stray.is_empty() {
        return;
    }
    println!(
        "  WARNING: {} CLEO script(s) are in CLEO subfolders and will NOT load (CLEO scans CLEO/ non-recursively):",
        stray.len()
    );
    for path in stray {
        println!("    {}", path.display());
    }
}

/// True when `path` is a `.cs*` file below `cleo_dir` whose first path segment
/// under CLEO is a real (non-module, non-config) subfolder.
fn script_is_in_stray_subfolder(cleo_dir: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(cleo_dir) else {
        return false;
    };
    let mut segments = relative.components();
    let Some(first) = segments.next() else {
        return false;
    };
    // A file directly in CLEO/ has no further component after the first — that is
    // the file name itself, so it is a top-level script (fine).
    if segments.next().is_none() {
        return false;
    }
    let first = first.as_os_str().to_string_lossy().to_ascii_lowercase();
    !CLEO_NON_SCRIPT_SUBDIRS.contains(&first.as_str())
}

/// Surface the mod-management-relevant settings from `CLEO/.cleo_config.ini`.
fn print_cleo_config(game_root: &Path) {
    let path = game_root.join("CLEO").join(".cleo_config.ini");
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    let config = parse_cleo_config(&text);
    println!("CLEO config ({} keys):", config.key_count);
    if config.highlights.is_empty() {
        println!("  (no notable settings)");
        return;
    }
    for highlight in &config.highlights {
        println!(
            "  {} = {}  ({})",
            highlight.key, highlight.value, highlight.note
        );
    }
}

/// Surface script load failures and errors CLEO recorded in `cleo.log`.
fn print_cleo_log_diagnostics(game_root: &Path) {
    let path = game_root.join("cleo.log");
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    let summary = parse_cleo_log(&text);
    if summary.is_clean() {
        println!(
            "cleo.log        : {} lines, no script load failures",
            summary.total_lines
        );
        return;
    }
    println!("cleo.log        : {} lines", summary.total_lines);
    if !summary.failed_scripts.is_empty() {
        println!(
            "  {} script(s) FAILED to load:",
            summary.failed_scripts.len()
        );
        for name in &summary.failed_scripts {
            println!("    {name}");
        }
    }
    if !summary.errors.is_empty() {
        let shown = summary.errors.len().min(10); // literal: allow external format or runtime boundary value means itself here
        println!(
            "  {} error line(s) (showing {shown}):",
            summary.errors.len()
        );
        for line in summary.errors.iter().take(shown) {
            println!("    {line}");
        }
    }
}

/// CLEO scripts live directly in `CLEO/`: `.cs` (CLEO5) and the `.cs4`/`.cs3`
/// compatibility-mode scripts, per the shared extension list.
fn collect_cleo_scripts(game_root: &Path) -> Vec<PathBuf> {
    list_matching(&game_root.join("CLEO"), |path| {
        CLEO_SCRIPT_EXTENSIONS
            .iter()
            .any(|ext| extension_eq(path, ext))
    })
    .unwrap_or_default()
}

/// CLEO5 plugin modules (`.cleo`) load from `CLEO/cleo_plugins/`. Some also sit
/// loose in `CLEO/` on older setups, so scan both.
fn collect_cleo_plugins(game_root: &Path) -> Vec<PathBuf> {
    let cleo_dir = game_root.join("CLEO");
    let mut plugins = list_matching(&cleo_dir.join("cleo_plugins"), |path| {
        extension_eq(path, "cleo")
    })
    .unwrap_or_default();
    let loose = list_matching(&cleo_dir, |path| extension_eq(path, "cleo")).unwrap_or_default();
    plugins.extend(loose);
    plugins
}

fn print_root_asi_files(asi_files: Vec<PathBuf>) {
    println!("root ASI files : {}", asi_files.len());
    for path in asi_files {
        println!("  {}", path.display());
    }
}

/// The CLEO version a script's extension targets: `.cs` runs as CLEO5,
/// `.cs4`/`.cs3` run in CLEO4/CLEO3 compatibility mode.
fn cleo_script_version(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("cs") => "CLEO5",
        Some("cs4") => "CLEO4 compat",
        Some("cs3") => "CLEO3 compat",
        _ => "CLEO",
    }
}

fn print_cleo_scripts(cleo_scripts: &[PathBuf]) {
    println!("CLEO scripts   : {}", cleo_scripts.len());
    for path in cleo_scripts {
        println!("  {} [{}]", path.display(), cleo_script_version(path));
    }
    let compat = cleo_scripts
        .iter()
        .filter(|path| cleo_script_version(path).contains("compat"))
        .count();
    if compat > 0 {
        println!(
            "  NOTE: {compat} script(s) run in CLEO3/CLEO4 compatibility mode (.cs4/.cs3); they need a CLEO build that still supports that mode"
        );
    }
}

/// Inventory of `CLEO/cleo_modules/` — shared script modules (`.s`) loaded via
/// the `modules:` path prefix. Only printed when the folder exists. `.s` modules
/// are validated against their header magic.
fn print_cleo_modules(game_root: &Path) {
    let dir = game_root.join("CLEO").join("cleo_modules");
    if !dir.is_dir() {
        return;
    }
    let files = list_matching(&dir, |_| true).unwrap_or_default();
    println!("CLEO modules   : {}", files.len());
    for path in &files {
        let tag = if extension_eq(path, "s") && !is_cleo_module(path) {
            "  [invalid module header]"
        } else {
            ""
        };
        println!("  {}{tag}", path.display());
    }
}

/// A compiled CLEO module (`.s`) begins with the CLEO segment-header magic
/// `FF 7F FE 00 00` (from `CModuleSystem`). Used to flag mislabeled/corrupt
/// modules, the way [`is_pe_image`] validates plugins.
fn is_cleo_module(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 5]; // literal: allow external format or runtime boundary value means itself here
    file.read_exact(&mut magic).is_ok() && magic == [0xFF, 0x7F, 0xFE, 0x00, 0x00] // literal: allow external format or runtime boundary value means itself here
}

/// Inventory of `CLEO/cleo_saves/` — runtime-generated per-script save data, not
/// mod content. Only printed when the folder exists.
fn print_cleo_saves(game_root: &Path) {
    let dir = game_root.join("CLEO").join("cleo_saves");
    if !dir.is_dir() {
        return;
    }
    let files = list_matching(&dir, |_| true).unwrap_or_default();
    println!(
        "CLEO saves     : {} (runtime user data, not mod content)",
        files.len()
    );
}

fn print_cleo_plugins(game_root: &Path, cleo_plugins: &[PathBuf]) {
    println!("CLEO plugins   : {}", cleo_plugins.len());
    let cleo_plugins_dir = game_root.join("CLEO").join("cleo_plugins");
    for path in cleo_plugins {
        println!("  {}", path.display());
    }
    // CLEO5 still loads .cleo modules from the CLEO/ root as a *legacy* location,
    // but plugins in cleo_plugins/ load last and win, so cleo_plugins/ is
    // preferred. Point out loose modules as a tidy-up suggestion, not an error.
    let loose = cleo_plugins
        .iter()
        .filter(|path| path.parent() != Some(cleo_plugins_dir.as_path()))
        .count();
    if loose > 0 && detect_cleo_runtime(game_root) == CleoRuntime::Cleo5 {
        println!(
            "  NOTE: {loose} .cleo plugin(s) sit in CLEO/ (legacy location); CLEO5 prefers CLEO/cleo_plugins/"
        );
    }
}

/// The plugin modules bundled with CLEO5. Scripts frequently depend on one of
/// these (e.g. a script calling INI opcodes needs `SA.IniFiles`), so showing
/// which are installed is the practical stand-in for a per-script dependency
/// graph — the compiled bytecode references plugin opcodes by number, not name,
/// so a true graph would need a disassembler plus the `sa.json` opcode table.
const BUNDLED_CLEO5_PLUGINS: [&str; 9] = [
    "SA.Audio",
    "SA.DebugUtils",
    "SA.FileSystemOperations",
    "SA.GameEntities",
    "SA.IniFiles",
    "SA.Input",
    "SA.Math",
    "SA.MemoryOperations",
    "SA.Text",
];

/// For each bundled CLEO5 plugin, whether a matching module is installed. Keyed
/// on the lowercased file stem (`SA.Audio.cleo` → `sa.audio`).
fn bundled_plugin_status(installed_stems: &BTreeSet<String>) -> Vec<(&'static str, bool)> {
    BUNDLED_CLEO5_PLUGINS
        .iter()
        .map(|name| (*name, installed_stems.contains(&name.to_ascii_lowercase())))
        .collect()
}

fn print_cleo_bundled_plugin_checklist(game_root: &Path, cleo_plugins: &[PathBuf]) {
    // The bundled set is a CLEO5 concept; the checklist is noise on CLEO4/none.
    if detect_cleo_runtime(game_root) != CleoRuntime::Cleo5 {
        return;
    }
    let installed: BTreeSet<String> = cleo_plugins
        .iter()
        .map(|path| plugin_stem_lower(path))
        .collect();
    println!("CLEO5 bundled plugins:");
    for (name, present) in bundled_plugin_status(&installed) {
        println!("  [{}] {name}", if present { "x" } else { " " });
    }
}

/// Lowercased file stem of a plugin path (`.../SA.Audio.cleo` → `sa.audio`).
fn plugin_stem_lower(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Warn about files whose bytes contradict their extension: `.cleo` plugins and
/// `.asi` plugins must be DLLs (PE image, `MZ` magic); compiled `.cs` scripts are
/// SCM bytecode and must not be. A `.cs` that is really a DLL, or a `.cleo` that
/// is empty/corrupt, will silently fail to load — this surfaces it up front.
fn print_cleo_content_validation(cleo_scripts: &[PathBuf], cleo_plugins: &[PathBuf]) {
    let mut problems = Vec::new();
    for script in cleo_scripts {
        if is_pe_image(script) {
            problems.push(format!(
                "{} looks like a DLL, not a compiled CLEO script (a misnamed .cleo/.asi?)",
                script.display()
            ));
        }
    }
    for plugin in cleo_plugins {
        if !is_pe_image(plugin) {
            problems.push(format!(
                "{} is not a valid plugin DLL (empty or corrupt .cleo)",
                plugin.display()
            ));
        }
    }
    if problems.is_empty() {
        return;
    }
    println!("CLEO content check:");
    for problem in problems {
        println!("  WARNING: {problem}");
    }
}

/// Per-script plugin dependency analysis: disassemble each CLEO script and, using
/// the game's `sa.json` opcode database, report which bundled plugins it needs and
/// which of those are not installed. Only meaningful on CLEO5 (that is where
/// `sa.json` lives); a script whose bytecode cannot be fully walked is flagged as
/// partial rather than reported as having no further dependencies.
fn print_cleo_script_dependencies(game_root: &Path, scripts: &[PathBuf], plugins: &[PathBuf]) {
    if detect_cleo_runtime(game_root) != CleoRuntime::Cleo5 || scripts.is_empty() {
        return;
    }
    let sa_json = game_root.join("CLEO").join(".config").join("sa.json");
    let Some(db) = load_opcode_db(&sa_json) else {
        println!(
            "CLEO script dependencies: skipped (opcode database CLEO/.config/sa.json not found or unreadable)"
        );
        return;
    };

    let installed: BTreeSet<String> = plugins.iter().map(|path| plugin_stem_lower(path)).collect();
    let mut reported = false;
    for script in scripts {
        let Ok(bytes) = fs::read(script) else {
            continue;
        };
        let deps = analyze_script(&bytes, &db);
        let missing: Vec<&String> = deps
            .plugins
            .iter()
            .filter(|plugin| !installed.contains(&plugin.to_ascii_lowercase()))
            .collect();
        // Only surface scripts that need something, exercise an elevated
        // capability, or that we could not fully read.
        if deps.plugins.is_empty()
            && deps.unmapped_extensions.is_empty()
            && deps.capabilities.is_empty()
            && deps.complete
        {
            continue;
        }
        if !reported {
            println!("CLEO script dependencies (via CLEO/.config/sa.json):");
            reported = true;
        }
        print_one_script_dependencies(script, &deps, &missing);
    }
    if !reported {
        println!("CLEO script dependencies: none detected");
    }
}

fn print_one_script_dependencies(
    script: &Path,
    deps: &super::cleo_deps::ScriptDeps,
    missing: &[&String],
) {
    let name = script
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<script>");
    let mut parts = Vec::new();
    if !deps.plugins.is_empty() {
        parts.push(format!(
            "needs {}",
            deps.plugins.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    if !missing.is_empty() {
        let names: Vec<&str> = missing.iter().map(|plugin| plugin.as_str()).collect();
        parts.push(format!("MISSING {}", names.join(", ")));
    }
    if !deps.unmapped_extensions.is_empty() {
        parts.push(format!(
            "uses {} (no bundled file mapping)",
            deps.unmapped_extensions
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !deps.capabilities.is_empty() {
        parts.push(format!(
            "elevated: {}",
            deps.capabilities
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !deps.complete {
        parts.push("partial: undecodable bytecode, dependencies may be incomplete".to_string());
    }
    println!("  {name}: {}", parts.join("; "));
}

/// Whether a file begins with the PE/DLL magic `MZ`. CLEO plugins (`.cleo`) and
/// ASI plugins are DLLs; compiled CLEO scripts (`.cs`/`.cs4`/`.cs3`) are not.
fn is_pe_image(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2]; // literal: allow external format or runtime boundary value means itself here
    file.read_exact(&mut magic).is_ok() && &magic == b"MZ"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "sa-mod-manager-cleo-{tag}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cleo_runtime_detects_version_from_layout() {
        let none = temp_dir("runtime-none");
        assert_eq!(detect_cleo_runtime(&none), CleoRuntime::Absent);

        let four = temp_dir("runtime-4");
        fs::create_dir_all(four.join("CLEO").join("cleo_modules")).unwrap();
        assert_eq!(detect_cleo_runtime(&four), CleoRuntime::Cleo4);

        let five = temp_dir("runtime-5");
        fs::create_dir_all(five.join("CLEO")).unwrap();
        fs::write(five.join("CLEO").join(".cleo_config.ini"), "").unwrap();
        assert_eq!(detect_cleo_runtime(&five), CleoRuntime::Cleo5);

        fs::remove_dir_all(&none).unwrap();
        fs::remove_dir_all(&four).unwrap();
        fs::remove_dir_all(&five).unwrap();
    }

    #[test]
    fn cleo_script_version_maps_extension_to_runtime() {
        assert_eq!(cleo_script_version(Path::new("a/speedo.cs")), "CLEO5");
        assert_eq!(
            cleo_script_version(Path::new("a/legacy.cs4")),
            "CLEO4 compat"
        );
        assert_eq!(
            cleo_script_version(Path::new("a/older.cs3")),
            "CLEO3 compat"
        );
    }

    #[test]
    fn bundled_plugin_status_marks_installed_and_missing() {
        let installed = BTreeSet::from(["sa.inifiles".to_string(), "sa.audio".to_string()]);
        let status = bundled_plugin_status(&installed);
        let present: BTreeSet<&str> = status
            .iter()
            .filter(|(_, ok)| *ok)
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(present, BTreeSet::from(["SA.Audio", "SA.IniFiles"]));
        // Every bundled plugin is accounted for, present or not.
        assert_eq!(status.len(), BUNDLED_CLEO5_PLUGINS.len());
        assert!(status.iter().any(|(name, ok)| *name == "SA.Text" && !ok));
    }

    #[test]
    fn collect_cleo_diagnostics_finds_blacklist_and_fxt_conflicts() {
        let base = temp_dir("diag");
        let game = base.join("game");
        let cleo = game.join("CLEO");
        fs::create_dir_all(cleo.join("cleo_plugins")).unwrap();
        fs::create_dir_all(cleo.join("cleo_text")).unwrap();
        fs::write(cleo.join(".cleo_config.ini"), "").unwrap(); // marks CLEO5
        // A blacklisted legacy plugin, and two .fxt defining the same key.
        fs::write(cleo.join("cleo_plugins").join("IniFiles.cleo"), b"MZ..").unwrap();
        fs::write(cleo.join("cleo_text").join("a.fxt"), "HELLO hi\nONLY_A x\n").unwrap();
        fs::write(cleo.join("cleo_text").join("b.fxt"), "HELLO bye\n").unwrap();

        let diag = collect_cleo_diagnostics(&game);
        assert_eq!(diag.blacklisted_plugins, vec!["IniFiles.cleo"]);
        assert_eq!(diag.fxt_conflicts.len(), 1, "only HELLO is shared");
        assert_eq!(diag.fxt_conflicts[0].key, "HELLO");
        assert_eq!(diag.fxt_conflicts[0].files, vec!["a.fxt", "b.fxt"]);
        assert!(!diag.is_empty());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn is_cleo_module_checks_header_magic() {
        let dir = temp_dir("module");
        let good = dir.join("lib.s");
        fs::write(&good, [0xFF, 0x7F, 0xFE, 0x00, 0x00, 0x01, 0x02]).unwrap(); // literal: allow test fixture value is the specimen under judgment
        let bad = dir.join("bad.s");
        fs::write(&bad, b"not a module").unwrap();
        assert!(is_cleo_module(&good));
        assert!(!is_cleo_module(&bad));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn is_pe_image_distinguishes_dll_from_bytecode() {
        let dir = temp_dir("pe");
        let dll = dir.join("plugin.cleo");
        // A PE image starts with the "MZ" DOS header magic.
        fs::write(&dll, b"MZ\x90\x00rest of a dll").unwrap();
        let script = dir.join("script.cs");
        // A compiled CLEO script is SCM bytecode, here a goto opcode 0002.
        fs::write(&script, b"\x02\x00\x01\x00\x00\x00\x00").unwrap();
        let empty = dir.join("empty.cleo");
        fs::write(&empty, b"").unwrap();

        assert!(is_pe_image(&dll));
        assert!(!is_pe_image(&script));
        assert!(!is_pe_image(&empty));
        assert!(!is_pe_image(&dir.join("missing.cleo")));

        fs::remove_dir_all(&dir).unwrap();
    }
}
