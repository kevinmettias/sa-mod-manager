
fn env_overrides() -> EnvOverrides
{
    return EnvOverrides {
        game_root: env_var_nonempty("SA_MOD_MANAGER_GAME_ROOT").map(PathBuf::from),
        mod_roots: env_mod_roots(),
        seven_zip: env_var_nonempty("SA_MOD_MANAGER_7Z").map(PathBuf::from),
    };
}

fn env_var_nonempty(key: &str) -> Option<String>
{
    return match env::var(key)
    {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    };
}

fn env_mod_roots() -> Option<Vec<PathBuf>>
{
    let value = env::var_os("SA_MOD_MANAGER_MOD_ROOTS")?;
    let roots: Vec<PathBuf> = env::split_paths(&value)
        .filter(|path| !path.as_os_str().is_empty())
        .collect();
    return if roots.is_empty() { None } else
    {
        Some(roots) };
}

fn resolve_settings(file: SettingsFile, env: EnvOverrides) -> Settings
{
    let game_root = env
        .game_root
        .or_else(|| file.game_root.as_deref().map(PathBuf::from))
        .unwrap_or_else(resolved_default_game_root);
    // No universal default scan location exists, so an unconfigured mod-roots list
    // is empty rather than a personal path; `scan` asks for one when it is.
    let mod_roots = env
        .mod_roots
        .or_else(|| {
            file.mod_roots
                .as_ref()
                .map(|roots| roots.iter().map(PathBuf::from).collect())
        })
        .unwrap_or_default();
    let seven_zip = env
        .seven_zip
        .or_else(|| file.seven_zip.as_deref().map(PathBuf::from));

    let mut component_rules = builtin_component_rules();
    component_rules.extend(
        file.component_rules
            .into_iter()
            .filter_map(component_rule_from_file),
    );
    let mut context_rules = builtin_context_rules();
    context_rules.extend(file.context_rules.into_iter().map(context_rule_from_file));

    // Behavioral tunables: clamp confidences into [0,1] and require a positive
    // watch interval, so a malformed config degrades to something sane.
    let readme_auto_confidence = file
        .readme_auto_confidence
        .map(|value| value.clamp(0.0, 1.0))
        .unwrap_or(DEFAULT_README_AUTO_CONFIDENCE);
    // The review threshold must not exceed the auto threshold, or an inverted
    // config (auto below review) would make the "needs review" band unreachable.
    let readme_review_confidence = file
        .readme_review_confidence
        .map(|value| value.clamp(0.0, 1.0))
        .unwrap_or(DEFAULT_README_REVIEW_CONFIDENCE)
        .min(readme_auto_confidence);
    let pending_run_watch_secs = file
        .pending_run_watch_secs
        .filter(|&secs| secs > 0)
        .unwrap_or(DEFAULT_PENDING_RUN_WATCH_SECS);
    let modloader_order_prefix = file
        .modloader_order_prefix
        .unwrap_or(DEFAULT_MODLOADER_ORDER_PREFIX);
    let infrastructure_checks = file
        .infrastructure_checks
        .into_iter()
        .filter(|check| !check.label.trim().is_empty() && !check.path.trim().is_empty())
        .map(|check| (check.label, check.path))
        .collect();

    return Settings {
        game_root,
        mod_roots,
        seven_zip,
        component_rules,
        context_rules,
        readme_auto_confidence,
        readme_review_confidence,
        pending_run_watch_secs,
        modloader_order_prefix,
        infrastructure_checks,
        default_launch_args: file.default_launch_args,
        default_launch_env: file.default_launch_env,
        // An unrecognized log level falls back to Info rather than failing.
        log_level: file
            .log_level
            .as_deref()
            .and_then(crate::logging::Level::from_name)
            .unwrap_or(crate::logging::Level::Info),
        // A blank default profile name resolves to the built-in "default".
        default_profile: file
            .default_profile
            .map(|name| safe_name(&name))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "default".to_string()),
    };
}

fn read_config_file() -> SettingsFile
{
    let Some(path) = config_file_path() else {
        return SettingsFile::default();
    };
    let Ok(text) = read_capped(&path, MAX_CONTROL_FILE_BYTES) else {
        return SettingsFile::default();
    };
    return match serde_json::from_str(&text)
    {
        Ok(file) => file,
        Err(err) => {
            log_warn!("ignoring invalid config {}: {err}", path.display());
            SettingsFile::default()
        }
    };
}

fn component_rule_from_file(rule: ComponentRuleFile) -> Option<ComponentRule>
{
    let component = component_from_key(&rule.component)?;
    return Some(ComponentRule {
        component,
        contains: rule.contains,
        prefixes: rule.prefixes,
        suffixes: rule.suffixes,
    });
}

fn component_from_key(key: &str) -> Option<Component>
{
    return match key.trim().to_ascii_lowercase().as_str()
    {
        "modloader" => Some(Component::ModLoader),
        "modloader_content" | "modloader-content" => Some(Component::ModLoaderContent),
        "cleo" => Some(Component::Cleo),
        "cleo_text" | "cleo-text" => Some(Component::CleoText),
        "cleo_plugin" | "cleo-plugin" | "cleo_plugins" => Some(Component::CleoPlugin),
        "cleo_module" | "cleo-module" | "cleo_modules" => Some(Component::CleoModules),
        "cleo_save" | "cleo-save" | "cleo_saves" => Some(Component::CleoSaves),
        "asi" => Some(Component::Asi),
        "img" | "img_replacement" | "img-replacement" => Some(Component::ImgReplacement),
        "script_data" | "script-data" => Some(Component::ScriptData),
        "data" => Some(Component::Data),
        "models" => Some(Component::Models),
        "text" => Some(Component::Text),
        "anim" => Some(Component::Anim),
        "audio" => Some(Component::Audio),
        _ => None,
    };
}

fn context_rule_from_file(rule: ContextRuleFile) -> ContextRule
{
    return ContextRule {
        aliases: rule.aliases,
        hint: rule.hint,
    };
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn env_beats_config_beats_default_for_game_root()
    {
        // default: with nothing configured, the game root is auto-detected (or the
        // working directory), i.e. exactly what `resolved_default_game_root` returns.
        let s = resolve_settings(SettingsFile::default(), empty_env());
        assert_eq!(s.game_root, resolved_default_game_root());

        // config overrides default
        let file = SettingsFile {
            game_root: Some(r"D:\Games\GTA SA".to_string()),
            ..Default::default()
        };
        let s = resolve_settings(file, empty_env());
        assert_eq!(s.game_root, PathBuf::from(r"D:\Games\GTA SA"));

        // env overrides config
        let file = SettingsFile {
            game_root: Some(r"D:\Games\GTA SA".to_string()),
            ..Default::default()
        };
        let env = EnvOverrides {
            game_root: Some(PathBuf::from(r"E:\GTA")),
            ..empty_env()
        };
        let s = resolve_settings(file, env);
        assert_eq!(s.game_root, PathBuf::from(r"E:\GTA"));
    }

    #[test]
    fn config_mod_roots_override_defaults()
    {
        let file = SettingsFile {
            mod_roots: Some(vec!["A".to_string(), "B".to_string()]),
            ..Default::default()
        };
        let s = resolve_settings(file, empty_env());
        assert_eq!(s.mod_roots, vec![PathBuf::from("A"), PathBuf::from("B")]);
    }

    #[test]
    fn unconfigured_mod_roots_default_to_empty()
    {
        // No personal path is baked in: an unconfigured scan list is empty, and
        // the CLI turns that into a prompt rather than scanning someone's drive.
        let s = resolve_settings(SettingsFile::default(), empty_env());
        assert!(s.mod_roots.is_empty());
    }

    #[test]
    fn detect_game_root_walks_up_to_the_install_folder()
    {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-detect-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let install = base.join("Grand Theft Auto San Andreas");
        let nested = install.join("sa-mod-manager").join("target").join("debug");
        fs::create_dir_all(&nested).expect("the test fixture is created before this assertion reads it");
        fs::write(install.join("gta_sa.exe"), b"").expect("the test fixture is created before this assertion reads it");

        // Walking up from a deeply nested start finds the folder with the exe;
        // a start with no GTA install above it yields nothing.
        assert_eq!(game_root_at_or_above(&nested), Some(install.clone()));
        assert_eq!(game_root_at_or_above(base.parent().expect("the test fixture is created before this assertion reads it")), None);
        fs::remove_dir_all(&base).expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn config_rules_extend_builtins_and_classify_custom_patterns()
    {
        let builtin_components = builtin_component_rules().len();
        let file = SettingsFile {
            component_rules: vec![ComponentRuleFile {
                component: "cleo".to_string(),
                contains: Vec::new(),
                prefixes: Vec::new(),
                suffixes: vec![".cs4".to_string()],
            }],
            context_rules: vec![ContextRuleFile {
                aliases: vec!["myenb".to_string()],
                hint: "custom".to_string(),
            }],
            ..Default::default()
        };
        let s = resolve_settings(file, empty_env());

        assert_eq!(s.component_rules.len(), builtin_components + 1);
        // The appended rule classifies a pattern the built-ins would miss.
        let custom = s.component_rules.last().expect("the test fixture is created before this assertion reads it");
        assert_eq!(custom.component, Component::Cleo);
        assert!(custom.suffixes.iter().any(|suffix| suffix == ".cs4"));
        assert!(s.context_rules.iter().any(|rule| rule.hint == "custom"));
    }

    #[test]
    fn log_level_and_default_profile_default_and_normalize()
    {
        // Unset → Info level and the built-in "default" profile.
        let s = resolve_settings(SettingsFile::default(), empty_env());
        assert_eq!(s.log_level, crate::logging::Level::Info);
        assert_eq!(s.default_profile, "default");

        // An unrecognized log level falls back to Info; the profile name is
        // normalized via safe_name.
        let file = SettingsFile {
            log_level: Some("nonsense".to_string()),
            default_profile: Some("My Profile!".to_string()),
            ..Default::default()
        };
        let s = resolve_settings(file, empty_env());
        assert_eq!(s.log_level, crate::logging::Level::Info);
        assert_eq!(s.default_profile, safe_name("My Profile!"));
    }

    #[test]
    fn unknown_component_key_is_dropped()
    {
        let file = SettingsFile {
            component_rules: vec![ComponentRuleFile {
                component: "not-a-component".to_string(),
                contains: vec!["x".to_string()],
                prefixes: Vec::new(),
                suffixes: Vec::new(),
            }],
            ..Default::default()
        };
        let s = resolve_settings(file, empty_env());
        assert_eq!(s.component_rules.len(), builtin_component_rules().len());
    }

    #[test]
    fn example_config_round_trips_through_serde()
    {
        let example = SettingsFile {
            game_root: Some("G".to_string()),
            mod_roots: Some(vec!["M".to_string()]),
            seven_zip: None,
            readme_auto_confidence: Some(0.9), // literal: allow test fixture value is the specimen under judgment
            readme_review_confidence: Some(0.5), // literal: allow test fixture value is the specimen under judgment
            pending_run_watch_secs: Some(5), // literal: allow test fixture value is the specimen under judgment
            modloader_order_prefix: Some(200), // literal: allow test fixture value is the specimen under judgment
            component_rules: vec![ComponentRuleFile {
                component: "asi".to_string(),
                contains: Vec::new(),
                prefixes: Vec::new(),
                suffixes: vec![".asi2".to_string()],
            }],
            context_rules: Vec::new(),
            infrastructure_checks: vec![InfrastructureCheckFile {
                label: "ASI loader (dinput8)".to_string(),
                path: "dinput8.dll".to_string(),
            }],
            default_launch_args: vec!["-nointro".to_string()],
            default_launch_env: BTreeMap::from([("K".to_string(), "V".to_string())]),
            log_level: Some("debug".to_string()),
            default_profile: Some("racing".to_string()),
        };
        let text = serde_json::to_string_pretty(&example).expect("the test fixture is created before this assertion reads it");
        let parsed: SettingsFile = serde_json::from_str(&text).expect("the test fixture is created before this assertion reads it");
        let resolved = resolve_settings(parsed, empty_env());
        assert_eq!(resolved.game_root, PathBuf::from("G"));
        assert_eq!(resolved.mod_roots, vec![PathBuf::from("M")]);
        assert_eq!(
            resolved.component_rules.len(),
            builtin_component_rules().len() + 1
        );
        // Behavioral tunables round-trip through the config too.
        assert!((resolved.readme_auto_confidence - 0.9).abs() < f32::EPSILON); // literal: allow test fixture value is the specimen under judgment
        assert!((resolved.readme_review_confidence - 0.5).abs() < f32::EPSILON); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(resolved.pending_run_watch_secs, 5); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(resolved.modloader_order_prefix, 200); // literal: allow test fixture value is the specimen under judgment
        // Configured infrastructure checks round-trip and drop blank entries.
        assert_eq!(
            resolved.infrastructure_checks,
            vec![(
                "ASI loader (dinput8)".to_string(),
                "dinput8.dll".to_string()
            )]
        );
        // Default launch args/env round-trip through the config too.
        assert_eq!(resolved.default_launch_args, vec!["-nointro".to_string()]);
        assert_eq!(
            resolved.default_launch_env.get("K").map(String::as_str),
            Some("V")
        );
        // Log level and default profile round-trip and normalize.
        assert_eq!(resolved.log_level, crate::logging::Level::Debug);
        assert_eq!(resolved.default_profile, "racing");
    }

    #[test]
    fn behavioral_tunables_default_and_clamp()
    {
        // Defaults when unset.
        let s = resolve_settings(SettingsFile::default(), empty_env());
        assert!((s.readme_auto_confidence - DEFAULT_README_AUTO_CONFIDENCE).abs() < f32::EPSILON);
        assert_eq!(s.pending_run_watch_secs, DEFAULT_PENDING_RUN_WATCH_SECS);
        assert_eq!(s.modloader_order_prefix, DEFAULT_MODLOADER_ORDER_PREFIX);

        // Out-of-range confidence is clamped; a zero interval falls back to default.
        let file = SettingsFile {
            readme_auto_confidence: Some(9.0), // literal: allow test fixture value is the specimen under judgment
            readme_review_confidence: Some(-1.0),
            pending_run_watch_secs: Some(0),
            ..Default::default()
        };
        let s = resolve_settings(file, empty_env());
        assert!((s.readme_auto_confidence - 1.0).abs() < f32::EPSILON);
        assert!((s.readme_review_confidence - 0.0).abs() < f32::EPSILON);
        assert_eq!(s.pending_run_watch_secs, DEFAULT_PENDING_RUN_WATCH_SECS);

        // An inverted config (review above auto) is pulled down to auto so the
        // needs-review band never becomes unreachable.
        let inverted = SettingsFile {
            readme_auto_confidence: Some(0.4), // literal: allow test fixture value is the specimen under judgment
            readme_review_confidence: Some(0.9), // literal: allow test fixture value is the specimen under judgment
            ..Default::default()
        };
        let s = resolve_settings(inverted, empty_env());
        assert!(s.readme_review_confidence <= s.readme_auto_confidence);
        assert!((s.readme_review_confidence - 0.4).abs() < f32::EPSILON); // literal: allow test fixture value is the specimen under judgment
    }

    fn empty_env() -> EnvOverrides
    {
        return EnvOverrides {
            game_root: None,
            mod_roots: None,
            seven_zip: None,
        };
    }
}
