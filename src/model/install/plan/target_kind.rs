use crate::prelude::*;

#[derive(Clone, Copy)]
pub(crate) enum TargetKind {
    ModLoader,
    /// CLEO scripts (`.cs`/`.cs4`/`.cs3`) that live directly in `CLEO/`.
    Cleo,
    /// CLEO text (`.fxt`) that belongs in `CLEO/cleo_text/`.
    CleoText,
    /// CLEO5 plugin modules (`.cleo`) that load from `CLEO/cleo_plugins/`.
    CleoPlugin,
    /// CLEO script modules reached via the `modules:` path prefix, in
    /// `CLEO/cleo_modules/`.
    CleoModules,
    /// CLEO per-script save data in `CLEO/cleo_saves/` — runtime user data.
    CleoSaves,
    Asi,
    Bootstrap,
    DirectManaged,
}

impl fmt::Display for TargetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetKind::ModLoader => write!(f, "modloader"),
            TargetKind::Cleo => write!(f, "cleo"),
            TargetKind::CleoText => write!(f, "cleo_text"),
            TargetKind::CleoPlugin => write!(f, "cleo_plugins"),
            TargetKind::CleoModules => write!(f, "cleo_modules"),
            TargetKind::CleoSaves => write!(f, "cleo_saves"),
            TargetKind::Asi => write!(f, "asi"),
            TargetKind::Bootstrap => write!(f, "bootstrap"),
            TargetKind::DirectManaged => write!(f, "direct-managed"),
        }
    }
}
