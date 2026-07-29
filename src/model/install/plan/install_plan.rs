use crate::prelude::*;

pub(crate) struct InstallPlan
{
    pub(crate) package_id: String,
    pub(crate) package: PathBuf,
    pub(crate) game_root: PathBuf,
    pub(crate) profile: String,
    pub(crate) operations: Vec<InstallOperation>,
    pub(crate) skipped_options: Vec<String>,
    pub(crate) warnings: Vec<String>,
}
