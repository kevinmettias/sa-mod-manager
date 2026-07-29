use crate::prelude::*;

pub(crate) struct CandidateMetadata
{
    pub(crate) component_paths: BTreeSet<Component>,
    pub(crate) notes: BTreeSet<String>,
}
