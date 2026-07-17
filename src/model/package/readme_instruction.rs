use crate::prelude::*;

pub(crate) struct ReadmeInstruction {
    pub(crate) source_readme: String,
    pub(crate) line_number: usize,
    pub(crate) action: ReadmeAction,
    pub(crate) source: Option<String>,
    pub(crate) target: Option<String>,
    pub(crate) confidence: f32,
    pub(crate) text: String,
    pub(crate) normalized_text: String,
    pub(crate) confidence_reasons: Vec<String>,
}

pub(crate) enum ReadmeAction {
    Copy,
    Requires,
    Optional,
    Conflict,
    LoadAfter,
    DoNotInstall,
}

impl fmt::Display for ReadmeAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadmeAction::Copy => write!(f, "copy"),
            ReadmeAction::Requires => write!(f, "requires"),
            ReadmeAction::Optional => write!(f, "optional"),
            ReadmeAction::Conflict => write!(f, "conflict"),
            ReadmeAction::LoadAfter => write!(f, "load-after"),
            ReadmeAction::DoNotInstall => write!(f, "do-not-install"),
        }
    }
}
