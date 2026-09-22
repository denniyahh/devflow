//! State recovery and stale-state detection.
//!
//! `devflow recover` reads the existing state file, determines if the
//! agent process is still running, and either reports status or
//! offers to clean up / restart.

use crate::phase_id::PhaseId;
use crate::state::State;
use crate::workflow::{self, WorkflowError};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Maximum age before a state is considered stale (>24h).
pub const STALE_THRESHOLD: Duration = Duration::from_secs(24 * 60 * 60);

/// Errors produced by recover operations.
#[derive(Debug, thiserror::Error)]
pub enum RecoverError {
    /// No state file exists — nothing to recover.
    #[error("no state to recover — project is idle")]
    NothingToRecover,
    /// Filesystem operation failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// State loading failed.
    #[error("{0}")]
    Workflow(#[from] WorkflowError),
    /// Phase-lock acquisition failed.
    #[error("{0}")]
    Lock(#[from] crate::lock::LockError),
    /// Gate cleanup failed.
    #[error("{0}")]
    Gate(#[from] crate::gates::GateError),
}

/// Result of inspecting an existing workflow state.
#[derive(Debug)]
pub struct RecoveryStatus {
    /// The loaded workflow state.
    pub state: State,
    /// Whether the agent process is still running.
    pub agent_running: bool,
    /// Whether the state is considered stale (>24h without agent).
    pub is_stale: bool,
    /// Human-readable age of the state.
    pub age: String,
    /// Whether a lock file is present (shows holder PID).
    pub lock_held: Option<String>,
}

/// Inspect every active phase state, producing one recovery status per phase
/// (sorted by phase number). Errors with [`RecoverError::NothingToRecover`]
/// when no phase has persisted state.
pub fn inspect_all(project_root: &Path) -> Result<Vec<RecoveryStatus>, RecoverError> {
    let states = workflow::list_states(project_root);
    if states.is_empty() {
        return Err(RecoverError::NothingToRecover);
    }
    Ok(states
        .into_iter()
        .map(|state| inspect_state(project_root, state))
        .collect())
}

fn inspect_state(project_root: &Path, state: State) -> RecoveryStatus {
    let agent_running = agent_pid_for(&state).is_some_and(crate::agent::agent_running);
    let is_stale = is_stale_state(&state);
    let age = format_age(state.started_at.as_str());
    let lock_held = crate::lock::holder(project_root, state.phase).map(|(pid, _)| pid);

    RecoveryStatus {
        state,
        agent_running,
        is_stale,
        age,
        lock_held,
    }
}

/// Clean up stale or abandoned workflow state.
///
/// 14-CR-01: only STALE phases are swept — a phase whose agent is still
/// running, or whose state is simply fresh, is kept (with a warning naming
/// the explicit `--phase` escape hatch), so cleaning one dead phase under
/// `devflow parallel` can never orphan a healthy sibling. Also removes an
/// unparsable legacy `state.json` (14-CR-04 — this reset is the one
/// sanctioned place), lock files whose holder is dead (the sweep lives in
/// [`crate::lock::remove_stale_locks`], which refuses to delete a live
/// holder's lock), gate files whose phase has no state file, and
/// cron-instruction records for phases that no longer have state —
/// self-describing "auto-re-run this phase" records that must not survive an
/// operator-driven reset. Returns warnings for anything kept or that could
/// not be removed.
pub fn clean(project_root: &Path) -> Result<Vec<String>, RecoverError> {
    clean_report(project_root).map(|report| report.warnings)
}

/// What [`clean_report`] did: the phases whose state it cleared, the
/// stateless phases whose orphaned gate files it removed, and warnings for
/// anything it kept or could not remove.
#[derive(Debug, Default)]
pub struct CleanReport {
    /// Phases whose persisted state was cleared.
    pub cleared: Vec<PhaseId>,
    /// Phases with no state file whose leftover gate files were removed.
    pub orphan_gates_cleared: Vec<PhaseId>,
    /// Whether a removal the sweep attempted failed. Each failure is also
    /// described in `warnings`.
    pub removal_failed: bool,
    /// Phases kept, and removals that failed, in human-readable form.
    pub warnings: Vec<String>,
}

impl CleanReport {
    fn fail(&mut self, warning: String) {
        self.removal_failed = true;
        self.warnings.push(warning);
    }
}

/// [`clean`], reporting what it removed so a caller can say so instead of
/// claiming a cleanup that did not happen. One phase's failure is recorded
/// and the sweep goes on (48-REVIEW WR-04): aborting would hide the phases
/// already cleared and skip every later step.
pub fn clean_report(project_root: &Path) -> Result<CleanReport, RecoverError> {
    let mut report = CleanReport::default();
    let states = workflow::list_states(project_root);
    for state in &states {
        sweep_phase(project_root, state, &mut report);
    }
    for phase in workflow::state_file_phases(project_root) {
        if !states.iter().any(|state| state.phase == phase) {
            report.warnings.push(format!(
                "kept phase {phase} — its state file cannot be parsed (clear explicitly with --phase {phase})"
            ));
        }
    }
    sweep_orphan_gates(project_root, &mut report);
    match workflow::remove_corrupt_legacy_state(project_root) {
        Ok(true) => report
            .warnings
            .push("removed unparsable legacy state.json".into()),
        Ok(false) => {}
        Err(err) => report.fail(format!("could not remove corrupt legacy state.json: {err}")),
    }
    report
        .warnings
        .append(&mut crate::lock::remove_stale_locks(project_root));
    // Drop cron records only for phases without surviving state, so a kept
    // phase's pending re-run record is preserved.
    for instructions in crate::ship::list_cron_instructions(project_root) {
        if workflow::state_path(project_root, instructions.phase).exists() {
            continue;
        }
        let (removed, result) = crate::ship::delete_cron_instructions_with_partial_result(
            project_root,
            instructions.phase,
        );
        if let Err(err) = result {
            let partial = if removed {
                " after removing its per-phase record"
            } else {
                ""
            };
            report.fail(format!(
                "could not remove cron-instructions for phase {}{partial}: {err}",
                instructions.phase,
            ));
        }
    }
    Ok(report)
}

/// Clear one listed phase if it is stale and its lock is free.
fn sweep_phase(project_root: &Path, state: &State, report: &mut CleanReport) {
    let phase = state.phase;
    if agent_pid_for(state).is_some_and(crate::agent::agent_running) {
        report.warnings.push(format!(
            "kept phase {phase} — its agent is still running (clear explicitly with --phase {phase})"
        ));
        return;
    }
    if !is_stale_state(state) {
        report.warnings.push(format!(
            "kept phase {phase} — state is not stale yet (clear explicitly with --phase {phase})"
        ));
        return;
    }
    // A monitor waiting at a gate holds this lock after its agent exited,
    // so staleness by agent pid alone cannot license the delete.
    let Some(_guard) = lock_for_sweep(project_root, phase, "phase", report) else {
        return;
    };
    // Same order as `clean_phase_report`: a gate left behind would outlive
    // the state that explains it, and no command would answer it. So a
    // failed gate removal keeps the state.
    let (gates_removed, gate_result) = remove_gate_files(project_root, phase);
    if let Err(err) = gate_result {
        let partial = if gates_removed {
            " after removing some of its gate files"
        } else {
            ""
        };
        report.fail(format!(
            "kept phase {phase}'s state{partial} — could not remove all of its gate files: {err}"
        ));
        return;
    }
    let (state_removed, state_result) =
        workflow::clear_state_with_partial_result(project_root, phase);
    match state_result {
        Ok(()) if state_removed => report.cleared.push(phase),
        Ok(()) => {}
        Err(err) => {
            if state_removed {
                report.cleared.push(phase);
            }
            let partial = if state_removed {
                " after removing some of its state files"
            } else {
                ""
            };
            report.fail(format!(
                "could not clear phase {phase}'s state{partial} after removing its gate files: {err}"
            ));
        }
    }
}

/// Remove gate files whose phase has no state file (48-REVIEW WR-02): left by
/// a sweep that predates gate cleanup, or by `abort` for another stage. They
/// show as open gates nothing will ever answer.
fn sweep_orphan_gates(project_root: &Path, report: &mut CleanReport) {
    for phase in crate::gates::Gates::phases_on_disk(project_root) {
        if workflow::state_path(project_root, phase).exists() {
            continue;
        }
        let what = "the orphan gate files of phase";
        let Some(_guard) = lock_for_sweep(project_root, phase, what, report) else {
            continue;
        };
        // A run may have started between the listing and the lock.
        if workflow::state_path(project_root, phase).exists() {
            continue;
        }
        let (removed, result) = remove_gate_files(project_root, phase);
        match result {
            Ok(()) if removed => report.orphan_gates_cleared.push(phase),
            Ok(()) => {}
            Err(err) => {
                let partial = if removed {
                    " after removing some of them"
                } else {
                    ""
                };
                report.fail(format!(
                    "could not remove all of {what} {phase}{partial}: {err}"
                ));
            }
        }
    }
}

/// Take `phase`'s lock for the sweep, or record why `what` was kept.
fn lock_for_sweep(
    project_root: &Path,
    phase: PhaseId,
    what: &str,
    report: &mut CleanReport,
) -> Option<crate::lock::LockGuard> {
    match crate::lock::acquire(project_root, phase) {
        Ok(guard) => Some(guard),
        Err(crate::lock::LockError::Contended { pid, .. }) => {
            report.warnings.push(format!(
                "kept {what} {phase} — its per-phase lock is live or contended by pid {pid}"
            ));
            None
        }
        Err(err) => {
            report.fail(format!(
                "kept {what} {phase} — could not take its per-phase lock: {err}"
            ));
            None
        }
    }
}

/// Remove every stage's gate files for `phase`, retaining partial removal on error.
fn remove_gate_files(
    project_root: &Path,
    phase: PhaseId,
) -> (bool, Result<(), crate::gates::GateError>) {
    let mut removed = false;
    for stage in STAGES {
        let (stage_removed, result) =
            crate::gates::Gates::cleanup_with_partial_result(project_root, phase, stage);
        removed |= stage_removed;
        if let Err(error) = result {
            return (removed, Err(error));
        }
    }
    (removed, Ok(()))
}

/// Explicitly clean ONE phase, regardless of staleness — the operator's
/// escape hatch for a wedged-but-fresh run. Clears its state and cron
/// record; warns (but proceeds) when the recorded agent still looks alive.
/// Deletes nothing and returns [`RecoverError::Lock`] with
/// [`crate::lock::LockError::Contended`] when the per-phase lock is live or
/// contended, so a refused clean cannot read as a successful one. A removal
/// that fails is among the warnings; [`clean_phase_report`] flags it.
pub fn clean_phase(project_root: &Path, phase: PhaseId) -> Result<Vec<String>, RecoverError> {
    clean_phase_report(project_root, phase).map(|report| report.warnings)
}

/// Every stage whose gate files a phase can leave behind.
const STAGES: [crate::stage::Stage; 5] = [
    crate::stage::Stage::Define,
    crate::stage::Stage::Plan,
    crate::stage::Stage::Code,
    crate::stage::Stage::Validate,
    crate::stage::Stage::Ship,
];

/// What [`clean_phase_report`] did for one phase.
#[derive(Debug, Default)]
pub struct PhaseCleanReport {
    /// Whether this call removed any of the phase's state (including a
    /// legacy single-slot file naming it), gate files, cron record, or their
    /// orphaned write temps.
    pub removed_anything: bool,
    /// Whether a removal failed. Each failure is also in `warnings`.
    pub removal_failed: bool,
    /// Removals that failed and agents that looked alive, in human-readable
    /// form.
    pub warnings: Vec<String>,
}

/// [`clean_phase`], reporting what it removed and whether a removal failed,
/// so a caller claims neither a cleanup that did not happen nor "nothing to
/// clean" after deleting something (48-REVIEW WR-03). A failure stops the
/// clean where it is: gate files that could not go keep the state that
/// explains them, and state that could not go keeps its cron record.
pub fn clean_phase_report(
    project_root: &Path,
    phase: PhaseId,
) -> Result<PhaseCleanReport, RecoverError> {
    let guard = crate::lock::acquire(project_root, phase)?;
    let mut report = PhaseCleanReport::default();
    if let Ok(state) = workflow::load_state(project_root, phase)
        && agent_pid_for(&state).is_some_and(crate::agent::agent_running)
    {
        report.warnings.push(format!(
            "phase {phase}'s agent appears to still be running — cleared anyway (explicit --phase)"
        ));
    }
    clean_phase_files(project_root, phase, &mut report);
    drop(guard);
    report
        .warnings
        .append(&mut crate::lock::remove_stale_locks(project_root));
    Ok(report)
}

fn clean_phase_files(project_root: &Path, phase: PhaseId, report: &mut PhaseCleanReport) {
    let (gates_removed, gate_result) = remove_gate_files(project_root, phase);
    report.removed_anything |= gates_removed;
    match gate_result {
        Ok(()) => {}
        Err(err) => {
            return report.fail(format!(
                "kept phase {phase}'s state — could not remove all of its gate files: {err}"
            ));
        }
    }
    let (state_removed, state_result) =
        workflow::clear_state_with_partial_result(project_root, phase);
    report.removed_anything |= state_removed;
    match state_result {
        Ok(()) => {}
        Err(err) => {
            return report.fail(format!(
                "kept phase {phase}'s cron record — could not clear its state: {err}"
            ));
        }
    }
    let (cron_removed, cron_result) =
        crate::ship::delete_cron_instructions_with_partial_result(project_root, phase);
    report.removed_anything |= cron_removed;
    if let Err(err) = cron_result {
        report.fail(format!("could not remove cron-instructions: {err}"));
    }
}

impl PhaseCleanReport {
    fn fail(&mut self, warning: String) {
        self.removal_failed = true;
        self.warnings.push(warning);
    }
}

/// Check whether a state is stale: >24h old with no running agent.
pub fn is_stale_state(state: &State) -> bool {
    let age_secs = match state_age_secs(&state.started_at) {
        Some(a) => a,
        None => return false,
    };

    if age_secs < STALE_THRESHOLD.as_secs() {
        return false;
    }

    // Only stale if the agent process is gone
    if let Some(pid) = agent_pid_for(state)
        && crate::agent::agent_running(pid)
    {
        return false;
    }

    true
}

/// Read the launched agent PID the monitor recorded for this state's phase, if
/// the pid file is present and parseable.
fn agent_pid_for(state: &State) -> Option<u32> {
    let path = crate::agent_result::agent_pid_path(&state.project_root, state.phase);
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Compute the age of a state's `started_at` timestamp in seconds.
fn state_age_secs(started_at: &str) -> Option<u64> {
    let started: u64 = started_at.parse().ok()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.checked_sub(started)
}

/// Format a unix-seconds timestamp's age as a human-readable string
/// ("5m ago"). Public since 14c: `devflow status` reuses it for elapsed
/// time and event recency.
pub fn format_age(started_at: &str) -> String {
    match state_age_secs(started_at) {
        Some(s) if s < 60 => format!("{s}s ago"),
        Some(s) if s < 3600 => format!("{}m ago", s / 60),
        Some(s) if s < 86400 => format!("{}h ago", s / 3600),
        Some(s) => format!("{}d ago", s / 86400),
        None => "unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gates::{GateResponse, Gates};
    use crate::mode::Mode;
    use crate::stage::Stage;
    use crate::state::{AgentKind, State};

    /// Build a state in `root` whose `started_at` is `age_secs` in the past,
    /// optionally writing the monitor's agent-pid file with `agent_pid`.
    fn state_aged(root: &Path, age_secs: u64, agent_pid: Option<u32>) -> State {
        state_aged_phase(root, PhaseId::new(1), age_secs, agent_pid)
    }

    fn state_aged_phase(
        root: &Path,
        phase: PhaseId,
        age_secs: u64,
        agent_pid: Option<u32>,
    ) -> State {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.started_at = now.saturating_sub(age_secs).to_string();
        if let Some(pid) = agent_pid {
            let path = crate::agent_result::agent_pid_path(root, state.phase);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, pid.to_string()).unwrap();
        }
        state
    }

    /// A PID that is essentially certain not to map to a live process.
    const DEAD_PID: u32 = 0x7FFF_FFFE;

    fn write_phase_artifacts(root: &Path, phase: PhaseId) -> Vec<std::path::PathBuf> {
        workflow::save_state(&state_aged_phase(root, phase, 0, None)).unwrap();
        let cron = crate::ship::build_single_agent_cron_instructions(root, phase, "");
        crate::ship::write_cron_instructions(root, &cron).unwrap();

        let mut paths = vec![
            workflow::state_path(root, phase),
            crate::ship::cron_instructions_path(root, phase),
        ];
        for stage in [Stage::Validate, Stage::Ship] {
            Gates::write_gate(root, phase, stage, "wedged gate").unwrap();
            Gates::respond(
                root,
                phase,
                stage,
                &GateResponse {
                    approved: false,
                    note: Some("abort: test".into()),
                    responded_by: Some("test".into()),
                },
            )
            .unwrap();
            Gates::ack(root, phase, stage).unwrap();
            let gate = Gates::gate_path(root, phase, stage);
            let temp = Gates::dir(root).join(format!(
                ".{}.recovery.tmp",
                gate.file_name().unwrap().to_string_lossy()
            ));
            std::fs::write(&temp, "orphaned gate temp").unwrap();
            paths.extend([
                gate,
                Gates::response_path(root, phase, stage),
                Gates::ack_path(root, phase, stage),
                temp,
            ]);
        }
        paths
    }

    fn artifact_bytes(paths: &[std::path::PathBuf]) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        paths
            .iter()
            .map(|path| (path.clone(), std::fs::read(path).unwrap()))
            .collect()
    }

    #[test]
    fn fresh_state_is_not_stale() {
        // One hour old, well under the 24h threshold.
        let dir = tempfile::tempdir().unwrap();
        let state = state_aged(dir.path(), 3600, None);
        assert!(!is_stale_state(&state));
    }

    #[test]
    fn old_state_with_no_agent_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_aged(dir.path(), STALE_THRESHOLD.as_secs() + 60, None);
        assert!(is_stale_state(&state));
    }

    #[test]
    fn old_state_with_dead_agent_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_aged(dir.path(), STALE_THRESHOLD.as_secs() + 60, Some(DEAD_PID));
        assert!(is_stale_state(&state));
    }

    #[test]
    fn old_state_with_live_agent_is_not_stale() {
        // Our own PID is guaranteed to be running.
        let dir = tempfile::tempdir().unwrap();
        let own_pid = std::process::id();
        let state = state_aged(dir.path(), STALE_THRESHOLD.as_secs() + 60, Some(own_pid));
        assert!(!is_stale_state(&state));
    }

    #[test]
    fn unparseable_timestamp_is_never_stale() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::new(
            PhaseId::new(1),
            AgentKind::Claude,
            Mode::Auto,
            dir.path().to_path_buf(),
        );
        state.started_at = "not-a-number".into();
        assert!(!is_stale_state(&state));
        assert_eq!(state_age_secs(&state.started_at), None);
    }

    #[test]
    fn state_age_secs_parses_epoch() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let started = (now - 120).to_string();
        let age = state_age_secs(&started).expect("age");
        // Allow a small window for clock drift during the test.
        assert!((118..=125).contains(&age), "unexpected age: {age}");
    }

    #[test]
    fn format_age_buckets_by_magnitude() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ago = |secs: u64| format_age(&(now - secs).to_string());
        assert!(ago(30).ends_with("s ago"));
        assert!(ago(120).ends_with("m ago"));
        assert!(ago(7200).ends_with("h ago"));
        assert!(ago(2 * 86400).ends_with("d ago"));
        assert_eq!(format_age("garbage"), "unknown");
    }

    #[test]
    fn inspect_all_missing_state_reports_nothing_to_recover() {
        let dir = std::env::temp_dir().join(format!("devflow-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let err = inspect_all(&dir).expect_err("should have no state");
        assert!(matches!(err, RecoverError::NothingToRecover));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 13-DEFERRED-CR-03 acceptance: recover must enumerate ALL active
    /// phases, not just the last one started.
    #[test]
    fn inspect_all_enumerates_every_active_phase() {
        let dir = tempfile::tempdir().unwrap();
        workflow::save_state(&state_aged(dir.path(), 60, None)).unwrap();
        let mut other = state_aged(dir.path(), 60, None);
        other.phase = PhaseId::new(2);
        workflow::save_state(&other).unwrap();

        let statuses = inspect_all(dir.path()).expect("two phases active");
        assert_eq!(
            statuses.iter().map(|s| s.state.phase).collect::<Vec<_>>(),
            vec![PhaseId::new(1), PhaseId::new(2)]
        );
    }

    /// 14-CR-01: `recover --clean` must never delete a phase whose agent is
    /// still running — under `devflow parallel`, cleaning a stale phase must
    /// not orphan a healthy sibling.
    #[test]
    fn clean_keeps_phase_with_live_agent() {
        let dir = tempfile::tempdir().unwrap();
        // Stale-aged but the recorded agent (our own pid) is alive.
        let live = state_aged_phase(
            dir.path(),
            PhaseId::new(1),
            STALE_THRESHOLD.as_secs() + 60,
            Some(std::process::id()),
        );
        workflow::save_state(&live).unwrap();
        // Genuinely stale sibling: old and dead.
        let stale = state_aged_phase(
            dir.path(),
            PhaseId::new(2),
            STALE_THRESHOLD.as_secs() + 60,
            Some(DEAD_PID),
        );
        workflow::save_state(&stale).unwrap();

        let warnings = clean(dir.path()).expect("clean");

        let remaining: Vec<PhaseId> = workflow::list_states(dir.path())
            .iter()
            .map(|s| s.phase)
            .collect();
        assert_eq!(
            remaining,
            vec![PhaseId::new(1)],
            "live phase must survive, stale cleared"
        );
        assert!(
            warnings.iter().any(|w| w.contains("phase 1")),
            "keeping a live phase must be reported: {warnings:?}"
        );
    }

    /// 14-CR-01: a fresh (not yet stale) phase is also kept — only stale
    /// phases are swept implicitly; anything else needs explicit `--phase`.
    #[test]
    fn clean_keeps_fresh_phase() {
        let dir = tempfile::tempdir().unwrap();
        workflow::save_state(&state_aged_phase(dir.path(), PhaseId::new(3), 60, None)).unwrap();

        let warnings = clean(dir.path()).expect("clean");

        assert_eq!(workflow::list_states(dir.path()).len(), 1);
        assert!(warnings.iter().any(|w| w.contains("--phase 3")));
    }

    #[test]
    fn clean_clears_stale_phase_state() {
        let dir = tempfile::tempdir().unwrap();
        workflow::save_state(&state_aged_phase(
            dir.path(),
            PhaseId::new(2),
            STALE_THRESHOLD.as_secs() + 60,
            Some(DEAD_PID),
        ))
        .unwrap();

        clean(dir.path()).expect("clean");

        assert!(workflow::list_states(dir.path()).is_empty());
    }

    /// Fix-review finding (agy C-2): the sweep cleared a stale phase's state
    /// but left its gate files, so `gate list` kept showing a gate no command
    /// would answer. `clean_keeps_the_gate_files_of_a_phase_it_keeps` is the
    /// opposite-result control.
    #[test]
    fn clean_removes_the_gate_files_of_a_stale_phase_it_clears() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(53);
        workflow::save_state(&state_aged_phase(
            root,
            phase,
            STALE_THRESHOLD.as_secs() + 60,
            Some(DEAD_PID),
        ))
        .unwrap();
        Gates::write_gate(root, phase, Stage::Code, "left open").unwrap();

        clean(root).expect("clean");

        assert!(!workflow::state_path(root, phase).exists());
        assert!(
            !Gates::gate_path(root, phase, Stage::Code).exists(),
            "clearing a stale phase must also remove its gate files"
        );
    }

    #[test]
    fn clean_keeps_the_gate_files_of_a_phase_it_keeps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(54);
        workflow::save_state(&state_aged_phase(root, phase, 60, Some(DEAD_PID))).unwrap();
        Gates::write_gate(root, phase, Stage::Code, "still fresh").unwrap();

        clean(root).expect("clean");

        assert!(workflow::state_path(root, phase).exists());
        assert!(
            Gates::gate_path(root, phase, Stage::Code).exists(),
            "a kept phase must keep its gate files"
        );
    }

    /// A monitor waiting at a gate holds the per-phase lock after its agent
    /// has exited, so an old run looks stale by agent pid alone. The implicit
    /// sweep must take the lock like `clean_phase`, and keep the phase when it
    /// cannot. `clean_clears_stale_phase_state` is the unlocked control.
    #[test]
    fn clean_keeps_a_stale_phase_whose_lock_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(4);
        workflow::save_state(&state_aged_phase(
            root,
            phase,
            STALE_THRESHOLD.as_secs() + 60,
            Some(DEAD_PID),
        ))
        .unwrap();
        let state_path = workflow::state_path(root, phase);
        let state_before = std::fs::read(&state_path).unwrap();
        // The gate files a waiting monitor is blocked on: the sweep must not
        // clean them either, so the lock has to be taken before gate cleanup
        // as well as before the state delete.
        Gates::write_gate(root, phase, Stage::Code, "waiting").unwrap();
        std::fs::write(
            Gates::response_path(root, phase, Stage::Code),
            r#"{"approved":true,"note":null,"responded_by":"test"}"#,
        )
        .unwrap();
        std::fs::write(Gates::ack_path(root, phase, Stage::Code), "{}").unwrap();
        let gate_paths = [
            Gates::gate_path(root, phase, Stage::Code),
            Gates::response_path(root, phase, Stage::Code),
            Gates::ack_path(root, phase, Stage::Code),
        ];
        let gates_before: Vec<Vec<u8>> = gate_paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();
        let guard = crate::lock::acquire(root, phase).expect("hold phase lock");
        let lock_path = crate::lock::lock_path(root, phase);
        let lock_before = std::fs::read(&lock_path).unwrap();

        let warnings = clean(root).expect("clean");

        assert_eq!(
            std::fs::read(&state_path).ok(),
            Some(state_before),
            "a stale phase whose lock is held must keep its state byte-identical"
        );
        for (path, before) in gate_paths.iter().zip(gates_before) {
            assert_eq!(
                std::fs::read(path).ok(),
                Some(before),
                "a stale phase whose lock is held must keep {} byte-identical",
                path.display()
            );
        }
        assert!(
            warnings
                .iter()
                .any(|w| w.contains(&format!("kept phase {phase}")) && w.contains("lock")),
            "the kept phase must be reported with its lock as the reason: {warnings:?}"
        );
        assert_eq!(
            std::fs::read(&lock_path).ok(),
            Some(lock_before),
            "the sweep must leave the holder's lock file untouched"
        );
        drop(guard);
    }

    /// 14-CR-04: a corrupt legacy `state.json` (old binary killed mid-write)
    /// can never be migrated or matched by a per-phase clear — the operator
    /// reset is the one sanctioned place to remove it.
    #[test]
    fn clean_removes_corrupt_legacy_state_json() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join(".devflow/state.json");
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, "{\"stage\":").unwrap();

        clean(dir.path()).expect("clean");

        assert!(
            !legacy.exists(),
            "recover --clean must remove an unparsable legacy state.json"
        );
    }

    /// 14-CR-01: explicit `--phase` cleanup clears exactly that phase, even
    /// when it is fresh, and leaves siblings alone.
    #[test]
    fn clean_phase_clears_only_the_named_phase() {
        let dir = tempfile::tempdir().unwrap();
        workflow::save_state(&state_aged_phase(dir.path(), PhaseId::new(4), 60, None)).unwrap();
        workflow::save_state(&state_aged_phase(dir.path(), PhaseId::new(5), 60, None)).unwrap();

        clean_phase(dir.path(), PhaseId::new(4)).expect("clean_phase");

        let remaining: Vec<PhaseId> = workflow::list_states(dir.path())
            .iter()
            .map(|s| s.phase)
            .collect();
        assert_eq!(remaining, vec![PhaseId::new(5)]);
    }

    /// D-19: recover::clean remains a reset path, distinct from lifecycle
    /// consumption, and deletes a deliberately unconsumed record.
    #[test]
    fn clean_still_deletes_unconsumed_cron_instructions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(30);
        let state = state_aged_phase(root, phase, STALE_THRESHOLD.as_secs() + 60, None);
        crate::workflow::save_state(&state).unwrap();
        let record = crate::ship::build_single_agent_cron_instructions(root, phase, "");
        crate::ship::write_cron_instructions(root, &record).unwrap();
        assert!(crate::ship::cron_instructions_path(root, phase).exists());

        clean(root).unwrap();

        assert!(!crate::ship::cron_instructions_path(root, phase).exists());
    }

    /// D-19: explicit clean_phase reset remains distinct from consumption and
    /// deletes only the named phase's deliberately unconsumed record.
    #[test]
    fn clean_phase_deletes_only_the_named_phase_cron_record() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let first = PhaseId::new(31);
        let second = PhaseId::new(32);
        let first_state = state_aged_phase(root, first, 0, None);
        crate::workflow::save_state(&first_state).unwrap();
        let first_record = crate::ship::build_single_agent_cron_instructions(root, first, "");
        let second_record = crate::ship::build_single_agent_cron_instructions(root, second, "");
        crate::ship::write_cron_instructions(root, &first_record).unwrap();
        crate::ship::write_cron_instructions(root, &second_record).unwrap();

        clean_phase(root, first).unwrap();

        assert!(!crate::ship::cron_instructions_path(root, first).exists());
        assert!(crate::ship::cron_instructions_path(root, second).exists());
    }

    #[test]
    fn clean_phase_report_finds_nothing_for_an_absent_phase() {
        let dir = tempfile::tempdir().unwrap();
        let report = clean_phase_report(dir.path(), PhaseId::new(51)).expect("clean");
        assert!(
            !report.removed_anything,
            "a phase with nothing on disk must not report a cleanup"
        );
    }

    /// Control for the test above: a leftover gate file alone, with no state,
    /// is something to clean.
    #[test]
    fn clean_phase_report_counts_a_lone_gate_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(52);
        Gates::write_gate(root, phase, Stage::Code, "leftover").unwrap();
        assert!(!workflow::state_path(root, phase).exists());

        let report = clean_phase_report(root, phase).expect("clean");

        assert!(
            report.removed_anything,
            "a lone gate file it removes must count as removed"
        );
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
    }

    /// R-5: an earlier successful gate removal remains reportable when a
    /// later stage's gate path cannot be removed.
    #[test]
    fn clean_phase_report_preserves_earlier_gate_removal_when_later_gate_removal_fails() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(61);
        let early_gate = Gates::gate_path(root, phase, Stage::Define);
        let later_gate = Gates::gate_path(root, phase, Stage::Plan);
        Gates::write_gate(root, phase, Stage::Define, "leftover").unwrap();
        std::fs::create_dir_all(&later_gate).unwrap();

        let report = clean_phase_report(root, phase).expect("clean");

        assert!(
            !early_gate.exists(),
            "the early Define gate must have been removed before the failure"
        );
        assert!(
            later_gate.is_dir(),
            "the Plan gate directory is the negative control for this failure"
        );
        assert!(
            report.removal_failed,
            "the Plan gate directory must be reported as a failed removal"
        );
        assert!(
            report.removed_anything,
            "the Define gate was removed before the Plan removal failed"
        );
    }

    /// R-4: an orphaned state-write temp that cannot be removed is a failed
    /// removal, even though the phase's actual state file was cleared first.
    #[test]
    fn clean_phase_report_reports_an_unremovable_state_temp_after_clearing_state() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(63);
        workflow::save_state(&state_aged_phase(root, phase, 0, None)).unwrap();
        let state_path = workflow::state_path(root, phase);
        let temp = workflow::devflow_dir(root).join(".state-63.json.1.0.tmp");
        std::fs::create_dir_all(&temp).unwrap();

        let report = clean_phase_report(root, phase).expect("clean");

        assert!(
            !state_path.exists(),
            "the state file must be removed before the temp-removal failure"
        );
        assert!(
            temp.is_dir(),
            "the state-temp directory is the negative control for this failure"
        );
        assert!(
            report.removal_failed,
            "the unremovable state temp must be reported as a failed removal"
        );
        assert!(
            report.removed_anything,
            "the successfully removed state file must remain reportable"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains(temp.file_name().unwrap().to_str().unwrap())),
            "the unremovable state temp must be named in a warning: {:?}",
            report.warnings
        );
    }

    /// R-3: a stale lock whose coordination inode cannot be opened must be
    /// reported as a failed removal, not only as a warning.
    #[test]
    fn clean_phase_report_flags_a_stale_lock_with_unusable_coordination() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let cleaned_phase = PhaseId::new(65);
        let stale_lock = crate::lock::lock_path(root, PhaseId::new(66));
        std::fs::create_dir_all(stale_lock.parent().unwrap()).unwrap();
        std::fs::write(&stale_lock, DEAD_PID.to_string()).unwrap();
        let coordination = stale_lock.with_file_name(format!(
            ".{}.coord",
            stale_lock.file_name().unwrap().to_string_lossy()
        ));
        // A directory where the coordination inode belongs: opening it fails
        // before the sweeper can remove the dead-holder lock.
        std::fs::create_dir_all(&coordination).unwrap();

        let report = clean_phase_report(root, cleaned_phase).expect("clean");

        assert!(
            stale_lock.is_file(),
            "the stale lock must remain when its coordination cannot be opened"
        );
        assert!(
            coordination.is_dir(),
            "the coordination directory is the negative control for this failure"
        );
        assert!(
            report.removal_failed,
            "the unremovable stale lock must be reported as a failed removal"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("could not coordinate")
                    && warning.contains("lock-66")),
            "the stale-lock failure must be reported: {:?}",
            report.warnings
        );
    }

    #[test]
    fn clean_phase_removes_gate_files_when_no_process_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(48);
        let paths = write_phase_artifacts(root, phase);

        clean_phase(root, phase).expect("clean lock-free phase");

        for path in paths {
            assert!(
                !path.exists(),
                "cleaning a lock-free phase must remove {}",
                path.display()
            );
        }
        assert!(
            !crate::lock::lock_path(root, phase).exists(),
            "the cleanup lock must be dropped before stale-lock sweeping"
        );
    }

    #[test]
    fn clean_phase_returns_without_cleanup_while_the_per_phase_lock_is_contended() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(49);
        let paths = write_phase_artifacts(root, phase);
        let guard = crate::lock::acquire(root, phase).expect("hold phase lock");
        let lock_path = crate::lock::lock_path(root, phase);
        let mut before = artifact_bytes(&paths);
        before.push((lock_path.clone(), std::fs::read(&lock_path).unwrap()));

        let err = clean_phase(root, phase).expect_err("a contended clean must be refused");

        assert!(
            matches!(
                err,
                RecoverError::Lock(crate::lock::LockError::Contended { .. })
            ),
            "contention must be reported as a lock refusal: {err:?}"
        );
        for (path, contents) in before {
            assert_eq!(
                std::fs::read(&path).unwrap(),
                contents,
                "contention must leave {} byte-identical",
                path.display()
            );
        }
        drop(guard);
    }

    #[test]
    fn clean_phase_returns_without_cleanup_for_a_recycled_pid_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(50);
        let paths = write_phase_artifacts(root, phase);
        let lock_path = crate::lock::lock_path(root, phase);
        let observed =
            crate::agent::process_start_time(std::process::id()).expect("test pid start time");
        std::fs::write(
            &lock_path,
            format!("{}\n{}", std::process::id(), observed + 1),
        )
        .unwrap();
        assert!(matches!(
            crate::lock::holder_status(root, phase),
            crate::lock::HolderStatus::Recycled { .. }
        ));
        let mut before = artifact_bytes(&paths);
        before.push((lock_path.clone(), std::fs::read(&lock_path).unwrap()));

        let err = clean_phase(root, phase).expect_err("a recycled-pid lock must be refused");

        assert!(
            matches!(
                err,
                RecoverError::Lock(crate::lock::LockError::Contended { .. })
            ),
            "acquisition contention, not read-only classification, controls cleanup: {err:?}"
        );
        for (path, contents) in before {
            assert_eq!(
                std::fs::read(&path).unwrap(),
                contents,
                "a recycled-pid lock must preserve {}",
                path.display()
            );
        }
    }

    /// WR-02 (48-REVIEW.md): gate files whose phase has no state file — left by
    /// a pre-`854bbce` sweep, or by `abort` for another stage — were never
    /// reached, because the sweep only cleaned gates inside its per-state loop.
    /// `clean_keeps_the_orphan_gate_files_of_a_locked_phase` is the control.
    #[test]
    fn clean_removes_the_gate_files_of_a_phase_with_no_state() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(9);
        Gates::write_gate(root, phase, Stage::Code, "orphaned").unwrap();
        Gates::write_gate(root, phase, Stage::Validate, "orphaned").unwrap();
        Gates::respond(
            root,
            phase,
            Stage::Validate,
            &GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        Gates::ack(root, phase, Stage::Validate).unwrap();
        assert!(!workflow::state_path(root, phase).exists());
        assert_eq!(Gates::list_open(root).len(), 1, "fixture: one open gate");

        clean(root).expect("clean");

        for path in [
            Gates::gate_path(root, phase, Stage::Code),
            Gates::gate_path(root, phase, Stage::Validate),
            Gates::response_path(root, phase, Stage::Validate),
            Gates::ack_path(root, phase, Stage::Validate),
        ] {
            assert!(
                !path.exists(),
                "the sweep must remove orphan gate file {}",
                path.display()
            );
        }
        assert!(Gates::list_open(root).is_empty());
    }

    /// Control: an orphan gate whose phase lock is held is left alone, like a
    /// locked phase's state.
    #[test]
    fn clean_keeps_the_orphan_gate_files_of_a_locked_phase() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(10);
        Gates::write_gate(root, phase, Stage::Code, "held").unwrap();
        let guard = crate::lock::acquire(root, phase).expect("hold phase lock");

        let warnings = clean(root).expect("clean");

        assert!(
            Gates::gate_path(root, phase, Stage::Code).exists(),
            "an orphan gate under a held lock must be kept"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains(&format!("phase {phase}")) && w.contains("lock")),
            "the kept orphan gate must be reported with its lock as the reason: {warnings:?}"
        );
        drop(guard);
    }

    /// A state file that cannot be parsed is skipped by `list_states`, so its
    /// phase was dropped from the sweep without a word. It must be kept — a
    /// newer binary may have written it — but reported.
    #[test]
    fn clean_reports_a_phase_whose_state_cannot_be_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(11);
        let state_path = workflow::state_path(root, phase);
        std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        std::fs::write(&state_path, "{\"stage\":").unwrap();
        Gates::write_gate(root, phase, Stage::Code, "behind corrupt state").unwrap();

        let warnings = clean(root).expect("clean");

        assert!(state_path.exists(), "an unparsable state file must be kept");
        assert!(
            Gates::gate_path(root, phase, Stage::Code).exists(),
            "the gates of a phase with unparsable state must be kept with it"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains(&format!("kept phase {phase}")) && w.contains("parse")),
            "a phase kept for unparsable state must be reported: {warnings:?}"
        );
    }

    /// WR-04 (48-REVIEW.md): one phase's failed gate removal aborted the whole
    /// sweep with `?`, hiding phases already cleared and skipping the rest.
    /// The failing phase must keep its state (its gate is still there to
    /// explain) and be reported; the other phase must still be cleared.
    #[test]
    fn a_failed_gate_removal_does_not_abort_the_sweep() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let failing = PhaseId::new(61);
        let healthy = PhaseId::new(62);
        for phase in [failing, healthy] {
            workflow::save_state(&state_aged_phase(
                root,
                phase,
                STALE_THRESHOLD.as_secs() + 60,
                Some(DEAD_PID),
            ))
            .unwrap();
        }
        // A directory where a gate file belongs: `remove_file` fails EISDIR.
        std::fs::create_dir_all(Gates::gate_path(root, failing, Stage::Code)).unwrap();

        let report = clean_report(root).expect("one phase's failure must not abort the sweep");

        assert_eq!(report.cleared, vec![healthy]);
        assert!(
            workflow::state_path(root, failing).exists(),
            "a phase whose gates could not be removed must keep its state"
        );
        assert!(!workflow::state_path(root, healthy).exists());
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains(&format!("phase {failing}")) && w.contains("gate")),
            "the failed phase must be reported: {:?}",
            report.warnings
        );
    }

    /// R-4: the stale-state sweep must report an orphaned state temp it
    /// cannot remove, while retaining the record of the state it did clear.
    #[test]
    fn clean_report_flags_an_unremovable_state_temp_for_a_lock_free_stale_phase() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(64);
        workflow::save_state(&state_aged_phase(
            root,
            phase,
            STALE_THRESHOLD.as_secs() + 60,
            Some(DEAD_PID),
        ))
        .unwrap();
        let state_path = workflow::state_path(root, phase);
        let temp = workflow::devflow_dir(root).join(".state-64.json.1.0.tmp");
        std::fs::create_dir_all(&temp).unwrap();

        let report = clean_report(root).expect("clean lock-free stale phase");

        assert!(
            !state_path.exists(),
            "the stale state file must be removed before the temp-removal failure"
        );
        assert!(
            temp.is_dir(),
            "the state-temp directory is the negative control for this failure"
        );
        assert!(
            report.removal_failed,
            "the unremovable state temp must be reported as a failed removal"
        );
        assert_eq!(
            report.cleared,
            vec![phase],
            "the successfully removed state must remain reportable"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains(temp.file_name().unwrap().to_str().unwrap())),
            "the unremovable state temp must be named in a warning: {:?}",
            report.warnings
        );
    }

    /// R-3: the stale-lock sweep must classify an unusable coordination inode
    /// as a failed removal rather than a harmless kept-lock notice.
    #[test]
    fn clean_report_flags_a_stale_lock_with_unusable_coordination() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let stale_lock = crate::lock::lock_path(root, PhaseId::new(67));
        std::fs::create_dir_all(stale_lock.parent().unwrap()).unwrap();
        std::fs::write(&stale_lock, DEAD_PID.to_string()).unwrap();
        let coordination = stale_lock.with_file_name(format!(
            ".{}.coord",
            stale_lock.file_name().unwrap().to_string_lossy()
        ));
        // A directory where the coordination inode belongs: opening it fails
        // before the sweeper can remove the dead-holder lock.
        std::fs::create_dir_all(&coordination).unwrap();

        let report = clean_report(root).expect("clean");

        assert!(
            stale_lock.is_file(),
            "the stale lock must remain when its coordination cannot be opened"
        );
        assert!(
            coordination.is_dir(),
            "the coordination directory is the negative control for this failure"
        );
        assert!(
            report.removal_failed,
            "the unremovable stale lock must be reported as a failed removal"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("could not coordinate")
                    && warning.contains("lock-67")),
            "the stale-lock failure must be reported: {:?}",
            report.warnings
        );
    }

    /// A live-holder lock is kept intentionally, so its notice must not make
    /// the stale-lock sweep report a failed removal.
    #[test]
    fn clean_report_does_not_flag_a_live_stale_lock_notice_as_a_removal_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let live_lock = crate::lock::lock_path(root, PhaseId::new(68));
        std::fs::create_dir_all(live_lock.parent().unwrap()).unwrap();
        std::fs::write(&live_lock, std::process::id().to_string()).unwrap();

        let report = clean_report(root).expect("clean");

        assert!(live_lock.is_file(), "a live holder's lock must be kept");
        assert!(
            !report.removal_failed,
            "a live-holder notice must not be classified as a failed removal"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("still alive") && warning.contains("lock-68")),
            "the kept live lock must be reported: {:?}",
            report.warnings
        );
    }

    /// WR-03 (48-REVIEW.md): `found_anything` was computed before the legacy
    /// `state.json` migration, so clearing a phase held only in the legacy
    /// file reported "nothing to clean" after deleting its state.
    #[test]
    fn clean_phase_report_counts_legacy_state_it_removes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(7);
        let legacy = workflow::legacy_state_path(root);
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        let state = state_aged_phase(root, phase, 0, None);
        std::fs::write(&legacy, serde_json::to_string(&state).unwrap()).unwrap();

        let report = clean_phase_report(root, phase).expect("clean");

        assert!(!legacy.exists());
        assert!(!workflow::state_path(root, phase).exists());
        assert!(
            report.removed_anything,
            "removing a phase's legacy state is a cleanup, not \"nothing to clean\""
        );
    }

    /// Control for the test below: a cron record the clean does remove is a
    /// cleanup.
    #[test]
    fn clean_phase_report_counts_a_lone_cron_record() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(12);
        let record = crate::ship::build_single_agent_cron_instructions(root, phase, "");
        crate::ship::write_cron_instructions(root, &record).unwrap();

        let report = clean_phase_report(root, phase).expect("clean");

        assert!(!crate::ship::cron_instructions_path(root, phase).exists());
        assert!(report.removed_anything && !report.removal_failed);
    }

    /// WR-03: a removal that fails is not a cleanup. The only artifact here is
    /// a cron record that cannot be removed (a directory in its place).
    #[test]
    fn clean_phase_report_does_not_count_a_failed_removal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(8);
        std::fs::create_dir_all(crate::ship::cron_instructions_path(root, phase)).unwrap();

        let report = clean_phase_report(root, phase).expect("clean");

        assert!(
            !report.removed_anything,
            "a cron record that could not be removed must not count as cleaned"
        );
        assert!(
            report.removal_failed,
            "a failed removal must be flagged, not only warned about"
        );
        assert!(
            report.warnings.iter().any(|w| w.contains("cron")),
            "the failed removal must be reported: {:?}",
            report.warnings
        );
    }
}
