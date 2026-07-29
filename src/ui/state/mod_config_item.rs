use crate::prelude::*;

#[derive(Clone)]
pub(crate) struct ModConfigItem
{
    pub(crate) path: PathBuf,
    pub(crate) config: ModConfigJson,
    pub(crate) in_selected_profile: bool,
}
