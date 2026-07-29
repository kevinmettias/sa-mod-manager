struct ProfileOrderListActions
{
    activation: Option<(String, bool)>,
    reorder: Option<Vec<String>>,
    remove_profile_fixture: Option<String>,
    focus: Option<(String, bool)>,
    details: Option<String>,
    separator_edit: Option<(String, String)>,
    separator_start_edit: Option<String>,
    separator_collapse: Option<String>,
    separator_rename: Option<(String, String)>,
    separator_move: Option<(String, usize)>,
    separator_remove: Option<String>,
}

impl SanAndreasModUi
{
    fn apply_profile_order_list_actions(&mut self, actions: ProfileOrderListActions)
    {
        if let Some((mod_id, enabled)) = actions.activation
        {
            let activation = if enabled {
                ProfileModActivation::Enabled
            } else {
                ProfileModActivation::Disabled
            };
            self.set_mod_activation(&mod_id, activation);
        }
        if let Some(ordered_ids) = actions.reorder
        {
            self.reorder_profile_mods(ordered_ids);
        }
        if let Some(mod_id) = actions.remove_profile_fixture
        {
            self.request_remove_mod(&mod_id);
        }
        if let Some((mod_id, conflicts_only)) = actions.focus
        {
            let focus = if conflicts_only {
                ContentFocus::ConflictsOnly
            } else {
                ContentFocus::AllFiles
            };
            self.focus_mod_in_content(&mod_id, focus);
        }
        if let Some(mod_id) = actions.details
        {
            self.open_mod_details(&mod_id);
        }
        self.separator_edit = actions.separator_edit;
        if let Some(id) = actions.separator_start_edit
        {
            let name = self
                .separators
                .iter()
                .find(|sep| sep.id == id)
                .map(|sep| sep.name.clone())
                .unwrap_or_default();
            self.separator_edit = Some((id, name));
        }
        if let Some(id) = actions.separator_collapse
        {
            self.toggle_separator_collapsed(&id);
        }
        if let Some((id, name)) = actions.separator_rename
        {
            self.rename_separator(&id, name);
        }
        if let Some((id, position)) = actions.separator_move
        {
            self.move_separator(&id, position);
        }
        if let Some(id) = actions.separator_remove
        {
            self.remove_separator(&id);
        }
    }
}
