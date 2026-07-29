use crate::prelude::*;

pub(crate) struct InfrastructureItem
{
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) present: bool,
}
