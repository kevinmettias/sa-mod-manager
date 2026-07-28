#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiTab {
    Home,
    Profiles,
    Mods,
    Content,
    ModLoader,
    Import,
    Run,
    Telemetry,
}

impl UiTab {
    /// Stable identifier used when persisting the last-open tab between sessions.
    pub(crate) fn as_key(self) -> &'static str {
        match self {
            UiTab::Home => "home",
            UiTab::Profiles => "profiles",
            UiTab::Mods => "mods",
            UiTab::Content => "content",
            UiTab::ModLoader => "modloader",
            UiTab::Import => "import",
            UiTab::Run => "run",
            UiTab::Telemetry => "telemetry",
        }
    }

    /// Restore a tab from its persisted key, ignoring anything unrecognized.
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "home" => Some(UiTab::Home),
            "profiles" => Some(UiTab::Profiles),
            "mods" => Some(UiTab::Mods),
            "content" => Some(UiTab::Content),
            "modloader" => Some(UiTab::ModLoader),
            "import" => Some(UiTab::Import),
            "run" => Some(UiTab::Run),
            "telemetry" => Some(UiTab::Telemetry),
            _ => None,
        }
    }
}
