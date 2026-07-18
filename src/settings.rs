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
    readme_auto_confidence: f32,
    readme_review_confidence: f32,
    pending_run_watch_secs: u64,
    modloader_order_prefix: u32,
    /// Extra `(label, relative-path)` infrastructure checks from the config,
    /// appended to the built-in set so alternate loaders (e.g. `dinput8.dll`) can
    /// be recognized in the status panel.
    infrastructure_checks: Vec<(String, String)>,
    /// Launch arguments applied to any profile that sets none of its own.
    default_launch_args: Vec<String>,
    /// Launch environment applied under each profile's own env on launch.
    default_launch_env: BTreeMap<String, String>,
    /// Console log level when no `-v`/`-q` flag is given.
    log_level: crate::logging::Level,
    /// Profile used when no active profile has been selected yet.
    default_profile: String,
}

/// Confidence at or above which a readme copy instruction is auto-selected.
const DEFAULT_README_AUTO_CONFIDENCE: f32 = 0.85;
/// Confidence at or above which a readme instruction is surfaced for review.
const DEFAULT_README_REVIEW_CONFIDENCE: f32 = 0.60;
/// How often the UI re-checks pending ephemeral runs for cleanup.
const DEFAULT_PENDING_RUN_WATCH_SECS: u64 = 2;
/// Load-order number prefixed to a mod's Mod Loader folder (`<prefix>_<mod>`).
const DEFAULT_MODLOADER_ORDER_PREFIX: u32 = 100;

static SETTINGS: OnceLock<Settings> = OnceLock::new();
/// A `--config <path>` supplied on the command line. Set once at startup, before
/// any settings are read, so it wins over the env var and the per-user default.
static CLI_CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();
/// A `--7z <path>` supplied on the command line; wins over env and config.
static CLI_SEVEN_ZIP: OnceLock<PathBuf> = OnceLock::new();
/// `--mod-roots <a;b>` supplied on the command line; wins over env and config.
static CLI_MOD_ROOTS: OnceLock<Vec<PathBuf>> = OnceLock::new();

fn settings() -> &'static Settings {
    SETTINGS.get_or_init(|| {
        let env = env_overrides();
        let file = read_config_file();
        // Only an *explicitly* configured (env/config) game root is validated here:
        // a wrong path is worth flagging early, while an auto-detected or fallback
        // root is a best guess that later `ensure_gta_install` checks will catch.
        let explicit_game_root = env
            .game_root
            .clone()
            .or_else(|| file.game_root.as_deref().map(PathBuf::from));
        warn_if_configured_game_root_invalid(explicit_game_root.as_deref());
        let explicit_mod_roots = env.mod_roots.clone().or_else(|| {
            file.mod_roots
                .as_ref()
                .map(|roots| roots.iter().map(PathBuf::from).collect())
        });
        warn_if_configured_mod_roots_missing(explicit_mod_roots.as_deref());
        // Same precedence as `configured_seven_zip`: an explicit path that points
        // nowhere is flagged now, rather than only failing at first .7z/.rar use.
        let explicit_seven_zip = CLI_SEVEN_ZIP
            .get()
            .cloned()
            .or_else(|| env.seven_zip.clone())
            .or_else(|| file.seven_zip.as_deref().map(PathBuf::from));
        warn_if_configured_seven_zip_missing(explicit_seven_zip.as_deref());
        resolve_settings(file, env)
    })
}

/// Record a `--config` path chosen on the command line. Must be called before the
/// first settings access; a later call is ignored (settings resolve only once).
pub(crate) fn set_cli_config_path(path: PathBuf) {
    let _ = CLI_CONFIG_PATH.set(path);
}

/// Record a `--7z` path chosen on the command line; wins over env and config.
pub(crate) fn set_cli_seven_zip(path: PathBuf) {
    let _ = CLI_SEVEN_ZIP.set(path);
}

/// Record `--mod-roots` scan folders chosen on the command line; wins over env
/// and config. An empty list is ignored so the flag cannot blank the roots.
pub(crate) fn set_cli_mod_roots(roots: Vec<PathBuf>) {
    if !roots.is_empty() {
        let _ = CLI_MOD_ROOTS.set(roots);
    }
}

/// Confidence at/above which a readme copy is applied automatically on import.
pub(crate) fn readme_auto_confidence() -> f32 {
    settings().readme_auto_confidence
}

/// Confidence at/above which a readme instruction is shown for human review.
pub(crate) fn readme_review_confidence() -> f32 {
    settings().readme_review_confidence
}

/// The pair of readme-confidence thresholds, resolved together so plan-building
/// and UI classification can be driven by explicit values. This gives callers an
/// injection seam: production resolves `from_settings`, while tests pass explicit
/// thresholds and stay independent of the machine's config file.
#[derive(Clone, Copy)]
pub(crate) struct ReadmeThresholds {
    pub(crate) auto: f32,
    pub(crate) review: f32,
}

impl ReadmeThresholds {
    pub(crate) fn from_settings() -> Self {
        Self {
            auto: readme_auto_confidence(),
            review: readme_review_confidence(),
        }
    }
}

impl Default for ReadmeThresholds {
    fn default() -> Self {
        Self {
            auto: DEFAULT_README_AUTO_CONFIDENCE,
            review: DEFAULT_README_REVIEW_CONFIDENCE,
        }
    }
}

/// How often the UI polls pending ephemeral runs for cleanup.
pub(crate) fn pending_run_watch_interval() -> Duration {
    Duration::from_secs(settings().pending_run_watch_secs)
}

/// The Mod Loader folder name for a mod: `<order-prefix>_<package-id>`.
pub(crate) fn modloader_folder_name(package_id: &str) -> String {
    format!("{}_{package_id}", settings().modloader_order_prefix)
}

/// User-configured `(label, relative-path)` infrastructure checks that extend the
/// built-in status-panel checks.
pub(crate) fn extra_infrastructure_checks() -> &'static [(String, String)] {
    &settings().infrastructure_checks
}

fn warn_if_configured_game_root_invalid(game_root: Option<&Path>) {
    let Some(root) = game_root else {
        return;
    };
    if game_executable_path(root).is_none() {
        log_warn!(
            "configured game root `{}` has no gta_sa.exe/gta-sa.exe; fix SA_MOD_MANAGER_GAME_ROOT or game_root in the config, or clear it to auto-detect",
            root.display()
        );
    }
}

/// Warn about any *explicitly* configured scan root that does not exist, so a
/// typo in `mod_roots` surfaces at startup instead of silently scanning nothing.
fn warn_if_configured_mod_roots_missing(mod_roots: Option<&[PathBuf]>) {
    let Some(roots) = mod_roots else {
        return;
    };
    for root in roots {
        if !root.exists() {
            log_warn!(
                "configured mod root `{}` does not exist; fix mod_roots in the config or SA_MOD_MANAGER_MOD_ROOTS",
                root.display()
            );
        }
    }
}

/// Warn about an *explicitly* configured 7-Zip path that points nowhere, so a
/// stale `--7z`/`SA_MOD_MANAGER_7Z`/config path surfaces at startup instead of
/// only at first `.7z`/`.rar` use. A bare command name (no path separator) is
/// left alone — it is resolved on `PATH` later.
fn warn_if_configured_seven_zip_missing(seven_zip: Option<&Path>) {
    let Some(path) = seven_zip else {
        return;
    };
    let is_bare_name = path
        .parent()
        .map(|parent| parent.as_os_str().is_empty())
        .unwrap_or(true);
    if is_bare_name {
        return;
    }
    if !path.is_file() {
        log_warn!(
            "configured 7-Zip `{}` does not exist; fix --7z, SA_MOD_MANAGER_7Z, or seven_zip in the config",
            path.display()
        );
    }
}

/// The fallback game root when nothing was explicitly configured: the GTA install
/// this manager lives inside (found by walking up from our own executable, then
/// the working directory), else the working directory as a last resort.
fn resolved_default_game_root() -> PathBuf {
    detect_game_root().unwrap_or_else(|| env::current_dir().unwrap_or_default())
}

fn detect_game_root() -> Option<PathBuf> {
    let mut starts = Vec::new();
    if let Ok(exe) = env::current_exe() {
        starts.push(exe);
    }
    if let Ok(cwd) = env::current_dir() {
        starts.push(cwd);
    }
    starts
        .iter()
        .find_map(|start| game_root_at_or_above(start))
}

/// Nearest ancestor of `start` (inclusive) that holds a GTA San Andreas
/// executable, or `None` if none of them do.
fn game_root_at_or_above(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(candidate) = dir {
        if game_executable_path(candidate).is_some() {
            return Some(candidate.to_path_buf());
        }
        dir = candidate.parent();
    }
    None
}

pub(crate) fn default_game_root() -> PathBuf {
    settings().game_root.clone()
}

pub(crate) fn default_mod_roots() -> Vec<PathBuf> {
    if let Some(cli) = CLI_MOD_ROOTS.get() {
        return cli.clone();
    }
    settings().mod_roots.clone()
}

/// Launch arguments to use when a profile declares none of its own.
pub(crate) fn default_launch_args() -> &'static [String] {
    &settings().default_launch_args
}

/// Launch environment applied as a base under each profile's own launch env.
pub(crate) fn default_launch_env() -> &'static BTreeMap<String, String> {
    &settings().default_launch_env
}

/// The configured console log level, used when no `-v`/`-q` flag is present.
pub(crate) fn configured_log_level() -> crate::logging::Level {
    settings().log_level
}

/// The profile to fall back to when no active profile has been selected.
pub(crate) fn configured_default_profile() -> String {
    settings().default_profile.clone()
}

pub(crate) fn configured_seven_zip() -> Option<PathBuf> {
    if let Some(cli) = CLI_SEVEN_ZIP.get() {
        return Some(cli.clone());
    }
    settings().seven_zip.clone()
}

pub(crate) fn component_rules() -> &'static [ComponentRule] {
    &settings().component_rules
}

pub(crate) fn context_rules() -> &'static [ContextRule] {
    &settings().context_rules
}

/// The config file path: `--config` if supplied, else `SA_MOD_MANAGER_CONFIG`,
/// else a per-user location under the platform config directory.
pub(crate) fn config_file_path() -> Option<PathBuf> {
    if let Some(cli) = CLI_CONFIG_PATH.get() {
        return Some(cli.clone());
    }
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
        mod_roots: Some(example_mod_roots()),
        seven_zip: None,
        readme_auto_confidence: Some(DEFAULT_README_AUTO_CONFIDENCE),
        readme_review_confidence: Some(DEFAULT_README_REVIEW_CONFIDENCE),
        pending_run_watch_secs: Some(DEFAULT_PENDING_RUN_WATCH_SECS),
        modloader_order_prefix: Some(DEFAULT_MODLOADER_ORDER_PREFIX),
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
        infrastructure_checks: vec![InfrastructureCheckFile {
            label: "ASI loader (dinput8)".to_string(),
            path: "dinput8.dll".to_string(),
        }],
        default_launch_args: vec!["-nointro".to_string()],
        default_launch_env: BTreeMap::from([(
            "SA_EXAMPLE".to_string(),
            "1".to_string(),
        )]),
        log_level: Some("info".to_string()),
        default_profile: Some("default".to_string()),
    };
    let text = serde_json::to_string_pretty(&example)
        .map_err(|err| AppError::Tool(format!("failed to serialize config: {err}")))?;
    fs::write(&path, format!("{text}\n"))?;
    Ok(path)
}

/// Mod-scan roots to show in the generated example config: the resolved defaults
/// if any are configured, otherwise a single illustrative placeholder so the user
/// has something concrete to edit rather than an empty list.
fn example_mod_roots() -> Vec<String> {
    let configured = default_mod_roots();
    if configured.is_empty() {
        return vec!["C:\\Mods\\GTA San Andreas".to_string()];
    }
    configured
        .iter()
        .map(|root| root.display().to_string())
        .collect()
}

#[derive(Serialize, Deserialize, Default)]
struct SettingsFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    game_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mod_roots: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seven_zip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    readme_auto_confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    readme_review_confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_run_watch_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modloader_order_prefix: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    component_rules: Vec<ComponentRuleFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    context_rules: Vec<ContextRuleFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    infrastructure_checks: Vec<InfrastructureCheckFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    default_launch_args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    default_launch_env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    log_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_profile: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct InfrastructureCheckFile {
    label: String,
    path: String,
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
    component_rules.extend(file.component_rules.into_iter().filter_map(component_rule_from_file));
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

    Settings {
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
    fn config_mod_roots_override_defaults() {
        let file = SettingsFile {
            mod_roots: Some(vec!["A".to_string(), "B".to_string()]),
            ..Default::default()
        };
        let s = resolve_settings(file, empty_env());
        assert_eq!(s.mod_roots, vec![PathBuf::from("A"), PathBuf::from("B")]);
    }

    #[test]
    fn unconfigured_mod_roots_default_to_empty() {
        // No personal path is baked in: an unconfigured scan list is empty, and
        // the CLI turns that into a prompt rather than scanning someone's drive.
        let s = resolve_settings(SettingsFile::default(), empty_env());
        assert!(s.mod_roots.is_empty());
    }

    #[test]
    fn detect_game_root_walks_up_to_the_install_folder() {
        let base = env::temp_dir().join(format!(
            "sa-mod-manager-detect-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let install = base.join("Grand Theft Auto San Andreas");
        let nested = install.join("sa-mod-manager").join("target").join("debug");
        fs::create_dir_all(&nested).unwrap();
        fs::write(install.join("gta_sa.exe"), b"").unwrap();

        // Walking up from a deeply nested start finds the folder with the exe;
        // a start with no GTA install above it yields nothing.
        assert_eq!(game_root_at_or_above(&nested), Some(install.clone()));
        assert_eq!(game_root_at_or_above(base.parent().unwrap()), None);
        fs::remove_dir_all(&base).unwrap();
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
    fn log_level_and_default_profile_default_and_normalize() {
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
            readme_auto_confidence: Some(0.9),
            readme_review_confidence: Some(0.5),
            pending_run_watch_secs: Some(5),
            modloader_order_prefix: Some(200),
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
        let text = serde_json::to_string_pretty(&example).unwrap();
        let parsed: SettingsFile = serde_json::from_str(&text).unwrap();
        let resolved = resolve_settings(parsed, empty_env());
        assert_eq!(resolved.game_root, PathBuf::from("G"));
        assert_eq!(resolved.mod_roots, vec![PathBuf::from("M")]);
        assert_eq!(
            resolved.component_rules.len(),
            builtin_component_rules().len() + 1
        );
        // Behavioral tunables round-trip through the config too.
        assert!((resolved.readme_auto_confidence - 0.9).abs() < f32::EPSILON);
        assert!((resolved.readme_review_confidence - 0.5).abs() < f32::EPSILON);
        assert_eq!(resolved.pending_run_watch_secs, 5);
        assert_eq!(resolved.modloader_order_prefix, 200);
        // Configured infrastructure checks round-trip and drop blank entries.
        assert_eq!(
            resolved.infrastructure_checks,
            vec![("ASI loader (dinput8)".to_string(), "dinput8.dll".to_string())]
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
    fn behavioral_tunables_default_and_clamp() {
        // Defaults when unset.
        let s = resolve_settings(SettingsFile::default(), empty_env());
        assert!((s.readme_auto_confidence - DEFAULT_README_AUTO_CONFIDENCE).abs() < f32::EPSILON);
        assert_eq!(s.pending_run_watch_secs, DEFAULT_PENDING_RUN_WATCH_SECS);
        assert_eq!(s.modloader_order_prefix, DEFAULT_MODLOADER_ORDER_PREFIX);

        // Out-of-range confidence is clamped; a zero interval falls back to default.
        let file = SettingsFile {
            readme_auto_confidence: Some(9.0),
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
            readme_auto_confidence: Some(0.4),
            readme_review_confidence: Some(0.9),
            ..Default::default()
        };
        let s = resolve_settings(inverted, empty_env());
        assert!(s.readme_review_confidence <= s.readme_auto_confidence);
        assert!((s.readme_review_confidence - 0.4).abs() < f32::EPSILON);
    }
}
