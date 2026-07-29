
impl SanAndreasModUi
{
    /// Validate then persist an edited install root. Returns whether it was saved
    /// so the caller can keep an invalid edit open for correction.
    pub(super) fn save_mod_install_root(
        &mut self,
        config_path: &Path,
        root_index: usize,
        root: ModInstallRootJson,
    ) -> bool
    {
        if let Err(err) = validate_install_root(&root)
        {
            self.record_error(err);
            return false;
        }
        let result = update_mod_config_install_root(config_path, root_index, &root)
            .map(|_| "updated install root".to_string());
        let saved = result.is_ok();
        self.set_action_result("updated install root", result);
        return saved;
    }
}

/// Reject an install root before it reaches disk: source and target must be
/// non-empty relative paths with no `..` escape or drive-letter, the same
/// containment rule the installer enforces. `kind` comes from a fixed dropdown,
/// so it needs no check here.
#[derive(Default)]
struct ModInfoFilesAndReadmes
{
    files: Vec<String>,
    readmes: Vec<(String, String)>,
}

fn mod_info_files_and_readmes(root: &Path) -> ModInfoFilesAndReadmes
{
    let Ok(paths) = collect_files_recursive(root) else {
        return ModInfoFilesAndReadmes {
            files: Vec::new(),
            readmes: Vec::new(),
        };
    };
    let mut files = Vec::new();
    let mut readmes = Vec::new();
    for path in &paths
    {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        push_mod_info_readme(path, &relative, &mut readmes);
        files.push(relative);
    }
    return ModInfoFilesAndReadmes { files, readmes };
}

fn push_mod_info_readme(path: &Path, relative: &str, readmes: &mut Vec<(String, String)>)
{
    if readmes.len() >= MAX_MOD_INFO_READMES
    {
        // literal: allow UI tuning threshold is local to this control
        return;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    if !is_readme_name(name)
    {
        return;
    }
    if let Ok(text) = read_capped(path, MAX_CONTROL_FILE_BYTES)
    {
        readmes.push((relative.to_string(), text));
    }
}

fn validate_install_root(root: &ModInstallRootJson) -> Result<(), AppError>
{
    validate_install_root_path(InstallRootPathField { label: "source", value: &root.source })?;
    return validate_install_root_path(InstallRootPathField { label: "target", value: &root.target });
}

struct InstallRootPathField<'a>
{
    label: &'a str,
    value: &'a str,
}

fn validate_install_root_path(field: InstallRootPathField<'_>) -> Result<(), AppError>
{
    let label = field.label;
    let value = field.value;
    let trimmed = value.trim();
    if trimmed.is_empty()
    {
        return Err(AppError::Usage(format!("install root {label} is required")));
    }
    return path_from_package_root(&normalize_path(trimmed))
        .map(|_| ())
        .map_err(|err| {
            AppError::Usage(format!(
                "install root {label} `{trimmed}` is invalid: {err}"
            ))
        });
}

impl SanAndreasModUi
{
    /// Run the selected target: index 0 plays the current profile (materialize +
    /// launch + cleanup), any other index launches that external tool directly.
    pub(super) fn run_selected_target(&mut self, ctx: &egui::Context)
    {
        if self.selected_run_target == 0
        {
            self.launch_selected_profile(ctx);
            return;
        }
        let Some(tool) = self.executables.get(self.selected_run_target - 1).cloned() else {
            self.selected_run_target = 0;
            return;
        };
        match launch_external_tool(&PathBuf::from(&tool.path), &tool.arg_list())
        {
            Ok(_child) => {
                // The tool runs detached; we neither wait on nor clean up after it.
                self.last_error = None;
                self.status = format!("launched tool: {}", tool.name);
            }
            Err(err) => self.record_error(err),
        }
    }

    pub(super) fn add_executable(&mut self)
    {
        let name = self.new_tool_name.trim().to_string();
        let path = self.new_tool_path.trim().to_string();
        if name.is_empty() || path.is_empty()
        {
            self.record_error(AppError::Usage(
                "a run target needs both a name and an executable path".to_string(),
            ));
            return;
        }
        let mut executables = self.executables.to_vec();
        executables.push(Executable {
            name,
            path,
            args: self.new_tool_args.trim().to_string(),
        });
        let state_root = state_directory(&self.game_root());
        match write_executables(&state_root, &executables)
        {
            Ok(()) => self.after_added_executable(),
            Err(err) => self.record_error(err),
        }
    }

    fn after_added_executable(&mut self)
    {
        self.new_tool_name.clear();
        self.new_tool_path.clear();
        self.new_tool_args.clear();
        self.status = "added run target".to_string();
        if let Err(err) = self.reload_state()
        {
            self.record_error(err);
        }
    }

    pub(super) fn remove_executable(&mut self, index: usize)
    {
        if index >= self.executables.len()
        {
            return;
        }
        let mut executables = self.executables.to_vec();
        executables.remove(index);
        let state_root = state_directory(&self.game_root());
        match write_executables(&state_root, &executables)
        {
            Ok(()) => self.after_removed_executable(),
            Err(err) => self.record_error(err),
        }
    }

    fn after_removed_executable(&mut self)
    {
        self.selected_run_target = 0;
        self.status = "removed run target".to_string();
        if let Err(err) = self.reload_state()
        {
            self.record_error(err);
        }
    }
    pub(super) fn browse_tool_path(&mut self)
    {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Select a tool executable")
            .add_filter("Executables", &["exe", "bat", "cmd"])
            .pick_file()
        {
            if self.new_tool_name.trim().is_empty()
            {
                if let Some(stem) = path.file_stem()
                {
                    self.new_tool_name = stem.to_string_lossy().to_string();
                }
            }
            self.new_tool_path = path.display().to_string();
        }
    }

    pub(super) fn launch_selected_profile(&mut self, ctx: &egui::Context)
    {
        self.refresh_pending_runs();
        if self.has_unsafe_pending_runs_for_launch()
        {
            self.tab = crate::ui::state::UiTab::Run;
            self.status = "clean temporary files before playing another profile".to_string();
            return;
        }
        if self.game_child.is_some()
        {
            self.status = "game is already running for this manager session".to_string();
            return;
        }
        let game_root = self.game_root();
        let profile = self.selected_profile.to_owned();
        // Materialize (extract + copy) can be slow; run it off the UI thread.
        self.spawn_task(ctx, "preparing and launching", move || {
            let launch_result = materialize_and_launch_profile(&game_root, &profile);
            TaskResult::Play(launch_result)
        });
    }

    pub(super) fn apply_play(
        &mut self,
        result: Result<(ActiveRun, std::process::Child), AppError>,
    )
    {
        match result
        {
            Ok((active, child)) => self.after_play_started(active, child),
            Err(err) => {
                self.record_error(err);
                self.refresh_pending_runs();
            }
        }
    }
    fn after_play_started(&mut self, active: ActiveRun, child: std::process::Child)
    {
        let journal = active.journal.clone();
        self.pending_journal = Some(journal.clone());
        self.active_run = Some(active);
        self.game_child = Some(child);
        self.last_error = None;
        self.status = format!("playing profile; cleanup record {}", journal.display());
        if let Err(err) = self.reload_state()
        {
            self.record_error(err);
        }
    }
}

impl SanAndreasModUi
{
    pub(super) fn cleanup_pending_run(&mut self)
    {
        let Some(record) = self.selected_pending_run() else {
            self.status = "no cleanup record selected".to_string();
            return;
        };
        if self.pending_run_is_running(&record)
        {
            self.status = "game is still running; cleanup is blocked".to_string();
            return;
        }
        let journal = record.journal.clone();
        let cleanup_result = self.cleanup_journal(&journal);
        self.set_action_result(
            "cleaned temporary files",
            cleanup_result.map(|_| "cleaned temporary files".to_string()),
        );
        if !pending_record_path(&self.game_root(), &journal).exists()
        {
            self.pending_journal = None;
        }
        self.refresh_pending_runs();
    }
}

impl SanAndreasModUi
{
    pub(super) fn cleanup_pending_run_record(&mut self, record: PendingRunRecord)
    {
        if self.pending_run_is_running(&record)
        {
            self.status = "game is still running; cleanup is blocked".to_string();
            return;
        }
        let journal = record.journal.clone();
        let cleanup_result = self.cleanup_journal(&journal);
        self.set_action_result(
            "cleaned temporary files",
            cleanup_result.map(|_| "cleaned temporary files".to_string()),
        );
        if self
            .pending_journal
            .as_ref()
            .map(|pending| pending == &journal)
            .unwrap_or(false)
        {
            self.pending_journal = None;
        }
        self.refresh_pending_runs();
    }
}

impl SanAndreasModUi
{
    pub(super) fn cleanup_stale_pending_runs(&mut self)
    {
        self.refresh_pending_runs();
        let stale_runs = self
            .pending_runs
            .iter()
            .filter(|record| record.status == PendingRunStatus::Stale)
            .cloned()
            .collect::<Vec<_>>();
        if stale_runs.is_empty()
        {
            self.status = "no finished runs need cleanup".to_string();
            return;
        }

        let game_root = self.game_root();
        let mut cleaned = 0;
        let mut first_error = None;
        for record in stale_runs
        {
            let result = cleanup_journal_for_game_root(&game_root, &record.journal);
            match result
            {
                Ok(()) => cleaned += 1,
                Err(err) => {
                    first_error = Some(err);
                    break;
                }
            }
        }
        self.refresh_pending_runs();
        self.status = match first_error {
            Some(err) => format!("cleaned {cleaned} finished runs; then failed: {err}"),
            None => format!("cleaned {cleaned} finished runs"),
        };
    }
}










