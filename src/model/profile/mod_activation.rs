#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModActivation
{
    Enabled,
    Disabled,
}

impl ModActivation
{
    pub(crate) fn is_enabled(self) -> bool
    {
        return matches!(self, Self::Enabled);
    }

    pub(crate) fn label(self) -> &'static str
    {
        return match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        };
    }
}
