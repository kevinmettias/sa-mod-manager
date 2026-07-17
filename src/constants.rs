use crate::prelude::*;

pub(crate) const DEFAULT_GAME_ROOT: &str =
    r"F:\SteamLibrary\steamapps\common\Grand Theft Auto San Andreas";
pub(crate) const CONTEXT_RULES: &[ContextRule] = &[
    ContextRule {
        aliases: &["rosa"],
        hint: "RoSA compatibility path likely matters",
    },
    ContextRule {
        aliases: &["proper fixes"],
        hint: "Proper Fixes can have RoSA-specific variants",
    },
    ContextRule {
        aliases: &["open limit adjuster"],
        hint: "Limit adjuster should be installed before large model/IMG packs",
    },
    ContextRule {
        aliases: &["improvedstreaming", "improved streaming"],
        hint: "Streaming settings should match texture/model pack size",
    },
    ContextRule {
        aliases: &["sky gfx", "skygfx", "enb"],
        hint: "Graphics pipeline mod; ENB/SkyGfx/DirectX presets may conflict",
    },
    ContextRule {
        aliases: &["cleo+"],
        hint: "Requires CLEO and CLEO+ runtime",
    },
    ContextRule {
        aliases: &["save"],
        hint: "Savegame package; should target user documents, not game root",
    },
    ContextRule {
        aliases: &["gta_sa.exe"],
        hint: "Executable replacement requires explicit bootstrap approval",
    },
];
pub(crate) const COMPONENT_RULES: &[ComponentRule] = &[
    ComponentRule {
        component: Component::ModLoader,
        contains: &["modloader"],
        prefixes: &[],
        suffixes: &[],
    },
    ComponentRule {
        component: Component::Cleo,
        contains: &["cleo"],
        prefixes: &[],
        suffixes: &[".cs", ".cleo"],
    },
    ComponentRule {
        component: Component::Asi,
        contains: &[],
        prefixes: &[],
        suffixes: &[".asi"],
    },
    ComponentRule {
        component: Component::ImgReplacement,
        contains: &["gta3.img"],
        prefixes: &[],
        suffixes: &[".dff", ".txd"],
    },
    ComponentRule {
        component: Component::Data,
        contains: &["/data/"],
        prefixes: &["data/"],
        suffixes: &[],
    },
    ComponentRule {
        component: Component::Models,
        contains: &["/models/"],
        prefixes: &["models/"],
        suffixes: &[],
    },
    ComponentRule {
        component: Component::Text,
        contains: &["/text/"],
        prefixes: &["text/"],
        suffixes: &[".gxt"],
    },
    ComponentRule {
        component: Component::Anim,
        contains: &["/anim/"],
        prefixes: &["anim/"],
        suffixes: &[],
    },
    ComponentRule {
        component: Component::Audio,
        contains: &["/audio/"],
        prefixes: &["audio/"],
        suffixes: &[],
    },
];
pub(crate) const DEFAULT_MOD_ROOTS: [&str; 2] = [r"E:\Mods\GTa Sa", r"E:\Mods\GTa Sa\New"];
