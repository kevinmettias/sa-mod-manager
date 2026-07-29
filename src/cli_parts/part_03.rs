
/// Single source of truth for the command list. Both the full usage screen and
/// per-command help (`help <cmd>`, `<cmd> --help`) render from this, so there is
/// no second table to keep in sync.
const COMMAND_USAGE_LINES: &[&str] = &[
    "  ui [game-root]                   Open the desktop manager UI",
    "  config-init                      Write an example user config (paths, detection rules, readme thresholds, watch interval, modloader prefix)",
    "  game [game-root]                 Inspect installed GTA SA mod infrastructure",
    "  logs [game-root]                 Show the persistent diagnostics log (recent lines)",
    "  init [game-root]                 Create manager state folders",
    "  profiles [game-root]             List profiles",
    "  profile-new <name> [game-root]   Create a profile manifest",
    "  profile-use <name> [--game path] Set the active profile",
    "  profile-copy <src> <dest>        Copy a profile to a new name",
    "  profile-rename <old> <new>       Rename a profile",
    "  profile-delete <name>            Delete a profile",
    "  profile-args [--game p] <name> ..  Set profile launch arguments",
    "  profile-root <name> <mod> <src> on|off  Toggle a mod's install root for a profile",
    "  profile-root-target <name> <mod> <src> <target>  Retarget a mod's install root for a profile",
    "  scan [mod-root ...]              Inventory archives and folders",
    "  analyze <archive-or-folder>      Detect install roots, options, risks, and compatibility hints",
    "  dry-install <package> [options]  Build a no-write install plan",
    "  install <package> --permanent    Apply plan with backups and a journal",
    "  config-new <package> [options]   Write human-editable mod JSON config",
    "  import <package> [options]       Pre-extract package into library and write mod JSON",
    "  profile-json <name> [game-root]  Write human-editable profile JSON",
    "  profile-add <profile> <mod.json> Add mod config to profile JSON",
    "  profile-show <profile> [--game]  Show enabled mods and load order",
    "  profile-enable <profile> <mod>   Enable a profile mod",
    "  profile-disable <profile> <mod>  Disable a profile mod",
    "  profile-order <profile> <mod> N  Set profile load order",
    "  profile-remove_profile_fixture <profile> <mod>   Remove a mod from a profile",
    "  profile-all-off <profile>        Disable every mod (vanilla mode)",
    "  profile-all-on <profile>         Enable every mod in a profile",
    "  prepare-run [profile] [--game]   Materialize a profile (default: active) into the game folder",
    "  cleanup-run <journal> [--game]   Remove temporary materialized files",
    "  extract-stage <package> [options] Extract package into managed staging only",
    "  rollback <journal> [--game path] Restore a recorded install transaction",
    "  recover [game-root]              Roll back an interrupted permanent install",
];

fn print_command_usage()
{
    println!("Commands:");
    for line in COMMAND_USAGE_LINES
    {
        println!("{line}");
    }
    println!();
}

/// Help for a single command: its synopsis plus the option groups that apply.
/// Falls back to a pointer to full help when the command is unknown.
fn print_command_help(command: &str)
{
    println!("sa-mod-manager {command}");
    println!();
    let matches: Vec<&&str> = COMMAND_USAGE_LINES
        .iter()
        .filter(|line| command_line_name(line) == command)
        .collect();
    if matches.is_empty()
    {
        println!("Unknown command `{command}`. Run `sa-mod-manager help` for all commands.");
        return;
    }
    for line in matches
    {
        println!("{line}");
    }
    println!();
    print_plan_option_usage();
    print_global_option_usage();
}

/// The command token a usage line describes (first word after the indent).
fn command_line_name(line: &str) -> &str
{
    return line.split_whitespace().next().unwrap_or("");
}

fn print_plan_option_usage()
{
    println!("Plan options:");
    println!("  --game <path>                    Override game root");
    println!("  --profile <name>                 Profile name for generated plan");
    println!("  --include <source-root>          Include an optional/install root");
    println!("  --exclude <source-root>          Exclude an install root");
    println!("  --manifest                       Write package and plan manifests");
    println!();
}

fn print_default_usage()
{
    // Resolved defaults after config file + env overrides, not just the
    // compiled-in fallbacks.
    let configured_roots = default_mod_roots();
    let mod_roots = if configured_roots.is_empty() {
        "(none set â€” pass folders to `scan` or set mod_roots in the config)".to_string()
    } else {
        configured_roots
            .iter()
            .map(|root| root.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    println!("Defaults (config/env overridable):");
    println!("  game root: {}", default_game_root_path().display());
    println!("  mod roots : {mod_roots}");
    if let Some(path) = crate::settings::config_file_path()
    {
        println!(
            "  config    : {} (run `config-init` to create)",
            path.display()
        );
    }
}
