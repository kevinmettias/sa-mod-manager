impl SanAndreasModUi
{
    /// Per-profile install-root overrides (disable or retarget a single root),
    /// previously reachable only via `profile-root`/`profile-root-target`.
    fn profile_root_overrides_panel(&mut self, ui: &mut egui::Ui, entries: &[ProfileModEntry])
    {
        let mods = self.state.mods.to_vec();
        let has_any = entries.iter().any(|entry| {
            mods.iter()
                .any(|item| item.config.id == entry.id && !item.config.install_roots.is_empty())
        });
        if !has_any
        {
            return;
        }
        ui.separator();
        ui.heading("Per-Profile Install Root Overrides");
        ui.label("Disable or retarget individual install roots for this profile without editing the mod.");
        for entry in entries
        {
            let Some(item) = mods.iter().find(|item| item.config.id == entry.id) else {
                continue;
            };
            if item.config.install_roots.is_empty()
            {
                continue;
            }
            egui::CollapsingHeader::new(format!(
                "{} â€” {} roots",
                entry.id,
                item.config.install_roots.len()
            ))
            .id_salt(format!("root_overrides_{}", entry.id))
            .show(ui, |ui| {
                for root in &item.config.install_roots
                {
                    self.profile_root_override_row(ui, &entry.id, entry, root);
                }
            });
        }
    }

    fn profile_root_override_row(
        &mut self,
        ui: &mut egui::Ui,
        mod_id: &str,
        entry: &ProfileModEntry,
        root: &ModInstallRootJson,
    )
    {
        let key = format!("{mod_id}#{}", root.source);
        let override_entry = entry.root_overrides.get(&root.source);
        let mut enabled = override_entry
            .and_then(|override_value| override_value.enabled)
            .unwrap_or(root.enabled);
        let effective_target = override_entry
            .and_then(|override_value| override_value.target.clone())
            .unwrap_or_else(|| root.target.clone());
        let mut enabled_changed = false;
        let mut save_target = None;
        // Borrow the edit buffer up front so the closure never touches `self`.
        let target_buf = self
            .profile_root_target_edits
            .entry(key.clone())
            .or_insert(effective_target);
        ui.horizontal(|ui| {
            enabled_changed = ui
                .checkbox(&mut enabled, "")
                .on_hover_text("Enable this install root for this profile")
                .changed();
            ui.label(&root.source);
            ui.label("â†’");
            ui.add_sized(
                [ROOT_OVERRIDE_TARGET_WIDTH, ROW_HEIGHT],
                egui::TextEdit::singleline(target_buf),
            );
            if ui
                .button("Retarget")
                .on_hover_text("Point this root at a different game target for this profile")
                .clicked()
            {
                save_target = Some(target_buf.clone());
            }
        });
        if enabled_changed
        {
            let state = ProfileRootEnabledState::from_bool(enabled);
            self.set_profile_root_enabled(ProfileRootToggle { mod_id: mod_id, source: &root.source, state: state });
        }
        if let Some(target) = save_target
        {
            self.save_profile_root_target(ProfileRootTargetEdit { mod_id: mod_id, source: &root.source, target: &target });
            self.profile_root_target_edits.remove(&key);
        }
    }
}
