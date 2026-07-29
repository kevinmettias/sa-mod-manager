fn write_modloader_priorities_for_run(
    context: ModLoaderPriorityRunContext<'_>,
    journal: &mut fs::File,
) -> Result<(), AppError>
{
    let ini_path = context.game_root.join("modloader").join("modloader.ini");
    let existing = read_capped(&ini_path, MAX_CONTROL_FILE_BYTES).ok();
    let existing_ref = existing.as_deref().unwrap_or("");
    let limit = modloader_priority_limit(existing_ref);

    let mut folder_priorities = modloader_folder_priorities(context.entries, limit)?;
    if folder_priorities.is_empty()
    {
        return Ok(());
    }
    // Disabled modloader mods still sitting in `modloader/` (a persistent install)
    // are expressed as IgnoreMods so ModLoader skips them; never ignore a folder an
    // enabled mod is actively using.
    let mut ignore_folders =
        disabled_modloader_folders(context.game_root, context.profile_name, &folder_priorities)?;
    // Layer the profile's ModLoader priority-panel overrides on top of the
    // load-order-derived defaults: a positive value overrides that folder's
    // priority, while 0 disables it *in ModLoader* (via IgnoreMods) even though the
    // mod stays manager-enabled and its files are installed.
    let overrides =
        read_modloader_overrides(&state_directory(context.game_root), context.profile_name);
    for (folder, priority) in &overrides
    {
        let Some(key) = folder_priorities
            .keys()
            .find(|existing| existing.eq_ignore_ascii_case(folder))
            .cloned()
        else {
            continue;
        };
        if *priority <= 0
        {
            folder_priorities.remove(&key);
            if !ignore_folders.iter().any(|f| f.eq_ignore_ascii_case(&key))
            {
                ignore_folders.push(key);
            }
        }
        else
        {
            let clamped_priority = (*priority).clamp(1, limit);
            folder_priorities.insert(key, clamped_priority);
        }
    }
    // Per-file exclusion globs the profile declares (a hand-added `ignore_files`
    // list), mapped to ModLoader's own `[IgnoreFiles]` so one file can be hidden
    // inside a mod without editing the mod.
    let ignore_files = profile_ignore_files(context.game_root, context.profile_name)?;
    let managed = modloader_managed_profile_name(context.profile_name);
    // Inherit the user's own active profile so their hand-set priorities and
    // ignore lists still apply; our section only layers the managed mods on top.
    let parent = modloader_active_profile(existing_ref);
    let Some(rendered) = render_modloader_managed_profile(ModLoaderManagedProfileRender {
        existing: existing.as_deref(),
        profile: &managed,
        parent: &parent,
        limit,
        folder_priorities: &folder_priorities,
        ignore_folders: &ignore_folders,
        ignore_files: &ignore_files,
    }) else {
        return Ok(());
    };
    let mut copy_context = CopyJournalContext {
        game_root: context.game_root,
        backup_root: context.backup_root,
        journal,
    };
    return apply_generated_file_with_journal(&rendered, &ini_path, &mut copy_context);
}

/// Assign each modloader-sandboxed mod a ModLoader priority from its rank in the
/// (already load-order-sorted) profile: later mods get higher priority so they
/// win, matching how later mods overwrite in the manager's own materialize order.
/// Mods with no `modloader/<folder>` target contribute nothing.
fn modloader_folder_priorities(
    entries: &[ProfileModEntry],
    limit: i32,
) -> Result<BTreeMap<String, i32>, AppError>
{
    let mut mods_folders: Vec<BTreeSet<String>> = Vec::new();
    for entry in entries
    {
        let config = read_mod_config_json(&entry.config)?;
        if !config.enabled
        {
            continue;
        }
        let folders = modloader_folders_of(&config, &entry.root_overrides);
        if !folders.is_empty()
        {
            mods_folders.push(folders);
        }
    }

    let count = mods_folders.len();
    let mut folder_priorities = BTreeMap::new();
    for (rank, folders) in mods_folders.iter().enumerate()
    {
        let priority = spread_priority(rank, count, limit);
        for folder in folders
        {
            folder_priorities.insert(folder.clone(), priority);
        }
    }
    return Ok(folder_priorities);
}

/// Folders of modloader mods the profile has **disabled**, so ModLoader can be
/// told to ignore them even when they persist on disk. Excludes any folder an
/// enabled mod uses, so a shared folder is never ignored out from under an active
/// mod. Reads the full (unfiltered) profile; a disabled entry whose config is
/// missing or unreadable is skipped rather than failing the run.
fn disabled_modloader_folders(
    game_root: &Path,
    profile_name: &str,
    enabled_priorities: &BTreeMap<String, i32>,
) -> Result<Vec<String>, AppError>
{
    let profile_path = state_directory(game_root)
        .join("profiles")
        .join(format!("{profile_name}.json"));
    let profile = read_profile_json(&profile_path)?;
    let enabled: BTreeSet<String> = enabled_priorities.keys().cloned().collect();
    let mut ignore: BTreeSet<String> = BTreeSet::new();
    for entry in &profile.mods
    {
        if entry.enabled
        {
            continue;
        }
        let Ok(config) = read_mod_config_json(&entry.config) else {
            continue;
        };
        for folder in modloader_folders_of(&config, &entry.root_overrides)
        {
            if !enabled.contains(&folder)
            {
                ignore.insert(folder);
            }
        }
    }
    return Ok(ignore.into_iter().collect());
}

/// Per-file exclusion globs declared by the profile's hand-added `ignore_files`
/// array (preserved verbatim in the profile's `extra` map). Each becomes a
/// `[Profiles.<name>.IgnoreFiles]` entry; ModLoader matches them against a file's
/// basename and its mod-relative path (`*`/`?` globs, case-insensitive). Absent or
/// malformed → no exclusions.
fn profile_ignore_files(game_root: &Path, profile_name: &str) -> Result<Vec<String>, AppError>
{
    let profile_path = state_directory(game_root)
        .join("profiles")
        .join(format!("{profile_name}.json"));
    let profile = read_profile_json(&profile_path)?;
    return Ok(extract_ignore_files(&profile.extra));
}

/// Pull a `["*.dff", …]` string array out of the profile's unmodeled `extra`,
/// trimming blanks. Anything that is not an array of strings is ignored.
fn extract_ignore_files(extra: &BTreeMap<String, serde_json::Value>) -> Vec<String>
{
    return extra
        .get("ignore_files")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
}

/// The `-modprof` profile name to activate for this run, or `None` when no
/// managed profile is present to activate. Decided by the just-written
/// `modloader.ini` itself — the presence of our `[Profiles.<name>.Priority]`
/// section is exactly the condition for activating it — so the launch argument
/// can never disagree with what was written, and an unreadable/absent ini simply
/// means "don't pass -modprof" (a no-op for the game either way).
/// Whether the user's launch args already pick a ModLoader mode (`-nomods`,
/// `-mod`, or `-modprof`). ModLoader treats these as mutually exclusive, so the
/// manager must not append its own `-modprof` on top — it would be ignored, and
/// silently overriding an explicit `-nomods` would be worse. Matched
/// case-insensitively, like ModLoader's own `_wcsicmp` parsing.
fn launch_args_select_modloader_mode(args: &[String]) -> bool
{
    return args.iter().any(|arg| {
        let arg = arg.trim();
        arg.eq_ignore_ascii_case("-nomods")
            || arg.eq_ignore_ascii_case("-mod")
            || arg.eq_ignore_ascii_case("-modprof")
    });
}

fn modloader_run_profile(game_root: &Path, profile_name: &str) -> Option<String>
{
    let managed = modloader_managed_profile_name(profile_name);
    let ini_path = game_root.join("modloader").join("modloader.ini");
    let has_section = read_capped(&ini_path, MAX_CONTROL_FILE_BYTES)
        .map(|text| text.contains(&format!("[Profiles.{managed}.Priority]")))
        .unwrap_or(false);
    return has_section.then_some(managed);
}

/// The `modloader/<folder>` sandbox folders a mod config installs into, after the
/// profile's per-root overrides are applied.
fn modloader_folders_of(
    config: &ModConfigJson,
    overrides: &BTreeMap<String, ProfileRootOverride>,
) -> BTreeSet<String>
{
    let roots = enabled_install_roots(config.install_roots.clone(), overrides);
    return roots
        .iter()
        .filter_map(|root| modloader_folder_from_target(&root.target))
        .collect();
}

#[cfg(test)]
mod tests
{
    include!("part_02_tests_01.rs");
    include!("part_02_tests_02.rs");
}
