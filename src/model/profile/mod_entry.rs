use crate::prelude::*;

#[derive(Clone, Default)]
pub(crate) struct ModEntry {
    pub(crate) id: String,
    pub(crate) enabled: bool,
    pub(crate) load_order: i32,
    pub(crate) config: PathBuf,
    /// Per-profile overrides of this mod's install-root enablement, keyed by the
    /// root's `source`. Lets one profile turn a shared mod's roots on or off
    /// without editing the mod's global config.
    pub(crate) root_overrides: BTreeMap<String, bool>,
}
