pub(crate) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string() // literal: allow external interface text or file-format spelling
}

pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(crate) fn is_readme_name(name: &str) -> bool {
    name.contains("readme") // literal: allow external interface text or file-format spelling
        || name.contains("leiame") // literal: allow external interface text or file-format spelling
        || name.contains("install") // literal: allow external interface text or file-format spelling
        || name.contains("changelog") // literal: allow external interface text or file-format spelling
        || name.contains("compatibility") // literal: allow external interface text or file-format spelling
        || name.contains("bugs") // literal: allow external interface text or file-format spelling
        || name.contains("misc") // literal: allow external interface text or file-format spelling
        || name.ends_with(".url") // literal: allow external interface text or file-format spelling
}

pub(crate) fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
