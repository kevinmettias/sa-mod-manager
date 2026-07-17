use crate::prelude::*;

#[derive(Clone)]
pub(crate) struct ModConfigJson {
    pub(crate) id: String,
    pub(crate) package: PathBuf,
    pub(crate) source_root: Option<PathBuf>,
    pub(crate) enabled: bool,
    pub(crate) install_roots: Vec<ModInstallRootJson>,
}
