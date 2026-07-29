use crate::prelude::*;

#[derive(Clone, Default, Debug)]
pub(crate) struct ModEntry
{
    pub(crate) id: String,
    pub(crate) enabled: bool,
    pub(crate) load_order: i32,
    pub(crate) config: PathBuf,
    /// Per-profile overrides of this mod's install roots, keyed by the root's
    /// `source`. A profile can disable a root or retarget it to a different game
    /// folder without editing the mod's shared config.
    pub(crate) root_overrides: BTreeMap<String, RootOverride>,
}

/// A profile's override of one of a mod's install roots. `None` fields fall back
/// to the mod config's own value for that root.
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct RootOverride
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
}
