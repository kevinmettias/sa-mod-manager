
impl SanAndreasModUi
{
    /// A Mod-Organizer-style ordered mod list: a drag handle for seamless
    /// drag-to-reorder, â–²/â–¼ nudges, a directly editable priority index, and a
    /// per-mod enable checkbox. All mutations are collected during the immutable
    /// pass over `entries` and applied afterwards, so `self` is never borrowed
    /// mutably while the rows are drawn.
    /// Resolve which mods the filter bar leaves visible. Returns `None` when no
    /// filter is active, which keeps the list fully reorderable; `Some(set)` of
    /// mod ids otherwise. Category/conflict filters consult the last content scan.
    fn compute_visible_mods(
        &self,
        entries: &[ProfileModEntry],
    ) -> Option<std::collections::BTreeSet<String>>
    {
        let text = self.filters.mod_text.trim().to_ascii_lowercase();
        let filtering = !text.is_empty()
            || self.filters.mod_status != ModStatusFilter::All
            || self.filters.mod_category.is_some()
            || self.filters.mod_conflicts
            || self.filters.mod_user_category.is_some();
        if !filtering
        {
            return None;
        }
        let flags = self.content.index.as_ref().map(per_mod_flags);
        let set = entries
            .iter()
            .filter(|entry| {
                let text_ok = text.is_empty() || entry.id.to_ascii_lowercase().contains(&text);
                let status_ok = match self.filters.mod_status {
                    ModStatusFilter::All => true,
                    ModStatusFilter::Enabled => entry.enabled,
                    ModStatusFilter::Disabled => !entry.enabled,
                };
                let mod_flags = flags.as_ref().and_then(|map| map.get(&entry.id));
                let category_ok = self.filters.mod_category.is_none_or(|category| {
                    mod_flags.is_some_and(|flags| flags.categories.contains(&category))
                });
                let conflict_ok = !self.filters.mod_conflicts
                    || mod_flags.is_some_and(|flags| flags.overwrites_others || flags.overwritten);
                let user_category_ok =
                    self.filters.mod_user_category.as_ref().is_none_or(|wanted| {
                        self.mod_meta.get(&entry.id).is_some_and(|meta| {
                            meta.categories
                                .iter()
                                .any(|category| category.eq_ignore_ascii_case(wanted))
                        })
                    });
                text_ok && status_ok && category_ok && conflict_ok && user_category_ok
            })
            .map(|entry| entry.id.clone())
            .collect();
        return Some(set);
    }

}
