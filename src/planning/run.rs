use crate::prelude::*;

use super::content::{
    modloader_active_profile, modloader_folder_from_target, modloader_managed_profile_name,
    modloader_priority_limit, render_modloader_managed_profile, spread_priority,
};
use super::copy_journal::{
    apply_copy_tree_with_journal, apply_generated_file_with_journal, sync_journal,
};
use super::rollback::rollback_journal;

pub(crate) fn prepare_run(game_root: &Path, profile_name: &str) -> Result<(), AppError> {
    let (mut launch_args, launch_env) = profile_launch_settings(game_root, profile_name)?;
    let journal_path = materialize_profile_for_run(game_root, profile_name)?;
    // Activate the manager-owned ModLoader profile written during materialize so
    // ModLoader applies this profile's per-folder priority natively for the run —
    // unless the user's own launch args already choose a ModLoader mode. `-nomods`,
    // `-mod`, and `-modprof` are mutually exclusive (nomods > modprof > mod), so a
    // second `-modprof` would be dead; respect the explicit choice instead.
    if !launch_args_select_modloader_mode(&launch_args) {
        if let Some(managed) = modloader_run_profile(game_root, profile_name) {
            launch_args.push("-modprof".to_string());
            launch_args.push(managed);
        }
    }
    let started_unix = unix_now();
    let status_result = launch_game_and_wait(game_root, &launch_args, &launch_env);
    // Record how the launch actually went before rolling the run back, so a
    // failed launch is distinguishable from a real play session in telemetry.
    record_run_outcome(
        game_root,
        &journal_path,
        profile_name,
        &launch_args,
        started_unix,
        &status_result,
    );
    let launch_result = interpret_launch_status(status_result);
    let rollback_result = rollback_journal(&journal_path, game_root);
    launch_result?;
    rollback_result
}

fn record_run_outcome(
    game_root: &Path,
    journal_path: &Path,
    profile_name: &str,
    launch_args: &[String],
    started_unix: u64,
    status_result: &Result<ExitStatus, AppError>,
) {
    let finished_unix = unix_now();
    let (result, exit_code) = match status_result {
        Ok(status) if status.success() => (RUN_RESULT_SUCCESS, status.code()),
        Ok(status) => (RUN_RESULT_GAME_ERROR, status.code()),
        Err(_) => (RUN_RESULT_LAUNCH_FAILED, None),
    };
    let outcome = RunOutcome {
        version: 1,
        txid: txid_from_journal(journal_path),
        profile: profile_name.to_string(),
        result: result.to_string(),
        exit_code,
        duration_ms: Some(finished_unix.saturating_sub(started_unix).saturating_mul(1000)),
        launch_args: launch_args.to_vec(),
        started_unix,
        finished_unix,
    };
    // Telemetry is best-effort: a failed write must not fail the run itself.
    if let Err(err) = write_run_outcome(&state_directory(game_root), &outcome) {
        log_warn!("could not record run outcome: {err}");
    }
}

/// Map a finished launch to the run's result. A non-zero game exit and a spawn
/// failure both surface as errors, matching the previous behavior.
fn interpret_launch_status(status_result: Result<ExitStatus, AppError>) -> Result<(), AppError> {
    match status_result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(AppError::Usage(format!("game exited with status: {status}"))),
        Err(err) => Err(err),
    }
}

fn launch_game_and_wait(
    game_root: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<ExitStatus, AppError> {
    let exe = game_executable(game_root).ok_or_else(|| {
        AppError::Usage(format!(
            "game executable not found under {}",
            game_root.display()
        ))
    })?;
    let status = Command::new(&exe)
        .current_dir(game_root)
        .args(args)
        .envs(env)
        .status()?;
    Ok(status)
}

fn game_executable(game_root: &Path) -> Option<PathBuf> {
    game_executable_path(game_root)
}

pub(crate) fn materialize_profile_for_run(
    game_root: &Path,
    profile_name: &str,
) -> Result<PathBuf, AppError> {
    ensure_gta_install(game_root)?;
    ensure_state(game_root)?;
    let profile = read_enabled_profile(game_root, profile_name)?;
    let run_state = create_run_state(game_root, profile_name)?;
    let mut journal = fs::File::create(&run_state.journal_path)?;
    write_run_journal_header(&mut journal, &run_state.txid, profile_name)?;

    let materialize_result = apply_profile_mods_for_run(
        &profile.mods,
        game_root,
        &run_state.backup_root,
        &mut journal,
    );
    if let Err(err) = materialize_result {
        return Err(rollback_failed_materialization(
            game_root,
            &run_state.journal_path,
            journal,
            err,
        ));
    }

    // Reflect the profile's load order into ModLoader's own per-folder priority,
    // so the order the user set actually governs which mod wins at runtime — for
    // sandboxed modloader mods, copy order into distinct folders decides nothing.
    // Journaled like any other write, so an ephemeral run's rollback restores the
    // prior modloader.ini.
    if let Err(err) = write_modloader_priorities_for_run(
        &profile.mods,
        profile_name,
        game_root,
        &run_state.backup_root,
        &mut journal,
    ) {
        return Err(rollback_failed_materialization(
            game_root,
            &run_state.journal_path,
            journal,
            err,
        ));
    }

    sync_journal(&mut journal)?;
    log_info!("prepared ephemeral run: {profile_name}");
    log_info!("journal: {}", run_state.journal_path.display());
    Ok(run_state.journal_path)
}

fn apply_profile_mods_for_run(
    entries: &[ProfileModEntry],
    game_root: &Path,
    backup_root: &Path,
    journal: &mut fs::File,
) -> Result<(), AppError> {
    for entry in entries {
        write_profile_mod_start(journal, entry)?;
        apply_profile_mod_for_run(entry, game_root, backup_root, journal)?;
    }
    Ok(())
}

/// Roll back a partially-materialized run and return the error to surface.
///
/// Returns the original `materialize_error` when rollback succeeds, or a
/// combined error when rollback itself fails. Returning [`AppError`] directly
/// (rather than `Result`) makes "this function always yields an error" a
/// type-level guarantee, so the caller needs no `unreachable!`.
fn rollback_failed_materialization(
    game_root: &Path,
    journal_path: &Path,
    mut journal: fs::File,
    materialize_error: AppError,
) -> AppError {
    let flush_result = journal.flush();
    drop(journal);

    let rollback_result = match flush_result {
        Ok(()) => rollback_journal(journal_path, game_root),
        Err(err) => Err(AppError::Io(err)),
    };

    match rollback_result {
        Ok(()) => materialize_error,
        Err(rollback_error) => AppError::Usage(format!(
            "failed to materialize profile: {materialize_error}; rollback also failed: {rollback_error}; journal: {}",
            journal_path.display()
        )),
    }
}

fn read_enabled_profile(game_root: &Path, profile_name: &str) -> Result<ProfileJson, AppError> {
    let profile_path = state_directory(game_root)
        .join("profiles") // literal: allow external interface text or file-format spelling
        .join(format!("{profile_name}.json"));
    let mut profile = read_profile_json(&profile_path)?;
    profile.mods.retain(|entry| entry.enabled);
    profile.mods.sort_by(|a, b| {
        a.load_order
            .cmp(&b.load_order)
            .then_with(|| a.id.cmp(&b.id))
    });
    validate_profile_mods(&profile)?;
    Ok(profile)
}

fn validate_profile_mods(profile: &ProfileJson) -> Result<(), AppError> {
    let mut ids = BTreeSet::new();
    let mut configs = BTreeSet::new();
    for entry in &profile.mods {
        if !ids.insert(entry.id.clone()) {
            return Err(AppError::Usage(format!(
                "profile `{}` contains duplicate enabled mod id `{}`",
                profile.name, entry.id
            )));
        }
        let config = normalize_path(&entry.config.display().to_string());
        if !configs.insert(config.clone()) {
            return Err(AppError::Usage(format!(
                "profile `{}` contains duplicate enabled mod config `{config}`",
                profile.name
            )));
        }
        if !entry.config.exists() {
            return Err(AppError::Usage(format!(
                "profile `{}` references missing mod config for `{}`: {}",
                profile.name,
                entry.id,
                entry.config.display()
            )));
        }
    }
    Ok(())
}

fn create_run_state(game_root: &Path, profile_name: &str) -> Result<RunApplyState, AppError> {
    let txid = format!("run-{}-{}", safe_name(profile_name), unix_now());
    let journal_path = state_directory(game_root)
        .join("journals") // literal: allow external interface text or file-format spelling
        .join(format!("{txid}.journal"));
    let backup_root = state_directory(game_root).join("backups").join(&txid); // literal: allow external interface text or file-format spelling
    fs::create_dir_all(&backup_root)?;
    Ok(RunApplyState {
        txid,
        journal_path,
        backup_root,
    })
}

fn write_run_journal_header(
    journal: &mut fs::File,
    txid: &str,
    profile_name: &str,
) -> Result<(), AppError> {
    writeln!(journal, "version=1")?;
    writeln!(journal, "txid={}", escape_value(txid))?;
    writeln!(journal, "profile={}", escape_value(profile_name))?;
    writeln!(journal, "mode=ephemeral-run")?;
    writeln!(journal, "created_unix={}", unix_now())?;
    Ok(())
}

fn write_profile_mod_start(
    journal: &mut fs::File,
    entry: &ProfileModEntry,
) -> Result<(), AppError> {
    writeln!(
        journal,
        "profile_mod={}|{}|{}",
        escape_value(&entry.id),
        entry.load_order,
        escape_value(&entry.config.display().to_string())
    )?;
    Ok(())
}

fn apply_profile_mod_for_run(
    entry: &ProfileModEntry,
    game_root: &Path,
    backup_root: &Path,
    journal: &mut fs::File,
) -> Result<(), AppError> {
    let config = read_mod_config_json(&entry.config)?;
    if !config.enabled {
        return Ok(());
    }
    let staging_root = staging_root_for_mod_config(&config, game_root)?;
    let roots = enabled_install_roots(config.install_roots, &entry.root_overrides);
    let mut run_context = RunInstallContext {
        game_root,
        backup_root,
        journal,
    };
    for root in roots {
        apply_run_install_root(&root, &staging_root, &mut run_context)?;
    }
    Ok(())
}

fn staging_root_for_mod_config(
    config: &ModConfigJson,
    game_root: &Path,
) -> Result<PathBuf, AppError> {
    if let Some(source_root) = &config.source_root {
        Ok(source_root.clone())
    } else if config.package.is_dir() {
        Ok(config.package.clone())
    } else {
        extract_archive_to_named_staging(&config.package, game_root, &config.id)
    }
}

fn enabled_install_roots(
    mut roots: Vec<ModInstallRootJson>,
    overrides: &BTreeMap<String, ProfileRootOverride>,
) -> Vec<ModInstallRootJson> {
    // A per-profile override for a root's `source` wins over the mod's own
    // config, letting a profile toggle a shared mod's roots or retarget them.
    roots.retain(|root| {
        overrides
            .get(&root.source)
            .and_then(|over| over.enabled)
            .unwrap_or(root.enabled)
    });
    for root in &mut roots {
        if let Some(target) = overrides.get(&root.source).and_then(|over| over.target.clone()) {
            root.target = target;
        }
    }
    roots.sort_by(|a, b| a.source.cmp(&b.source));
    roots
}

fn apply_run_install_root(
    root: &ModInstallRootJson,
    staging_root: &Path,
    context: &mut RunInstallContext,
) -> Result<(), AppError> {
    /* literal: allow external interface text or file-format spelling */
    /* literal: allow external interface text or file-format spelling */
    if root.kind.eq_ignore_ascii_case("bootstrap") {
        // literal: allow external interface text or file-format spelling
        write_blocked_bootstrap(context.journal, &root.source, &root.target)?;
        return Ok(());
    }

    let source_abs = staging_root.join(path_from_package_root(&root.source)?);
    if !source_abs.exists() {
        write_missing_source(context.journal, &source_abs)?;
        return Ok(());
    }
    let target_root = context
        .game_root
        .join(path_from_package_root(&root.target)?);
    let mut copy_context = CopyJournalContext {
        game_root: context.game_root,
        backup_root: context.backup_root,
        journal: context.journal,
    };
    apply_copy_tree_with_journal(&source_abs, &target_root, &mut copy_context)
}

fn write_blocked_bootstrap(
    journal: &mut fs::File,
    source: &str,
    target: &str,
) -> Result<(), AppError> {
    writeln!(
        journal,
        "blocked_bootstrap={}|{}",
        escape_value(source),
        escape_value(target)
    )?;
    Ok(())
}

fn write_missing_source(journal: &mut fs::File, source_abs: &Path) -> Result<(), AppError> {
    writeln!(
        journal,
        "missing_source={}",
        escape_value(&source_abs.display().to_string())
    )?;
    Ok(())
}

/// Express the profile's load order as a native ModLoader profile in
/// `modloader/modloader.ini`, so ModLoader's own per-folder priority — the thing
/// that actually decides which sandboxed mod wins a shared asset — matches the
/// order the user set. Written into a manager-owned `SAMM_<profile>` profile
/// (inheriting the user's `Default`) and activated per launch with `-modprof`, so
/// the user's own ModLoader config is never disturbed. A no-op when the profile
/// has no modloader-sandboxed mods or the file already reflects the same order.
fn write_modloader_priorities_for_run(
    entries: &[ProfileModEntry],
    profile_name: &str,
    game_root: &Path,
    backup_root: &Path,
    journal: &mut fs::File,
) -> Result<(), AppError> {
    let ini_path = game_root.join("modloader").join("modloader.ini");
    let existing = read_capped(&ini_path, MAX_CONTROL_FILE_BYTES).ok();
    let existing_ref = existing.as_deref().unwrap_or("");
    let limit = modloader_priority_limit(existing_ref);

    let mut folder_priorities = modloader_folder_priorities(entries, limit)?;
    if folder_priorities.is_empty() {
        return Ok(());
    }
    // Disabled modloader mods still sitting in `modloader/` (a persistent install)
    // are expressed as IgnoreMods so ModLoader skips them; never ignore a folder an
    // enabled mod is actively using.
    let mut ignore_folders =
        disabled_modloader_folders(game_root, profile_name, &folder_priorities)?;
    // Layer the profile's ModLoader priority-panel overrides on top of the
    // load-order-derived defaults: a positive value overrides that folder's
    // priority, while 0 disables it *in ModLoader* (via IgnoreMods) even though the
    // mod stays manager-enabled and its files are installed.
    let overrides = read_modloader_overrides(&state_directory(game_root), profile_name);
    for (folder, priority) in &overrides {
        let Some(key) = folder_priorities
            .keys()
            .find(|existing| existing.eq_ignore_ascii_case(folder))
            .cloned()
        else {
            continue;
        };
        if *priority <= 0 {
            folder_priorities.remove(&key);
            if !ignore_folders.iter().any(|f| f.eq_ignore_ascii_case(&key)) {
                ignore_folders.push(key);
            }
        } else {
            folder_priorities.insert(key, (*priority).clamp(1, limit));
        }
    }
    // Per-file exclusion globs the profile declares (a hand-added `ignore_files`
    // list), mapped to ModLoader's own `[IgnoreFiles]` so one file can be hidden
    // inside a mod without editing the mod.
    let ignore_files = profile_ignore_files(game_root, profile_name)?;
    let managed = modloader_managed_profile_name(profile_name);
    // Inherit the user's own active profile so their hand-set priorities and
    // ignore lists still apply; our section only layers the managed mods on top.
    let parent = modloader_active_profile(existing_ref);
    let Some(rendered) = render_modloader_managed_profile(
        existing.as_deref(),
        &managed,
        &parent,
        limit,
        &folder_priorities,
        &ignore_folders,
        &ignore_files,
    ) else {
        return Ok(());
    };
    let mut context = CopyJournalContext {
        game_root,
        backup_root,
        journal,
    };
    apply_generated_file_with_journal(&rendered, &ini_path, &mut context)
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
fn launch_args_select_modloader_mode(args: &[String]) -> bool {
    args.iter().any(|arg| {
        let arg = arg.trim();
        arg.eq_ignore_ascii_case("-nomods")
            || arg.eq_ignore_ascii_case("-mod")
            || arg.eq_ignore_ascii_case("-modprof")
    })
}

fn modloader_run_profile(game_root: &Path, profile_name: &str) -> Option<String> {
    let managed = modloader_managed_profile_name(profile_name);
    let ini_path = game_root.join("modloader").join("modloader.ini");
    let has_section = read_capped(&ini_path, MAX_CONTROL_FILE_BYTES)
        .map(|text| text.contains(&format!("[Profiles.{managed}.Priority]")))
        .unwrap_or(false);
    has_section.then_some(managed)
}

/// Assign each modloader-sandboxed mod a ModLoader priority from its rank in the
/// (already load-order-sorted) profile: later mods get higher priority so they
/// win, matching how later mods overwrite in the manager's own materialize order.
/// Mods with no `modloader/<folder>` target contribute nothing.
fn modloader_folder_priorities(
    entries: &[ProfileModEntry],
    limit: i32,
) -> Result<BTreeMap<String, i32>, AppError> {
    let mut mods_folders: Vec<BTreeSet<String>> = Vec::new();
    for entry in entries {
        let config = read_mod_config_json(&entry.config)?;
        if !config.enabled {
            continue;
        }
        let folders = modloader_folders_of(&config, &entry.root_overrides);
        if !folders.is_empty() {
            mods_folders.push(folders);
        }
    }

    let count = mods_folders.len();
    let mut folder_priorities = BTreeMap::new();
    for (rank, folders) in mods_folders.iter().enumerate() {
        let priority = spread_priority(rank, count, limit);
        for folder in folders {
            folder_priorities.insert(folder.clone(), priority);
        }
    }
    Ok(folder_priorities)
}

/// The `modloader/<folder>` sandbox folders a mod config installs into, after the
/// profile's per-root overrides are applied.
fn modloader_folders_of(
    config: &ModConfigJson,
    overrides: &BTreeMap<String, ProfileRootOverride>,
) -> BTreeSet<String> {
    let roots = enabled_install_roots(config.install_roots.clone(), overrides);
    roots
        .iter()
        .filter_map(|root| modloader_folder_from_target(&root.target))
        .collect()
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
) -> Result<Vec<String>, AppError> {
    let profile_path = state_directory(game_root)
        .join("profiles")
        .join(format!("{profile_name}.json"));
    let profile = read_profile_json(&profile_path)?;
    let enabled: BTreeSet<String> = enabled_priorities.keys().cloned().collect();
    let mut ignore: BTreeSet<String> = BTreeSet::new();
    for entry in &profile.mods {
        if entry.enabled {
            continue;
        }
        let Ok(config) = read_mod_config_json(&entry.config) else {
            continue;
        };
        for folder in modloader_folders_of(&config, &entry.root_overrides) {
            if !enabled.contains(&folder) {
                ignore.insert(folder);
            }
        }
    }
    Ok(ignore.into_iter().collect())
}

/// Per-file exclusion globs declared by the profile's hand-added `ignore_files`
/// array (preserved verbatim in the profile's `extra` map). Each becomes a
/// `[Profiles.<name>.IgnoreFiles]` entry; ModLoader matches them against a file's
/// basename and its mod-relative path (`*`/`?` globs, case-insensitive). Absent or
/// malformed → no exclusions.
fn profile_ignore_files(game_root: &Path, profile_name: &str) -> Result<Vec<String>, AppError> {
    let profile_path = state_directory(game_root)
        .join("profiles")
        .join(format!("{profile_name}.json"));
    let profile = read_profile_json(&profile_path)?;
    Ok(extract_ignore_files(&profile.extra))
}

/// Pull a `["*.dff", …]` string array out of the profile's unmodeled `extra`,
/// trimming blanks. Anything that is not an array of strings is ignored.
fn extract_ignore_files(extra: &BTreeMap<String, serde_json::Value>) -> Vec<String> {
    extra
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
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_modloader_flags_suppress_our_modprof() {
        // A user who set any ModLoader mode keeps it; we don't append a dead -modprof.
        for arg in ["-nomods", "-NoMods", "-mod", "-modprof"] {
            assert!(launch_args_select_modloader_mode(&[arg.to_string()]), "{arg}");
        }
        // Unrelated args leave us free to add -modprof.
        assert!(!launch_args_select_modloader_mode(&["-nointro".to_string(), "-windowed".to_string()]));
        assert!(!launch_args_select_modloader_mode(&[]));
    }

    #[test]
    fn ignore_files_are_pulled_from_profile_extra() {
        let mut extra = BTreeMap::new();
        extra.insert(
            "ignore_files".to_string(),
            serde_json::json!(["*.dff", "  to_ignore/x.txd  ", "", 42]),
        );
        let files = extract_ignore_files(&extra);
        // Strings are trimmed and kept; blanks and non-strings dropped.
        assert_eq!(files, vec!["*.dff".to_string(), "to_ignore/x.txd".to_string()]);
        // No key -> empty.
        assert!(extract_ignore_files(&BTreeMap::new()).is_empty());
    }

    #[test]
    fn materialize_profile_rolls_back_files_when_later_mod_fails() {
        let game_root = test_root("materialize_rollback");
        let first_source = game_root.join("sources").join("first");
        let second_source = game_root.join("sources").join("second");
        let first_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("first")
            .join("mod.json");
        let second_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("second")
            .join("mod.json");

        fs::create_dir_all(first_source.join("payload")).unwrap();
        fs::create_dir_all(second_source.join("payload")).unwrap();
        fs::write(first_source.join("payload").join("first.txt"), "first").unwrap();
        fs::write(second_source.join("payload").join("second.txt"), "second").unwrap();

        ensure_state(&game_root).unwrap();
        write_test_mod_config(
            &first_config,
            "first",
            &first_source,
            "payload",
            "modloader/first",
        );
        write_test_mod_config(
            &second_config,
            "second",
            &second_source,
            "payload",
            "../bad",
        );
        write_test_profile(&game_root, &first_config, &second_config);

        let result = materialize_profile_for_run(&game_root, "default");

        assert!(result.is_err());
        assert!(
            !game_root
                .join("modloader")
                .join("first")
                .join("first.txt")
                .exists()
        );
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn materialize_profile_applies_enabled_mods_in_load_order() {
        let game_root = test_root("materialize_load_order");
        let early_source = game_root.join("sources").join("early");
        let late_source = game_root.join("sources").join("late");
        let disabled_source = game_root.join("sources").join("disabled");
        let early_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("early")
            .join("mod.json");
        let late_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("late")
            .join("mod.json");
        let disabled_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("disabled")
            .join("mod.json");

        fs::create_dir_all(early_source.join("payload")).unwrap();
        fs::create_dir_all(late_source.join("payload")).unwrap();
        fs::create_dir_all(disabled_source.join("payload")).unwrap();
        fs::write(early_source.join("payload").join("shared.txt"), "early").unwrap();
        fs::write(late_source.join("payload").join("shared.txt"), "late").unwrap();
        fs::write(
            disabled_source.join("payload").join("disabled.txt"),
            "disabled",
        )
        .unwrap();

        ensure_state(&game_root).unwrap();
        write_test_mod_config(
            &early_config,
            "early",
            &early_source,
            "payload",
            "modloader/order",
        );
        write_test_mod_config(
            &late_config,
            "late",
            &late_source,
            "payload",
            "modloader/order",
        );
        write_test_mod_config(
            &disabled_config,
            "disabled",
            &disabled_source,
            "payload",
            "modloader/order",
        );
        write_profile_entries(
            &game_root,
            &[
                test_profile_entry("late", true, 200, &late_config),
                test_profile_entry("disabled", false, 300, &disabled_config),
                test_profile_entry("early", true, 100, &early_config),
            ],
        );

        let journal = materialize_profile_for_run(&game_root, "default").unwrap();

        assert_eq!(
            fs::read_to_string(game_root.join("modloader").join("order").join("shared.txt"))
                .unwrap(),
            "late"
        );
        assert!(
            !game_root
                .join("modloader")
                .join("order")
                .join("disabled.txt")
                .exists()
        );
        let journal_text = fs::read_to_string(journal).unwrap();
        let early_idx = journal_text.find("profile_mod=early|100").unwrap();
        let late_idx = journal_text.find("profile_mod=late|200").unwrap();
        assert!(early_idx < late_idx);
        assert!(!journal_text.contains("profile_mod=disabled"));
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn profile_root_override_disables_a_specific_install_root() {
        let game_root = test_root("root_override");
        let source = game_root.join("sources").join("mod");
        let config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("mod")
            .join("mod.json");
        fs::create_dir_all(source.join("payload")).unwrap();
        fs::write(source.join("payload").join("file.txt"), "payload").unwrap();

        ensure_state(&game_root).unwrap();
        write_test_mod_config(&config, "mod", &source, "payload", "modloader/target");

        let mut overrides = BTreeMap::new();
        overrides.insert(
            "payload".to_string(),
            ProfileRootOverride {
                enabled: Some(false),
                target: None,
            },
        );
        let entry = ProfileModEntry {
            id: "mod".to_string(),
            enabled: true,
            load_order: 100,
            config: config.clone(),
            root_overrides: overrides,
        };
        write_profile_entries(&game_root, &[entry]);

        materialize_profile_for_run(&game_root, "default").unwrap();

        assert!(
            !game_root
                .join("modloader")
                .join("target")
                .join("file.txt")
                .exists(),
            "root disabled by profile override should not materialize"
        );
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn profile_root_override_retargets_a_specific_install_root() {
        let game_root = test_root("root_retarget");
        let source = game_root.join("sources").join("mod");
        let config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("mod")
            .join("mod.json");
        fs::create_dir_all(source.join("payload")).unwrap();
        fs::write(source.join("payload").join("file.txt"), "payload").unwrap();

        ensure_state(&game_root).unwrap();
        write_test_mod_config(&config, "mod", &source, "payload", "modloader/target");

        let mut overrides = BTreeMap::new();
        overrides.insert(
            "payload".to_string(),
            ProfileRootOverride {
                enabled: None,
                target: Some("modloader/retargeted".to_string()),
            },
        );
        let entry = ProfileModEntry {
            id: "mod".to_string(),
            enabled: true,
            load_order: 100,
            config: config.clone(),
            root_overrides: overrides,
        };
        write_profile_entries(&game_root, &[entry]);

        materialize_profile_for_run(&game_root, "default").unwrap();

        // Files land at the profile's overridden target, not the mod's default.
        assert!(
            game_root
                .join("modloader")
                .join("retargeted")
                .join("file.txt")
                .exists(),
            "root should materialize at the profile-overridden target"
        );
        assert!(
            !game_root
                .join("modloader")
                .join("target")
                .join("file.txt")
                .exists(),
            "root should not materialize at the mod's default target"
        );
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn materialize_profile_rejects_duplicate_enabled_mod_ids() {
        let game_root = test_root("materialize_duplicate_profile_id");
        let source = game_root.join("sources").join("dup");
        let config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("dup")
            .join("mod.json");
        fs::create_dir_all(source.join("payload")).unwrap();
        fs::write(source.join("payload").join("file.txt"), "payload").unwrap();
        ensure_state(&game_root).unwrap();
        write_test_mod_config(&config, "dup", &source, "payload", "modloader/dup");
        write_profile_entries(
            &game_root,
            &[
                test_profile_entry("dup", true, 100, &config),
                test_profile_entry("dup", true, 200, &config),
            ],
        );

        let err = materialize_profile_for_run(&game_root, "default")
            .unwrap_err()
            .to_string();

        assert!(err.contains("duplicate enabled mod id"));
        assert!(!game_root.join("modloader").join("dup").exists());
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn materialize_profile_rejects_missing_enabled_mod_config_before_copying() {
        let game_root = test_root("materialize_missing_config");
        let missing_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("missing")
            .join("mod.json");
        ensure_state(&game_root).unwrap();
        write_profile_entries(
            &game_root,
            &[test_profile_entry("missing", true, 100, &missing_config)],
        );

        let err = materialize_profile_for_run(&game_root, "default")
            .unwrap_err()
            .to_string();

        assert!(err.contains("references missing mod config"));
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn materialize_writes_modloader_priorities_from_load_order() {
        let game_root = test_root("materialize_ml_priorities");
        let early_source = game_root.join("sources").join("early");
        let late_source = game_root.join("sources").join("late");
        let early_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("early")
            .join("mod.json");
        let late_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("late")
            .join("mod.json");
        fs::create_dir_all(early_source.join("payload")).unwrap();
        fs::create_dir_all(late_source.join("payload")).unwrap();
        fs::write(early_source.join("payload").join("a.dff"), "e").unwrap();
        fs::write(late_source.join("payload").join("b.dff"), "l").unwrap();

        ensure_state(&game_root).unwrap();
        // Each mod is sandboxed in its own modloader/<folder>; on disk they never
        // collide, so only the written priority can order them.
        write_test_mod_config(
            &early_config,
            "early",
            &early_source,
            "payload",
            "modloader/early_mod",
        );
        write_test_mod_config(
            &late_config,
            "late",
            &late_source,
            "payload",
            "modloader/late_mod",
        );
        write_profile_entries(
            &game_root,
            &[
                test_profile_entry("late", true, 200, &late_config),
                test_profile_entry("early", true, 100, &early_config),
            ],
        );

        let journal = materialize_profile_for_run(&game_root, "default").unwrap();

        let ini =
            fs::read_to_string(game_root.join("modloader").join("modloader.ini")).unwrap();
        // Later load order -> higher ModLoader priority (wins at runtime).
        let priorities = crate::planning::read_modloader_priorities(&game_root).unwrap();
        assert!(
            priorities.for_folder("late_mod") > priorities.for_folder("early_mod"),
            "later mod must get higher ModLoader priority; ini was:\n{ini}"
        );
        // Written into a manager-owned native profile that inherits Default.
        assert!(ini.contains("[Profiles.SAMM_default.Priority]"), "ini was:\n{ini}");
        assert!(ini.contains("[Profiles.SAMM_default.Config]"), "ini was:\n{ini}");
        assert!(ini.contains("Parents = Default"), "ini was:\n{ini}");
        // That profile is what the run activates via -modprof.
        assert_eq!(
            modloader_run_profile(&game_root, "default").as_deref(),
            Some("SAMM_default")
        );
        // The ini write is journaled, so an ephemeral run's rollback restores it.
        let journal_text = fs::read_to_string(&journal).unwrap();
        assert!(
            journal_text.contains("modloader.ini"),
            "ini write should be journaled for rollback"
        );
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn disabled_modloader_mod_is_written_as_ignoremods() {
        let game_root = test_root("materialize_ignoremods");
        let on_source = game_root.join("sources").join("on");
        let off_source = game_root.join("sources").join("off");
        let on_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("on")
            .join("mod.json");
        let off_config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("off")
            .join("mod.json");
        fs::create_dir_all(on_source.join("payload")).unwrap();
        fs::create_dir_all(off_source.join("payload")).unwrap();
        fs::write(on_source.join("payload").join("a.dff"), "on").unwrap();
        fs::write(off_source.join("payload").join("b.dff"), "off").unwrap();

        ensure_state(&game_root).unwrap();
        write_test_mod_config(&on_config, "on", &on_source, "payload", "modloader/on_mod");
        write_test_mod_config(&off_config, "off", &off_source, "payload", "modloader/off_mod");
        write_profile_entries(
            &game_root,
            &[
                test_profile_entry("on", true, 100, &on_config),
                // Disabled: its folder should be ignored, not materialized.
                test_profile_entry("off", false, 200, &off_config),
            ],
        );

        materialize_profile_for_run(&game_root, "default").unwrap();

        let ini =
            fs::read_to_string(game_root.join("modloader").join("modloader.ini")).unwrap();
        assert!(ini.contains("[Profiles.SAMM_default.IgnoreMods]"), "ini was:\n{ini}");
        assert!(ini.contains("off_mod"), "disabled folder should be ignored; ini was:\n{ini}");
        // The disabled mod is never copied into the sandbox.
        assert!(!game_root.join("modloader").join("off_mod").exists());
        remove_dir_if_exists(&game_root).unwrap();
    }

    #[test]
    fn no_modloader_mods_writes_no_profile_and_no_modprof() {
        let game_root = test_root("materialize_no_ml");
        let source = game_root.join("sources").join("cleo_only");
        let config = game_root
            .join(".sa-mod-manager")
            .join("mods")
            .join("cleo_only")
            .join("mod.json");
        fs::create_dir_all(source.join("payload")).unwrap();
        fs::write(source.join("payload").join("script.cs"), "x").unwrap();

        ensure_state(&game_root).unwrap();
        // A CLEO-only mod targets CLEO/, never a modloader sandbox.
        write_test_mod_config(&config, "cleo_only", &source, "payload", "CLEO");
        write_profile_entries(&game_root, &[test_profile_entry("cleo_only", true, 100, &config)]);

        materialize_profile_for_run(&game_root, "default").unwrap();

        // No modloader mods -> no managed profile written, so nothing to activate.
        assert!(!game_root.join("modloader").join("modloader.ini").exists());
        assert_eq!(modloader_run_profile(&game_root, "default"), None);
        remove_dir_if_exists(&game_root).unwrap();
    }

    fn write_test_mod_config(
        path: &Path,
        id: &str,
        source_root: &Path,
        source: &str,
        target: &str,
    ) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let text = format!(
            concat!(
                "{{\n",
                "  \"version\": 1,\n",
                "  \"id\": \"{}\",\n",
                "  \"package\": \"{}\",\n",
                "  \"source_root\": \"{}\",\n",
                "  \"enabled\": true,\n",
                "  \"install_roots\": [\n",
                "    {{\n",
                "      \"source\": \"{}\",\n",
                "      \"target\": \"{}\",\n",
                "      \"kind\": \"modloader\",\n",
                "      \"enabled\": true\n",
                "    }}\n",
                "  ]\n",
                "}}\n"
            ),
            json_escape(id),
            json_escape(&source_root.display().to_string()),
            json_escape(&source_root.display().to_string()),
            json_escape(source),
            json_escape(target)
        );
        fs::write(path, text).unwrap();
    }

    fn write_test_profile(game_root: &Path, first_config: &Path, second_config: &Path) {
        write_profile_entries(
            game_root,
            &[
                test_profile_entry("first", true, 100, first_config),
                test_profile_entry("second", true, 200, second_config),
            ],
        );
    }

    fn write_profile_entries(game_root: &Path, entries: &[ProfileModEntry]) {
        let profile_path = game_root
            .join(".sa-mod-manager")
            .join("profiles")
            .join("default.json");
        let profile = ProfileJson {
            name: "default".to_string(),
            mods: entries.to_vec(),
            ..Default::default()
        };
        write_profile_json_file(&profile_path, game_root, &profile).unwrap();
    }

    fn test_profile_entry(
        id: &str,
        enabled: bool,
        load_order: i32,
        config: &Path,
    ) -> ProfileModEntry {
        ProfileModEntry {
            id: id.to_string(),
            enabled,
            load_order,
            config: config.to_path_buf(),
            ..Default::default()
        }
    }

    fn test_root(name: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        remove_dir_if_exists(&root).unwrap();
        fs::create_dir_all(&root).unwrap();
        // Materialization now requires a real-looking GTA install; give the
        // temp root the executable ensure_gta_install checks for.
        fs::write(root.join("gta_sa.exe"), b"").unwrap();
        root
    }

    fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}
