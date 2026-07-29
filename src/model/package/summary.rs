use crate::prelude::*;

pub(crate) struct Summary
{
    pub(crate) count: usize,
    pub(crate) total_bytes: u64,
    pub(crate) by_kind: BTreeMap<String, usize>,
}
