use crate::prelude::*;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// The built-in component-classification rules. Users can add more via the
/// config file; those are appended to these (see [`crate::settings`]).
pub(crate) fn builtin_component_rules() -> Vec<ComponentRule> {
    vec![
        ComponentRule {
            component: Component::ModLoader,
            contains: strings(&["modloader"]),
            prefixes: strings(&[]),
            suffixes: strings(&[]),
        },
        ComponentRule {
            component: Component::Cleo,
            contains: strings(&["cleo"]),
            prefixes: strings(&[]),
            suffixes: strings(&[".cs", ".cleo"]),
        },
        ComponentRule {
            component: Component::Asi,
            contains: strings(&[]),
            prefixes: strings(&[]),
            suffixes: strings(&[".asi"]),
        },
        ComponentRule {
            component: Component::ImgReplacement,
            contains: strings(&["gta3.img"]),
            prefixes: strings(&[]),
            suffixes: strings(&[".dff", ".txd"]),
        },
        ComponentRule {
            component: Component::Data,
            contains: strings(&["/data/"]),
            prefixes: strings(&["data/"]),
            suffixes: strings(&[]),
        },
        ComponentRule {
            component: Component::Models,
            contains: strings(&["/models/"]),
            prefixes: strings(&["models/"]),
            suffixes: strings(&[]),
        },
        ComponentRule {
            component: Component::Text,
            contains: strings(&["/text/"]),
            prefixes: strings(&["text/"]),
            suffixes: strings(&[".gxt"]),
        },
        ComponentRule {
            component: Component::Anim,
            contains: strings(&["/anim/"]),
            prefixes: strings(&["anim/"]),
            suffixes: strings(&[]),
        },
        ComponentRule {
            component: Component::Audio,
            contains: strings(&["/audio/"]),
            prefixes: strings(&["audio/"]),
            suffixes: strings(&[]),
        },
    ]
}

/// The built-in context/compatibility hint rules. Extended by the config file.
pub(crate) fn builtin_context_rules() -> Vec<ContextRule> {
    vec![
        ContextRule {
            aliases: strings(&["rosa"]),
            hint: "RoSA compatibility path likely matters".to_string(),
        },
        ContextRule {
            aliases: strings(&["proper fixes"]),
            hint: "Proper Fixes can have RoSA-specific variants".to_string(),
        },
        ContextRule {
            aliases: strings(&["open limit adjuster"]),
            hint: "Limit adjuster should be installed before large model/IMG packs".to_string(),
        },
        ContextRule {
            aliases: strings(&["improvedstreaming", "improved streaming"]),
            hint: "Streaming settings should match texture/model pack size".to_string(),
        },
        ContextRule {
            aliases: strings(&["sky gfx", "skygfx", "enb"]),
            hint: "Graphics pipeline mod; ENB/SkyGfx/DirectX presets may conflict".to_string(),
        },
        ContextRule {
            aliases: strings(&["cleo+"]),
            hint: "Requires CLEO and CLEO+ runtime".to_string(),
        },
        ContextRule {
            aliases: strings(&["save"]),
            hint: "Savegame package; should target user documents, not game root".to_string(),
        },
        ContextRule {
            aliases: strings(&["gta_sa.exe"]),
            hint: "Executable replacement requires explicit bootstrap approval".to_string(),
        },
    ]
}
