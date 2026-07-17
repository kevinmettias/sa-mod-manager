use crate::prelude::*;

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Folder,
    Zip,
    SevenZip,
    Rar,
    Wrap,
}

impl Kind {
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        if path.is_dir() {
            return Some(Self::Folder);
        }
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        match ext.as_str() {
            "zip" => Some(Self::Zip), // literal: allow external interface text or file-format spelling
            "7z" => Some(Self::SevenZip), // literal: allow external interface text or file-format spelling
            "rar" => Some(Self::Rar), // literal: allow external interface text or file-format spelling
            "wrap" => Some(Self::Wrap), // literal: allow external interface text or file-format spelling
            _ => None,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Folder => write!(f, "folder"),
            Kind::Zip => write!(f, "zip"),
            Kind::SevenZip => write!(f, "7z"),
            Kind::Rar => write!(f, "rar"),
            Kind::Wrap => write!(f, "wrap"),
        }
    }
}
