use crate::prelude::*;

/// Persistent record_log_message_from_arguments of a single profile run's launch outcome.
///
/// A run journal records *what was materialized*, but not whether the game
/// actually started, how it exited, or how long it ran â€” so a launch that never
/// started and a successful play session look identical on disk. This record_log_message_from_arguments,
/// written beside the journals under `outcomes/<txid>.json`, carries that
/// missing dimension so telemetry can tell runs from failed launches and show
/// exit status and duration. Serialized with `serde_json`, like the other
/// on-disk manifests, so the format survives reformatting.
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub(crate) struct RunOutcome
{
    pub(crate) version: u32,
    pub(crate) txid: String,
    pub(crate) profile: String,
    /// One of [`RUN_RESULT_SUCCESS`], [`RUN_RESULT_GAME_ERROR`], or
    /// [`RUN_RESULT_LAUNCH_FAILED`].
    pub(crate) result: String,
    /// The game's process exit code, when it started and reported one.
    pub(crate) exit_code: Option<i32>,
    /// Wall-clock time from launch to exit, in milliseconds (second-resolution).
    pub(crate) duration_ms: Option<u64>,
    pub(crate) launch_args: Vec<String>,
    pub(crate) started_unix: u64,
    pub(crate) finished_unix: u64,
}

/// The game started and exited zero.
pub(crate) const RUN_RESULT_SUCCESS: &str = "success";
/// The game started but exited non-zero.
pub(crate) const RUN_RESULT_GAME_ERROR: &str = "game_error";
/// The executable never started (spawn failed); the run was rolled back.
pub(crate) const RUN_RESULT_LAUNCH_FAILED: &str = "launch_failed";

/// The transaction id embedded in a run journal's file name (its stem), used to
/// pair a journal with its outcome record_log_message_from_arguments.
pub(crate) fn txid_from_journal(journal: &Path) -> String
{
    return journal
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
}

/// Persist a run outcome under the manager state directory. `state_root` is the
/// game folder's state directory (i.e. `state_directory(game_root)`).
pub(crate) fn write_run_outcome(state_root: &Path, outcome: &RunOutcome) -> Result<(), AppError>
{
    let dir = outcomes_directory(state_root);
    fs::create_dir_all(&dir)
        .with_context(|| format!("create outcomes directory {}", dir.display()))?;
    let path = outcome_path(state_root, &outcome.txid);
    let mut text = serde_json::to_string_pretty(outcome)
        .map_err(|err| AppError::Usage(format!("serialize run outcome: {err}")))?;
    text.push('\n');
    fs::write(&path, text).with_context(|| format!("write run outcome {}", path.display()))?;
    return Ok(());
}

/// Read the outcome paired with a run journal, if one was recorded. A missing or
/// unreadable record_log_message_from_arguments yields `None` so telemetry degrades gracefully rather than
/// failing to load.
pub(crate) fn read_run_outcome_for_journal(
    state_root: &Path,
    journal: &Path,
) -> Option<RunOutcome>
{
    let txid = txid_from_journal(journal);
    if txid.is_empty()
    {
        return None;
    }
    let path = outcome_path(state_root, &txid);
    let text = read_capped(&path, MAX_CONTROL_FILE_BYTES).ok()?;
    return serde_json::from_str(&text).ok();
}

fn outcomes_directory(state_root: &Path) -> PathBuf
{
    return state_root.join("outcomes");
}

fn outcome_path(state_root: &Path, txid: &str) -> PathBuf
{
    return outcomes_directory(state_root).join(format!("{txid}.json"));
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn txid_from_journal_uses_the_file_stem()
    {
        let journal = PathBuf::from("state")
            .join("journals")
            .join("run-default-42.journal");
        assert_eq!(txid_from_journal(&journal), "run-default-42");
    }

    #[test]
    fn outcome_round_trips_through_disk()
    {
        let state_root = test_state_root("round_trip");
        let journal = state_root.join("journals").join("run-default-42.journal");
        let outcome = RunOutcome {
            version: 1,
            txid: txid_from_journal(&journal),
            profile: "default".to_string(),
            result: RUN_RESULT_GAME_ERROR.to_string(),
            exit_code: Some(3), // literal: allow test fixture value is the specimen under judgment
            duration_ms: Some(42_000), // literal: allow test fixture value is the specimen under judgment
            launch_args: vec!["-w".to_string()],
            started_unix: 100, // literal: allow test fixture value is the specimen under judgment
            finished_unix: 142, // literal: allow test fixture value is the specimen under judgment
        };

        write_run_outcome(&state_root, &outcome)
            .expect("the test fixture is created before this assertion reads it");
        let read = read_run_outcome_for_journal(&state_root, &journal)
            .expect("the test fixture is created before this assertion reads it");

        assert_eq!(read.result, RUN_RESULT_GAME_ERROR);
        assert_eq!(read.exit_code, Some(3)); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(read.duration_ms, Some(42_000)); // literal: allow test fixture value is the specimen under judgment
        assert_eq!(read.launch_args, vec!["-w".to_string()]);
        fs::remove_dir_all(&state_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    #[test]
    fn missing_outcome_reads_as_none()
    {
        let state_root = test_state_root("missing");
        let journal = state_root.join("journals").join("run-none-1.journal");
        assert!(read_run_outcome_for_journal(&state_root, &journal).is_none());
        fs::remove_dir_all(&state_root)
            .expect("the test fixture is created before this assertion reads it");
    }

    fn test_state_root(name: &str) -> PathBuf
    {
        let root = env::temp_dir().join(format!(
            "sa-mod-manager-outcome-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        if root.exists()
        {
            fs::remove_dir_all(&root)
                .expect("the test fixture is created before this assertion reads it");
        }
        fs::create_dir_all(&root)
            .expect("the test fixture is created before this assertion reads it");
        return root;
    }
}
