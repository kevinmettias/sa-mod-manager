use crate::prelude::*;

pub(crate) struct CompletedPlanParts
{
    pub(crate) package_id: String,
    pub(crate) operations: Vec<InstallOperation>,
    pub(crate) warnings: Vec<String>,
}
