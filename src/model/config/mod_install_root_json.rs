use crate::prelude::*;

#[derive(Clone, Serialize)]
pub(crate) struct ModInstallRootJson {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) kind: String,
    pub(crate) enabled: bool,
    pub(crate) optional: bool,
}
