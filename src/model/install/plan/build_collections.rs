use crate::prelude::*;

pub(crate) struct BuildCollections<'a> {
    pub(crate) operations: &'a mut Vec<InstallOperation>,
    pub(crate) warnings: &'a mut Vec<String>,
}
