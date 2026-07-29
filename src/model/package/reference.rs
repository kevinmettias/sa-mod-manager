use crate::prelude::*;

#[derive(Clone)]
pub(crate) struct Reference
{
    pub(crate) path: PathBuf,
    pub(crate) kind: PackageKind,
    pub(crate) size: u64,
}
