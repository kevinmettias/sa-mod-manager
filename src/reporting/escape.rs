use crate::prelude::*;

pub(crate) fn escape_value(value: &str) -> String
{
    return value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('|', "\\|");
}

pub(crate) fn json_escape(value: &str) -> String
{
    let mut out = String::new();
    for ch in value.chars()
    {
        match ch
        {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    return out;
}

pub(crate) fn unescape_value(value: &str) -> String
{
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next()
    {
        if ch != '\\'
        {
            out.push(ch);
            continue;
        }
        match chars.next()
        {
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
    return out;
}

pub(crate) fn unix_now() -> u64
{
    return SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
}
