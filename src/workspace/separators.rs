use crate::prelude::*;

/// A labeled divider row in a profile's load-order list (MO2's separators). It is
/// purely organizational: `position` is a *display slot* â€” the separator renders
/// immediately above the mod currently at that slot â€” so it is decoupled from the
/// mods' own load order and survives mod reordering without touching it. Stored
/// per profile under `.sa-mod-manager/separators/<profile>.json`.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub(crate) struct Separator
{
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) position: usize,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct SeparatorsFile
{
    version: u32,
    separators: Vec<Separator>,
}

/// Read a profile's separators (sorted by position), or an empty list when none
/// are saved / the file is unreadable â€” separators are optional cosmetics.
pub(crate) fn read_separators(state_root: &Path, profile: &str) -> Vec<Separator>
{
    let path = separators_path(state_root, profile);
    let Ok(text) = read_capped(&path, MAX_CONTROL_FILE_BYTES) else {
        return Vec::new();
    };
    let mut separators = serde_json::from_str::<SeparatorsFile>(&text)
        .map(|file| file.separators)
        .unwrap_or_default();
    separators.sort_by_key(|separator| separator.position);
    return separators;
}

/// Persist a profile's separators.
pub(crate) fn write_separators(
    state_root: &Path,
    profile: &str,
    separators: &[Separator],
) -> Result<(), AppError>
{
    let path = separators_path(state_root, profile);
    if let Some(parent) = path.parent()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create separators directory {}", parent.display()))?;
    }
    let file = SeparatorsFile {
        version: 1,
        separators: separators.to_vec(),
    };
    let mut text = serde_json::to_string_pretty(&file)
        .map_err(|err| AppError::Usage(format!("serialize separators: {err}")))?;
    text.push('\n');
    fs::write(&path, text).with_context(|| format!("write separators {}", path.display()))?;
    return Ok(());
}

fn separators_path(state_root: &Path, profile: &str) -> PathBuf
{
    return state_root
        .join("separators")
        .join(format!("{}.json", safe_name(profile)));
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn separators_round_trip_sorted_by_position()
    {
        let root = state_root("round_trip");
        let separators = vec![
            Separator {
                id: "b".to_string(),
                name: "Vehicles".to_string(),
                position: 4, // literal: allow test fixture value is the specimen under judgment
            },
            Separator {
                id: "a".to_string(),
                name: "Graphics".to_string(),
                position: 0,
            },
        ];
        write_separators(&root, "default", &separators)
            .expect("the test fixture is created before this assertion reads it");
        let read = read_separators(&root, "default");

        // Read back sorted by position.
        let names: Vec<&str> = read.iter().map(|sep| sep.name.as_str()).collect();
        assert_eq!(names, vec!["Graphics", "Vehicles"]);
        assert_eq!(read[0].position, 0);
        fs::remove_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn missing_file_reads_as_empty()
    {
        let root = state_root("missing");
        assert!(read_separators(&root, "default").is_empty());
        fs::remove_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
    }

    fn state_root(name: &str) -> PathBuf
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-sep-{name}-{}-{}",
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
