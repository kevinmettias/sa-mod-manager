use crate::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Component
{
    ModLoader,
    ModLoaderContent,
    Cleo,
    CleoText,
    CleoPlugin,
    CleoModules,
    CleoSaves,
    Asi,
    ImgReplacement,
    ScriptData,
    Data,
    Models,
    Text,
    Anim,
    Audio,
}

impl fmt::Display for Component
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        return match self {
            Component::ModLoader => write!(f, "modloader runtime"),
            Component::ModLoaderContent => write!(f, "modloader content"),
            Component::Cleo => write!(f, "CLEO"),
            Component::CleoText => write!(f, "CLEO text"),
            Component::CleoPlugin => write!(f, "CLEO plugin"),
            Component::CleoModules => write!(f, "CLEO module"),
            Component::CleoSaves => write!(f, "CLEO save data"),
            Component::Asi => write!(f, "ASI plugin"),
            Component::ImgReplacement => write!(f, "IMG/DFF/TXD replacement"),
            Component::ScriptData => write!(f, "script data"),
            Component::Data => write!(f, "data files"),
            Component::Models => write!(f, "models"),
            Component::Text => write!(f, "text/GXT"),
            Component::Anim => write!(f, "animation"),
            Component::Audio => write!(f, "audio"),
        };
    }
}
