#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModActivation {
    Enabled,
    Disabled,
}

impl ModActivation {
    pub(crate) fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Enabled => "enabled", // literal: allow external interface text or file-format spelling
            Self::Disabled => "disabled", // literal: allow external interface text or file-format spelling
        }
    }
}
