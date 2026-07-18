use crate::prelude::*;

pub(crate) fn main() {
    let result = run();
    if let Err(err) = result {
        report_error(&err);
        std::process::exit(1);
    }
}

fn report_error(err: &AppError) {
    match err.kind() {
        AppErrorKind::Usage => {
            log_error!("usage error: {err}");
            log_error!("run `sa-mod-manager help` for usage");
        }
        AppErrorKind::Tool => log_error!("tool error: {err}"),
        AppErrorKind::Io => log_error!("error: {err}"),
    }
}

fn run() -> Result<(), AppError> {
    let mut shell_arguments: Vec<String> = env::args().skip(1).collect();
    crate::logging::set_console_level(extract_verbosity(&mut shell_arguments));
    // Resolve `--config` before any command touches settings, so it wins over the
    // env var and per-user default the way an explicit override should.
    if let Some(config) = extract_config_path(&mut shell_arguments) {
        crate::settings::set_cli_config_path(config);
    }

    let mut shell_arguments = shell_arguments.into_iter();
    let Some(command) = shell_arguments.next() else {
        print_usage();
        return Ok(());
    };

    let command_arguments: Vec<String> = shell_arguments.collect();
    dispatch_command(&command, command_arguments)
}

/// Remove a global `--config <path>` flag (usable anywhere on the line) and
/// return the path. A trailing `--config` with no value is dropped and ignored.
fn extract_config_path(arguments: &mut Vec<String>) -> Option<PathBuf> {
    let index = arguments.iter().position(|arg| arg == "--config")?;
    arguments.remove(index);
    if index < arguments.len() {
        Some(PathBuf::from(arguments.remove(index)))
    } else {
        None
    }
}

/// Remove global `--verbose`/`--quiet` flags (usable anywhere on the line) and
/// resolve them to a console log level. Each `-v` raises verbosity, each `-q`
/// lowers it.
fn extract_verbosity(arguments: &mut Vec<String>) -> crate::logging::Level {
    use crate::logging::Level;
    let mut verbosity: i32 = 0;
    arguments.retain(|arg| match arg.as_str() {
        "-v" | "--verbose" => {
            verbosity += 1;
            false
        }
        "-vv" => {
            verbosity += 2;
            false
        }
        "-q" | "--quiet" => {
            verbosity -= 1;
            false
        }
        "-qq" => {
            verbosity -= 2;
            false
        }
        _ => true,
    });
    match verbosity {
        i if i <= -2 => Level::Error,
        -1 => Level::Warn,
        0 => Level::Info,
        1 => Level::Debug,
        _ => Level::Trace,
    }
}

fn dispatch_command(command: &str, arguments: Vec<String>) -> Result<(), AppError> {
    // `<command> --help`/`-h` shows that command's help instead of erroring on an
    // unexpected flag. The `help`/`version` verbs handle their own arguments.
    if !matches!(command, "help" | "--help" | "-h" | "version" | "--version" | "-V")
        && arguments.iter().any(|arg| arg == "--help" || arg == "-h")
    {
        print_command_help(command);
        return Ok(());
    }
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
        "recover" => handle_recover_command(arguments), // literal: allow external interface text or file-format spelling
        "init" => handle_init_command(arguments), // literal: allow external interface text or file-format spelling
        "config-init" => handle_config_init_command(arguments), // literal: allow external interface text or file-format spelling
        "profiles" => handle_profiles_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-new" => handle_profile_new_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-use" => handle_profile_use_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-copy" => handle_profile_copy_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-rename" => handle_profile_rename_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-delete" => handle_profile_delete_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-args" => handle_profile_args_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-root" => handle_profile_root_command(arguments), // literal: allow external interface text or file-format spelling
        "profile-root-target" => handle_profile_root_target_command(arguments), // literal: allow external interface text or file-format spelling
        "game" => handle_game_command(arguments), // literal: allow external interface text or file-format spelling
        "logs" => handle_logs_command(arguments), // literal: allow external interface text or file-format spelling
        /* literal: allow external interface text or file-format spelling */
        /* literal: allow external interface text or file-format spelling */
        "help" | "--help" | "-h" => {
            // literal: allow external interface text or file-format spelling
            match arguments.first() {
                Some(subcommand) => print_command_help(subcommand),
                None => print_usage(),
            }
            Ok(())
        }
        "version" | "--version" | "-V" => {
            // literal: allow external interface text or file-format spelling
            print_version();
            Ok(())
        }
        _ => {
            let message = format!("unknown command `{command}`");
            Err(AppError::Usage(message))
        }
    }
}

fn handle_scan_command(arguments: Vec<String>) -> Result<(), AppError> {
    let roots: Vec<PathBuf> = if arguments.is_empty() {
        default_mod_roots()
    } else {
        arguments.into_iter().map(PathBuf::from).collect()
    };
    if roots.is_empty() {
        return Err(usage_error(
            "no mod roots to scan; pass one or more folders, or set `mod_roots` in the config (config-init)",
        ));
    }
    scan_roots(&roots)
}

fn default_mod_roots() -> Vec<PathBuf> {
    crate::settings::default_mod_roots()
}

fn handle_ui_command(arguments: Vec<String>) -> Result<(), AppError> {
    let game_root = optional_explicit_game_root(arguments);
    crate::ui::run_ui(game_root)
}

/// Like [`optional_game_root_from_raw_arguments`], but returns `None` when the
/// user gave no folder, so the UI can fall back to the last-used folder rather
/// than always forcing the compiled default.
fn optional_explicit_game_root(arguments: Vec<String>) -> Option<PathBuf> {
    let mut cursor = CliArguments::new(arguments);
    match cursor.next_optional() {
        Some(flag) if flag == "--game" => cursor.next_optional().map(PathBuf::from),
        Some(path) => Some(PathBuf::from(path)),
        None => None,
    }
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
    let profile = ProfileJson {
        name: profile_name,
        ..Default::default()
    };
    write_profile_json(&game_root, &profile)
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
    // Profile is the first non-flag token; when omitted, use the active profile.
    let mut profile = None;
    let mut rest = Vec::new();
    for arg in arguments {
        if profile.is_none() && !arg.starts_with("--") {
            profile = Some(arg);
        } else {
            rest.push(arg);
        }
    }
    let game_root = parse_game_root_from_arguments(rest, default_game_root_path())?;
    let profile_name = match profile {
        Some(profile) => safe_name(&profile),
        None => read_active_profile(&game_root),
    };
    prepare_run(&game_root, &profile_name)
}

fn handle_profile_use_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let name = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    set_active_profile(&game_root, &safe_name(&name))
}

fn handle_profile_copy_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let source = cursor.required("source profile name")?; // literal: allow external interface text or file-format spelling
    let dest = cursor.required("destination profile name")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    copy_profile(&game_root, &safe_name(&source), &dest)
}

fn handle_profile_rename_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let old_name = cursor.required("current profile name")?; // literal: allow external interface text or file-format spelling
    let new_name = cursor.required("new profile name")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    rename_profile(&game_root, &safe_name(&old_name), &new_name)
}

fn handle_profile_delete_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let name = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    delete_profile(&game_root, &safe_name(&name))
}

fn handle_profile_args_command(arguments: Vec<String>) -> Result<(), AppError> {
    // Optional `--game <path>` must lead; the profile name follows, then every
    // remaining token becomes a launch argument (so args may start with `-`).
    let mut iter = arguments.into_iter();
    let mut game_root = default_game_root_path();
    let mut next = iter.next();
    if next.as_deref() == Some("--game") {
        // literal: allow external interface text or file-format spelling
        let value = iter.next().ok_or_else(|| usage_error("--game requires a path"))?; // literal: allow external interface text or file-format spelling
        game_root = PathBuf::from(value);
        next = iter.next();
    }
    let profile = next.ok_or_else(|| usage_error("missing profile name"))?; // literal: allow external interface text or file-format spelling
    let launch_args: Vec<String> = iter.collect();
    set_profile_launch_args(&game_root, &safe_name(&profile), launch_args)
}

fn handle_profile_root_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let mod_id = cursor.required("mod id")?; // literal: allow external interface text or file-format spelling
    let source = cursor.required("install root source")?; // literal: allow external interface text or file-format spelling
    let state = cursor.required("on or off")?; // literal: allow external interface text or file-format spelling
    let enabled = parse_on_off(&state)?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    set_profile_root_override(&game_root, &safe_name(&profile), &safe_name(&mod_id), &source, enabled)
}

fn handle_profile_root_target_command(arguments: Vec<String>) -> Result<(), AppError> {
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?; // literal: allow external interface text or file-format spelling
    let mod_id = cursor.required("mod id")?; // literal: allow external interface text or file-format spelling
    let source = cursor.required("install root source")?; // literal: allow external interface text or file-format spelling
    let target = cursor.required("target path")?; // literal: allow external interface text or file-format spelling
    let game_root = game_root_from_cli_arguments(cursor)?;
    set_profile_root_target(&game_root, &safe_name(&profile), &safe_name(&mod_id), &source, &target)
}

fn parse_on_off(value: &str) -> Result<bool, AppError> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "true" | "enable" | "enabled" | "yes" => Ok(true),
        "off" | "false" | "disable" | "disabled" | "no" => Ok(false),
        _ => Err(usage_error("expected on or off")),
    }
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

fn handle_recover_command(arguments: Vec<String>) -> Result<(), AppError> {
    let game_root = optional_game_root_from_raw_arguments(arguments);
    match recover_interrupted_install(&game_root)? {
        Some(txid) => println!("recovered interrupted install {txid}: game folder restored"),
        None => println!("no interrupted install to recover under {}", game_root.display()),
    }
    Ok(())
}

fn handle_init_command(arguments: Vec<String>) -> Result<(), AppError> {
    let game_root = optional_game_root_from_raw_arguments(arguments);
    init_state(&game_root)
}

fn handle_config_init_command(_arguments: Vec<String>) -> Result<(), AppError> {
    let path = write_example_config()?;
    println!("wrote example config: {}", path.display());
    println!("edit it to set default game root, mod roots, 7-Zip path, and detection rules");
    Ok(())
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

fn handle_logs_command(arguments: Vec<String>) -> Result<(), AppError> {
    let game_root = optional_game_root_from_raw_arguments(arguments);
    let log_path = state_directory(&game_root)
        .join("logs")
        .join("sa-mod-manager.log");
    println!("log: {}", log_path.display());
    match fs::read_to_string(&log_path) {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(40);
            if start > 0 {
                println!("... ({} earlier lines)", start);
            }
            for line in &lines[start..] {
                println!("{line}");
            }
        }
        Err(_) => println!("(no diagnostics logged yet for this game folder)"),
    }
    Ok(())
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
    crate::settings::default_game_root()
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
    println!("sa-mod-manager {}", env!("CARGO_PKG_VERSION"));
    println!();
    print_command_usage();
    print_plan_option_usage();
    print_global_option_usage();
    print_default_usage();
}

fn print_version() {
    println!("sa-mod-manager {}", env!("CARGO_PKG_VERSION"));
}

fn print_global_option_usage() {
    println!("Global options (any position):");
    println!("  -v, --verbose                    More diagnostics (repeat/-vv for trace)");
    println!("  -q, --quiet                      Fewer diagnostics (-qq for errors only)");
    println!("  -h, --help                       Show this help");
    println!("  -V, --version                    Print the version and exit");
    println!("  --config <path>                  Use this config file (overrides SA_MOD_MANAGER_CONFIG)");
    println!("  Diagnostics also append to <game-root>/.sa-mod-manager/logs/sa-mod-manager.log");
    println!();
}

/// Single source of truth for the command list. Both the full usage screen and
/// per-command help (`help <cmd>`, `<cmd> --help`) render from this, so there is
/// no second table to keep in sync.
const COMMAND_USAGE_LINES: &[&str] = &[
    "  ui [game-root]                   Open the desktop manager UI",
    "  config-init                      Write an example user config (paths + detection rules)",
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
    "  profile-remove <profile> <mod>   Remove a mod from a profile",
    "  prepare-run [profile] [--game]   Materialize a profile (default: active) into the game folder",
    "  cleanup-run <journal> [--game]   Remove temporary materialized files",
    "  extract-stage <package> [options] Extract package into managed staging only",
    "  rollback <journal> [--game path] Restore a recorded install transaction",
    "  recover [game-root]              Roll back an interrupted permanent install",
];

fn print_command_usage() {
    println!("Commands:");
    for line in COMMAND_USAGE_LINES {
        println!("{line}");
    }
    println!();
}

/// Help for a single command: its synopsis plus the option groups that apply.
/// Falls back to a pointer to full help when the command is unknown.
fn print_command_help(command: &str) {
    println!("sa-mod-manager {command}");
    println!();
    let matches: Vec<&&str> = COMMAND_USAGE_LINES
        .iter()
        .filter(|line| command_line_name(line) == command)
        .collect();
    if matches.is_empty() {
        println!("Unknown command `{command}`. Run `sa-mod-manager help` for all commands.");
        return;
    }
    for line in matches {
        println!("{line}");
    }
    println!();
    print_plan_option_usage();
    print_global_option_usage();
}

/// The command token a usage line describes (first word after the indent).
fn command_line_name(line: &str) -> &str {
    line.split_whitespace().next().unwrap_or("")
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
    // Resolved defaults after config file + env overrides, not just the
    // compiled-in fallbacks.
    let configured_roots = default_mod_roots();
    let mod_roots = if configured_roots.is_empty() {
        "(none set — pass folders to `scan` or set mod_roots in the config)".to_string()
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
    if let Some(path) = crate::settings::config_file_path() {
        println!("  config    : {} (run `config-init` to create)", path.display());
    }
}
