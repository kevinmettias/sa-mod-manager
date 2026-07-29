use crate::planning::{readme_copy_install_root, ReadmeCopyInstallRoot};
use crate::prelude::*;
const MILLISECONDS_PER_SECOND: u64 = 1000;
const README_ACCEPT_MAX_BYTES: u64 = 64 * 1024;
const RUN_OUTCOME_JOURNAL_MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MOD_DETAILS_READMES: usize = 12;
#[cfg(test)]
const TEST_PID: u32 = 1234;
#[cfg(test)]
const TEST_PENDING_PID: u32 = 42;
#[cfg(test)]
const OTHER_TEST_PID: u32 = 99;
use crate::workspace::{ProfileCopyRequest, ProfileModSelection, ProfileRenameRequest, ProfileRootSelector, append_mod_config_install_root};
use eframe::egui;

use super::san_andreas_mod_ui::{
    ActiveRun, ConfirmAction, ModDetailsTab, ModDetailsView, PendingConfirm, PendingRunRecord, PendingRunStatus,
    ReadmeProposal, ReadmeProposalState, SanAndreasModUi, TaskResult, export_telemetry_summary,
};

pub(super) enum ContentFocus
{
    AllFiles,
    ConflictsOnly,
}

pub(super) struct ModCategoryToggle<'a>
{
    pub(super) mod_id: &'a str,
    pub(super) category: &'a str,
}

impl SanAndreasModUi
{
    pub(super) fn export_telemetry(&mut self)
    {
        match export_telemetry_summary(&self.game_root(), &self.telemetry)
        {
            Ok(path) => self.status = format!("exported telemetry: {}", path.display()),
            Err(err) => self.status = err.to_string(),
        }
    }
}

impl SanAndreasModUi
{
    pub(super) fn browse_game_folder(&mut self)
    {
        let mut dialog = rfd::FileDialog::new().set_title("Select GTA San Andreas folder");
        let current = self.game_root();
        if current.is_dir()
        {
            dialog = dialog.set_directory(&current);
        }
        if let Some(path) = dialog.pick_folder()
        {
            self.inputs.game_root = path.display().to_string();
            self.refresh();
        }
    }

    pub(super) fn browse_package_file(&mut self)
    {
        let dialog = rfd::FileDialog::new()
            .set_title("Select a mod package")
            .add_filter("Mod packages", &["zip", "wrap", "7z", "rar"]);
        if let Some(path) = dialog.pick_file()
        {
            self.set_import_path(path);
        }
    }

    pub(super) fn browse_package_folder(&mut self)
    {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Select a mod folder")
            .pick_folder()
        {
            self.set_import_path(path);
        }
    }

    /// Drop analysis results tied to a previously reviewed package. Called
    /// whenever the package path changes so a stale proposal can never be applied
    /// to a different package's config.
    pub(super) fn clear_readme_review(&mut self)
    {
        self.readme_proposals.clear();
        self.analysis_summary = None;
    }

    /// Route a path dropped onto the window: a real GTA install folder fills the
    /// game-folder field; anything else fills the package field and jumps to Import.
    pub(super) fn handle_dropped_path(&mut self, path: PathBuf)
    {
        let dropped_path_is_game_folder = game_executable_path(&path).is_some();
        if path.is_dir() && dropped_path_is_game_folder
        {
            self.inputs.game_root = path.display().to_string();
            self.status = format!("set game folder: {}", path.display());
            self.refresh();
        }
        else
        {
            let display = path.display().to_string();
            self.set_import_path(path);
            self.status = format!("loaded package: {display}");
        }
    }

    fn set_import_path(&mut self, path: PathBuf)
    {
        self.inputs.import_path = path.display().to_string();
        self.clear_readme_review();
        self.tab = crate::ui::state::UiTab::Import;
    }
}

impl SanAndreasModUi
{
    pub(super) fn initialize_state(&mut self)
    {
        self.run_action("initialized manager state", |game_root| {
            init_state(game_root)
        });
    }
}

impl SanAndreasModUi
{
    pub(super) fn add_mod_to_profile(&mut self, mod_id: &str)
    {
        let config_path = self
            .game_root()
            .join(".sa-mod-manager")
            .join("mods")
            .join(mod_id)
            .join("mod.json");
        let profile = self.selected_profile.to_owned();
        self.run_action("added mod to profile", |game_root| {
            add_mod_to_profile_json(game_root, &profile, &config_path)
        });
    }
}

impl SanAndreasModUi
{
    pub(super) fn set_mod_activation(&mut self, mod_id: &str, activation: ProfileModActivation)
    {
        let profile = self.selected_profile.to_owned();
        let mod_id = mod_id.to_string();
        self.run_action("updated mod toggle", |game_root| {
            set_profile_mod_activation(game_root, ProfileModSelection { profile_name: &profile, mod_id: &mod_id }, activation)
        });
    }
}

impl SanAndreasModUi
{
    /// Persist a full drag/button/index reorder of the profile's mods in one
    /// write, then reload so the list reflects the new positions immediately.
    pub(super) fn reorder_profile_mods(&mut self, ordered_ids: Vec<String>)
    {
        let profile = self.selected_profile.to_owned();
        self.run_action("updated load order", move |game_root| {
            set_profile_mod_order_list(game_root, &profile, &ordered_ids)
        });
    }
}

impl SanAndreasModUi
{
    /// Open the per-mod info window (MO2's Mod Info dialog). Gathers the mod's
    /// file list and any readme text once, up front; conflicts and roots are read
    /// live while the window is shown.
    pub(super) fn open_mod_details(&mut self, mod_id: &str)
    {
        let Some(item) = self
            .state
            .mods
            .iter()
            .find(|item| item.config.id == mod_id)
            .cloned()
        else {
            self.record_error(AppError::Usage(format!(
                "mod `{mod_id}` is not in the library"
            )));
            return;
        };
        let source_root = item
            .config
            .source_root
            .clone()
            .filter(|path| path.is_dir())
            .or_else(|| {
                item.config
                    .package
                    .is_dir()
                    .then(|| item.config.package.clone())
            });
        let files_and_readmes = source_root
            .as_deref()
            .map(mod_details_files_and_readmes)
            .unwrap_or_else(ModDetailsFilesAndReadmes::default);
        let files = files_and_readmes.files;
        let readmes = files_and_readmes.readmes;
        let note_edit = self
            .mod_meta
            .get(mod_id)
            .map(|meta| meta.note.clone())
            .unwrap_or_default();
        self.mod_info = Some(ModDetailsView {
            id: mod_id.to_string(),
            tab: ModDetailsTab::Files,
            files,
            readmes,
            source_root,
            note_edit,
            new_category: String::new(),
        });
    }

    /// Set (or clear, with `None`) a mod's color label and persist.
    pub(super) fn set_mod_color(&mut self, mod_id: &str, color: Option<String>)
    {
        self.mod_meta.entry(mod_id.to_string()).or_default().color = color;
        self.persist_mod_meta();
    }

    /// Add the category if absent, remove_profile_fixture it if present, then persist.
    pub(super) fn toggle_mod_category(&mut self, toggle: ModCategoryToggle<'_>)
    {
        let mod_id = toggle.mod_id;
        let category = toggle.category;
        let category = category.trim();
        if category.is_empty()
        {
            return;
        }
        let entry = self.mod_meta.entry(mod_id.to_string()).or_default();
        if let Some(index) = entry
            .categories
            .iter()
            .position(|existing| existing.eq_ignore_ascii_case(category))
        {
            entry.categories.remove(index);
        }
        else
        {
            entry.categories.push(category.to_string());
            entry.categories.sort();
        }
        self.persist_mod_meta();
    }

    pub(super) fn set_mod_note(&mut self, mod_id: &str, note: String)
    {
        self.mod_meta.entry(mod_id.to_string()).or_default().note = note;
        self.persist_mod_meta();
    }

    /// Add a separator at the given display slot (0 = top of the list).
    pub(super) fn add_separator(&mut self, position: usize)
    {
        let id = format!("sep-{}-{}", std::process::id(), unix_now());
        self.separators.push(Separator {
            id: id.clone(),
            name: "New separator".to_string(),
            position,
        });
        self.separators.sort_by_key(|separator| separator.position);
        self.persist_separators();
        // Drop straight into rename so the placeholder name can be replaced.
        self.separator_edit = Some((id, "New separator".to_string()));
    }

    pub(super) fn remove_separator(&mut self, id: &str)
    {
        self.separators.retain(|separator| separator.id != id);
        self.collapsed_separators.remove(id);
        if self
            .separator_edit
            .as_ref()
            .is_some_and(|(edit_id, _)| edit_id == id)
        {
            self.separator_edit = None;
        }
        self.persist_separators();
    }

    pub(super) fn rename_separator(&mut self, id: &str, name: String)
    {
        if let Some(separator) = self.separators.iter_mut().find(|sep| sep.id == id)
        {
            separator.name = name;
        }
        self.separator_edit = None;
        self.persist_separators();
    }

    /// Move a separator to a new display slot, clamped to the list length.
    pub(super) fn move_separator(&mut self, id: &str, position: usize)
    {
        if let Some(separator) = self.separators.iter_mut().find(|sep| sep.id == id)
        {
            separator.position = position;
        }
        self.separators.sort_by_key(|separator| separator.position);
        self.persist_separators();
    }

    pub(super) fn toggle_separator_collapsed(&mut self, id: &str)
    {
        if !self.collapsed_separators.insert(id.to_string())
        {
            self.collapsed_separators.remove(id);
        }
    }

    /// Set a ModLoader folder's priority for this profile (0 = disabled in
    /// ModLoader). Persisted to the profile's sidecar; applied to modloader.ini
    /// via [`Self::apply_modloader_priorities`].
    pub(super) fn set_modloader_priority(&mut self, folder: &str, priority: i32)
    {
        self.modloader_overrides
            .insert(folder.to_string(), priority);
        self.persist_modloader_overrides();
    }

    fn persist_modloader_overrides(&mut self)
    {
        let state_root = state_directory(&self.game_root());
        let profile = self.selected_profile.to_owned();
        if let Err(err) =
            write_modloader_overrides(&state_root, &profile, &self.modloader_overrides)
        {
            self.record_error(err);
        }
    }

    /// Write this profile's ModLoader priority overrides into modloader.ini's
    /// active-profile Priority section.
    pub(super) fn apply_modloader_priorities(&mut self)
    {
        let game_root = self.game_root();
        match apply_modloader_priorities(&game_root, &self.modloader_overrides)
        {
            Ok(()) => {
                self.last_error = None;
                self.status = "applied ModLoader priorities to modloader.ini".to_string();
                if let Err(err) = self.reload_state()
                {
                    self.record_error(err);
                }
            }
            Err(err) => self.record_error(err),
        }
    }

    /// Jump to the Content tab focused on one mod: scan if needed, then filter
    /// the viewer to that mod's files (optionally only its conflicts). Powers the
    /// per-mod "Show files / Show conflicts" cross-link from the load-order list.
    pub(super) fn focus_mod_in_content(&mut self, mod_id: &str, focus: ContentFocus)
    {
        if self.content.index.is_none()
        {
            self.rescan_content();
        }
        self.content.category = None;
        self.content.search = mod_id.to_string();
        self.content.conflicts_only = matches!(focus, ContentFocus::ConflictsOnly);
        self.tab = crate::ui::state::UiTab::Content;
    }

    /// Walk the selected profile's enabled mods (in load order) and rebuild the
    /// content/conflict index. Deferred to an explicit call because it reads
    /// every mod's files; the result is cached until state changes.
    pub(super) fn rescan_content(&mut self)
    {
        let entries = super::widgets::selected_profile_entries(&self.state, &self.selected_profile);
        let mods_by_id: BTreeMap<&str, &super::state::ModConfigItem> = self
            .state
            .mods
            .iter()
            .map(|item| (item.config.id.as_str(), item))
            .collect();

        let mut indexed = Vec::new();
        let mut archive_only = Vec::new();
        for entry in entries.iter().filter(|entry| entry.enabled)
        {
            let Some(item) = mods_by_id.get(entry.id.as_str()) else {
                continue;
            };
            // Prefer the extracted library files; fall back to a folder package.
            // An archive that was never imported has no readable tree to index.
            let source_root = item
                .config
                .source_root
                .clone()
                .filter(|path| path.is_dir())
                .or_else(|| {
                    item.config
                        .package
                        .is_dir()
                        .then(|| item.config.package.clone())
                });
            let Some(source_root) = source_root else {
                archive_only.push(entry.id.clone());
                continue;
            };
            let roots = effective_install_roots(&item.config.install_roots, &entry.root_overrides);
            indexed.push(IndexedMod {
                id: entry.id.clone(),
                source_root,
                roots,
            });
        }

        let mut index = build_content_index(&indexed);
        index.not_indexed.extend(archive_only);
        index.not_indexed.sort();
        index.not_indexed.dedup();
        self.status = format!(
            "content scan: {} files, {} conflicts",
            index.entries.len(),
            index.conflict_count()
        );
        // Files in the game folder's mod areas that no enabled mod provides
        // (MO2's "overwrite") â€” computed from the index's owned target paths.
        let owned: BTreeSet<String> = index
            .entries
            .iter()
            .map(|entry| entry.target.clone())
            .collect();
        self.overwrite_files = collect_overwrite_files(&self.game_root(), &owned);
        self.content.index = Some(index);
        // ModLoader's own priority config governs load order within modloader/;
        // read it alongside the scan so the ModLoader viewer can surface it.
        self.modloader_priorities = read_modloader_priorities(&self.game_root());
        // And what ModLoader actually did last run, for the after-the-fact check.
        self.modloader_log = read_modloader_log(&self.game_root());
        // CLEO health for the installed game folder, surfaced in the CLEO viewer.
        self.cleo_diagnostics = Some(collect_cleo_diagnostics(&self.game_root()));
    }

    fn persist_mod_meta(&mut self)
    {
        let state_root = state_directory(&self.game_root());
        if let Err(err) = write_mod_meta(&state_root, &self.mod_meta)
        {
            self.record_error(err);
        }
    }

    fn persist_separators(&mut self)
    {
        let state_root = state_directory(&self.game_root());
        let profile = self.selected_profile.to_owned();
        if let Err(err) = write_separators(&state_root, &profile, &self.separators)
        {
            self.record_error(err);
        }
    }
}

impl SanAndreasModUi
{
    pub(super) fn remove_mod_from_profile(&mut self, mod_id: &str)
    {
        let profile = self.selected_profile.to_owned();
        let mod_id = mod_id.to_string();
        self.run_action("removed mod from profile", |game_root| {
            remove_mod_from_profile_json(game_root, ProfileModSelection { profile_name: &profile, mod_id: &mod_id })
        });
    }
}
