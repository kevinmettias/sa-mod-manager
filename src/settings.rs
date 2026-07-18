use crate::prelude::*;
use std::sync::OnceLock;

/// User-overridable settings, resolved once per process from (in precedence
/// order) environment variables, a JSON config file, then compiled-in defaults.
/// Detection rules from the config file *extend* the built-in rule set.
pub(crate) struct Settings {
    game_root: PathBuf,
    mod_roots: Vec<PathBuf>,
    seven_zip: Option<PathBuf>,
    component_rules: Vec<ComponentRule>,
    context_rules: Vec<ContextRule>,
}

static SETTINGS: OnceLock<Settings> = OnceLock::new();

fn settings() -> &'static Settings {
    SETTINGS.get_or_init(|| resolve_settings(read_config_file(), env_overrides()))
}

pub(crate) fn default_game_root() -> PathBuf {
    settings().game_root.clone()
}

pub(crate) fn default_mod_roots() -> Vec<PathBuf> {
    settings().mod_roots.clone()
}

pub(crate) fn configured_seven_zip() -> Option<PathBuf> {
    settings().seven_zip.clone()
}

pub(crate) fn component_rules() -> &'static [ComponentRule] {
    &settings().component_rules
}

pub(crate) fn context_rules() -> &'static [ContextRule] {
    &settings().context_rules
}

/// The config file path: `SA_MOD_MANAGER_CONFIG` if set, else a per-user
/// location under the platform config directory.
pub(crate) fn config_file_path() -> Option<PathBuf> {
    if let Some(explicit) = env_var_nonempty("SA_MOD_MANAGER_CONFIG") {
        return Some(PathBuf::from(explicit));
    }
    let base = env_var_nonempty("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env_var_nonempty("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| env_var_nonempty("HOME").map(|home| PathBuf::from(home).join(".config")))
        .or_else(|| {
            env_var_nonempty("USERPROFILE").map(|home| PathBuf::from(home).join(".config"))
        })?;
    Some(base.join("sa-mod-manager").join("config.json"))
}

/// Write a documented example config to the resolved path, via serde so the file
/// round-trips exactly with the reader. Returns the path written.
pub(crate) fn write_example_config() -> Result<PathBuf, AppError> {
    let path = config_file_path()
        .ok_or_else(|| AppError::Usage("could not determine a config file location".to_string()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let example = SettingsFile {
        game_root: Some(default_game_root().display().to_string()),
        mod_roots: Some(
            default_mod_roots()
                .iter()
                .map(|root| root.display().to_string())
                .collect(),
        ),
        seven_zip: None,
        component_rules: vec![ComponentRuleFile {
            component: "cleo".to_string(),
            contains: vec!["cleoplus".to_string()],
            prefixes: Vec::new(),
            suffixes: vec![".cs4".to_string()],
        }],
        context_rules: vec![ContextRuleFile {
            aliases: vec!["myenb".to_string()],
            hint: "Custom ENB preset; back up your existing enbseries".to_string(),
        }],
    };
    let text = serde_json::to_string_pretty(&example)
        .map_err(|err| AppError::Tool(format!("failed to serialize config: {err}")))?;
    fs::write(&path, format!("{text}\n"))?;
    Ok(path)
}

#[derive(Serialize, Deserialize, Default)]
struct SettingsFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    game_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mod_roots: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seven_zip: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    component_rules: Vec<ComponentRuleFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    context_rules: Vec<ContextRuleFile>,
}

#[derive(Serialize, Deserialize)]
struct ComponentRuleFile {
    component: String,
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    prefixes: Vec<String>,
    #[serde(default)]
    suffixes: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ContextRuleFile {
    #[serde(default)]
    aliases: Vec<String>,
    hint: String,
}

struct EnvOverrides {
    game_root: Option<PathBuf>,
    mod_roots: Option<Vec<PathBuf>>,
    seven_zip: Option<PathBuf>,
}

fn env_overrides() -> EnvOverrides {
    EnvOverrides {
        game_root: env_var_nonempty("SA_MOD_MANAGER_GAME_ROOT").map(PathBuf::from),
        mod_roots: env_mod_roots(),
        seven_zip: env_var_nonempty("SA_MOD_MANAGER_7Z").map(PathBuf::from),
    }
}

fn resolve_settings(file: SettingsFile, env: EnvOverrides) -> Settings {
    let game_root = env
        .game_root
        .or_else(|| file.game_root.as_deref().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_GAME_ROOT));
    let mod_roots = env
        .mod_roots
        .or_else(|| {
            file.mod_roots
                .as_ref()
                .map(|roots| roots.iter().map(PathBuf::from).collect())
        })
        .unwrap_or_else(|| DEFAULT_MOD_ROOTS.iter().map(PathBuf::from).collect());
    let seven_zip = env
        .seven_zip
        .or_else(|| file.seven_zip.as_deref().map(PathBuf::from));

    let mut component_rules = builtin_component_rules();
    component_rules.extend(file.component_rules.into_iter().filter_map(component_rule_from_file));
    let mut context_rules = builtin_context_rules();
    context_rules.extend(file.context_rules.into_iter().map(context_rule_from_file));

    Settings {
        game_root,
        mod_roots,
        seven_zip,
        component_rules,
        context_rules,
    }
}

fn read_config_file() -> SettingsFile {
    let Some(path) = config_file_path() else {
        return SettingsFile::default();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return SettingsFile::default();
    };
    match serde_json::from_str(&text) {
        Ok(file) => file,
        Err(err) => {
            log_warn!("ignoring invalid config {}: {err}", path.display());
            SettingsFile::default()
        }
    }
}

fn component_rule_from_file(rule: ComponentRuleFile) -> Option<ComponentRule> {
    let component = component_from_key(&rule.component)?;
    Some(ComponentRule {
        component,
        contains: rule.contains,
        prefixes: rule.prefixes,
        suffixes: rule.suffixes,
    })
}

fn context_rule_from_file(rule: ContextRuleFile) -> ContextRule {
    ContextRule {
        aliases: rule.aliases,
        hint: rule.hint,
    }
}

fn component_from_key(key: &str) -> Option<Component> {
    match key.trim().to_ascii_lowercase().as_str() {
        "modloader" => Some(Component::ModLoader),
        "modloader_content" | "modloader-content" => Some(Component::ModLoaderContent),
        "cleo" => Some(Component::Cleo),
        "cleo_text" | "cleo-text" => Some(Component::CleoText),
        "asi" => Some(Component::Asi),
        "img" | "img_replacement" | "img-replacement" => Some(Component::ImgReplacement),
        "script_data" | "script-data" => Some(Component::ScriptData),
        "data" => Some(Component::Data),
        "models" => Some(Component::Models),
        "text" => Some(Component::Text),
        "anim" => Some(Component::Anim),
        "audio" => Some(Component::Audio),
        _ => None,
    }
}

fn env_var_nonempty(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

fn env_mod_roots() -> Option<Vec<PathBuf>> {
    let value = env::var_os("SA_MOD_MANAGER_MOD_ROOTS")?;
    let roots: Vec<PathBuf> = env::split_paths(&value)
        .filter(|path| !path.as_os_str().is_empty())
        .collect();
    if roots.is_empty() {
        None
    } else {
        Some(roots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_env() -> EnvOverrides {
        EnvOverrides {
            game_root: None,
            mod_roots: None,
            seven_zip: None,
        }
    }

    #[test]
    fn env_beats_config_beats_default_for_game_root() {
        // default
        let s = resolve_settings(SettingsFile::default(), empty_env());
        assert_eq!(s.game_root, PathBuf::from(DEFAULT_GAME_ROOT));

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
    fn config_mod_roots_override_defaults() {
        let file = SettingsFile {
            mod_roots: Some(vec!["A".to_string(), "B".to_string()]),
            ..Default::default()
        };
        let s = resolve_settings(file, empty_env());
        assert_eq!(s.mod_roots, vec![PathBuf::from("A"), PathBuf::from("B")]);
    }

    #[test]
    fn config_rules_extend_builtins_and_classify_custom_patterns() {
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
        let custom = s.component_rules.last().unwrap();
        assert_eq!(custom.component, Component::Cleo);
        assert!(custom.suffixes.iter().any(|suffix| suffix == ".cs4"));
        assert!(s.context_rules.iter().any(|rule| rule.hint == "custom"));
    }

    #[test]
    fn unknown_component_key_is_dropped() {
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
    fn example_config_round_trips_through_serde() {
        let example = SettingsFile {
            game_root: Some("G".to_string()),
            mod_roots: Some(vec!["M".to_string()]),
            seven_zip: None,
            component_rules: vec![ComponentRuleFile {
                component: "asi".to_string(),
                contains: Vec::new(),
                prefixes: Vec::new(),
                suffixes: vec![".asi2".to_string()],
            }],
            context_rules: Vec::new(),
        };
        let text = serde_json::to_string_pretty(&example).unwrap();
        let parsed: SettingsFile = serde_json::from_str(&text).unwrap();
        let resolved = resolve_settings(parsed, empty_env());
        assert_eq!(resolved.game_root, PathBuf::from("G"));
        assert_eq!(resolved.mod_roots, vec![PathBuf::from("M")]);
        assert_eq!(
            resolved.component_rules.len(),
            builtin_component_rules().len() + 1
        );
    }
}
