use crate::prelude::*;

pub(crate) struct ApplyState {
    pub(crate) txid: String,
    pub(crate) journal_path: PathBuf,
    pub(crate) backup_root: PathBuf,
}
