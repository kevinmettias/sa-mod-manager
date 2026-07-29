use crate::prelude::*;

pub(crate) struct CopyJournalContext<'a>
{
    pub(crate) game_root: &'a Path,
    pub(crate) backup_root: &'a Path,
    pub(crate) journal: &'a mut fs::File,
}
