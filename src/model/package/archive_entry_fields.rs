use crate::prelude::*;

#[derive(Default)]
pub(crate) struct ArchiveEntryFields
{
    pub(crate) path: Option<String>,
    pub(crate) size: u64,
    pub(crate) is_dir: bool,
}

impl ArchiveEntryFields
{
    pub(crate) fn flush_into(&mut self, entries: &mut Vec<PackageEntry>)
    {
        let Some(path) = self.path.take() else {
            return;
        };
        if path.is_empty() || path.contains(":\\")
        {
            return;
        }
        entries.push(PackageEntry {
            path,
            size: self.size,
            is_dir: self.is_dir,
        });
    }
}
