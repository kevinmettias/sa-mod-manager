use crate::prelude::*;

pub(crate) struct InstallCandidate {
    pub(crate) source_root: String,
    pub(crate) target_strategy: String,
    pub(crate) file_count: usize,
    pub(crate) total_bytes: u64,
    pub(crate) components: BTreeSet<Component>,
    pub(crate) notes: BTreeSet<String>,
}
