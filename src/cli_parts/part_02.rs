use crate::workspace::ProfileRenameRequest;

fn handle_profile_rename_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let old_name = cursor.required("current profile name")?;
    let new_name = cursor.required("new profile name")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    return rename_profile(&game_root, ProfileRenameRequest { old_name: &safe_name(&old_name), new_name: &safe_profile_name_warned(&new_name) });
}

fn handle_profile_delete_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let name = cursor.required("profile name")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    return delete_profile(&game_root, &safe_name(&name));
}

fn handle_profile_args_command(arguments: Vec<String>) -> Result<(), AppError>
{
    // Optional `--game <path>` must lead; the profile name follows, then every
    // remaining token becomes a launch argument (so args may start with `-`).
    let mut iter = arguments.into_iter();
    let mut game_root = default_game_root_path();
    let mut next = iter.next();
    if next.as_deref() == Some(GAME_FLAG)
    {
        let value = iter
            .next()
            .ok_or_else(|| usage_error(GAME_FLAG_REQUIRES_PATH))?;
        game_root = PathBuf::from(value);
        next = iter.next();
    }
    let profile = next.ok_or_else(|| usage_error("missing profile name"))?;
    let launch_args: Vec<String> = iter.collect();
    return set_profile_launch_args(&game_root, &safe_name(&profile), launch_args);
}

fn handle_profile_root_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?;
    let mod_id = cursor.required("mod id")?;
    let source = cursor.required("install root source")?;
    let state = cursor.required("on or off")?;
    let enabled = parse_on_off(&state)?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let mod_id = safe_name(&mod_id);
    let selector = ProfileRootSelector {
        profile_name: &profile_name,
        mod_id: &mod_id,
        source: &source,
    };
    return set_profile_root_override(&game_root, selector, ProfileRootEnabledState::from_bool(enabled));
}

fn parse_on_off(value: &str) -> Result<bool, AppError>
{
    return match value.to_ascii_lowercase().as_str()
    {
        "on" | "true" | "enable" | "enabled" | "yes" => Ok(true),
        "off" | "false" | "disable" | "disabled" | "no" => Ok(false),
        _ => Err(usage_error("expected on or off")),
    };
}

fn handle_profile_root_target_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?;
    let mod_id = cursor.required("mod id")?;
    let source = cursor.required("install root source")?;
    let target = cursor.required("target path")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let mod_id = safe_name(&mod_id);
    let selector = ProfileRootSelector {
        profile_name: &profile_name,
        mod_id: &mod_id,
        source: &source,
    };
    return set_profile_root_target(&game_root, selector, &target);
}

fn handle_cleanup_run_command(arguments: Vec<String>) -> Result<(), AppError>
{
    return rollback_from_arguments(arguments, "journal path");
}

fn handle_extract_stage_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let package = cursor.required("package path")?;
    let options = options_from_cli_arguments(cursor)?;
    let package_path = PathBuf::from(package);
    return extract_to_staging(&package_path, &options.game_root);
}

fn options_from_cli_arguments(cursor: CliArguments) -> Result<CommandOptions, AppError>
{
    let remaining_arguments = cursor.remaining();
    let fallback_game_root = default_game_root_path();
    return parse_options_from_arguments(remaining_arguments, fallback_game_root);
}

fn parse_options_from_arguments(
    args: Vec<String>,
    fallback_game_root: PathBuf,
) -> Result<CommandOptions, AppError>
{
    let mut options = CommandOptions {
        game_root: fallback_game_root,
        profile: "default".to_string(),
        includes: BTreeSet::new(),
        excludes: BTreeSet::new(),
        write_manifest: false,
    };

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next()
    {
        apply_option_argument(&mut options, &mut iter, &arg)?;
    }

    return Ok(options);
}

fn apply_option_argument(
    options: &mut CommandOptions,
    iter: &mut impl Iterator<Item = String>,
    arg: &str,
) -> Result<(), AppError>
{
    match arg
    {
        GAME_FLAG => {
            let value = next_option_value(iter, GAME_FLAG_REQUIRES_PATH)?;
            options.game_root = PathBuf::from(value);
        }
        PROFILE_FLAG => {
            let value = next_option_value(iter, PROFILE_FLAG_REQUIRES_NAME)?;
            options.profile = safe_name(&value);
        }
        INCLUDE_FLAG => {
            let value = next_option_value(iter, INCLUDE_FLAG_REQUIRES_SOURCE_ROOT)?;
            let source_root = normalize_path(&value);
            options.includes.insert(source_root);
        }
        EXCLUDE_FLAG => {
            let value = next_option_value(iter, EXCLUDE_FLAG_REQUIRES_SOURCE_ROOT)?;
            let source_root = normalize_path(&value);
            options.excludes.insert(source_root);
        }
        MANIFEST_FLAG => options.write_manifest = true,
        other => {
            let message = format!("unexpected argument `{other}`");
            return Err(AppError::Usage(message));
        }
    }
    return Ok(());
}

fn next_option_value(
    iter: &mut impl Iterator<Item = String>,
    missing_message: &str,
) -> Result<String, AppError>
{
    return match iter.next()
    {
        Some(value) => Ok(value),
        None => Err(usage_error(missing_message)),
    };
}

fn handle_rollback_command(arguments: Vec<String>) -> Result<(), AppError>
{
    return rollback_from_arguments(arguments, "journal path");
}

fn handle_recover_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let game_root = optional_game_root_from_raw_arguments(arguments);
    match recover_interrupted_install(&game_root)?
    {
        Some(txid) => println!("recovered interrupted install {txid}: game folder restored"),
        None => println!(
            "no interrupted install to recover under {}",
            game_root.display()
        ),
    }
    return Ok(());
}

fn handle_init_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let game_root = optional_game_root_from_raw_arguments(arguments);
    return init_state(&game_root);
}

fn handle_config_init_command(_arguments: Vec<String>) -> Result<(), AppError>
{
    let path = write_example_config()?;
    println!("wrote example config: {}", path.display());
    println!(
        "edit it to set game root, mod roots, 7-Zip path, detection rules, readme thresholds, watch interval, modloader prefix, infrastructure checks, launch defaults, log level, and default profile"
    );
    return Ok(());
}

fn handle_profiles_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let game_root = optional_game_root_from_raw_arguments(arguments);
    return list_profiles(&game_root);
}

fn handle_profile_new_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let name = cursor.required("profile name")?;
    let game_root = optional_game_root_from_cli_arguments(cursor);
    return create_profile(&game_root, &safe_profile_name_warned(&name));
}

fn optional_game_root_from_cli_arguments(cursor: CliArguments) -> PathBuf
{
    let remaining_arguments = cursor.remaining();
    return optional_game_root_from_raw_arguments(remaining_arguments);
}

fn handle_profile_all_off_command(arguments: Vec<String>) -> Result<(), AppError>
{
    return set_all_profile_mods_from_cli_arguments(arguments, ProfileModActivation::Disabled);
}

fn handle_profile_all_on_command(arguments: Vec<String>) -> Result<(), AppError>
{
    return set_all_profile_mods_from_cli_arguments(arguments, ProfileModActivation::Enabled);
}
fn handle_game_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let game_root = optional_game_root_from_raw_arguments(arguments);
    return inspect_game(&game_root);
}

fn handle_logs_command(arguments: Vec<String>) -> Result<(), AppError>
{
    let game_root = optional_game_root_from_raw_arguments(arguments);
    let log_path = state_directory(&game_root)
        .join("logs")
        .join("sa-mod-manager.log");
    println!("log: {}", log_path.display());
    match fs::read_to_string(&log_path)
    {
        Ok(text) => print_log_tail(&text),
        Err(_) => println!("(no diagnostics logged yet for this game folder)"),
    }
    return Ok(());
}

fn print_log_tail(text: &str)
{
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(40); // literal: allow domain threshold is documented by the surrounding code
    if start > 0
    {
        println!("... ({} earlier lines)", start);
    }
    for line in &lines[start..]
    {
        println!("{line}");
    }
}

pub(crate) fn usage_error(message: &str) -> AppError
{
    let owned_message = message.to_owned();
    return AppError::Usage(owned_message);
}

fn set_profile_activation_from_cli_arguments(
    arguments: Vec<String>,
    activation: ProfileModActivation,
) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?;
    let mod_id = cursor.required("mod id")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let profile_name = safe_name(&profile);
    let safe_mod_id = safe_name(&mod_id);
    return set_profile_mod_activation(&game_root, ProfileModSelection { profile_name: &profile_name, mod_id: &safe_mod_id }, activation);
}

fn print_usage()
{
    println!("sa-mod-manager {}", env!("CARGO_PKG_VERSION"));
    println!();
    print_command_usage();
    print_plan_option_usage();
    print_global_option_usage();
    print_default_usage();
}

fn print_global_option_usage()
{
    println!("Global options (any position):");
    println!("  -v, --verbose                    More diagnostics (repeat/-vv for trace)");
    println!("  -q, --quiet                      Fewer diagnostics (-qq for errors only)");
    println!("  -h, --help                       Show this help");
    println!("  -V, --version                    Print the version and exit");
    println!(
        "  --config <path>                  Use this config file (overrides SA_MOD_MANAGER_CONFIG)"
    );
    println!(
        "  --7z <path>                      Use this 7-Zip executable (overrides SA_MOD_MANAGER_7Z)"
    );
    println!(
        "  --mod-roots <a;b>                Scan roots (overrides SA_MOD_MANAGER_MOD_ROOTS); path-separated"
    );
    println!("  Diagnostics also append to <game-root>/.sa-mod-manager/logs/sa-mod-manager.log");
    println!();
}

fn print_version()
{
    println!("sa-mod-manager {}", env!("CARGO_PKG_VERSION"));
}

fn set_all_profile_mods_from_cli_arguments(
    arguments: Vec<String>,
    activation: ProfileModActivation,
) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let profile = cursor.required("profile name")?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    return set_all_profile_mods(&game_root, &safe_name(&profile), activation);
}

/// Normalize a name and warn (to stderr/log) if it had to be rewritten, so a CLI
/// user is told when e.g. `My Profile!` becomes `my_profile`.
fn safe_profile_name_warned(input: &str) -> String
{
    let result = safe_profile_name(input);
    let safe = result.name;
    let note = result.note;
    if let Some(note) = note
    {
        log_warn!("{note}");
    }
    return safe;
}

fn rollback_from_arguments(arguments: Vec<String>, label: &str) -> Result<(), AppError>
{
    let mut cursor = CliArguments::new(arguments);
    let journal = cursor.required(label)?;
    let game_root = game_root_from_cli_arguments(cursor)?;
    let journal_path = PathBuf::from(journal);
    return rollback_journal(&journal_path, &game_root);
}

fn game_root_from_cli_arguments(cursor: CliArguments) -> Result<PathBuf, AppError>
{
    let remaining_arguments = cursor.remaining();
    let fallback_game_root = default_game_root_path();
    return parse_game_root_from_arguments(remaining_arguments, fallback_game_root);
}

struct CliArguments
{
    values: VecDeque<String>,
}

impl CliArguments
{
    fn new(values: Vec<String>) -> Self
    {
        let values = VecDeque::from(values);
        return Self { values };
    }

    fn required(&mut self, label: &str) -> Result<String, AppError>
    {
        return match self.values.pop_front()
        {
            Some(value) => Ok(value),
            None => {
                let message = format!("missing {label}");
                Err(AppError::Usage(message))
            }
        };
    }

    fn next_optional(&mut self) -> Option<String>
    {
        return self.values.pop_front();
    }

    fn remaining(self) -> Vec<String>
    {
        return self.values.into_iter().collect();
    }
}

fn optional_game_root_from_raw_arguments(arguments: Vec<String>) -> PathBuf
{
    let mut cursor = CliArguments::new(arguments);
    return match cursor.next_optional()
    {
        Some(flag) if flag == GAME_FLAG => cursor
            .next_optional()
            .map(PathBuf::from)
            .unwrap_or_else(default_game_root_path),
        Some(path) => PathBuf::from(path),
        None => default_game_root_path(),
    };
}

fn default_game_root_path() -> PathBuf
{
    return crate::settings::default_game_root();
}

fn parse_game_root_from_arguments(
    args: Vec<String>,
    fallback: PathBuf,
) -> Result<PathBuf, AppError>
{
    let mut game_root = fallback;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next()
    {
        match arg.as_str()
        {
            GAME_FLAG => {
                let Some(value) = iter.next() else {
                    return Err(usage_error(GAME_FLAG_REQUIRES_PATH));
                };
                game_root = PathBuf::from(value);
            }
            other => {
                let message = format!("unexpected argument `{other}`");
                return Err(AppError::Usage(message));
            }
        }
    }
    return Ok(game_root);
}
