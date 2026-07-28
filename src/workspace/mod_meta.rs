use crate::prelude::*;

/// User annotations for a single library mod (MO2's categories / color / notes):
/// freeform, separate from the mod's own `mod.json`, so they are the manager
/// user's organizational layer rather than the mod author's data. Stored keyed
/// by mod id in `.sa-mod-manager/mod_meta.json`.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub(crate) struct ModMeta {
    /// User-assigned category names (freeform, e.g. "Vehicles", "Fixes").
    pub(crate) categories: Vec<String>,
    /// A color-label name from the manager's small preset palette, if set.
    pub(crate) color: Option<String>,
    /// A freeform note.
    pub(crate) note: String,
}

impl ModMeta {
    /// True when there is nothing worth persisting for this mod.
    pub(crate) fn is_empty(&self) -> bool {
        self.categories.is_empty() && self.color.is_none() && self.note.trim().is_empty()
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ModMetaFile {
    version: u32,
    mods: BTreeMap<String, ModMeta>,
}

fn mod_meta_path(state_root: &Path) -> PathBuf {
    state_root.join("mod_meta.json")
}

/// Read all per-mod annotations, or an empty map when none are saved / the file
/// is unreadable — annotations are optional and must never break loading.
pub(crate) fn read_mod_meta(state_root: &Path) -> BTreeMap<String, ModMeta> {
    let Ok(text) = read_capped(&mod_meta_path(state_root), MAX_CONTROL_FILE_BYTES) else {
        return BTreeMap::new();
    };
    serde_json::from_str::<ModMetaFile>(&text)
        .map(|file| file.mods)
        .unwrap_or_default()
}

/// Persist per-mod annotations, dropping entries that carry nothing so the file
/// stays tidy as mods are cleared back to defaults.
pub(crate) fn write_mod_meta(
    state_root: &Path,
    meta: &BTreeMap<String, ModMeta>,
) -> Result<(), AppError> {
    fs::create_dir_all(state_root)
        .with_context(|| format!("create state directory {}", state_root.display()))?;
    let mods: BTreeMap<String, ModMeta> = meta
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let file = ModMetaFile { version: 1, mods };
    let mut text = serde_json::to_string_pretty(&file)
        .map_err(|err| AppError::Usage(format!("serialize mod metadata: {err}")))?;
    text.push('\n');
    let path = mod_meta_path(state_root);
    fs::write(&path, text).with_context(|| format!("write mod metadata {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-meta-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn mod_meta_round_trips_and_drops_empty() {
        let root = state_root("round_trip");
        let mut meta = BTreeMap::new();
        meta.insert(
            "imvehft".to_string(),
            ModMeta {
                categories: vec!["Vehicles".to_string()],
                color: Some("green".to_string()),
                note: "handling tweaks".to_string(),
            },
        );
        // An all-default entry should not survive the write.
        meta.insert("empty".to_string(), ModMeta::default());

        write_mod_meta(&root, &meta).unwrap();
        let read = read_mod_meta(&root);

        assert_eq!(read.len(), 1);
        let imvehft = read.get("imvehft").unwrap();
        assert_eq!(imvehft.categories, vec!["Vehicles".to_string()]);
        assert_eq!(imvehft.color.as_deref(), Some("green"));
        assert_eq!(imvehft.note, "handling tweaks");
        assert!(read.get("empty").is_none());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let root = state_root("missing");
        assert!(read_mod_meta(&root).is_empty());
        fs::remove_dir_all(&root).unwrap();
    }
}
