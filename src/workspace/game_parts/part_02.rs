
/// Surface script load failures and errors CLEO recorded in `cleo.log`.
fn print_cleo_log_diagnostics(game_root: &Path)
{
    let path = game_root.join("cleo.log");
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    let summary = parse_cleo_log(&text);
    if summary.is_clean()
    {
        println!(
            "cleo.log        : {} lines, no script load failures",
            summary.total_lines
        );
        return;
    }
    println!("cleo.log        : {} lines", summary.total_lines);
    if !summary.failed_scripts.is_empty()
    {
        println!(
            "  {} script(s) FAILED to load:",
            summary.failed_scripts.len()
        );
        for name in &summary.failed_scripts
        {
            println!("    {name}");
        }
    }
    if !summary.errors.is_empty()
    {
        let shown = summary.errors.len().min(10); // literal: allow external format or runtime boundary value means itself here
        println!(
            "  {} error line(s) (showing {shown}):",
            summary.errors.len()
        );
        for line in summary.errors.iter().take(shown)
        {
            println!("    {line}");
        }
    }
}

/// CLEO scripts live directly in `CLEO/`: `.cs` (CLEO5) and the `.cs4`/`.cs3`
/// compatibility-mode scripts, per the shared extension list.
fn collect_cleo_scripts(game_root: &Path) -> Vec<PathBuf>
{
    return list_matching(&game_root.join("CLEO"), |path| {
        CLEO_SCRIPT_EXTENSIONS
            .iter()
            .any(|ext| has_extension_equal_to(path, ext))
    })
    .unwrap_or_default();
}

/// CLEO5 plugin modules (`.cleo`) load from `CLEO/cleo_plugins/`. Some also sit
/// loose in `CLEO/` on older setups, so scan both.
fn collect_cleo_plugins(game_root: &Path) -> Vec<PathBuf>
{
    let cleo_dir = game_root.join("CLEO");
    let mut plugins = list_matching(&cleo_dir.join("cleo_plugins"), |path| {
        has_extension_equal_to(path, "cleo")
    })
    .unwrap_or_default();
    let loose = list_matching(&cleo_dir, |path| has_extension_equal_to(path, "cleo")).unwrap_or_default();
    plugins.extend(loose);
    return plugins;
}

fn print_root_asi_files(asi_files: Vec<PathBuf>)
{
    println!("root ASI files : {}", asi_files.len());
    for path in asi_files
    {
        println!("  {}", path.display());
    }
}

fn print_cleo_scripts(cleo_scripts: &[PathBuf])
{
    println!("CLEO scripts   : {}", cleo_scripts.len());
    for path in cleo_scripts
    {
        println!("  {} [{}]", path.display(), cleo_script_version(path));
    }
    let compat = cleo_scripts
        .iter()
        .filter(|path| cleo_script_version(path).contains("compat"))
        .count();
    if compat > 0
    {
        println!(
            "  NOTE: {compat} script(s) run in CLEO3/CLEO4 compatibility mode (.cs4/.cs3); they need a CLEO build that still supports that mode"
        );
    }
}

/// The CLEO version a script's extension targets: `.cs` runs as CLEO5,
/// `.cs4`/`.cs3` run in CLEO4/CLEO3 compatibility mode.
fn cleo_script_version(path: &Path) -> &'static str
{
    return match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("cs") => "CLEO5",
        Some("cs4") => "CLEO4 compat",
        Some("cs3") => "CLEO3 compat",
        _ => "CLEO",
    };
}

/// Inventory of `CLEO/cleo_modules/` â€” shared script modules (`.s`) loaded via
/// the `modules:` path prefix. Only printed when the folder exists. `.s` modules
/// are validated against their header magic.
fn print_cleo_modules(game_root: &Path)
{
    let dir = game_root.join("CLEO").join("cleo_modules");
    if !dir.is_dir()
    {
        return;
    }
    let files = list_matching(&dir, |_| true).unwrap_or_default();
    println!("CLEO modules   : {}", files.len());
    for path in &files
    {
        let tag = if has_extension_equal_to(path, "s") && !is_cleo_module(path) {
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
fn is_cleo_module(path: &Path) -> bool
{
    use std::io::Read;
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 5]; // literal: allow external format or runtime boundary value means itself here
    return file.read_exact(&mut magic).is_ok() && magic == [0xFF, 0x7F, 0xFE, 0x00, 0x00] // literal: allow external format or runtime boundary value means itself here;
}

/// Inventory of `CLEO/cleo_saves/` â€” runtime-generated per-script save data, not
/// mod content. Only printed when the folder exists.
fn print_cleo_saves(game_root: &Path)
{
    let dir = game_root.join("CLEO").join("cleo_saves");
    if !dir.is_dir()
    {
        return;
    }
    let files = list_matching(&dir, |_| true).unwrap_or_default();
    println!(
        "CLEO saves     : {} (runtime user data, not mod content)",
        files.len()
    );
}

fn print_cleo_plugins(game_root: &Path, cleo_plugins: &[PathBuf])
{
    println!("CLEO plugins   : {}", cleo_plugins.len());
    let cleo_plugins_dir = game_root.join("CLEO").join("cleo_plugins");
    for path in cleo_plugins
    {
        println!("  {}", path.display());
    }
    // CLEO5 still loads .cleo modules from the CLEO/ root as a *legacy* location,
    // but plugins in cleo_plugins/ load last and win, so cleo_plugins/ is
    // preferred. Point out loose modules as a tidy-up suggestion, not an error.
    let loose = cleo_plugins
        .iter()
        .filter(|path| path.parent() != Some(cleo_plugins_dir.as_path()))
        .count();
    if loose > 0 && detect_cleo_runtime(game_root) == CleoRuntime::Cleo5
    {
        println!(
            "  NOTE: {loose} .cleo plugin(s) sit in CLEO/ (legacy location); CLEO5 prefers CLEO/cleo_plugins/"
        );
    }
}

/// The plugin modules bundled with CLEO5. Scripts frequently depend on one of
/// these (e.g. a script calling INI opcodes needs `SA.IniFiles`), so showing
/// which are installed is the practical stand-in for a per-script dependency
/// graph â€” the compiled bytecode references plugin opcodes by number, not name,
/// so a true graph would need a disassembler plus the `sa.json` opcode_bytes table.
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

fn print_cleo_bundled_plugin_checklist(game_root: &Path, cleo_plugins: &[PathBuf])
{
    // The bundled set is a CLEO5 concept; the checklist is noise on CLEO4/none.
    if detect_cleo_runtime(game_root) != CleoRuntime::Cleo5
    {
        return;
    }
    let installed: BTreeSet<String> = cleo_plugins
        .iter()
        .map(|path| plugin_stem_lower(path))
        .collect();
    println!("CLEO5 bundled plugins:");
    for (name, present) in bundled_plugin_status(&installed)
    {
        println!("  [{}] {name}", if present { "x" } else { " " });
    }
}

/// For each bundled CLEO5 plugin, whether a matching module is installed. Keyed
/// on the lowercased file stem (`SA.Audio.cleo` â†’ `sa.audio`).
fn bundled_plugin_status(installed_stems: &BTreeSet<String>) -> Vec<(&'static str, bool)>
{
    return BUNDLED_CLEO5_PLUGINS
        .iter()
        .map(|name| (*name, installed_stems.contains(&name.to_ascii_lowercase())))
        .collect();
}

/// Warn about files whose bytes contradict their extension: `.cleo` plugins and
/// `.asi` plugins must be DLLs (PE image, `MZ` magic); compiled `.cs` scripts are
/// SCM bytecode and must not be. A `.cs` that is really a DLL, or a `.cleo` that
/// is empty/corrupt, will silently fail to load â€” this surfaces it up front.
fn print_cleo_content_validation(cleo_scripts: &[PathBuf], cleo_plugins: &[PathBuf])
{
    let mut problems = Vec::new();
    for script in cleo_scripts
    {
        if is_pe_image(script)
        {
            problems.push(format!(
                "{} looks like a DLL, not a compiled CLEO script (a misnamed .cleo/.asi?)",
                script.display()
            ));
        }
    }
    for plugin in cleo_plugins
    {
        if !is_pe_image(plugin)
        {
            problems.push(format!(
                "{} is not a valid plugin DLL (empty or corrupt .cleo)",
                plugin.display()
            ));
        }
    }
    if problems.is_empty()
    {
        return;
    }
    println!("CLEO content check:");
    for problem in problems
    {
        println!("  WARNING: {problem}");
    }
}

/// Whether a file begins with the PE/DLL magic `MZ`. CLEO plugins (`.cleo`) and
/// ASI plugins are DLLs; compiled CLEO scripts (`.cs`/`.cs4`/`.cs3`) are not.
fn is_pe_image(path: &Path) -> bool
{
    use std::io::Read;
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2]; // literal: allow external format or runtime boundary value means itself here
    return file.read_exact(&mut magic).is_ok() && &magic == b"MZ";
}

/// Per-script plugin dependency analysis: disassemble each CLEO script and, using
/// the game's `sa.json` opcode_bytes database, report which bundled plugins it needs and
/// which of those are not installed. Only meaningful on CLEO5 (that is where
/// `sa.json` lives); a script whose bytecode cannot be fully walked is flagged as
/// partial rather than reported as having no further dependencies.
fn print_cleo_script_dependencies(game_root: &Path, scripts: &[PathBuf], plugins: &[PathBuf])
{
    if detect_cleo_runtime(game_root) != CleoRuntime::Cleo5 || scripts.is_empty()
    {
        return;
    }
    let sa_json = game_root.join("CLEO").join(".config").join("sa.json");
    let Some(db) = load_opcode_db(&sa_json) else {
        println!(
            "CLEO script dependencies: skipped (opcode_bytes database CLEO/.config/sa.json not found or unreadable)"
        );
        return;
    };

    let installed: BTreeSet<String> = plugins.iter().map(|path| plugin_stem_lower(path)).collect();
    let mut reported = false;
    for script in scripts
    {
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
        if !reported
        {
            println!("CLEO script dependencies (via CLEO/.config/sa.json):");
            reported = true;
        }
        print_one_script_dependencies(script, &deps, &missing);
    }
    if !reported
    {
        println!("CLEO script dependencies: none detected");
    }
}

fn print_one_script_dependencies(
    script: &Path,
    deps: &super::cleo_deps::ScriptDeps,
    missing: &[&String],
)
{
    let name = script
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<script>");
    let mut parts = Vec::new();
    if !deps.plugins.is_empty()
    {
        parts.push(format!(
            "needs {}",
            deps.plugins.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    if !missing.is_empty()
    {
        let names: Vec<&str> = missing.iter().map(|plugin| plugin.as_str()).collect();
        parts.push(format!("MISSING {}", names.join(", ")));
    }
    if !deps.unmapped_extensions.is_empty()
    {
        parts.push(format!(
            "uses {} (no bundled file mapping)",
            deps.unmapped_extensions
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !deps.capabilities.is_empty()
    {
        parts.push(format!(
            "elevated: {}",
            deps.capabilities
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !deps.complete
    {
        parts.push("partial: undecodable bytecode, dependencies may be incomplete".to_string());
    }
    println!("  {name}: {}", parts.join("; "));
}

/// Lowercased file stem of a plugin path (`.../SA.Audio.cleo` â†’ `sa.audio`).
fn plugin_stem_lower(path: &Path) -> String
{
    return path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
}

#[cfg(test)]
include!("part_02_tests_01.rs");
