pub(crate) struct CandidateDetection<'a>
{
    pub(crate) path: &'a str,
    pub(crate) lower: &'a str,
    pub(crate) parts: &'a [&'a str],
    pub(crate) original_parts: &'a [&'a str],
    pub(crate) size: u64,
}
