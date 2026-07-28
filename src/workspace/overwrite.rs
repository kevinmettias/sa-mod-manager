use crate::prelude::*;

/// ModLoader / CLEO internals and loader stubs that live in these folders but are
/// not "unmanaged content" — they belong to the loaders themselves.
const INFRASTRUCTURE_FILES: [&str; 6] = [
    "modloader.ini",
    "modloader.log",
    "modloader.asi",
    "cleo.asi",
    "cleo.log",
    "cleo.ini",
];

/// Files sitting in the game folder's mod areas (`modloader/`, `cleo/`) that no
/// enabled mod in the current profile provides — MO2's "overwrite": content
/// installed by hand or left behind by a tool, which the manager does not touch.
///
/// `owned` is the set of materialized target paths from the content index
/// (forward-slash, relative to the game root). Only `modloader/` and `cleo/` are
/// scanned because they do not exist in a vanilla install, so anything there that
/// is neither owned nor loader infrastructure is genuinely unmanaged; `data/` and
/// friends are skipped since they mix with vanilla files.
pub(crate) fn collect_overwrite_files(game_root: &Path, owned: &BTreeSet<String>) -> Vec<String> {
    let mut result = Vec::new();
    for folder in ["modloader", "cleo"] {
        let root = game_root.join(folder);
        if !root.is_dir() {
            continue;
        }
        let Ok(files) = collect_files_recursive(&root) else {
            continue;
        };
        for file in files {
            let Ok(relative) = file.strip_prefix(game_root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            // ModLoader/CLEO keep internals in dot-folders (.data, .profiles); skip.
            if relative.split('/').any(|segment| segment.starts_with('.')) {
                continue;
            }
            let base = relative
                .rsplit('/')
                .next()
                .unwrap_or(&relative)
                .to_ascii_lowercase();
            if INFRASTRUCTURE_FILES.contains(&base.as_str()) {
                continue;
            }
            if owned.contains(&relative) {
                continue;
            }
            result.push(relative);
        }
    }
    result.sort();
    result.dedup();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn overwrite_lists_unmanaged_and_skips_owned_and_infra() {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-overwrite-{}-{}",
            std::process::id(),
            unix_now()
        ));
        // Unmanaged: present in modloader/ but not owned.
        write_file(
            &root.join("modloader").join("HandInstalled").join("a.dff"),
            "x",
        );
        // Owned by an enabled mod → excluded.
        write_file(&root.join("modloader").join("FromMod").join("b.dff"), "x");
        // Loader internals / infrastructure → excluded.
        write_file(&root.join("modloader").join(".data").join("cache"), "x");
        write_file(&root.join("modloader").join("modloader.log"), "x");

        let mut owned = BTreeSet::new();
        owned.insert("modloader/FromMod/b.dff".to_string());

        let result = collect_overwrite_files(&root, &owned);
        assert_eq!(result, vec!["modloader/HandInstalled/a.dff".to_string()]);
        fs::remove_dir_all(&root).unwrap();
    }
}
