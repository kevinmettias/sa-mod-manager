use crate::prelude::*;

#[derive(Clone, Default)]
pub(crate) struct Json {
    pub(crate) name: String,
    pub(crate) mods: Vec<ProfileModEntry>,
    /// Extra command-line arguments passed to the game executable on launch.
    pub(crate) launch_args: Vec<String>,
    /// Extra environment variables set for the game process on launch.
    pub(crate) launch_env: BTreeMap<String, String>,
}
