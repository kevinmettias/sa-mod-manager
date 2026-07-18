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

/// Format a Unix timestamp (seconds) as a readable UTC datetime. Returns an
/// em dash for a zero/unknown timestamp.
pub(crate) fn human_datetime(unix_secs: u64) -> String {
    if unix_secs == 0 {
        return "—".to_string();
    }
    let days = (unix_secs / 86_400) as i64;
    let seconds_of_day = unix_secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

/// Howard Hinnant's civil-from-days: days since 1970-01-01 to (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365; // [0, 399]
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // [0, 365]
    let month_position = (5 * day_of_year + 2) / 153; // [0, 11]
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u32; // [1, 31]
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u32; // [1, 12]
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_datetime_formats_known_timestamps() {
        assert_eq!(human_datetime(0), "—");
        assert_eq!(human_datetime(86_400), "1970-01-02 00:00:00 UTC");
        assert_eq!(human_datetime(1_000_000_000), "2001-09-09 01:46:40 UTC");
        assert_eq!(human_datetime(1_700_000_000), "2023-11-14 22:13:20 UTC");
    }
}
