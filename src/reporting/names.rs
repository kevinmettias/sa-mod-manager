const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3600;
const SECONDS_PER_DAY: u64 = 86_400;
const CIVIL_EPOCH_OFFSET_DAYS: i64 = 719_468;
const DAYS_PER_ERA: i64 = 146_097;
const DAYS_BEFORE_ERA: i64 = 146_096;
const FOUR_YEAR_CORRECTION_DAYS: i64 = 1460;
const CENTURY_CORRECTION_DAYS: i64 = 36524;
const DAYS_PER_COMMON_YEAR: i64 = 365;
const YEARS_PER_ERA: i64 = 400;
const YEARS_PER_CENTURY: i64 = 100;
const LEAP_YEAR_DIVISOR: i64 = 4;
const MONTH_FORMULA_MULTIPLIER: i64 = 5;
const MONTH_FORMULA_OFFSET: i64 = 2;
const MONTH_FORMULA_DIVISOR: i64 = 153;
const MARCH_BASED_MONTH_COUNT: i64 = 10;
const JANUARY_FEBRUARY_SHIFT: i64 = 3;
const MARCH_BASED_YEAR_END_SHIFT: i64 = 9;
const FEBRUARY_MONTH: u32 = 2;
const BYTE_UNIT_SCALE: f64 = 1024.0;
pub(crate) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(crate) fn is_readme_name(name: &str) -> bool {
    name.contains("readme")
        || name.contains("leiame")
        || name.contains("install")
        || name.contains("changelog")
        || name.contains("compatibility")
        || name.contains("bugs")
        || name.contains("misc")
        || name.ends_with(".url")
}

/// Format a Unix timestamp (seconds) as a readable UTC datetime. Returns an
/// em dash for a zero/unknown timestamp.
pub(crate) fn human_datetime(unix_secs: u64) -> String {
    if unix_secs == 0 {
        return "—".to_string();
    }
    let days = (unix_secs / SECONDS_PER_DAY) as i64;
    let seconds_of_day = unix_secs % SECONDS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / SECONDS_PER_HOUR;
    let minute = (seconds_of_day % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let second = seconds_of_day % SECONDS_PER_MINUTE;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

/// Howard Hinnant's civil-from-days: days since 1970-01-01 to (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + CIVIL_EPOCH_OFFSET_DAYS;
    let era = if z >= 0 { z } else { z - DAYS_BEFORE_ERA } / DAYS_PER_ERA;
    let day_of_era = z - era * DAYS_PER_ERA; // [0, 146096]
    let year_of_era = (day_of_era - day_of_era / FOUR_YEAR_CORRECTION_DAYS
        + day_of_era / CENTURY_CORRECTION_DAYS
        - day_of_era / DAYS_BEFORE_ERA)
        / DAYS_PER_COMMON_YEAR; // [0, 399]
    let year = year_of_era + era * YEARS_PER_ERA;
    let day_of_year = day_of_era
        - (DAYS_PER_COMMON_YEAR * year_of_era + year_of_era / LEAP_YEAR_DIVISOR
            - year_of_era / YEARS_PER_CENTURY); // [0, 365]
    let month_position =
        (MONTH_FORMULA_MULTIPLIER * day_of_year + MONTH_FORMULA_OFFSET) / MONTH_FORMULA_DIVISOR; // [0, 11]
    let day = (day_of_year
        - (MONTH_FORMULA_DIVISOR * month_position + MONTH_FORMULA_OFFSET)
            / MONTH_FORMULA_MULTIPLIER
        + 1) as u32; // [1, 31]
    let month = if month_position < MARCH_BASED_MONTH_COUNT {
        month_position + JANUARY_FEBRUARY_SHIFT
    } else {
        month_position - MARCH_BASED_YEAR_END_SHIFT
    } as u32; // [1, 12]
    let year = if month <= FEBRUARY_MONTH {
        year + 1
    } else {
        year
    };
    (year, month, day)
}

pub(crate) fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= BYTE_UNIT_SCALE && unit < UNITS.len() - 1 {
        value /= BYTE_UNIT_SCALE;
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
    const ONE_DAY_UNIX: u64 = SECONDS_PER_DAY;
    const BILLION_SECONDS_UNIX: u64 = 1_000_000_000;
    const RECENT_SAMPLE_UNIX: u64 = 1_700_000_000;

    #[test]
    fn human_datetime_formats_known_timestamps() {
        assert_eq!(human_datetime(0), "—");
        assert_eq!(human_datetime(ONE_DAY_UNIX), "1970-01-02 00:00:00 UTC");
        assert_eq!(
            human_datetime(BILLION_SECONDS_UNIX),
            "2001-09-09 01:46:40 UTC"
        );
        assert_eq!(
            human_datetime(RECENT_SAMPLE_UNIX),
            "2023-11-14 22:13:20 UTC"
        );
    }
}
