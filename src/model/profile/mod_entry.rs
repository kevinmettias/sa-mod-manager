use crate::prelude::*;

#[derive(Clone)]
pub(crate) struct ModEntry {
    pub(crate) id: String,
    pub(crate) enabled: bool,
    pub(crate) load_order: i32,
    pub(crate) config: PathBuf,
}
