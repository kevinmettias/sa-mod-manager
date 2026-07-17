use crate::prelude::*;

pub(crate) struct CandidateMetadata {
    pub(crate) components: BTreeSet<Component>,
    pub(crate) notes: BTreeSet<String>,
}
