
fn load_import_telemetry(
    state_root: &Path,
    summary: &mut TelemetrySummary,
) -> Result<(), AppError>
{
    let library_root = state_root.join("library");
    if !library_root.exists()
    {
        return Ok(());
    }
    for entry in fs::read_dir(library_root)?
    {
        let manifest = entry?.path().join("import.json");
        if !manifest.exists()
        {
            continue;
        }
        let import = read_import_manifest(&manifest)?;
        summary.imports += 1;
        let id = if import.id.is_empty() {
            "imported mod".to_string()
        } else {
            import.id
        };
        let created_unix = import.imported_unix;
        let operations = import.operation_count;
        upsert_mod_telemetry(summary, &id, |mod_row| {
            mod_row.imports += 1;
            mod_row.copied_files += operations;
            mod_row.last_seen_unix = mod_row.last_seen_unix.max(created_unix);
        });
        summary.recent_events.push(TelemetryEvent {
            created_unix,
            kind: "import".to_string(),
            title: id,
            detail: format!(
                "{} entries, {} operations",
                import.entry_count, import.operation_count
            ),
        });
    }
    return Ok(());
}

fn load_journal_telemetry(
    state_root: &Path,
    summary: &mut TelemetrySummary,
) -> Result<(), AppError>
{
    let journals_root = state_root.join("journals");
    if !journals_root.exists()
    {
        return Ok(());
    }
    for entry in fs::read_dir(journals_root)?
    {
        let path = entry?.path();
        if !path.is_file()
        {
            continue;
        }
        // Journals are `key=value` lines, one per installed file; cap the read so
        // a corrupt/oversized journal can't blow up telemetry aggregation.
        let text = read_capped(&path, 64 * 1024 * 1024)?; // literal: allow UI tuning threshold is local to this control
        summary.journals += 1;
        let mode = telemetry_line_value(TelemetryLineKey { text: &text, key: "mode" }).unwrap_or_default();
        let created_unix = telemetry_line_value(TelemetryLineKey { text: &text, key: "created_unix" })
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let title = journal_title(&text, &path);
        if mode == "ephemeral-run"
        {
            let outcome = read_run_outcome_for_journal(state_root, &path);
            if outcome
                .as_ref()
                .map(|record_log_message_from_arguments| record_log_message_from_arguments.result == RUN_RESULT_LAUNCH_FAILED)
                .unwrap_or(false)
            {
                // A launch that never started was rolled back: count it as a
                // failed launch, not a run, and leave its undone file operations
                // out of the aggregates.
                summary.failed_launches += 1;
                summary.recent_events.push(TelemetryEvent {
                    created_unix,
                    kind: "launch failed".to_string(),
                    title,
                    detail: launch_failed_detail(outcome.as_ref()),
                });
                continue;
            }
            summary.run_journals += 1;
            add_run_mod_telemetry(summary, &text, created_unix);
            accumulate_journal_file_counts(summary, &text);
            summary.recent_events.push(TelemetryEvent {
                created_unix,
                kind: "run".to_string(),
                title,
                detail: run_event_detail(&text, outcome.as_ref()),
            });
        }
        else
        {
            summary.install_journals += 1;
            add_install_mod_telemetry(summary, &text, created_unix);
            accumulate_journal_file_counts(summary, &text);
            summary.recent_events.push(TelemetryEvent {
                created_unix,
                kind: "install".to_string(),
                title,
                detail: journal_event_detail(&text),
            });
        }
    }
    return Ok(());
}

fn journal_title(text: &str, path: &Path) -> String
{
    return telemetry_line_value(TelemetryLineKey { text: text, key: "profile" })
        .or_else(|| telemetry_line_value(TelemetryLineKey { text: text, key: "package_id" }))
        .unwrap_or_else(|| file_name(&path.display().to_string()).to_string());
}

fn launch_failed_detail(outcome: Option<&RunOutcome>) -> String
{
    return match outcome
    {
        Some(record_log_message_from_arguments) if !record_log_message_from_arguments.launch_args.is_empty() => {
            format!(
                "launch never started (args: {})",
                record_log_message_from_arguments.launch_args.join(" ")
            )
        }
        _ => "launch never started".to_string(),
    };
}

fn add_run_mod_telemetry(summary: &mut TelemetrySummary, text: &str, created_unix: u64)
{
    let mut current_mod = None;
    for line in text.lines()
    {
        if let Some(value) = line.strip_prefix("profile_mod=")
        {
            let id = value
                .split('|')
                .next()
                .map(unescape_value)
                .unwrap_or_default();
            upsert_mod_telemetry(summary, &id, |mod_row| {
                mod_row.runs += 1;
                mod_row.last_seen_unix = mod_row.last_seen_unix.max(created_unix);
            });
            current_mod = Some(id);
            continue;
        }
        let Some(id) = current_mod.as_deref() else {
            continue;
        };
        if line.starts_with("copy=")
        {
            upsert_mod_telemetry(summary, id, |mod_row| mod_row.copied_files += 1);
        }
        else if line.starts_with("backup=")
        {
            upsert_mod_telemetry(summary, id, |mod_row| mod_row.overwritten_files += 1);
        }
        else if line.starts_with("missing_source=")
        {
            upsert_mod_telemetry(summary, id, |mod_row| mod_row.missing_sources += 1);
        }
        else if line.starts_with("blocked_bootstrap=")
        {
            upsert_mod_telemetry(summary, id, |mod_row| mod_row.blocked_bootstrap += 1);
        }
    }
}

fn accumulate_journal_file_counts(summary: &mut TelemetrySummary, text: &str)
{
    summary.copied_files += line_count(TelemetryLinePattern { text: text, pattern: "copy=" });
    summary.new_files += line_count(TelemetryLinePattern { text: text, pattern: "new=" });
    summary.overwritten_files += line_count(TelemetryLinePattern { text: text, pattern: "backup=" });
    summary.blocked_bootstrap += line_count(TelemetryLinePattern { text: text, pattern: "blocked_bootstrap=" });
    summary.missing_sources += line_count(TelemetryLinePattern { text: text, pattern: "missing_source=" });
}

/// Append the recorded exit status and duration to a run's event detail, when an
/// outcome was captured. Runs launched before outcome tracking (or from another
/// manager session) simply show the file-operation summary.
fn run_event_detail(text: &str, outcome: Option<&RunOutcome>) -> String
{
    let base = journal_event_detail(text);
    return match outcome
    {
        Some(record_log_message_from_arguments) => format!("{base}; {}", outcome_status_phrase(record_log_message_from_arguments)),
        None => base,
    };
}

fn add_install_mod_telemetry(summary: &mut TelemetrySummary, text: &str, created_unix: u64)
{
    let Some(package_id) = telemetry_line_value(TelemetryLineKey { text: text, key: "package_id" }) else {
        return;
    };
    let copies = line_count(TelemetryLinePattern { text: text, pattern: "copy=" });
    let backups = line_count(TelemetryLinePattern { text: text, pattern: "backup=" });
    let missing = line_count(TelemetryLinePattern { text: text, pattern: "missing_source=" });
    let blocked = line_count(TelemetryLinePattern { text: text, pattern: "blocked_bootstrap=" });
    upsert_mod_telemetry(summary, &package_id, |mod_row| {
        mod_row.installs += 1;
        mod_row.copied_files += copies;
        mod_row.overwritten_files += backups;
        mod_row.missing_sources += missing;
        mod_row.blocked_bootstrap += blocked;
        mod_row.last_seen_unix = mod_row.last_seen_unix.max(created_unix);
    });
}

/// A short human phrase for a run's exit status and duration, e.g.
/// "exited 0 after 42s" or "exited with code 3 after 5s".
fn outcome_status_phrase(outcome: &RunOutcome) -> String
{
    let status = match (outcome.result.as_str(), outcome.exit_code) {
        (RUN_RESULT_SUCCESS, _) => "exited 0".to_string(),
        (_, Some(code)) => format!("exited with code {code}"),
        (_, None) => "exit status unknown".to_string(),
    };
    return match outcome.duration_ms
    {
        Some(ms) => format!("{status} after {}s", ms / 1000), // literal: allow UI tuning threshold is local to this control
        None => status,
    };
}

pub(super) fn export_telemetry_summary(
    game_root: &Path,
    summary: &TelemetrySummary,
) -> Result<PathBuf, AppError>
{
    let export_dir = state_directory(game_root).join("telemetry");
    fs::create_dir_all(&export_dir)?;
    let path = export_dir.join(format!("telemetry-{}.json", unix_now()));
    let mut file = fs::File::create(&path)?;
    writeln!(file, "{{")?;
    writeln!(file, "  \"version\": 1,")?;
    writeln!(file, "  \"exported_unix\": {},", unix_now())?;
    writeln!(file, "  \"summary\": {{")?;
    writeln!(file, "    \"imports\": {},", summary.imports)?;
    writeln!(file, "    \"journals\": {},", summary.journals)?;
    writeln!(file, "    \"runs\": {},", summary.run_journals)?;
    writeln!(
        file,
        "    \"failed_launches\": {},",
        summary.failed_launches
    )?;
    writeln!(file, "    \"installs\": {},", summary.install_journals)?;
    writeln!(file, "    \"copied_files\": {},", summary.copied_files)?;
    writeln!(file, "    \"new_files\": {},", summary.new_files)?;
    writeln!(
        file,
        "    \"overwritten_files\": {},",
        summary.overwritten_files
    )?;
    writeln!(
        file,
        "    \"blocked_bootstrap\": {},",
        summary.blocked_bootstrap
    )?;
    writeln!(
        file,
        "    \"missing_sources\": {},",
        summary.missing_sources
    )?;
    writeln!(file, "    \"pending_cleanup\": {}", summary.pending_cleanup)?;
    writeln!(file, "  }},")?;
    write_mod_history_json(&mut file, &summary.mod_history)?;
    writeln!(file, ",")?;
    write_recent_events_json(&mut file, &summary.recent_events)?;
    writeln!(file)?;
    writeln!(file, "}}")?;
    return Ok(path);
}

fn write_mod_history_json(file: &mut fs::File, rows: &[ModTelemetry]) -> Result<(), AppError>
{
    writeln!(file, "  \"mods\": [")?;
    for (idx, row) in rows.iter().enumerate()
    {
        writeln!(file, "    {{")?;
        writeln!(file, "      \"id\": \"{}\",", json_escape(&row.id))?;
        writeln!(file, "      \"imports\": {},", row.imports)?;
        writeln!(file, "      \"runs\": {},", row.runs)?;
        writeln!(file, "      \"installs\": {},", row.installs)?;
        writeln!(file, "      \"copied_files\": {},", row.copied_files)?;
        writeln!(
            file,
            "      \"overwritten_files\": {},",
            row.overwritten_files
        )?;
        writeln!(file, "      \"missing_sources\": {},", row.missing_sources)?;
        writeln!(
            file,
            "      \"blocked_bootstrap\": {},",
            row.blocked_bootstrap
        )?;
        writeln!(file, "      \"last_seen_unix\": {}", row.last_seen_unix)?;
        write!(file, "    }}")?;
        if idx + 1 == rows.len()
        {
            writeln!(file)?;
        }
        else
        {
            writeln!(file, ",")?;
        }
    }
    write!(file, "  ]")?;
    return Ok(());
}

struct TelemetryLinePattern<'a>
{
    text: &'a str,
    pattern: &'a str,
}

fn write_recent_events_json(
    file: &mut fs::File,
    events: &[TelemetryEvent],
) -> Result<(), AppError>
{
    writeln!(file, "  \"events\": [")?;
    for (idx, event) in events.iter().enumerate()
    {
        writeln!(file, "    {{")?;
        writeln!(file, "      \"created_unix\": {},", event.created_unix)?;
        writeln!(file, "      \"kind\": \"{}\",", json_escape(&event.kind))?;
        writeln!(file, "      \"title\": \"{}\",", json_escape(&event.title))?;
        writeln!(file, "      \"detail\": \"{}\"", json_escape(&event.detail))?;
        write!(file, "    }}")?;
        if idx + 1 == events.len()
        {
            writeln!(file)?;
        }
        else
        {
            writeln!(file, ",")?;
        }
    }
    write!(file, "  ]")?;
    return Ok(());
}

struct TelemetryLineKey<'a>
{
    text: &'a str,
    key: &'a str,
}

fn upsert_mod_telemetry(
    summary: &mut TelemetrySummary,
    id: &str,
    update: impl FnOnce(&mut ModTelemetry),
)
{
    if let Some(row) = summary.mod_history.iter_mut().find(|row| row.id == id)
    {
        update(row);
        return;
    }
    let mut row = ModTelemetry {
        id: id.to_string(),
        ..ModTelemetry::default()
    };
    update(&mut row);
    summary.mod_history.push(row);
}

fn journal_event_detail(text: &str) -> String
{
    let copies = line_count(TelemetryLinePattern { text: text, pattern: "copy=" });
    let new_files = line_count(TelemetryLinePattern { text: text, pattern: "new=" });
    let backups = line_count(TelemetryLinePattern { text: text, pattern: "backup=" });
    let blocked = line_count(TelemetryLinePattern { text: text, pattern: "blocked_bootstrap=" });
    let missing = line_count(TelemetryLinePattern { text: text, pattern: "missing_source=" });
    return format!(
        "{copies} copied, {new_files} new, {backups} overwritten, {blocked} blocked, {missing} missing"
    );
}

fn line_count(pattern: TelemetryLinePattern<'_>) -> usize
{
    let text = pattern.text;
    let prefix = pattern.pattern;
    return text.lines().filter(|line| line.starts_with(prefix)).count();
}

fn telemetry_line_value(query: TelemetryLineKey<'_>) -> Option<String>
{
    let text = query.text;
    let key = query.key;
    let prefix = format!("{key}=");
    return text.lines()
        .find_map(|line| line.strip_prefix(&prefix).map(unescape_value));
}

#[cfg(test)]
mod tests
{
    include!("part_03_tests_01.rs");
}
