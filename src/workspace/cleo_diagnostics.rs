//! Reading CLEO's own runtime files — `cleo.log` and `.cleo_config.ini` — so the
//! manager can surface what CLEO actually did and how it is configured, rather
//! than only inferring statically.

// --- cleo.log -------------------------------------------------------------

/// A digest of `cleo.log`: scripts CLEO reported failing to load, plus other
/// error/warning lines, and the total non-empty line count.
pub(crate) struct CleoLogSummary {
    pub(crate) failed_scripts: Vec<String>,
    pub(crate) errors: Vec<String>,
    pub(crate) total_lines: usize,
}

impl CleoLogSummary {
    pub(crate) fn is_clean(&self) -> bool {
        self.failed_scripts.is_empty() && self.errors.is_empty()
    }
}

/// Parse `cleo.log`. Lines are written as `DD/MM/YYYY HH:MM:SS.mmm <message>`; we
/// strip the timestamp and key off the message text, tolerant of the timestamp
/// being present or absent.
pub(crate) fn parse_cleo_log(text: &str) -> CleoLogSummary {
    let mut failed_scripts = Vec::new();
    let mut errors = Vec::new();
    let mut total_lines = 0;
    for raw in text.lines() {
        let line = strip_log_timestamp(raw).trim();
        if line.is_empty() {
            continue;
        }
        total_lines += 1;
        if let Some(name) = script_load_failure(line) {
            failed_scripts.push(name);
        } else if is_error_line(line) {
            errors.push(line.to_string());
        }
    }
    CleoLogSummary {
        failed_scripts,
        errors,
        total_lines,
    }
}

/// Drop a `DD/MM/YYYY HH:MM:SS.mmm ` prefix (24 chars) when the line begins with
/// one; otherwise return the line unchanged.
fn strip_log_timestamp(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.len() > 24
        && bytes[2] == b'/'
        && bytes[5] == b'/'
        && bytes[10] == b' '
        && bytes[23] == b' '
    {
        &line[24..]
    } else {
        line
    }
}

/// Extract the script name from a `Loading of custom script 'NAME' failed` line.
fn script_load_failure(line: &str) -> Option<String> {
    let rest = line.strip_prefix("Loading of custom script '")?;
    let name = rest.split('\'').next()?;
    line.contains("failed").then(|| name.to_string())
}

fn is_error_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    ["failed", "error", "invalid", "unable", "not found", "unsupported"]
        .iter()
        .any(|needle| lower.contains(needle))
}

// --- .cleo_config.ini -----------------------------------------------------

/// A CLEO config setting worth surfacing, with a plain-language meaning.
pub(crate) struct CleoConfigHighlight {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) note: &'static str,
}

/// A parsed `.cleo_config.ini`: the highlighted mod-management-relevant settings
/// plus totals so the viewer can say how much more is in the file.
pub(crate) struct CleoConfig {
    pub(crate) highlights: Vec<CleoConfigHighlight>,
    pub(crate) key_count: usize,
}

/// Parse `.cleo_config.ini` (a plain `key = value ; comment` INI) and pull out
/// the settings that affect how mods behave.
pub(crate) fn parse_cleo_config(text: &str) -> CleoConfig {
    let mut highlights = Vec::new();
    let mut key_count = 0;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') || line.starts_with('[')
        {
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = rest.split(';').next().unwrap_or("").trim();
        key_count += 1;
        if let Some(note) = annotate_config_key(key) {
            highlights.push(CleoConfigHighlight {
                key: key.to_string(),
                value: value.to_string(),
                note,
            });
        }
    }
    CleoConfig {
        highlights,
        key_count,
    }
}

/// A plain-language note for the CLEO config keys that matter to mod management,
/// or `None` for keys not worth surfacing.
fn annotate_config_key(key: &str) -> Option<&'static str> {
    match key {
        "PluginBlacklist" => Some("legacy CLEO4 plugins CLEO refuses to load"),
        "DebugMode" => Some("global script debug mode (0 off / 1 on)"),
        "MainScmLegacyMode" => Some("main.scm compat mode (0 off / 3 CLEO3 / 4 CLEO4)"),
        "StrictValidation" => Some("opcode argument validation (0 = .cs4 behavior)"),
        "LogDirectory" => Some("where cleo.log / cleo_script.log are written"),
        "DebugUtils.ScriptLog.Enabled" => {
            Some("per-script execution log (0 never / 1 on crash / 2 always)")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleo_log_extracts_failures_and_errors() {
        let log = "\
02/07/2026 14:03:11.500 CLEO v5.4.0
02/07/2026 14:03:11.512 Listing CLEO scripts:
02/07/2026 14:03:11.520  speedo.cs
02/07/2026 14:03:11.600 Loading of custom script 'brokenmod.cs' failed
02/07/2026 14:03:11.700 Some unrelated informational line
02/07/2026 14:03:11.800 Invalid '0x0' pointer of input string argument";
        let summary = parse_cleo_log(log);
        assert_eq!(summary.failed_scripts, vec!["brokenmod.cs"]);
        assert_eq!(summary.errors.len(), 1);
        assert!(summary.errors[0].starts_with("Invalid"));
        assert!(!summary.is_clean());
        assert_eq!(summary.total_lines, 6);
    }

    #[test]
    fn cleo_log_without_timestamps_still_parses() {
        let summary = parse_cleo_log("Loading of custom script 'x.cs' failed\n");
        assert_eq!(summary.failed_scripts, vec!["x.cs"]);
    }

    #[test]
    fn clean_log_has_no_findings() {
        let summary = parse_cleo_log("01/01/2026 00:00:00.000 Starting CLEO scripts...\n");
        assert!(summary.is_clean());
    }

    #[test]
    fn cleo_config_highlights_known_keys_only() {
        let ini = "\
[General]
PluginBlacklist = IniFiles.cleo,GxtHook.cleo ; legacy
DebugMode = 1
UnknownKey = 42

[Plugins]
StrictValidation = 0";
        let config = parse_cleo_config(ini);
        assert_eq!(config.key_count, 4);
        let keys: Vec<&str> = config.highlights.iter().map(|h| h.key.as_str()).collect();
        assert_eq!(keys, vec!["PluginBlacklist", "DebugMode", "StrictValidation"]);
        let blacklist = &config.highlights[0];
        assert_eq!(blacklist.value, "IniFiles.cleo,GxtHook.cleo");
    }
}
