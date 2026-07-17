use crate::prelude::*;

#[derive(Clone, Copy)]
pub(crate) enum TargetKind {
    ModLoader,
    Cleo,
    Asi,
    Bootstrap,
    DirectManaged,
}

impl fmt::Display for TargetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetKind::ModLoader => write!(f, "modloader"),
            TargetKind::Cleo => write!(f, "cleo"),
            TargetKind::Asi => write!(f, "asi"),
            TargetKind::Bootstrap => write!(f, "bootstrap"),
            TargetKind::DirectManaged => write!(f, "direct-managed"),
        }
    }
}
