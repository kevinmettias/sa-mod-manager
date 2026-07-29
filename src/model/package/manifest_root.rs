use crate::prelude::*;

pub(crate) struct ManifestInstallRoot
{
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) kind: TargetKind,
    pub(crate) optional: bool,
    pub(crate) enabled: bool,
    pub(crate) notes: Vec<String>,
}
