use crate::prelude::*;

fn string_list(items: &[&str]) -> Vec<String>
{
    return items.iter().map(|item| item.to_string()).collect();
}

/// CLEO script extensions (no leading dot), the single source of truth shared by
/// detection, classification, the runtime inventory, and the content viewer so
/// they can never drift apart. These are the only script extensions the CLEO5
/// engine recognizes: `.cs` (CLEO5), `.cs4` (CLEO4 compatibility mode), `.cs3`
/// (CLEO3 compatibility mode). There is no `.cs5`. Plugin modules (`.cleo`) and
/// text (`.fxt`) are deliberately not here â€” they are their own subsystems.
pub(crate) const CLEO_SCRIPT_EXTENSIONS: [&str; 3] = ["cs", "cs4", "cs3"];

/// Whether a lowercased path ends in a CLEO script extension (with its dot), e.g.
/// `speedo.cs4`. Matches only a real extension boundary, so `discs` is not a hit.
pub(crate) fn has_cleo_script_extension(lower: &str) -> bool
{
    return CLEO_SCRIPT_EXTENSIONS.iter().any(|ext| {
        let dotted_len = ext.len() + 1;
        lower.len() >= dotted_len
            && lower.as_bytes()[lower.len() - dotted_len] == b'.'
            && lower[lower.len() - ext.len()..].eq_ignore_ascii_case(ext)
    });
}

/// The CLEO script extensions as dotted suffixes (`.cs`, `.cs4`, â€¦), for building
/// classification rules that match on suffix string_list.
pub(crate) fn cleo_script_suffixes() -> Vec<String>
{
    return CLEO_SCRIPT_EXTENSIONS
        .iter()
        .map(|ext| format!(".{ext}"))
        .collect();
}

/// The built-in component-classification rules. Users can add more via the
/// config file; those are appended to these (see [`crate::settings`]).
pub(crate) fn builtin_component_rules() -> Vec<ComponentRule>
{
    return vec![
        ComponentRule {
            component: Component::ModLoader,
            contains: string_list(&["modloader"]),
            prefixes: string_list(&[]),
            suffixes: string_list(&[]),
        },
        ComponentRule {
            // CLEO5 plugin modules load from CLEO/cleo_plugins/, so they must be
            // routed there rather than dropped in the CLEO script folder.
            component: Component::CleoPlugin,
            contains: string_list(&["cleo_plugins"]),
            prefixes: string_list(&[]),
            suffixes: string_list(&[".cleo"]),
        },
        ComponentRule {
            component: Component::CleoText,
            contains: string_list(&["cleo_text"]),
            prefixes: string_list(&[]),
            suffixes: string_list(&[".fxt"]),
        },
        ComponentRule {
            // Shared script modules reached via the `modules:` path prefix.
            component: Component::CleoModules,
            contains: string_list(&["cleo_modules"]),
            prefixes: string_list(&[]),
            suffixes: string_list(&[]),
        },
        ComponentRule {
            // Runtime-generated per-script save data; a mod shipping it is unusual.
            component: Component::CleoSaves,
            contains: string_list(&["cleo_saves"]),
            prefixes: string_list(&[]),
            suffixes: string_list(&[]),
        },
        ComponentRule {
            // .cs = CLEO5, .cs4 = CLEO4 compat mode, .cs3 = CLEO3 compat mode.
            component: Component::Cleo,
            contains: string_list(&["cleo"]),
            prefixes: string_list(&[]),
            suffixes: cleo_script_suffixes(),
        },
        ComponentRule {
            component: Component::Asi,
            contains: string_list(&[]),
            prefixes: string_list(&[]),
            suffixes: string_list(&[".asi"]),
        },
        ComponentRule {
            component: Component::ImgReplacement,
            contains: string_list(&["gta3.img"]),
            prefixes: string_list(&[]),
            suffixes: string_list(&[".dff", ".txd"]),
        },
        ComponentRule {
            component: Component::Data,
            contains: string_list(&["/data/"]),
            prefixes: string_list(&["data/"]),
            suffixes: string_list(&[]),
        },
        ComponentRule {
            component: Component::Models,
            contains: string_list(&["/models/"]),
            prefixes: string_list(&["models/"]),
            suffixes: string_list(&[]),
        },
        ComponentRule {
            component: Component::Text,
            contains: string_list(&["/text/"]),
            prefixes: string_list(&["text/"]),
            suffixes: string_list(&[".gxt"]),
        },
        ComponentRule {
            component: Component::Anim,
            contains: string_list(&["/anim/"]),
            prefixes: string_list(&["anim/"]),
            suffixes: string_list(&[]),
        },
        ComponentRule {
            component: Component::Audio,
            contains: string_list(&["/audio/"]),
            prefixes: string_list(&["audio/"]),
            suffixes: string_list(&[]),
        },
    ];
}

/// The built-in context/compatibility hint rules. Extended by the config file.
pub(crate) fn builtin_context_rules() -> Vec<ContextRule>
{
    return vec![
        ContextRule {
            aliases: string_list(&["rosa"]),
            hint: "RoSA compatibility path likely matters".to_string(),
        },
        ContextRule {
            aliases: string_list(&["proper fixes"]),
            hint: "Proper Fixes can have RoSA-specific variants".to_string(),
        },
        ContextRule {
            aliases: string_list(&["open limit adjuster"]),
            hint: "Limit adjuster should be installed before large model/IMG packs".to_string(),
        },
        ContextRule {
            aliases: string_list(&["improvedstreaming", "improved streaming"]),
            hint: "Streaming settings should match texture/model pack size".to_string(),
        },
        ContextRule {
            aliases: string_list(&["sky gfx", "skygfx", "enb"]),
            hint: "Graphics pipeline mod; ENB/SkyGfx/DirectX presets may conflict".to_string(),
        },
        ContextRule {
            aliases: string_list(&["cleo+"]),
            hint: "Requires CLEO and CLEO+ runtime".to_string(),
        },
        ContextRule {
            aliases: string_list(&["save"]),
            hint: "Savegame package; should target user documents, not game root".to_string(),
        },
        ContextRule {
            aliases: string_list(&["gta_sa.exe"]),
            hint: "Executable replacement requires explicit bootstrap approval".to_string(),
        },
    ];
}
