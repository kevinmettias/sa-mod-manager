/// A rule that emits a compatibility hint when a package path contains any
/// alias. Owned string_list so hints can be extended from a user config file.
pub(crate) struct ContextRule
{
    pub(crate) aliases: Vec<String>,
    pub(crate) hint: String,
}
