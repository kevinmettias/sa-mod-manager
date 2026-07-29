use crate::prelude::*;

#[derive(Clone)]
pub(crate) struct CommandOptions
{
    pub(crate) game_root: PathBuf,
    pub(crate) profile: String,
    pub(crate) includes: BTreeSet<String>,
    pub(crate) excludes: BTreeSet<String>,
    pub(crate) write_manifest: bool,
}
