use crate::prelude::*;

pub(crate) struct InstallOperation {
    pub(crate) source_root: String,
    pub(crate) target_kind: TargetKind,
    pub(crate) target_root: PathBuf,
    pub(crate) file_count: usize,
    pub(crate) total_bytes: u64,
    pub(crate) notes: Vec<String>,
    pub(crate) optional: bool,
}
