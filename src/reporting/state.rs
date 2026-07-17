use crate::prelude::*;

pub(crate) fn state_directory(game_root: &Path) -> PathBuf {
    game_root.join(".sa-mod-manager") // literal: allow external interface text or file-format spelling
}

pub(crate) fn package_id(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("package"); // literal: allow external interface text or file-format spelling
    safe_name(stem)
}

pub(crate) fn safe_name(value: &str) -> String {
    let mut out = String::new();
    let mut last_sep = false;
    for ch in value.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else if ch == '-' || ch == '_' || ch == '.' {
            ch
        } else {
            '_'
        };
        if mapped == '_' || mapped == '-' {
            if last_sep {
                continue;
            }
            last_sep = true;
        } else {
            last_sep = false;
        }
        out.push(mapped);
    }
    let trimmed = out.trim_matches(['_', '-', '.']).to_string();
    if trimmed.is_empty() {
        "unnamed".to_string() // literal: allow external interface text or file-format spelling
    } else {
        trimmed
    }
}
