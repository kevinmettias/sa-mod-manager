use crate::prelude::*;

pub(crate) struct InstallApplyState {
    pub(crate) txid: String,
    pub(crate) journal_path: PathBuf,
    pub(crate) backup_root: PathBuf,
    pub(crate) staging_root: PathBuf,
}
