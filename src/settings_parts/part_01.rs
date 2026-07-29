use crate::prelude::*;
use std::sync::OnceLock;

/// User-overridable settings, resolved once per process from (in precedence
/// order) environment variables, a JSON config file, then compiled-in defaults.
/// Detection rules from the config file *extend* the built-in rule set.
pub(crate) struct Settings
{
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

/// Record a `--config` path chosen on the command line. Must be called before the
/// first settings access; a later call is ignored (settings resolve only once).
pub(crate) fn set_cli_config_path(path: PathBuf)
{
    if CLI_CONFIG_PATH.set(path).is_err()
    {
        log_warn!("ignored duplicate CLI config path");
    }
}

/// Record a `--7z` path chosen on the command line; wins over env and config.
pub(crate) fn set_cli_seven_zip(path: PathBuf)
{
    if CLI_SEVEN_ZIP.set(path).is_err()
    {
        log_warn!("ignored duplicate CLI 7-Zip path");
    }
}

/// Record `--mod-roots` scan folders chosen on the command line; wins over env
/// and config. An empty list is ignored so the flag cannot blank the roots.
pub(crate) fn set_cli_mod_roots(roots: Vec<PathBuf>)
{
    if !roots.is_empty()
    {
        if CLI_MOD_ROOTS.set(roots).is_err()
        {
            log_warn!("ignored duplicate CLI mod roots");
        }
    }
}

/// Confidence at/above which a readme copy is applied automatically on import.
pub(crate) fn readme_auto_confidence() -> f32
{
    return settings().readme_auto_confidence;
}

/// Confidence at/above which a readme instruction is shown for human review.
pub(crate) fn readme_review_confidence() -> f32
{
    return settings().readme_review_confidence;
}

/// How often the UI polls pending ephemeral runs for cleanup.
pub(crate) fn pending_run_watch_interval() -> Duration
{
    return Duration::from_secs(settings().pending_run_watch_secs);
}

/// The pair of readme-confidence thresholds, resolved together so plan-building
/// and UI classification can be driven by explicit values. This gives callers an
/// injection seam: production resolves `from_settings`, while tests pass explicit
/// thresholds and stay independent of the machine's config file.
#[derive(Clone, Copy)]
pub(crate) struct ReadmeThresholds
{
    pub(crate) auto: f32,
    pub(crate) review: f32,
}

impl ReadmeThresholds
{
    pub(crate) fn from_settings() -> Self
    {
        return Self {
            auto: readme_auto_confidence(),
            review: readme_review_confidence(),
        };
    }
}

impl Default for ReadmeThresholds
{
    fn default() -> Self
    {
        return Self {
            auto: DEFAULT_README_AUTO_CONFIDENCE,
            review: DEFAULT_README_REVIEW_CONFIDENCE,
        };
    }
}

/// The Mod Loader folder name for a mod: `<order-prefix>_<package-id>`.
pub(crate) fn modloader_folder_name(package_id: &str) -> String
{
    return format!("{}_{package_id}", settings().modloader_order_prefix);
}

/// User-configured `(label, relative-path)` infrastructure checks that extend the
/// built-in status-panel checks.
pub(crate) fn extra_infrastructure_checks() -> &'static [(String, String)]
{
    return &settings().infrastructure_checks;
}

/// The fallback game root when nothing was explicitly configured: the GTA install
/// this manager lives inside (found by walking up from our own executable, then
/// the working directory), else the working directory as a last resort.
fn resolved_default_game_root() -> PathBuf
{
    return detect_game_root().unwrap_or_else(|| env::current_dir().unwrap_or_default());
}

fn detect_game_root() -> Option<PathBuf>
{
    let mut starts = Vec::new();
    if let Ok(exe) = env::current_exe()
    {
        starts.push(exe);
    }
    if let Ok(cwd) = env::current_dir()
    {
        starts.push(cwd);
    }
    return starts.iter().find_map(|start| game_root_at_or_above(start));
}

/// Nearest ancestor of `start` (inclusive) that holds a GTA San Andreas
/// executable, or `None` if none of them do.
fn game_root_at_or_above(start: &Path) -> Option<PathBuf>
{
    let mut dir = Some(start);
    while let Some(candidate) = dir
    {
        if game_executable_path(candidate).is_some()
        {
            return Some(candidate.to_path_buf());
        }
        dir = candidate.parent();
    }
    return None;
}

pub(crate) fn default_game_root() -> PathBuf
{
    return settings().game_root.clone();
}

pub(crate) fn default_mod_roots() -> Vec<PathBuf>
{
    if let Some(cli) = CLI_MOD_ROOTS.get()
    {
        return cli.clone();
    }
    return settings().mod_roots.clone();
}

/// Launch arguments to use when a profile declares none of its own.
pub(crate) fn default_launch_args() -> &'static [String]
{
    return &settings().default_launch_args;
}

/// Launch environment applied as a base under each profile's own launch env.
pub(crate) fn default_launch_env() -> &'static BTreeMap<String, String>
{
    return &settings().default_launch_env;
}

/// The configured console log level, used when no `-v`/`-q` flag is present.
pub(crate) fn configured_log_level() -> crate::logging::Level
{
    return settings().log_level;
}

/// The profile to fall back to when no active profile has been selected.
pub(crate) fn configured_default_profile() -> String
{
    return settings().default_profile.clone();
}

pub(crate) fn configured_seven_zip() -> Option<PathBuf>
{
    if let Some(cli) = CLI_SEVEN_ZIP.get()
    {
        return Some(cli.clone());
    }
    return settings().seven_zip.clone();
}

pub(crate) fn component_rules() -> &'static [ComponentRule]
{
    return &settings().component_rules;
}

pub(crate) fn context_rules() -> &'static [ContextRule]
{
    return &settings().context_rules;
}

/// The config file path: `--config` if supplied, else `SA_MOD_MANAGER_CONFIG`,
/// else a per-user location under the platform config directory.
pub(crate) fn config_file_path() -> Option<PathBuf>
{
    if let Some(cli) = CLI_CONFIG_PATH.get()
    {
        return Some(cli.clone());
    }
    if let Some(explicit) = env_var_nonempty("SA_MOD_MANAGER_CONFIG")
    {
        return Some(PathBuf::from(explicit));
    }
    let base = env_var_nonempty("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env_var_nonempty("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| env_var_nonempty("HOME").map(|home| PathBuf::from(home).join(".config")))
        .or_else(|| {
            env_var_nonempty("USERPROFILE").map(|home| PathBuf::from(home).join(".config"))
        })?;
    return Some(base.join("sa-mod-manager").join("config.json"));
}

/// Write a documented example config to the resolved path, via serde so the file
/// round-trips exactly with the reader. Returns the path written.
pub(crate) fn write_example_config() -> Result<PathBuf, AppError>
{
    let path = config_file_path()
        .ok_or_else(|| AppError::Usage("could not determine a config file location".to_string()))?;
    if let Some(parent) = path.parent()
    {
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
        default_launch_env: BTreeMap::from([("SA_EXAMPLE".to_string(), "1".to_string())]),
        log_level: Some("info".to_string()),
        default_profile: Some("default".to_string()),
    };
    let text = serde_json::to_string_pretty(&example)
        .map_err(|err| AppError::Tool(format!("failed to serialize config: {err}")))?;
    fs::write(&path, format!("{text}\n"))?;
    return Ok(path);
}

/// Mod-scan roots to show in the generated example config: the resolved defaults
/// if any are configured, otherwise a single illustrative placeholder so the user
/// has something concrete to edit rather than an empty list.
fn example_mod_roots() -> Vec<String>
{
    let configured = default_mod_roots();
    if configured.is_empty()
    {
        return vec!["C:\\Mods\\GTA San Andreas".to_string()];
    }
    return configured
        .iter()
        .map(|root| root.display().to_string())
        .collect();
}

fn settings() -> &'static Settings
{
    return SETTINGS.get_or_init(|| {
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
    });
}

fn warn_if_configured_game_root_invalid(game_root: Option<&Path>)
{
    let Some(root) = game_root else {
        return;
    };
    if game_executable_path(root).is_none()
    {
        log_warn!(
            "configured game root `{}` has no gta_sa.exe/gta-sa.exe; fix SA_MOD_MANAGER_GAME_ROOT or game_root in the config, or clear it to auto-detect",
            root.display()
        );
    }
}

/// Warn about any *explicitly* configured scan root that does not exist, so a
/// typo in `mod_roots` surfaces at startup instead of silently scanning nothing.
fn warn_if_configured_mod_roots_missing(mod_roots: Option<&[PathBuf]>)
{
    let Some(roots) = mod_roots else {
        return;
    };
    for root in roots
    {
        if !root.exists()
        {
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
/// left alone â€” it is resolved on `PATH` later.
fn warn_if_configured_seven_zip_missing(seven_zip: Option<&Path>)
{
    let Some(path) = seven_zip else {
        return;
    };
    let is_bare_name = path
        .parent()
        .map(|parent| parent.as_os_str().is_empty())
        .unwrap_or(true);
    if is_bare_name
    {
        return;
    }
    if !path.is_file()
    {
        log_warn!(
            "configured 7-Zip `{}` does not exist; fix --7z, SA_MOD_MANAGER_7Z, or seven_zip in the config",
            path.display()
        );
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SettingsFile
{
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
struct InfrastructureCheckFile
{
    label: String,
    path: String,
}

#[derive(Serialize, Deserialize)]
struct ComponentRuleFile
{
    component: String,
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    prefixes: Vec<String>,
    #[serde(default)]
    suffixes: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ContextRuleFile
{
    #[serde(default)]
    aliases: Vec<String>,
    hint: String,
}

struct EnvOverrides
{
    game_root: Option<PathBuf>,
    mod_roots: Option<Vec<PathBuf>>,
    seven_zip: Option<PathBuf>,
}

