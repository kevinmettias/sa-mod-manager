use crate::prelude::*;

#[derive(Clone, Default, Debug)]
pub(crate) struct Json {
    pub(crate) name: String,
    pub(crate) mods: Vec<ProfileModEntry>,
    /// Extra command-line arguments passed to the game executable on launch.
    pub(crate) launch_args: Vec<String>,
    /// Extra environment variables set for the game process on launch.
    pub(crate) launch_env: BTreeMap<String, String>,
    /// Unmodeled JSON fields, preserved verbatim so hand-added profile data
    /// survives a manager edit instead of being silently dropped.
    pub(crate) extra: BTreeMap<String, serde_json::Value>,
    /// An explicit `game_root` from the profile file, if one was set. Retained so
    /// a portable profile's game root survives a manager edit instead of being
    /// overwritten with whatever install the manager is currently operating in.
    pub(crate) game_root_override: Option<PathBuf>,
}
