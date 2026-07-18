use crate::prelude::*;

pub(crate) fn list_matching<F>(dir: &Path, predicate: F) -> io::Result<Vec<PathBuf>>
where
    F: Fn(&Path) -> bool,
{
    let mut matches = Vec::new();
    if !dir.exists() {
        return Ok(matches);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.metadata()?.is_file() && predicate(&path) {
            matches.push(path);
        }
    }
    Ok(matches)
}

pub(crate) fn extension_eq(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.eq_ignore_ascii_case(extension))
        .unwrap_or(false)
}

pub(crate) fn find_seven_zip() -> Option<PathBuf> {
    // An explicit path from `SA_MOD_MANAGER_7Z` or the config file wins.
    if let Some(path) = configured_seven_zip() {
        if command_candidate_works(&path) {
            return Some(path);
        }
    }

    let mut candidates = vec![
        PathBuf::from("7z"),
        PathBuf::from("7zz"),
        PathBuf::from("7za"),
        PathBuf::from("7z.exe"),
        PathBuf::from("7za.exe"),
    ];
    #[cfg(windows)]
    {
        candidates.push(PathBuf::from(r"C:\Program Files\7-Zip\7z.exe")); // literal: allow external interface text or file-format spelling
        candidates.push(PathBuf::from(r"C:\Program Files (x86)\7-Zip\7z.exe")); // literal: allow external interface text or file-format spelling
    }

    candidates.into_iter().find(command_candidate_works)
}

fn command_candidate_works(path: &PathBuf) -> bool {
    if path.is_absolute() && !path.exists() {
        return false;
    }
    Command::new(path).arg("-h").output().is_ok() // literal: allow external interface text or file-format spelling
}
