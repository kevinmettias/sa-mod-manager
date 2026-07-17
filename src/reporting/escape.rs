use crate::prelude::*;

pub(crate) fn escape_value(value: &str) -> String {
    value
        .replace('\\', "\\\\") // literal: allow external interface text or file-format spelling
        .replace('\n', "\\n") // literal: allow external interface text or file-format spelling
        .replace('\r', "\\r") // literal: allow external interface text or file-format spelling
        .replace('|', "\\|") // literal: allow external interface text or file-format spelling
}

pub(crate) fn json_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""), // literal: allow external interface text or file-format spelling
            '\\' => out.push_str("\\\\"), // literal: allow external interface text or file-format spelling
            '\n' => out.push_str("\\n"), // literal: allow external interface text or file-format spelling
            '\r' => out.push_str("\\r"), // literal: allow external interface text or file-format spelling
            '\t' => out.push_str("\\t"), // literal: allow external interface text or file-format spelling
            other => out.push(other),
        }
    }
    out
}

pub(crate) fn unescape_value(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('|') => out.push('|'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
