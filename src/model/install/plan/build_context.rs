use crate::prelude::*;

pub(crate) struct BuildContext<'a>
{
    pub(crate) report: &'a PackageReport,
    pub(crate) options: &'a CommandOptions,
    pub(crate) package_id: &'a str,
    pub(crate) operations: &'a mut Vec<InstallOperation>,
    pub(crate) warnings: &'a mut Vec<String>,
}
