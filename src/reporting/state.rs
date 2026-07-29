use crate::prelude::*;

pub(crate) fn state_directory(game_root: &Path) -> PathBuf
{
    return game_root.join(".sa-mod-manager");
}

/// Generous upper bound for a single manager-written control file (a profile,
/// mod, or import manifest â€” all small JSON). Anything larger is corrupt or
/// hostile and is refused before it is parsed.
pub(crate) const MAX_CONTROL_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Read a control file (journal, manifest, config) into a string, refusing any
/// input larger than `max_bytes`. Manager-written control files are small; a
/// file bigger than the cap is corrupt or hostile, and reading it whole would
/// let it dictate our memory use. Reads one byte past the cap so a file exactly
/// at the limit still succeeds while an over-limit one fails deterministically.
pub(crate) fn read_capped(path: &Path, max_bytes: u64) -> Result<String, AppError>
{
    use std::io::Read;
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    if bytes.len() as u64 > max_bytes
    {
        return Err(AppError::Usage(format!(
            "{} is larger than the {max_bytes}-byte limit for a manager control file; refusing to read it",
            path.display()
        )));
    }
    return String::from_utf8(bytes).map_err(|err| {
        AppError::Usage(format!("{} is not valid UTF-8", path.display()))
            .context(format!("decode capped file as UTF-8: {err}"))
    });
}

pub(crate) fn package_id(path: &Path) -> String
{
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("package");
    return safe_name(stem);
}

/// Normalize a user-supplied profile name and, when the safe form differs from
/// what was typed, return a note so the silent rename is surfaced to the user
/// instead of quietly mapping (e.g.) `My Profile!` to `my_profile`.
pub(crate) struct SafeProfileName
{
    pub(crate) name: String,
    pub(crate) note: Option<String>,
}

pub(crate) fn safe_profile_name(input: &str) -> SafeProfileName
{
    let trimmed = input.trim();
    let safe = safe_name(trimmed);
    let note = if safe == trimmed {
        None
    } else {
        Some(format!(
            "profile name `{trimmed}` was normalized to `{safe}`"
        ))
    };
    return SafeProfileName { name: safe, note };
}

pub(crate) fn safe_name(value: &str) -> String
{
    let mut out = String::new();
    let mut last_sep = false;
    for ch in value.chars()
    {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else if ch == '-' || ch == '_' || ch == '.' {
            ch
        } else {
            '_'
        };
        if mapped == '_' || mapped == '-'
        {
            if last_sep
            {
                continue;
            }
            last_sep = true;
        }
        else
        {
            last_sep = false;
        }
        out.push(mapped);
    }
    let trimmed = out.trim_matches(['_', '-', '.']).to_string();
    return if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    };
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn read_capped_accepts_up_to_the_limit_and_refuses_beyond_it()
    {
        let dir = env::temp_dir().join(format!(
            "sa-mod-manager-readcap-{}-{}",
            std::process::id(),
            unix_now()
        ));
        fs::create_dir_all(&dir)
            .expect("the test fixture is created before this assertion reads it");
        let path = dir.join("control.txt");

        // A file exactly at the limit is read in full.
        fs::write(&path, b"12345")
            .expect("the test fixture is created before this assertion reads it");
        const CONTROL_FILE_LIMIT: u64 = 5;
        assert_eq!(
            read_capped(&path, CONTROL_FILE_LIMIT)
                .expect("the test fixture is created before this assertion reads it"),
            "12345"
        );

        // One byte over the limit is refused rather than slurped.
        fs::write(&path, b"123456")
            .expect("the test fixture is created before this assertion reads it");
        let err = read_capped(&path, 5).unwrap_err().to_string(); // literal: allow test fixture value is the specimen under judgment
        assert!(err.contains("limit"), "{err}");

        fs::remove_dir_all(&dir)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn safe_profile_name_warns_only_when_it_rewrites()
    {
        // Already-safe names produce no note.
        let result = safe_profile_name("vanilla-plus");
        let name = result.name;
        let note = result.note;
        assert_eq!(name, "vanilla-plus");
        assert!(note.is_none());

        // A name needing normalization returns the safe form and a note that
        // mentions both the original and rewritten values.
        let result = safe_profile_name("My Profile!");
        let name = result.name;
        let note = result.note;
        assert_eq!(name, "my_profile");
        let note = note.expect("expected a normalization note");
        assert!(note.contains("My Profile!"), "{note}");
        assert!(note.contains("my_profile"), "{note}");
    }
}



