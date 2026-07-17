use crate::prelude::*;

#[derive(Clone)]
pub(crate) struct Json {
    pub(crate) name: String,
    pub(crate) mods: Vec<ProfileModEntry>,
}
