use crate::prelude::*;
use crate::workspace::{ProfileCopyRequest, ProfileModSelection, ProfileRootEnabledState, ProfileRootSelector};

const CONFIG_FLAG: &str = "--config";
const SEVEN_ZIP_FLAG: &str = "--7z";
const MOD_ROOTS_FLAG: &str = "--mod-roots";
const VERBOSE_FLAG: &str = "--verbose";
const VERBOSE_SHORT_FLAG: &str = "-v";
const TRACE_SHORT_FLAG: &str = "-vv";
const QUIET_FLAG: &str = "--quiet";
const QUIET_SHORT_FLAG: &str = "-q";
const ERROR_SHORT_FLAG: &str = "-qq";
const HELP_COMMAND: &str = "help";
const HELP_FLAG: &str = "--help";
const HELP_SHORT_FLAG: &str = "-h";
const VERSION_COMMAND: &str = "version";
const VERSION_FLAG: &str = "--version";
const VERSION_SHORT_FLAG: &str = "-V";
const GAME_FLAG: &str = "--game";
const PROFILE_FLAG: &str = "--profile";
const INCLUDE_FLAG: &str = "--include";
const EXCLUDE_FLAG: &str = "--exclude";
const MANIFEST_FLAG: &str = "--manifest";
const GAME_FLAG_REQUIRES_PATH: &str = "--game requires a path";
const PROFILE_FLAG_REQUIRES_NAME: &str = "--profile requires a name";
const INCLUDE_FLAG_REQUIRES_SOURCE_ROOT: &str = "--include requires a source root";
const EXCLUDE_FLAG_REQUIRES_SOURCE_ROOT: &str = "--exclude requires a source root";

pub(crate) fn main()
{
    let result = run();
    if let Err(err) = result
    {
        report_error(&err);
        std::process::exit(1);
    }
}

fn run() -> Result<(), AppError>
{
    let mut shell_arguments: Vec<String> = env::args().skip(1).collect();
    let cli_level = extract_verbosity(&mut shell_arguments);
    // Resolve `--config`/`--7z`/`--mod-roots` before any command touches settings,
    // so they win over env and config the way explicit overrides should.
    if let Some(config) = extract_flag_value(&mut shell_arguments, CONFIG_FLAG)
    {
        crate::settings::set_cli_config_path(PathBuf::from(config));
    }
    if let Some(seven_zip) = extract_flag_value(&mut shell_arguments, SEVEN_ZIP_FLAG)
    {
        crate::settings::set_cli_seven_zip(PathBuf::from(seven_zip));
    }
    if let Some(mod_roots) = extract_flag_value(&mut shell_arguments, MOD_ROOTS_FLAG)
    {
        let roots: Vec<PathBuf> = env::split_paths(&mod_roots)
            .filter(|path| !path.as_os_str().is_empty())
            .collect();
        crate::settings::set_cli_mod_roots(roots);
    }
    // A `-v`/`-q` flag wins; otherwise use the configured level (default Info).
    let level = cli_level.unwrap_or_else(crate::settings::configured_log_level);
    crate::logging::set_console_level(level);

    let mut shell_arguments = shell_arguments.into_iter();
    let Some(command) = shell_arguments.next() else {
        print_usage();
        return Ok(());
    };

    let command_arguments: Vec<String> = shell_arguments.collect();
    return dispatch_command(&command, command_arguments);
}

/// Remove global `--verbose`/`--quiet` flags (usable anywhere on the line) and
/// resolve them to a console log level. Each `-v` raises verbosity, each `-q`
/// lowers it.
fn extract_verbosity(arguments: &mut Vec<String>) -> Option<crate::logging::Level>
{
    use crate::logging::Level;
    let mut verbosity: i32 = 0;
    let mut saw_flag = false;
    arguments.retain(|arg| match arg.as_str() {
        VERBOSE_SHORT_FLAG | VERBOSE_FLAG => {
            verbosity += 1;
            saw_flag = true;
            false
        }
        TRACE_SHORT_FLAG => {
            verbosity += 2; // literal: allow domain threshold is documented by the surrounding code
            saw_flag = true;
            false
        }
        QUIET_SHORT_FLAG | QUIET_FLAG => {
            verbosity -= 1;
            saw_flag = true;
            false
        }
        ERROR_SHORT_FLAG => {
            verbosity -= 2; // literal: allow domain threshold is documented by the surrounding code
            saw_flag = true;
            false
        }
        _ => true,
    });
    // No flag means "defer to the configured level"; a flag maps to a level.
    if !saw_flag
    {
        return None;
    }
    return Some(match verbosity {
        i if i <= -2 => Level::Error, // literal: allow domain threshold is documented by the surrounding code
        -1 => Level::Warn,
        0 => Level::Info,
        1 => Level::Debug,
        _ => Level::Trace,
    });
}

/// Remove a global `--<flag> <value>` option (usable anywhere on the line) and
/// return its value. A trailing flag with no following value is dropped and
/// ignored rather than swallowing the next flag.
fn extract_flag_value(arguments: &mut Vec<String>, flag: &str) -> Option<String>
{
    let index = arguments.iter().position(|arg| arg == flag)?;
    arguments.remove(index);
    let value = arguments.get(index)?;
    if value.starts_with('-')
    {
        return None;
    }
    return Some(arguments.remove(index));
}

fn dispatch_command(command: &str, arguments: Vec<String>) -> Result<(), AppError>
{
    // `<command> --help`/`-h` shows that command's help instead of erroring on an
    // unexpected flag. The `help`/`version` verbs handle their own arguments.
    if !matches!(
        command,
        HELP_COMMAND
            | HELP_FLAG
            | HELP_SHORT_FLAG
            | VERSION_COMMAND
            | VERSION_FLAG
            | VERSION_SHORT_FLAG
    ) && arguments_request_help(&arguments)
    {
        print_command_help(command);
        return Ok(());
    }
    return match command
    {
        "scan" => handle_scan_command(arguments),
        "ui" => handle_ui_command(arguments),
        "analyze" | "plan" => handle_analyze_command(arguments),
        "dry-install" => handle_dry_install_command(arguments),
        "install" => handle_install_command(arguments),
        "config-new" => handle_config_new_command(arguments),
        "import" => handle_import_command(arguments),
        "profile-json" => handle_profile_json_command(arguments),
        "profile-add" => handle_profile_add_command(arguments),
        "profile-show" => handle_profile_show_command(arguments),
        "profile-enable" => handle_profile_enable_command(arguments),
        "profile-disable" => handle_profile_disable_command(arguments),
        "profile-order" => handle_profile_order_command(arguments),
        "profile-remove" => handle_profile_remove_command(arguments),
        "prepare-run" => handle_prepare_run_command(arguments),
        "cleanup-run" => handle_cleanup_run_command(arguments),
        "extract-stage" => handle_extract_stage_command(arguments),
        "rollback" => handle_rollback_command(arguments),
        "recover" => handle_recover_command(arguments),
        "init" => handle_init_command(arguments),
        "config-init" => handle_config_init_command(arguments),
        "profiles" => handle_profiles_command(arguments),
        "profile-new" => handle_profile_new_command(arguments),
        "profile-use" => handle_profile_use_command(arguments),
        "profile-copy" => handle_profile_copy_command(arguments),
        "profile-rename" => handle_profile_rename_command(arguments),
        "profile-delete" => handle_profile_delete_command(arguments),
        "profile-all-off" => handle_profile_all_off_command(arguments),
        "profile-all-on" => handle_profile_all_on_command(arguments),
        "profile-args" => handle_profile_args_command(arguments),
        "profile-root" => handle_profile_root_command(arguments),
        "profile-root-target" => handle_profile_root_target_command(arguments),
        "game" => handle_game_command(arguments),
        "logs" => handle_logs_command(arguments),
        HELP_COMMAND | HELP_FLAG | HELP_SHORT_FLAG => {
            match arguments.first()
            {
                Some(subcommand) => print_command_help(subcommand),
                None => print_usage(),
            }
            Ok(())
        }
        VERSION_COMMAND | VERSION_FLAG | VERSION_SHORT_FLAG => {
            print_version();
            Ok(())
        }
        _ => {
            let message = format!("unknown command `{command}`");
            Err(AppError::Usage(message))
        }
    };
}

fn arguments_request_help(arguments: &[String]) -> bool
{
    return arguments
        .iter()
        .any(|arg| arg == HELP_FLAG || arg == HELP_SHORT_FLAG);
}

fn handle_scan_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let roots: Vec<PathBuf> = if arguments.is_empty() {
        default_mod_roots()
    } else {
        arguments.into_iter().map(PathBuf::from).collect()
    };
    if roots.is_empty()
    {
        return Err(usage_error(
            "no mod roots to scan; pass one or more folders, or set `mod_roots` in the config (config-init)",
        ));
    }
    return scan_roots(&roots);
}

fn handle_ui_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let game_root = optional_explicit_game_root(arguments);
    return crate::ui::run_ui(game_root);
}

/// Like [`optional_game_root_from_raw_arguments`], but returns `None` when the
/// user gave no folder, so the UI can fall back to the last-used folder rather
/// than always forcing the compiled default.
fn optional_explicit_game_root(arguments: Vec<String>) -> Option<PathBuf>
{
    let mut cursor = CliArguments::new(arguments);
    return match cursor.next_optional()
    {
        Some(flag) if flag == GAME_FLAG => cursor.next_optional().map(PathBuf::from),
        Some(path) => Some(PathBuf::from(path)),
        None => None,
    };
}

fn handle_analyze_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    let report = analyze_package(&package_path, &game_root)?;
    print_report(&report);
    return Ok(());
}

fn handle_dry_install_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?;
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    let report = analyze_package(&package_path, &options.game_root)?;
    let plan = build_install_plan(&report, &options);
    print_install_plan(&plan);
    if options.write_manifest
    {
        write_package_manifest(&report, &plan)?;
        println!();
        let state_path = state_directory(&options.game_root);
        println!("manifest written under {}", state_path.display());
    }
    return Ok(());
}

fn handle_install_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut arguments = arguments;
    let permanent = arguments.iter().any(|arg| arg == "--permanent");
    arguments.retain(|arg| arg != "--permanent");
    if !permanent
    {
        return Err(usage_error(
            "install writes permanently; pass --permanent or use import/profile/prepare-run",
        ));
    }
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?;
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    let report = analyze_package(&package_path, &options.game_root)?;
    let mut plan = build_install_plan(&report, &options);
    sort_operations_for_apply(&mut plan);
    apply_install_plan(&package_path, &plan)?;
    return write_package_manifest(&report, &plan);
}

fn handle_config_new_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?;
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    let report = analyze_package(&package_path, &options.game_root)?;
    let plan = build_install_plan(&report, &options);
    return write_mod_config_json(&report, &plan);
}

fn handle_import_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?;
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    return import_package(&package_path, &options);
}

fn handle_profile_json_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let name = cursor.required("profile name")?;
    let game_root = optional_game_root_from_cli_arguments(cursor);
    let profile_name = safe_name(&name);
    let profile = ProfileJson {
        name: profile_name,
        ..Default::default()
    };
    return write_profile_json(&game_root, &profile);
}

fn handle_profile_add_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?;
    let config = cursor.required("mod config path")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let config_path = PathBuf::from(config);
    return add_mod_to_profile_json(&game_root, &profile_name, &config_path);
}

fn handle_profile_show_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    return show_profile_json(&game_root, &profile_name);
}

fn handle_profile_enable_command(arguments: Vec<String>) -> Result<(), AppError>
{
    return set_profile_activation_from_cli_arguments(arguments, ProfileModActivation::Enabled);
}

fn handle_profile_disable_command(arguments: Vec<String>) -> Result<(), AppError>
{
    return set_profile_activation_from_cli_arguments(arguments, ProfileModActivation::Disabled);
}

fn handle_profile_order_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?;
    let mod_id = cursor.required("mod id")?;
    let order_text = cursor.required("load order")?;
    let order = parse_load_order(&order_text)?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let safe_mod_id = safe_name(&mod_id);
    return set_profile_mod_order(&game_root, ProfileModSelection { profile_name: &profile_name, mod_id: &safe_mod_id }, order);
}

fn parse_load_order(value: &str) -> Result<i32, AppError>
{
    return match value.parse::<i32>()
    {
        Ok(order) => Ok(order),
        Err(_) => Err(usage_error("load order must be an integer")),
    };
}

fn handle_profile_remove_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?;
    let mod_id = cursor.required("mod id")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let safe_mod_id = safe_name(&mod_id);
    return remove_mod_from_profile_json(&game_root, ProfileModSelection { profile_name: &profile_name, mod_id: &safe_mod_id });
}

fn handle_prepare_run_command(arguments: Vec<String>) -> Result<(), AppError>
{
    // Profile is the first non-flag token; when omitted, use the active profile.
    let mut profile = None;
    let mut rest = Vec::new();
    for arg in arguments
    {
        if profile.is_none() && !arg.starts_with("--")
        {
            profile = Some(arg);
        }
        else
        {
            rest.push(arg);
        }
    }
    let game_root = parse_game_root_from_arguments(rest, default_game_root_path())?;
    let profile_name = match profile {
        Some(profile) => safe_name(&profile),
        None => read_active_profile(&game_root),
    };
    return prepare_run(&game_root, &profile_name);
}

fn handle_profile_use_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let name = cursor.required("profile name")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    return set_active_profile(&game_root, &safe_name(&name));
}

fn handle_profile_copy_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let source = cursor.required("source profile name")?;
    let dest = cursor.required("destination profile name")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    return copy_profile(&game_root, ProfileCopyRequest { source_name: &safe_name(&source), dest_name: &safe_profile_name_warned(&dest) });
}

fn report_error(err: &AppError)
{
    // `AppError`'s Display already renders the full `context: cause` chain down to
    // the underlying OS error (see `AppError::Context`), so `{err}` prints the
    // whole story; the category only sets the prefix and any follow-up hint.
    match err.kind()
    {
        AppErrorKind::Usage => {
            log_error!("usage error: {err}");
            log_error!("run `sa-mod-manager help` for usage");
        }
        AppErrorKind::Tool => log_error!("tool error: {err}"),
        AppErrorKind::Io => log_error!("i/o error: {err}"),
    }
}

fn default_mod_roots() -> Vec<PathBuf>
{
    return crate::settings::default_mod_roots();
}





