use crate::prelude::*;

/// A rule that classifies a package path into a [`Component`]. Owned strings so
/// rules can come from the built-in set *or* a user config file at runtime.
pub(crate) struct ComponentRule {
    pub(crate) component: Component,
    pub(crate) contains: Vec<String>,
    pub(crate) prefixes: Vec<String>,
    pub(crate) suffixes: Vec<String>,
}
