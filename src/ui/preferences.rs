use crate::prelude::*;

use super::state::UiTab;

const UI_STATE_FILE: &str = "ui-state.json";
const DEFAULT_WIDTH: f32 = 1180.0;
const DEFAULT_HEIGHT: f32 = 760.0;
/// Guard against a corrupt or hostile state file resizing the window to something
/// unusable; clamp restored dimensions into a sane range.
const MIN_WINDOW_SIDE: f32 = 480.0;
const MAX_WINDOW_SIDE: f32 = 8192.0;

/// UI-only session state persisted between runs: last game folder, profile,
/// open tab, dark-mode choice, and window size. Kept separate from the settings
/// config so it can be rewritten freely without touching user-authored rules.
#[derive(Serialize, Deserialize)]
pub(super) struct UiPreferences {
    #[serde(default)]
    pub(super) game_root: String,
    #[serde(default)]
    pub(super) profile: String,
    #[serde(default)]
    pub(super) tab: String,
    #[serde(default = "default_dark_mode")]
    pub(super) dark_mode: bool,
    #[serde(default = "default_width")]
    pub(super) width: f32,
    #[serde(default = "default_height")]
    pub(super) height: f32,
}

fn default_dark_mode() -> bool {
    true
}

fn default_width() -> f32 {
    DEFAULT_WIDTH
}

fn default_height() -> f32 {
    DEFAULT_HEIGHT
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            game_root: String::new(),
            profile: String::new(),
            tab: String::new(),
            dark_mode: default_dark_mode(),
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        }
    }
}

impl UiPreferences {
    /// Best-effort load; any missing/unreadable/corrupt file yields defaults so a
    /// broken preferences file can never block the UI from opening.
    pub(super) fn load() -> Self {
        let Some(path) = ui_state_path() else {
            return Self::default();
        };
        let Ok(text) = read_capped(&path, MAX_CONTROL_FILE_BYTES) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Best-effort save; a write failure is intentionally ignored, since losing a
    /// UI preference is never worth surfacing an error over the actual work.
    pub(super) fn save(&self) {
        let Some(path) = ui_state_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, format!("{text}\n"));
        }
    }

    /// The restored window size, clamped so a bad file cannot open an unusable
    /// window.
    pub(super) fn window_size(&self) -> [f32; 2] {
        [
            self.width.clamp(MIN_WINDOW_SIDE, MAX_WINDOW_SIDE),
            self.height.clamp(MIN_WINDOW_SIDE, MAX_WINDOW_SIDE),
        ]
    }

    /// The saved tab, or Home when nothing valid was persisted.
    pub(super) fn tab(&self) -> UiTab {
        UiTab::from_key(&self.tab).unwrap_or(UiTab::Home)
    }

    /// A cheap signature used to detect changes worth writing to disk.
    pub(super) fn signature(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}x{}",
            self.game_root,
            self.profile,
            self.tab,
            self.dark_mode,
            self.width.round(),
            self.height.round()
        )
    }
}

/// A sibling of the settings config file, so both live in the same per-user
/// directory and honor the same `SA_MOD_MANAGER_CONFIG` override location.
fn ui_state_path() -> Option<PathBuf> {
    crate::settings::config_file_path().map(|path| path.with_file_name(UI_STATE_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_corrupt_input_round_trip_to_safe_values() {
        // Missing fields fall back to defaults rather than failing to parse.
        let partial: UiPreferences = serde_json::from_str("{\"tab\":\"run\"}").unwrap();
        assert_eq!(partial.tab, "run");
        assert!(partial.dark_mode);
        assert_eq!(partial.window_size(), [DEFAULT_WIDTH, DEFAULT_HEIGHT]);
        assert!(matches!(partial.tab(), UiTab::Run));

        // Out-of-range or malformed sizes are clamped into a usable window.
        let hostile = UiPreferences {
            width: 1.0,
            height: 999_999.0,
            ..UiPreferences::default()
        };
        assert_eq!(hostile.window_size(), [MIN_WINDOW_SIDE, MAX_WINDOW_SIDE]);

        // Unknown tab keys degrade to Home instead of panicking.
        let unknown = UiPreferences {
            tab: "does-not-exist".to_string(),
            ..UiPreferences::default()
        };
        assert!(matches!(unknown.tab(), UiTab::Home));
    }

    #[test]
    fn signature_tracks_meaningful_changes() {
        let base = UiPreferences::default();
        let mut changed = UiPreferences::default();
        changed.profile = "vanilla-plus".to_string();
        assert_ne!(base.signature(), changed.signature());
    }
}
