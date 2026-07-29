use crate::prelude::*;

pub(crate) struct ReadmeInstruction
{
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

pub(crate) struct ReadmeInsight
{
    pub(crate) kind: ReadmeInsightKind,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) source_readme: String,
    pub(crate) line_number: usize,
    pub(crate) evidence: String,
    pub(crate) confidence: f32,
    pub(crate) rule_id: String,
}

pub(crate) enum ReadmeInsightKind
{
    Recipe,
    OptionSet,
    Dependency,
    Conflict,
    Layout,
    Rule,
    DryRun,
    Override,
    Language,
}

pub(crate) enum ReadmeAction
{
    Copy,
    Requires,
    Optional,
    Conflict,
    LoadAfter,
    DoNotInstall,
}

impl fmt::Display for ReadmeAction
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        return match self {
            ReadmeAction::Copy => write!(f, "copy"),
            ReadmeAction::Requires => write!(f, "requires"),
            ReadmeAction::Optional => write!(f, "optional"),
            ReadmeAction::Conflict => write!(f, "conflict"),
            ReadmeAction::LoadAfter => write!(f, "load-after"),
            ReadmeAction::DoNotInstall => write!(f, "do-not-install"),
        };
    }
}

impl fmt::Display for ReadmeInsightKind
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        return match self {
            ReadmeInsightKind::Recipe => write!(f, "recipe"),
            ReadmeInsightKind::OptionSet => write!(f, "option-set"),
            ReadmeInsightKind::Dependency => write!(f, "dependency"),
            ReadmeInsightKind::Conflict => write!(f, "conflict"),
            ReadmeInsightKind::Layout => write!(f, "layout"),
            ReadmeInsightKind::Rule => write!(f, "rule"),
            ReadmeInsightKind::DryRun => write!(f, "dry-run"),
            ReadmeInsightKind::Override => write!(f, "override"),
            ReadmeInsightKind::Language => write!(f, "language"),
        };
    }
}
