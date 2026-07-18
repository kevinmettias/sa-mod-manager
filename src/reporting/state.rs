use crate::prelude::*;

pub(crate) fn state_directory(game_root: &Path) -> PathBuf {
    game_root.join(".sa-mod-manager") // literal: allow external interface text or file-format spelling
}

/// Read a control file (journal, manifest, config) into a string, refusing any
/// input larger than `max_bytes`. Manager-written control files are small; a
/// file bigger than the cap is corrupt or hostile, and reading it whole would
/// let it dictate our memory use. Reads one byte past the cap so a file exactly
/// at the limit still succeeds while an over-limit one fails deterministically.
pub(crate) fn read_capped(path: &Path, max_bytes: u64) -> Result<String, AppError> {
    use std::io::Read;
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(AppError::Usage(format!(
            "{} is larger than the {max_bytes}-byte limit for a manager control file; refusing to read it",
            path.display()
        )));
    }
    String::from_utf8(bytes)
        .map_err(|_| AppError::Usage(format!("{} is not valid UTF-8", path.display())))
}

pub(crate) fn package_id(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("package"); // literal: allow external interface text or file-format spelling
    safe_name(stem)
}

/// Normalize a user-supplied profile name and, when the safe form differs from
/// what was typed, return a note so the silent rename is surfaced to the user
/// instead of quietly mapping (e.g.) `My Profile!` to `my_profile`.
pub(crate) fn safe_profile_name(input: &str) -> (String, Option<String>) {
    let trimmed = input.trim();
    let safe = safe_name(trimmed);
    let note = if safe == trimmed {
        None
    } else {
        Some(format!(
            "profile name `{trimmed}` was normalized to `{safe}`"
        ))
    };
    (safe, note)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_profile_name_warns_only_when_it_rewrites() {
        // Already-safe names produce no note.
        let (name, note) = safe_profile_name("vanilla-plus");
        assert_eq!(name, "vanilla-plus");
        assert!(note.is_none());

        // A name needing normalization returns the safe form and a note that
        // mentions both the original and rewritten values.
        let (name, note) = safe_profile_name("My Profile!");
        assert_eq!(name, "my_profile");
        let note = note.expect("expected a normalization note");
        assert!(note.contains("My Profile!"), "{note}");
        assert!(note.contains("my_profile"), "{note}");
    }
}
