use crate::prelude::*;

use super::copy_journal::{apply_copy_tree_with_journal, sync_journal};
use super::rollback::rollback_journal;

pub(crate) fn prepare_run(game_root: &Path, profile_name: &str) -> Result<(), AppError> {
    let (launch_args, launch_env) = profile_launch_settings(game_root, profile_name)?;
    let journal_path = materialize_profile_for_run(game_root, profile_name)?;
    let launch_result = launch_game_and_wait(game_root, &launch_args, &launch_env);
    let rollback_result = rollback_journal(&journal_path, game_root);
    launch_result?;
    rollback_result
}

fn launch_game_and_wait(
    game_root: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<(), AppError> {
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
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Usage(format!(
            "game exited with status: {status}"
        )))
    }
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
        rollback_failed_materialization(game_root, &run_state.journal_path, journal, err)?;
        unreachable!("rollback_failed_materialization always returns an error");
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

fn rollback_failed_materialization(
    game_root: &Path,
    journal_path: &Path,
    mut journal: fs::File,
    materialize_error: AppError,
) -> Result<(), AppError> {
    let flush_result = journal.flush();
    drop(journal);

    let rollback_result = match flush_result {
        Ok(()) => rollback_journal(journal_path, game_root),
        Err(err) => Err(AppError::Io(err)),
    };

    match rollback_result {
        Ok(()) => Err(materialize_error),
        Err(rollback_error) => Err(AppError::Usage(format!(
            "failed to materialize profile: {materialize_error}; rollback also failed: {rollback_error}; journal: {}",
            journal_path.display()
        ))),
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
    overrides: &BTreeMap<String, bool>,
) -> Vec<ModInstallRootJson> {
    // A per-profile override for a root's `source` wins over the mod's own
    // `enabled` flag, letting a profile toggle a shared mod's roots.
    roots.retain(|root| {
        overrides
            .get(&root.source)
            .copied()
            .unwrap_or(root.enabled)
    });
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

#[cfg(test)]
mod tests {
    use super::*;

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
        overrides.insert("payload".to_string(), false);
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
