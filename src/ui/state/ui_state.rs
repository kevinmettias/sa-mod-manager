use crate::prelude::*;

use super::{InfrastructureItem, ModConfigItem};

#[derive(Default)]
pub(crate) struct UiState
{
    pub(crate) profiles: Vec<String>,
    pub(crate) selected_profile: Option<ProfileJson>,
    pub(crate) mods: Vec<ModConfigItem>,
    pub(crate) infrastructure: Vec<InfrastructureItem>,
}
