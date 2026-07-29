use crate::prelude::*;

/// A user-configured run target (MO2's "executables"): an external tool the
/// manager can launch directly â€” a map editor, IMG tool, CLEO debugger, or the
/// game with custom arguments. Stored globally for the install under
/// `.sa-mod-manager/executables.json`; the built-in "play current profile"
/// target is not stored here (it always exists in the UI).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub(crate) struct Executable
{
    pub(crate) name: String,
    /// Absolute path to the executable, kept as a string for direct UI editing.
    pub(crate) path: String,
    /// Raw argument string, whitespace-split at launch.
    pub(crate) args: String,
}

impl Executable
{
    /// The arguments as a launch-ready vector (whitespace-split). Quoting is not
    /// interpreted â€” adequate for the simple flags SA tools take.
    pub(crate) fn arg_list(&self) -> Vec<String>
    {
        return self.args.split_whitespace().map(str::to_string).collect();
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExecutablesFile
{
    version: u32,
    executables: Vec<Executable>,
}

/// Read the configured run targets, or an empty list when none are saved or the
/// file is unreadable â€” a missing tool list must never break the Play tab.
pub(crate) fn read_executables(state_root: &Path) -> Vec<Executable>
{
    let Ok(text) = read_capped(&executables_path(state_root), MAX_CONTROL_FILE_BYTES) else {
        return Vec::new();
    };
    return serde_json::from_str::<ExecutablesFile>(&text)
        .map(|file| file.executables)
        .unwrap_or_default();
}

/// Persist the run targets under the manager state directory.
pub(crate) fn write_executables(
    state_root: &Path,
    executables: &[Executable],
) -> Result<(), AppError>
{
    fs::create_dir_all(state_root)
        .with_context(|| format!("create state directory {}", state_root.display()))?;
    let file = ExecutablesFile {
        version: 1,
        executables: executables.to_vec(),
    };
    let mut text = serde_json::to_string_pretty(&file)
        .map_err(|err| AppError::Usage(format!("serialize executables: {err}")))?;
    text.push('\n');
    let path = executables_path(state_root);
    fs::write(&path, text).with_context(|| format!("write executables {}", path.display()))?;
    return Ok(());
}

fn executables_path(state_root: &Path) -> PathBuf
{
    return state_root.join("executables.json");
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn executables_round_trip_through_disk()
    {
        let root = state_root("round_trip");
        let executables = vec![
            Executable {
                name: "Map editor".to_string(),
                path: "C:/tools/MapEditor.exe".to_string(),
                args: "--fast -x".to_string(),
            },
            Executable {
                name: "Windowed game".to_string(),
                path: "gta_sa.exe".to_string(),
                args: String::new(),
            },
        ];
        write_executables(&root, &executables)
            .expect("the test fixture is created before this assertion reads it");
        let read = read_executables(&root);

        assert_eq!(read.len(), 2); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(read[0].name, "Map editor");
        assert_eq!(read[0].arg_list(), vec!["--fast", "-x"]);
        assert!(read[1].arg_list().is_empty());
        fs::remove_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn missing_file_reads_as_empty()
    {
        let root = state_root("missing");
        assert!(read_executables(&root).is_empty());
        fs::remove_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
    }

    fn state_root(name: &str) -> PathBuf
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-exe-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        if root.exists()
        {
            fs::remove_dir_all(&root)
                .expect("the test fixture is created before this assertion reads it");
        }
        fs::create_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
        return root;
    }
}
