use crate::prelude::*;

pub(crate) fn main() {
    let result = run();
    if let Err(err) = result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), AppError> {
    let mut shell_arguments = env::args().skip(1);
    let Some(command) = shell_arguments.next() else {
        print_usage();
        return Ok(());
    };

    let command_arguments: Vec<String> = shell_arguments.collect();
    dispatch_command(&command, command_arguments)
}

fn dispatch_command(command: &str, arguments: Vec<String>) -> Result<(), AppError> {
    match command {
        "scan" => handle_scan_command(arguments), // literal: allow external interface text or file-format spelling
        "ui" => handle_ui_command(arguments), // literal: allow external interface text or file-format spelling
        "analyze" | "plan" => handle_analyze_command(arguments), // literal: allow external interface text or file-format spelling
        "dry-install" => handle_dry_install_command(arguments), // literal: allow external interface text or file-format spelling
        "install" => handle_install_command(arguments), // literal: allow external interface text or file-format spelling
        "config-new" => handle_config_new_command(arguments), // literal: allow external interface text or file-format spelling
        "import" => handle_import_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-json" => handle_profile_json_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-add" => handle_profile_add_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-show" => handle_profile_show_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-enable" => handle_profile_enable_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-disable" => handle_profile_disable_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-order" => handle_profile_order_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-remove" => handle_profile_remove_command(arguments), // literal: allow external interface text or file-format spelling
        "prepare-run" => handle_prepare_run_command(arguments), // literal: allow external interface text or file-format spelling
        "cleanup-run" => handle_cleanup_run_command(arguments), // literal: allow external interface text or file-format spelling
        "extract-stage" => handle_extract_stage_command(arguments), // literal: allow external interface text or file-format spelling
        "rollback" => handle_rollback_command(arguments), // literal: allow external interface text or file-format spelling
        "init" => handle_init_command(arguments), // literal: allow external interface text or file-format spelling
        "profiles" => handle_profiles_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-new" => handle_profile_new_command(arguments), // literal: allow external interface text or file-format spelling
        "game" => handle_game_command(arguments), // literal: allow external interface text or file-format spelling
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        "help" | "--help" | "-h" => {
            // literal: allow external interface text or file-format spelling
            print_usage();
            Ok(())
        }
        _ => {
            let message = format!("unknown command `{command}`");
            Err(AppError::Usage(message))
        }
    }
}

fn handle_scan_command(arguments: Vec<String>) -> Result<(), AppError> {
    let roots = if arguments.is_empty() {
        default_mod_roots()
    } else {
        arguments.into_iter().map(PathBuf::from).collect()
    };
    scan_roots(&roots)
}

fn default_mod_roots() -> Vec<PathBuf> {
    DEFAULT_MOD_ROOTS.iter().map(PathBuf::from).collect()
}

fn handle_ui_command(arguments: Vec<String>) -> Result<(), AppError> {
    let game_root = optional_game_root_from_raw_arguments(arguments);
    crate::ui::run_ui(game_root)
}

fn handle_analyze_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    let report = analyze_package(&package_path, &game_root)?;
    print_report(&report);
    Ok(())
}

fn handle_dry_install_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?; // literal: allow external interface text or file-format spelling
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    let report = analyze_package(&package_path, &options.game_root)?;
    let plan = build_install_plan(&report, &options);
    print_install_plan(&plan);
    if options.write_manifest {
        write_package_manifest(&report, &plan)?;
        println!();
        let state_path = state_directory(&options.game_root);
        println!("manifest written under {}", state_path.display());
    }
    Ok(())
}

fn handle_install_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut arguments = arguments;
    let permanent = arguments.iter().any(|arg| arg == "--permanent"); // literal: allow external interface text or file-format spelling
    arguments.retain(|arg| arg != "--permanent"); // literal: allow external interface text or file-format spelling
    if !permanent {
        return Err(usage_error(
            "install writes permanently; pass --permanent or use import/profile/prepare-run", // literal: allow external interface text or file-format spelling
        ));
    }
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?; // literal: allow external interface text or file-format spelling
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    let report = analyze_package(&package_path, &options.game_root)?;
    let mut plan = build_install_plan(&report, &options);
    sort_operations_for_apply(&mut plan);
    apply_install_plan(&package_path, &plan)?;
    write_package_manifest(&report, &plan)
}

fn handle_config_new_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?; // literal: allow external interface text or file-format spelling
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    let report = analyze_package(&package_path, &options.game_root)?;
    let plan = build_install_plan(&report, &options);
    write_mod_config_json(&report, &plan)
}

fn handle_import_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?; // literal: allow external interface text or file-format spelling
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    import_package(&package_path, &options)
}

fn handle_profile_json_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let name = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let game_root = optional_game_root_from_cli_arguments(cursor);
    let profile_name = safe_name(&name);
    write_profile_json(&game_root, &profile_name, &[])
}

fn handle_profile_add_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let config = cursor.required("mod config path")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let config_path = PathBuf::from(config);
    add_mod_to_profile_json(&game_root, &profile_name, &config_path)
}

fn handle_profile_show_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    show_profile_json(&game_root, &profile_name)
}

fn handle_profile_enable_command(arguments: Vec<String>) -> Result<(), AppError> {
    set_profile_activation_from_cli_arguments(arguments, ProfileModActivation::Enabled)
}

fn handle_profile_disable_command(arguments: Vec<String>) -> Result<(), AppError> {
    set_profile_activation_from_cli_arguments(arguments, ProfileModActivation::Disabled)
}

fn handle_profile_order_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let mod_id = cursor.required("mod id")?; // literal: allow external interface text or file-format spelling
    let order_text = cursor.required("load order")?; // literal: allow external interface text or file-format spelling
    let order = parse_load_order(&order_text)?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let safe_mod_id = safe_name(&mod_id);
    set_profile_mod_order(&game_root, &profile_name, &safe_mod_id, order)
}

fn parse_load_order(value: &str) -> Result<i32, AppError> {
    match value.parse::<i32>() {
        Ok(order) => Ok(order),
        Err(_) => Err(usage_error("load order must be an integer")), // literal: allow external interface text or file-format spelling
    }
}

fn handle_profile_remove_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let mod_id = cursor.required("mod id")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let safe_mod_id = safe_name(&mod_id);
    remove_mod_from_profile_json(&game_root, &profile_name, &safe_mod_id)
}

fn handle_prepare_run_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    prepare_run(&game_root, &profile_name)
}

fn handle_cleanup_run_command(arguments: Vec<String>) -> Result<(), AppError> {
    rollback_from_arguments(arguments, "journal path") // literal: allow external interface text or file-format spelling
}

fn handle_extract_stage_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?; // literal: allow external interface text or file-format spelling
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    extract_to_staging(&package_path, &options.game_root)
}

fn handle_rollback_command(arguments: Vec<String>) -> Result<(), AppError> {
    rollback_from_arguments(arguments, "journal path") // literal: allow external interface text or file-format spelling
}

fn handle_init_command(arguments: Vec<String>) -> Result<(), AppError> {
    let game_root = optional_game_root_from_raw_arguments(arguments);
    init_state(&game_root)
}

fn handle_profiles_command(arguments: Vec<String>) -> Result<(), AppError> {
    let game_root = optional_game_root_from_raw_arguments(arguments);
    list_profiles(&game_root)
}

fn handle_profile_new_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let name = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let game_root = optional_game_root_from_cli_arguments(cursor);
    create_profile(&game_root, &name)
}

fn handle_game_command(arguments: Vec<String>) -> Result<(), AppError> {
    let game_root = optional_game_root_from_raw_arguments(arguments);
    inspect_game(&game_root)
}

pub(crate) fn usage_error(message: &str) -> AppError {
    let owned_message = message.to_owned();
    AppError::Usage(owned_message)
}

fn set_profile_activation_from_cli_arguments(
    arguments: Vec<String>,
    activation: ProfileModActivation,
) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let mod_id = cursor.required("mod id")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let safe_mod_id = safe_name(&mod_id);
    set_profile_mod_activation(&game_root, &profile_name, &safe_mod_id, activation)
}

fn rollback_from_arguments(arguments: Vec<String>, label: &str) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let journal = cursor.required(label)?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let journal_path = PathBuf::from(journal);
    rollback_journal(&journal_path, &game_root)
}

fn game_root_from_cli_arguments(cursor: CliArguments) -> Result<PathBuf, AppError> {
    let remaining_arguments = cursor.remaining();
    let fallback_game_root = default_game_root_path();
    parse_game_root_from_arguments(remaining_arguments, fallback_game_root)
}

fn options_from_cli_arguments(cursor: CliArguments) -> Result<CommandOptions, AppError> {
    let remaining_arguments = cursor.remaining();
    let fallback_game_root = default_game_root_path();
    parse_options_from_arguments(remaining_arguments, fallback_game_root)
}

fn optional_game_root_from_cli_arguments(cursor: CliArguments) -> PathBuf {
    let remaining_arguments = cursor.remaining();
    optional_game_root_from_raw_arguments(remaining_arguments)
}

fn optional_game_root_from_raw_arguments(arguments: Vec<String>) -> PathBuf {
    let mut cursor = CliArguments::new(arguments);
    match cursor.next_optional() {
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        Some(flag) if flag == "--game" => {
            cursor // literal: allow external interface text or file-format spelling
                .next_optional()
                .map(PathBuf::from)
                .unwrap_or_else(default_game_root_path)
        }
        Some(path) => PathBuf::from(path),
        None => default_game_root_path(),
    }
}

fn default_game_root_path() -> PathBuf {
    PathBuf::from(DEFAULT_GAME_ROOT)
}

fn parse_game_root_from_arguments(
    args: Vec<String>,
    fallback: PathBuf,
) -> Result<PathBuf, AppError> {
    let mut game_root = fallback;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            /* literal: allow external interface text or file-format spelling */
            /* literal: allow external interface text or file-format spelling */
            "--game" => {
                // literal: allow external interface text or file-format spelling
                let Some(value) = iter.next() else {
                    return Err(usage_error("--game requires a path")); // literal: allow external interface text or file-format spelling
                };
                game_root = PathBuf::from(value);
            }
            other => {
                let message = format!("unexpected argument `{other}`");
                return Err(AppError::Usage(message));
            }
        }
    }
    Ok(game_root)
}

fn parse_options_from_arguments(
    args: Vec<String>,
    fallback_game_root: PathBuf,
) -> Result<CommandOptions, AppError> {
    let mut options = CommandOptions {
        game_root: fallback_game_root,
        profile: "default".to_string(), // literal: allow external interface text or file-format spelling
        includes: BTreeSet::new(),
        excludes: BTreeSet::new(),
        write_manifest: false,
    };

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        apply_option_argument(&mut options, &mut iter, &arg)?;
    }

    Ok(options)
}

fn apply_option_argument(
    options: &mut CommandOptions,
    iter: &mut impl Iterator<Item = String>,
    arg: &str,
) -> Result<(), AppError> {
    match arg {
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        "--game" => {
            // literal: allow external interface text or file-format spelling
            let value = next_option_value(iter, "--game requires a path")?; // literal: allow external interface text or file-format spelling
            options.game_root = PathBuf::from(value);
        }
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        "--profile" => {
            // literal: allow external interface text or file-format spelling
            let value = next_option_value(iter, "--profile requires a name")?; // literal: allow external interface text or file-format spelling
            options.profile = safe_name(&value);
        }
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        "--include" => {
            // literal: allow external interface text or file-format spelling
            let value = next_option_value(iter, "--include requires a source root")?; // literal: allow external interface text or file-format spelling
            let source_root = normalize_path(&value);
            options.includes.insert(source_root);
        }
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        "--exclude" => {
            // literal: allow external interface text or file-format spelling
            let value = next_option_value(iter, "--exclude requires a source root")?; // literal: allow external interface text or file-format spelling
            let source_root = normalize_path(&value);
            options.excludes.insert(source_root);
        }
        "--manifest" => options.write_manifest = true, // literal: allow external interface text or file-format spelling
        other => {
            let message = format!("unexpected argument `{other}`");
            return Err(AppError::Usage(message));
        }
    }
    Ok(())
}

fn next_option_value(
    iter: &mut impl Iterator<Item = String>,
    missing_message: &str,
) -> Result<String, AppError> {
    match iter.next() {
        Some(value) => Ok(value),
        None => Err(usage_error(missing_message)),
    }
}

struct CliArguments {
    values: VecDeque<String>,
}

impl CliArguments {
    fn new(values: Vec<String>) -> Self {
        let values = VecDeque::from(values);
        Self { values }
    }

    fn required(&mut self, label: &str) -> Result<String, AppError> {
        match self.values.pop_front() {
            Some(value) => Ok(value),
            None => {
                let message = format!("missing {label}");
                Err(AppError::Usage(message))
            }
        }
    }

    fn next_optional(&mut self) -> Option<String> {
        self.values.pop_front()
    }

    fn remaining(self) -> Vec<String> {
        self.values.into_iter().collect()
    }
}

fn print_usage() {
    println!("sa-mod-manager");
    println!();
    print_command_usage();
    print_plan_option_usage();
    print_default_usage();
}

fn print_command_usage() {
    println!("Commands:");
    println!("  ui [game-root]                   Open the desktop manager UI");
    println!("  game [game-root]                 Inspect installed GTA SA mod infrastructure");
    println!("  init [game-root]                 Create manager state folders");
    println!("  profiles [game-root]             List profiles");
    println!("  profile-new <name> [game-root]   Create a profile manifest");
    println!("  scan [mod-root ...]              Inventory archives and folders");
    println!(
        "  analyze <archive-or-folder>      Detect install roots, options, risks, and compatibility hints"
    );
    println!("  dry-install <package> [options]  Build a no-write install plan");
    println!("  install <package> --permanent    Apply plan with backups and a journal");
    println!("  config-new <package> [options]   Write human-editable mod JSON config");
    println!(
        "  import <package> [options]       Pre-extract package into library and write mod JSON"
    );
    println!("  profile-json <name> [game-root]  Write human-editable profile JSON");
    println!("  profile-add <profile> <mod.json> Add mod config to profile JSON");
    println!("  profile-show <profile> [--game]  Show enabled mods and load order");
    println!("  profile-enable <profile> <mod>   Enable a profile mod");
    println!("  profile-disable <profile> <mod>  Disable a profile mod");
    println!("  profile-order <profile> <mod> N  Set profile load order");
    println!("  profile-remove <profile> <mod>   Remove a mod from a profile");
    println!("  prepare-run <profile> [--game]   Temporarily materialize profile into game folder");
    println!("  cleanup-run <journal> [--game]   Remove temporary materialized files");
    println!("  extract-stage <package> [options] Extract package into managed staging only");
    println!("  rollback <journal> [--game path] Restore a recorded install transaction");
    println!();
}

fn print_plan_option_usage() {
    println!("Plan options:");
    println!("  --game <path>                    Override game root");
    println!("  --profile <name>                 Profile name for generated plan");
    println!("  --include <source-root>          Include an optional/install root");
    println!("  --exclude <source-root>          Exclude an install root");
    println!("  --manifest                       Write package and plan manifests");
    println!();
}

fn print_default_usage() {
    println!("Defaults:");
    println!("  game root: {DEFAULT_GAME_ROOT}");
    println!("  mod roots : {}", DEFAULT_MOD_ROOTS.join(", "));
}
