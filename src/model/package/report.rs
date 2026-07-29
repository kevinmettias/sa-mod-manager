use crate::prelude::*;

pub(crate) struct Report
{
    pub(crate) package: PathBuf,
    pub(crate) game_root: PathBuf,
    pub(crate) kind: PackageKind,
    pub(crate) entries: Vec<PackageEntry>,
    pub(crate) readmes: Vec<String>,
    pub(crate) readme_documents: Vec<ReadmeDocument>,
    pub(crate) readme_instructions: Vec<ReadmeInstruction>,
    pub(crate) readme_insights: Vec<ReadmeInsight>,
    pub(crate) manifest_roots: Vec<ManifestInstallRoot>,
    pub(crate) components: BTreeSet<Component>,
    pub(crate) install_candidates: Vec<InstallCandidate>,
    pub(crate) option_groups: Vec<String>,
    pub(crate) context_hints: BTreeSet<String>,
    pub(crate) risks: BTreeSet<String>,
}
