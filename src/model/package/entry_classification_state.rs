use crate::prelude::*;

#[derive(Default)]
pub(crate) struct EntryClassificationState {
    pub(crate) readmes: Vec<String>,
    pub(crate) components: BTreeSet<Component>,
    pub(crate) option_roots: BTreeSet<String>,
    pub(crate) context_hints: BTreeSet<String>,
    pub(crate) risks: BTreeSet<String>,
}
