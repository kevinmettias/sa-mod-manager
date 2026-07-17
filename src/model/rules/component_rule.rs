use crate::prelude::*;

pub(crate) struct ComponentRule {
    pub(crate) component: Component,
    pub(crate) contains: &'static [&'static str],
    pub(crate) prefixes: &'static [&'static str],
    pub(crate) suffixes: &'static [&'static str],
}
