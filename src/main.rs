mod cli;
mod constants;
mod game_launch;
mod model;
mod parsing;
mod planning;
mod reporting;
mod ui;
mod workspace;

mod prelude {
    pub(crate) use crate::cli::usage_error;
    pub(crate) use crate::constants::{
        COMPONENT_RULES, CONTEXT_RULES, DEFAULT_GAME_ROOT, DEFAULT_MOD_ROOTS,
    };
    pub(crate) use crate::game_launch::{
        ensure_gta_install, game_executable_path, launch_game_executable,
    };
    pub(crate) use crate::model::{
        AppError, ArchiveEntryFields, CandidateDetection, CandidateMetadata, CommandOptions,
        CompletedPlanParts, Component, ComponentRule, ContextRule, CopyJournalContext,
        EntryClassificationState, InstallApplyState, InstallCandidate, InstallOperation,
        InstallPlan, ManifestInstallRoot, ModConfigJson, ModInstallRootJson, PackageEntry,
        PackageKind, PackageRef, PackageReport, PackageSummary, PlanBuildCollections,
        PlanBuildContext, ProfileJson, ProfileModActivation, ProfileModEntry, ReadmeAction,
        ReadmeDocument, ReadmeInsight, ReadmeInsightKind, ReadmeInstruction, RunApplyState,
        RunInstallContext, TargetKind,
    };
    pub(crate) use crate::parsing::{
        backup_relative_for_destination, classify_component, classify_context, classify_risk,
        collect_files_recursive, copy_tree_contents, detect_install_candidate, detect_option_group,
        ensure_destination_allowed, extract_archive_to_directory, extract_archive_to_named_staging,
        list_archive_entries_native, list_archive_failed_error, missing_7zip_error_for_package,
        path_from_package_root, read_mod_config_json, read_package_text_file, read_profile_json,
        write_package_manifest,
    };
    pub(crate) use crate::planning::{
        analyze_package, apply_install_plan, build_install_plan, interrupted_install,
        materialize_profile_for_run, prepare_run, print_install_plan, recover_interrupted_install,
        rollback_journal, sort_operations_for_apply,
    };
    pub(crate) use crate::reporting::{
        escape_value, extension_eq, extract_to_staging, file_name, find_seven_zip, human_bytes,
        is_readme_name, json_escape, list_matching, normalize_path, package_id, print_report,
        safe_name, state_directory, unescape_value, unix_now,
    };
    pub(crate) use crate::workspace::{
        add_mod_to_profile_json, create_profile, ensure_state, import_package, init_state,
        inspect_game, list_profiles, load_profile_for_edit, remove_mod_from_profile_json,
        scan_roots, set_profile_mod_activation, set_profile_mod_order, show_profile_json,
        target_template, update_mod_config_install_root, write_mod_config_json,
        write_mod_config_json_with_source, write_profile_json, write_profile_json_file,
    };
    pub(crate) use serde::{Deserialize, Serialize};
    pub(crate) use std::collections::{BTreeMap, BTreeSet, VecDeque};
    pub(crate) use std::env;
    pub(crate) use std::ffi::OsStr;
    pub(crate) use std::fmt;
    pub(crate) use std::fs;
    pub(crate) use std::io::{self, Write};
    pub(crate) use std::path::{Path, PathBuf};
    pub(crate) use std::process::Command;
    pub(crate) use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
}

fn main() {
    cli::main();
}
