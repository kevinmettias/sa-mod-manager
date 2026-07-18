use crate::prelude::*;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// CLEO script extensions (no leading dot), the single source of truth shared by
/// detection, classification, the runtime inventory, and the content viewer so
/// they can never drift apart. These are the only script extensions the CLEO5
/// engine recognizes: `.cs` (CLEO5), `.cs4` (CLEO4 compatibility mode), `.cs3`
/// (CLEO3 compatibility mode). There is no `.cs5`. Plugin modules (`.cleo`) and
/// text (`.fxt`) are deliberately not here — they are their own subsystems.
pub(crate) const CLEO_SCRIPT_EXTENSIONS: [&str; 3] = ["cs", "cs4", "cs3"];

/// Whether a lowercased path ends in a CLEO script extension (with its dot), e.g.
/// `speedo.cs4`. Matches only a real extension boundary, so `discs` is not a hit.
pub(crate) fn has_cleo_script_extension(lower: &str) -> bool {
    CLEO_SCRIPT_EXTENSIONS.iter().any(|ext| {
        let dotted_len = ext.len() + 1;
        lower.len() >= dotted_len
            && lower.as_bytes()[lower.len() - dotted_len] == b'.'
            && lower[lower.len() - ext.len()..].eq_ignore_ascii_case(ext)
    })
}

/// The CLEO script extensions as dotted suffixes (`.cs`, `.cs4`, …), for building
/// classification rules that match on suffix strings.
pub(crate) fn cleo_script_suffixes() -> Vec<String> {
    CLEO_SCRIPT_EXTENSIONS
        .iter()
        .map(|ext| format!(".{ext}"))
        .collect()
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
            // CLEO5 plugin modules load from CLEO/cleo_plugins/, so they must be
            // routed there rather than dropped in the CLEO script folder.
            component: Component::CleoPlugin,
            contains: strings(&["cleo_plugins"]),
            prefixes: strings(&[]),
            suffixes: strings(&[".cleo"]),
        },
        ComponentRule {
            component: Component::CleoText,
            contains: strings(&["cleo_text"]),
            prefixes: strings(&[]),
            suffixes: strings(&[".fxt"]),
        },
        ComponentRule {
            // Shared script modules reached via the `modules:` path prefix.
            component: Component::CleoModules,
            contains: strings(&["cleo_modules"]),
            prefixes: strings(&[]),
            suffixes: strings(&[]),
        },
        ComponentRule {
            // Runtime-generated per-script save data; a mod shipping it is unusual.
            component: Component::CleoSaves,
            contains: strings(&["cleo_saves"]),
            prefixes: strings(&[]),
            suffixes: strings(&[]),
        },
        ComponentRule {
            // .cs = CLEO5, .cs4 = CLEO4 compat mode, .cs3 = CLEO3 compat mode.
            component: Component::Cleo,
            contains: strings(&["cleo"]),
            prefixes: strings(&[]),
            suffixes: cleo_script_suffixes(),
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
