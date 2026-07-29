mod cli;
mod constants;
mod game_launch;
mod logging;
mod model;
mod parsing;
mod planning;
mod reporting;
mod settings;
mod ui;
mod workspace;

mod prelude
{
    pub(crate) use crate::cli::usage_error;
    pub(crate) use crate::constants::{
        CLEO_SCRIPT_EXTENSIONS, builtin_component_rules, builtin_context_rules,
        has_cleo_script_extension,
    };
    pub(crate) use crate::game_launch::{
        ensure_gta_install, game_executable_path, launch_external_tool_from_arguments, launch_game_executable_from_arguments,
    };
    pub(crate) use crate::logging::{log_debug, log_error, log_info, log_warn};
    pub(crate) use crate::model::{
        AppError, AppErrorKind, ArchiveEntryFields, CandidateDetection, CandidateMetadata,
        CommandOptions, CompletedPlanParts, Component, ComponentRule, ContextRule,
        CopyJournalContext, EntryClassificationState, ErrorContext, InstallApplyState,
        InstallCandidate, InstallOperation, InstallPlan, ManifestInstallRoot, ModConfigJson,
        ModInstallRootJson, PackageEntry, PackageKind, PackageRef, PackageReport, PackageSummary,
        PlanBuildCollections, PlanBuildContext, ProfileJson, ProfileModActivation, ProfileModEntry,
        ProfileRootOverride, ReadmeAction, ReadmeDocument, ReadmeInsight, ReadmeInsightKind,
        ReadmeInstruction, RunApplyState, RunInstallContext, TargetKind,
    };
    pub(crate) use crate::parsing::{
        backup_relative_for_destination, classify_component, classify_context, classify_risk,
        collect_files_recursive, copy_tree_contents, detect_install_candidate, detect_option_group,
        ensure_destination_allowed, extract_archive_to_directory, extract_archive_to_named_staging,
        list_archive_entries_native, list_archive_failed_error_detail,
        missing_7zip_error_for_package, path_from_package_root, read_mod_config_json,
        read_package_text_file, read_profile_json, write_package_manifest,
    };
    pub(crate) use crate::planning::{
        ContentCategory, ContentEntry, ContentIndex, IndexedMod, ModLoaderLogSummary,
        ModLoaderPriorities, RUN_RESULT_GAME_ERROR, RUN_RESULT_LAUNCH_FAILED, RUN_RESULT_SUCCESS,
        RunOutcome, analyze_package, apply_install_plan, asi_view, build_content_index,
        build_install_plan, cleo_view, effective_install_roots, group_entries, interrupted_install,
        materialize_profile_for_run, modloader_conflicts, per_mod_flags, prepare_run,
        print_install_plan, read_modloader_log, read_modloader_priorities,
        read_run_outcome_for_journal, recover_interrupted_install, rollback_journal,
        sort_operations_for_apply, txid_from_journal, write_run_outcome,
    };
    pub(crate) use crate::reporting::{
        MAX_CONTROL_FILE_BYTES, escape_value, has_extension_equal_to, extract_to_staging, file_name,
        find_seven_zip, human_bytes, human_datetime, is_readme_name, json_escape, list_matching,
        normalize_path, package_id, print_report, read_capped, safe_name, safe_profile_name,
        state_directory, unescape_value, unix_now,
    };
    pub(crate) use crate::settings::{
        component_rules, configured_seven_zip, context_rules, write_example_config,
    };
    pub(crate) use crate::workspace::{
        CleoDiagnostics, Executable, ModMeta, Separator, add_mod_to_profile_json,
        apply_modloader_priorities, collect_cleo_diagnostics, collect_overwrite_files,
        copy_profile, create_profile, delete_profile, ensure_state, import_package, init_state,
        inspect_game, list_profiles, load_profile_for_edit, profile_launch_settings,
        read_active_profile, read_executables, read_import_manifest, read_mod_meta,
        read_modloader_overrides, read_separators, remove_mod_from_profile_json, rename_profile,
        scan_roots, set_active_profile, set_all_profile_mods, set_profile_launch_args,
        set_profile_mod_activation, set_profile_mod_order, set_profile_mod_order_list,
        set_profile_root_override, set_profile_root_target, show_profile_json, target_template,
        update_mod_config_install_root, write_executables, write_mod_config_json,
        write_mod_config_json_with_source, write_mod_meta, write_modloader_overrides,
        write_profile_json, write_profile_json_file, write_separators,
    };
    pub(crate) use serde::{Deserialize, Serialize};
    pub(crate) use std::collections::{BTreeMap, BTreeSet, VecDeque};
    pub(crate) use std::env;
    pub(crate) use std::ffi::OsStr;
    pub(crate) use std::fmt;
    pub(crate) use std::fs;
    pub(crate) use std::io::{self, Write};
    pub(crate) use std::path::{Path, PathBuf};
    pub(crate) use std::process::{Command, ExitStatus};
    pub(crate) use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
}

fn main()
{
    cli::main();
}
