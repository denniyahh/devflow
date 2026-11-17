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
#[non_exhaustive]
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
    /// Gate cleanup failed.
    #[error("{0}")]
    Gate(#[from] crate::gates::GateError),
}

/// Clean up stale or abandoned workflow state.
///
/// 14-CR-01: only STALE phases are swept — a phase whose agent is still
/// running, whose state is simply fresh, or whose supervisor, pending supervisor,
/// gate waiter or foreground holder may be live is kept (with a warning naming the
/// repair), so cleaning one dead phase beside running ones can never orphan a
/// healthy sibling. Also removes gate files whose phase has no state file, and
/// the liveness files and hold temps crashes left. Returns warnings for anything kept or
/// that could not be removed.
pub fn clean(project_root: &Path) -> Result<Vec<String>, RecoverError> {
    clean_report(project_root).map(|report| report.warnings)
}

/// What [`clean_report`] did: the phases whose state it cleared, the
/// stateless phases whose orphaned gate files it removed, the
/// abandoned markers it removed, the set-aside answers it left, and warnings for
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
    /// Every set-aside gate answer found. None is removed (D-05): each stays until the
    /// operator removes it by hand.
    pub quarantined: Vec<std::path::PathBuf>,
    /// Every abandoned liveness file, waiter registration and orphan hold temp removed.
    pub markers_removed: Vec<std::path::PathBuf>,
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
        sweep_phase(project_root, state.phase, &mut report);
    }
    for phase in workflow::state_file_phases(project_root) {
        if !states.iter().any(|state| state.phase == phase) {
            report.warnings.push(unparsable_state_warning(phase));
        }
    }
    sweep_orphan_gates(project_root, &mut report);
    sweep_abandoned_markers(project_root, &mut report);
    report.quarantined = crate::gates::Gates::quarantined(project_root)
        .into_iter()
        .map(|answer| answer.path)
        .collect();
    Ok(report)
}

/// Remove what crashes leave behind (R-06): an unheld `supervisor-*.live` under
/// `.devflow/`, an unheld `*.waiter` in the gates directory, and in both an orphan hold
/// temp. There is no age bound. An `Abandoned` hold never reads `Live` again, and a hold's
/// final name is published only once its temp is locked, so none is visible unheld while
/// its creator lives. Removing a `Live` hold is never safe, so a probe that cannot answer
/// keeps the file.
///
/// The names match exactly, so `.holder-NN`, `.state-NN.lock`, `.gates-NN.lock` (never
/// removed, or two lockers could hold different inodes) and `*.quarantined.json` (D-05)
/// can never be swept.
fn sweep_abandoned_markers(project_root: &Path, report: &mut CleanReport) {
    let devflow = workflow::devflow_dir(project_root);
    let gates = crate::gates::Gates::dir(project_root);
    remove_abandoned_holds(
        &devflow,
        |name| name.starts_with("supervisor-") && name.ends_with(".live"),
        report,
    );
    remove_abandoned_holds(
        &gates,
        |name| name.ends_with(crate::gates::WAITER_SUFFIX),
        report,
    );
    for dir in [&devflow, &gates] {
        match crate::liveness::remove_orphan_hold_temps(dir) {
            Ok(removed) => report.markers_removed.extend(removed),
            Err(err) => report.fail(format!(
                "could not sweep the hold temps of {}: {err}",
                dir.display()
            )),
        }
    }
}

/// Remove each file in `dir` named as `is_hold` says whose lock nothing holds.
fn remove_abandoned_holds(dir: &Path, is_hold: impl Fn(&str) -> bool, report: &mut CleanReport) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => {
            return report.fail(format!("could not list {}: {err}", dir.display()));
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_str().is_some_and(&is_hold) {
            continue;
        }
        let path = entry.path();
        let removed = crate::liveness::probe(&path).and_then(|held| match held {
            crate::liveness::Held::Abandoned => crate::durable::remove_durably(&path),
            crate::liveness::Held::Live | crate::liveness::Held::Missing => Ok(false),
        });
        match removed {
            Ok(true) => report.markers_removed.push(path),
            Ok(false) => {}
            Err(err) => report.fail(format!(
                "kept {} — could not check or remove it: {err}",
                path.display()
            )),
        }
    }
}

/// Clear one listed phase if, on a fresh read under its write lock, it is stale and
/// nothing about it may be live. The listing only names the phase: a run that reserved
/// after it needs the same lock, so the decision sees that run (R-07). A lock still held
/// after [`workflow::WRITE_LOCK_REPAIR_WAIT`] keeps the phase and names its holder (R-03).
fn sweep_phase(project_root: &Path, phase: PhaseId, report: &mut CleanReport) {
    #[cfg(test)]
    recover_test_hook(RecoverPoint::SweepListed, phase);
    let cleared = workflow::clear_state_if(
        project_root,
        phase,
        Some(workflow::WRITE_LOCK_REPAIR_WAIT),
        |existing| Ok::<_, WorkflowError>(sweep_decision(project_root, phase, existing, report)),
    );
    let cleared = match cleared {
        Ok(cleared) => cleared,
        Err(err) => return report_a_lock_failure(&format!("phase {phase}"), err, report),
    };
    match cleared.result {
        Ok(()) if cleared.removed => report.cleared.push(phase),
        Ok(()) => {}
        Err(err) => {
            if cleared.removed {
                report.cleared.push(phase);
            }
            let partial = if cleared.removed {
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

/// The sweep's decision for one phase, on what its write lock's holder just read. Whether
/// to clear the state; a kept phase or a failed gate removal is in `report`.
fn sweep_decision(
    project_root: &Path,
    phase: PhaseId,
    existing: workflow::Existing,
    report: &mut CleanReport,
) -> bool {
    let fresh = match existing {
        workflow::Existing::Loaded(state) => state,
        workflow::Existing::Missing => return false,
        workflow::Existing::Unloadable(_) => {
            report.warnings.push(unparsable_state_warning(phase));
            return false;
        }
    };
    if agent_pid_for(&fresh).is_some_and(crate::agent::agent_running) {
        report.warnings.push(format!(
            "kept phase {phase} — its agent is still running (clear explicitly with --phase {phase})"
        ));
        return false;
    }
    if !is_stale_state(&fresh) {
        report.warnings.push(format!(
            "kept phase {phase} — state is not stale yet (clear explicitly with --phase {phase})"
        ));
        return false;
    }
    if let Some(reason) = phase_may_be_live(project_root, &fresh) {
        report
            .warnings
            .push(kept_as_live(&format!("phase {phase}"), phase, &reason));
        return false;
    }
    // Same order as `clean_phase_report`: a gate left behind would outlive the state
    // that explains it, and no command would answer it. So a failed gate removal keeps
    // the state.
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
        return false;
    }
    true
}

fn unparsable_state_warning(phase: PhaseId) -> String {
    format!(
        "kept phase {phase} — its state file cannot be parsed (clear explicitly with --phase {phase})"
    )
}

fn kept_as_live(what: &str, phase: PhaseId, reason: &str) -> String {
    format!("kept {what} — {reason}; run `devflow stop --phase {phase}` first")
}

/// Record why a sweep step never reached its decision. A held lock is a keep, naming the
/// holder (R-03); any other error taking it is a failure.
fn report_a_lock_failure(what: &str, err: WorkflowError, report: &mut CleanReport) {
    match err {
        WorkflowError::WriteLockTimeout { .. } => {
            report.warnings.push(format!("kept {what} — {err}"));
        }
        err => report.fail(format!(
            "kept {what} — could not take its state write lock: {err}"
        )),
    }
}

/// Remove gate files whose phase has no state file (48-REVIEW WR-02): left by
/// a sweep that predates gate cleanup, or by `abort` for another stage. They
/// show as open gates nothing will ever answer.
///
/// A phase with a state file is skipped at the listing: that read can only keep. The
/// removal itself is decided on a fresh read under the write lock, which holds off a
/// reservation (R-07), so it has no state to remove and always keeps.
fn sweep_orphan_gates(project_root: &Path, report: &mut CleanReport) {
    for phase in crate::gates::Gates::phases_on_disk(project_root) {
        if workflow::state_path(project_root, phase).exists() {
            continue;
        }
        #[cfg(test)]
        recover_test_hook(RecoverPoint::OrphanGatesListed, phase);
        let what = format!("the orphan gate files of phase {phase}");
        let swept = workflow::clear_state_if(
            project_root,
            phase,
            Some(workflow::WRITE_LOCK_REPAIR_WAIT),
            |existing| {
                if matches!(existing, workflow::Existing::Missing) {
                    remove_orphan_gate_files(project_root, phase, &what, report);
                }
                Ok::<_, WorkflowError>(false)
            },
        );
        if let Err(err) = swept {
            report_a_lock_failure(&what, err, report);
        }
    }
}

fn remove_orphan_gate_files(
    project_root: &Path,
    phase: PhaseId,
    what: &str,
    report: &mut CleanReport,
) {
    if let Some(reason) = phase_activity_may_be_live(project_root, phase) {
        report.warnings.push(kept_as_live(what, phase, &reason));
        return;
    }
    let (removed, result) = remove_gate_files(project_root, phase);
    if removed && result.is_ok() {
        report.orphan_gates_cleared.push(phase);
    }
    if let Err(err) = result {
        let partial = if removed {
            " after removing some of them"
        } else {
            ""
        };
        report.fail(format!("could not remove all of {what}{partial}: {err}"));
    }
}

/// Why `state`'s run may still be live, or `None` when nothing says so. A probe that
/// cannot finish counts as live (fail closed, Phase 50 T-50-40). The state records no
/// launch, so only the phase's own activity can keep it.
fn phase_may_be_live(project_root: &Path, state: &State) -> Option<String> {
    phase_activity_may_be_live(project_root, state.phase)
}

/// A held launch liveness file (`.devflow/supervisor-<id>.live`), which no state can
/// record, so it may be a phase's supervisor before it publishes (D-01).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnrecordedLiveLaunch {
    /// The launch id from the file's name.
    pub launch_id: String,
    /// The file.
    pub path: std::path::PathBuf,
    /// The holder's pid from the file's record, when it reads.
    pub holder_pid: Option<u32>,
}

impl UnrecordedLiveLaunch {
    /// One line naming the launch, its holder and how to inspect it.
    pub fn describe(&self) -> String {
        match self.holder_pid {
            Some(pid) => format!(
                "launch {}'s liveness lock {} is held by pid {pid}, so it may be this phase's \
                 supervisor before it publishes (inspect it with `ps -o pid,lstart,args -p \
                 {pid}`)",
                self.launch_id,
                self.path.display()
            ),
            None => format!(
                "launch {}'s liveness lock {} is held, so it may be this phase's supervisor \
                 before it publishes",
                self.launch_id,
                self.path.display()
            ),
        }
    }
}

/// Every [`UnrecordedLiveLaunch`] in `project_root` but `own_launch_id`, for a `phase`
/// whose own state is missing or does not load and so cannot name its launch.
///
/// No state records a launch, so every held one is counted, whoever it belongs to: a
/// state that does not load cannot clear it. A lock that cannot be probed counts as
/// held.
///
/// # Errors
/// `.devflow/` exists but cannot be listed.
pub fn unrecorded_live_launches(
    project_root: &Path,
    _phase: PhaseId,
    own_launch_id: Option<&str>,
) -> std::io::Result<Vec<UnrecordedLiveLaunch>> {
    let dir = project_root.join(".devflow");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut live = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(launch_id) = name
            .to_str()
            .and_then(|name| name.strip_prefix("supervisor-"))
            .and_then(|name| name.strip_suffix(".live"))
            .filter(|id| Some(*id) != own_launch_id)
        else {
            continue;
        };
        let path = entry.path();
        if matches!(
            crate::liveness::probe(&path),
            Ok(crate::liveness::Held::Abandoned | crate::liveness::Held::Missing)
        ) {
            continue;
        }
        let holder_pid = crate::liveness::live_holder(&path)
            .ok()
            .flatten()
            .map(|(pid, _)| pid);
        live.push(UnrecordedLiveLaunch {
            launch_id: launch_id.to_string(),
            path,
            holder_pid,
        });
    }
    live.sort_by(|a, b| a.launch_id.cmp(&b.launch_id));
    Ok(live)
}

/// Why a phase with no loadable state may still be live through a launch no state
/// records, or `None`.
fn unrecorded_launch_may_be_live(project_root: &Path, phase: PhaseId) -> Option<String> {
    match unrecorded_live_launches(project_root, phase, None) {
        Ok(live) => live.first().map(UnrecordedLiveLaunch::describe),
        Err(error) => Some(format!(
            "its launch liveness files could not be listed ({error})"
        )),
    }
}

/// The part of [`phase_may_be_live`] that needs no state: a registered gate waiter, or
/// D-17's foreground holder.
fn phase_activity_may_be_live(project_root: &Path, phase: PhaseId) -> Option<String> {
    let waiters = match crate::gates::Gates::waiter_files(project_root, phase) {
        Ok(waiters) => waiters,
        Err(error) => {
            return Some(format!(
                "its gate waiter registrations could not be listed ({error})"
            ));
        }
    };
    for waiter in waiters {
        let held = crate::liveness::probe(&waiter);
        if let Some(reason) = hold_may_be_live("registered gate waiter", &waiter, held) {
            return Some(reason);
        }
    }
    let holder = crate::liveness::phase_holder_path(project_root, phase);
    match crate::liveness::probe(&holder) {
        Ok(crate::liveness::Held::Live) => Some(match crate::liveness::live_holder(&holder) {
            Ok(Some((pid, _))) => format!("its foreground holder pid {pid} is live"),
            _ => format!("its foreground holder {} is held", holder.display()),
        }),
        held => hold_may_be_live("foreground holder", &holder, held),
    }
}

/// Why a liveness file's probe keeps a phase, or `None` for an abandoned or missing one.
fn hold_may_be_live(
    what: &str,
    path: &Path,
    held: std::io::Result<crate::liveness::Held>,
) -> Option<String> {
    let path_text = path.display();
    match held {
        Ok(crate::liveness::Held::Live) => Some(format!("its {what} holds {path_text}")),
        Ok(crate::liveness::Held::Abandoned | crate::liveness::Held::Missing) => None,
        Err(error) => Some(format!(
            "its {what}'s {path_text} could not be probed ({error})"
        )),
    }
}

/// Close every stage's gate for `phase`, retaining partial removal on error. An
/// unconsumed answer is set aside rather than deleted (D-05), and counts as removed:
/// it is gone from the live gate files.
///
/// It takes the gate lock once for the whole phase (R2-03). It runs inside a
/// [`workflow::clear_state_if`] decision, so that one wait is what a write-lock hold is
/// made of: see the test
/// `a_recover_decisions_worst_case_is_counted_on_its_real_paths`.
fn remove_gate_files(
    project_root: &Path,
    phase: PhaseId,
) -> (bool, Result<(), crate::gates::GateError>) {
    let (reports, result) =
        crate::gates::Gates::close_phase_with_partial_result(project_root, phase);
    let removed = reports
        .iter()
        .any(|(_, report)| report.removed || !report.quarantined.is_empty());
    (removed, result)
}

#[cfg(test)]
thread_local! {
    /// What [`recover_test_hook`] runs on this thread.
    static RECOVER_HOOK: std::cell::RefCell<Option<RecoverHook>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
type RecoverHook = Box<dyn FnMut(RecoverPoint, PhaseId)>;

/// The named points between a sweep's listing and its locked decision.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoverPoint {
    /// `clean_report` listed a phase's state and has not decided it.
    SweepListed,
    /// `clean_report` listed a phase's gate files and has not decided them.
    OrphanGatesListed,
}

/// Run the hook this thread installed, with its slot free so the hook may run anything.
#[cfg(test)]
fn recover_test_hook(point: RecoverPoint, phase: PhaseId) {
    let hook = RECOVER_HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(mut hook) = hook {
        hook(point, phase);
        RECOVER_HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
    }
}

/// Explicitly clean ONE phase, regardless of staleness — the operator's
/// escape hatch for a wedged-but-fresh run. Clears its state and gate
/// files; warns (but proceeds) when the recorded agent still looks alive.
/// Keeps everything, and says why among the warnings, when its supervisor,
/// pending supervisor, gate waiter or foreground holder may be live; returns
/// [`RecoverError::Workflow`] with [`WorkflowError::WriteLockTimeout`], having
/// removed nothing, when its write lock stays held. A removal that fails or a
/// phase that was kept is among the warnings; [`clean_phase_report`] flags it.
pub fn clean_phase(project_root: &Path, phase: PhaseId) -> Result<Vec<String>, RecoverError> {
    clean_phase_report(project_root, phase).map(|report| report.warnings)
}

/// What [`clean_phase_report`] did for one phase.
#[derive(Debug, Default)]
pub struct PhaseCleanReport {
    /// Whether this call removed any of the phase's state, gate files, or
    /// their orphaned write temps.
    pub removed_anything: bool,
    /// Whether a removal failed or the phase was kept as live. Each is also in
    /// `warnings`.
    pub removal_failed: bool,
    /// Removals that failed, phases kept as live and agents that looked alive,
    /// in human-readable form.
    pub warnings: Vec<String>,
}

/// [`clean_phase`], reporting what it removed and whether a removal failed,
/// so a caller claims neither a cleanup that did not happen nor "nothing to
/// clean" after deleting something (48-REVIEW WR-03). A failure stops the
/// clean where it is: gate files that could not go keep the state that
/// explains them.
///
/// The decision, the gate files and the state share one hold of the write lock, on a
/// fresh read (R-07), so a `start` that reserves meanwhile loses nothing.
pub fn clean_phase_report(
    project_root: &Path,
    phase: PhaseId,
) -> Result<PhaseCleanReport, RecoverError> {
    let mut report = PhaseCleanReport::default();
    let cleared = workflow::clear_state_if(
        project_root,
        phase,
        Some(workflow::WRITE_LOCK_REPAIR_WAIT),
        |existing| {
            Ok::<_, RecoverError>(explicit_decision(
                project_root,
                phase,
                &existing,
                &mut report,
            ))
        },
    )?;
    report.removed_anything |= cleared.removed;
    if let Err(err) = cleared.result {
        report.fail(format!("could not clear phase {phase}'s state: {err}"));
    }
    Ok(report)
}

/// The explicit clean's decision: whether to clear the state, having removed the gate
/// files if so. A phase with no state, or one that does not parse, still has its
/// leftovers removed, unless a waiter, a foreground holder or a launch no other phase's
/// state records may be live.
fn explicit_decision(
    project_root: &Path,
    phase: PhaseId,
    existing: &workflow::Existing,
    report: &mut PhaseCleanReport,
) -> bool {
    let live = match existing {
        workflow::Existing::Loaded(fresh) => phase_may_be_live(project_root, fresh),
        workflow::Existing::Missing | workflow::Existing::Unloadable(_) => {
            phase_activity_may_be_live(project_root, phase)
                .or_else(|| unrecorded_launch_may_be_live(project_root, phase))
        }
    };
    if let Some(reason) = live {
        report.fail(kept_as_live(&format!("phase {phase}"), phase, &reason));
        return false;
    }
    if let workflow::Existing::Loaded(fresh) = existing
        && agent_pid_for(fresh).is_some_and(crate::agent::agent_running)
    {
        report.warnings.push(format!(
            "phase {phase}'s agent appears to still be running — cleared anyway (explicit --phase)"
        ));
    }
    let (gates_removed, gate_result) = remove_gate_files(project_root, phase);
    report.removed_anything |= gates_removed;
    if let Err(err) = gate_result {
        report.fail(format!(
            "kept phase {phase}'s state — could not remove all of its gate files: {err}"
        ));
        return false;
    }
    true
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

        let mut paths = vec![workflow::state_path(root, phase)];
        for stage in [Stage::Code, Stage::Validate] {
            let gate_id = Gates::open_gate(
                root,
                phase,
                stage,
                crate::gates::GateKind::StageFailure,
                "wedged gate",
            )
            .unwrap()
            .gate_id;
            let answer = Gates::answer(
                root,
                phase,
                stage,
                &gate_id,
                &GateResponse {
                    approved: false,
                    abort: true,
                    note: Some("abort: test".into()),
                    responded_by: Some("test".into()),
                },
            )
            .unwrap();
            Gates::claim(root, phase, stage, &gate_id).unwrap();
            let gate = Gates::gate_path(root, phase, stage);
            let temp = Gates::dir(root).join(format!(
                ".{}.recovery.tmp",
                gate.file_name().unwrap().to_string_lossy()
            ));
            std::fs::write(&temp, "orphaned gate temp").unwrap();
            paths.extend([
                gate,
                answer,
                Gates::claim_path(root, phase, stage, &gate_id),
                temp,
            ]);
        }
        paths
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

    /// 14-CR-01: `recover --clean` must never delete a phase whose agent is
    /// still running — with several phases running, cleaning a stale phase must
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
        Gates::write_gate(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "left open",
        )
        .unwrap();

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
        Gates::write_gate(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "still fresh",
        )
        .unwrap();

        clean(root).expect("clean");

        assert!(workflow::state_path(root, phase).exists());
        assert!(
            Gates::gate_path(root, phase, Stage::Code).exists(),
            "a kept phase must keep its gate files"
        );
    }

    /// A waiter blocked on a gate has outlived its agent, so an old run looks stale by
    /// agent pid alone. The sweep keeps a phase whose gate has a registered waiter, with
    /// the gate files it is blocked on. `clean_clears_stale_phase_state` is the control.
    #[test]
    fn clean_keeps_a_stale_phase_whose_gate_has_a_registered_waiter() {
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
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "waiting",
        )
        .unwrap();
        let gate = opened.gate_id;
        let answer = Gates::answer(
            root,
            phase,
            Stage::Code,
            &gate,
            &GateResponse {
                approved: true,
                abort: false,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        Gates::claim(root, phase, Stage::Code, &gate).unwrap();
        let gate_paths = [
            Gates::gate_path(root, phase, Stage::Code),
            answer,
            Gates::claim_path(root, phase, Stage::Code, &gate),
        ];
        let gates_before: Vec<Vec<u8>> = gate_paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();

        let warnings = clean(root).expect("clean");

        assert_eq!(
            std::fs::read(&state_path).ok(),
            Some(state_before),
            "a stale phase with a registered waiter must keep its state byte-identical"
        );
        for (path, before) in gate_paths.iter().zip(gates_before) {
            assert_eq!(
                std::fs::read(path).ok(),
                Some(before),
                "a stale phase with a registered waiter must keep {} byte-identical",
                path.display()
            );
        }
        assert!(
            warnings
                .iter()
                .any(|w| w.contains(&format!("kept phase {phase}")) && w.contains("waiter")),
            "the kept phase must be reported with its waiter as the reason: {warnings:?}"
        );
        drop(registration);
    }

    /// Phase 50 D-20: neither clean touches a `state.json` left by an old binary,
    /// parsable or not, and neither counts it as something cleaned. The parsable one
    /// is stale, so a clean that read it as this phase's state would clear it.
    #[test]
    fn clean_leaves_a_legacy_state_json_alone() {
        let phase = PhaseId::new(7);
        let parsable = serde_json::to_vec(&state_aged_phase(
            Path::new("/representative-project"),
            phase,
            STALE_THRESHOLD.as_secs() + 60,
            None,
        ))
        .unwrap();
        for bytes in [b"{\"stage\":".to_vec(), parsable] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            let legacy = root.join(".devflow/state.json");
            std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
            std::fs::write(&legacy, &bytes).unwrap();

            let swept = clean_report(root).expect("sweep");
            assert_eq!(std::fs::read(&legacy).unwrap(), bytes);
            assert!(swept.cleared.is_empty());
            assert!(!swept.removal_failed, "{:?}", swept.warnings);

            let explicit = clean_phase_report(root, phase).expect("explicit clean");
            assert_eq!(std::fs::read(&legacy).unwrap(), bytes);
            assert!(!explicit.removed_anything);
            assert!(!workflow::state_path(root, phase).exists());
        }
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
        Gates::write_gate(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "leftover",
        )
        .unwrap();
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
        let early_gate = Gates::gate_path(root, phase, Stage::Plan);
        let later_gate = Gates::gate_path(root, phase, Stage::Code);
        Gates::write_gate(
            root,
            phase,
            Stage::Plan,
            crate::gates::GateKind::StageFailure,
            "leftover",
        )
        .unwrap();
        std::fs::create_dir_all(&later_gate).unwrap();

        let report = clean_phase_report(root, phase).expect("clean");

        assert!(
            !early_gate.exists(),
            "the early Plan gate must have been removed before the failure"
        );
        assert!(
            later_gate.is_dir(),
            "the Code gate directory is the negative control for this failure"
        );
        assert!(
            report.removal_failed,
            "the Code gate directory must be reported as a failed removal"
        );
        assert!(
            report.removed_anything,
            "the Plan gate was removed before the Code removal failed"
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

    #[test]
    fn clean_phase_removes_gate_files_when_nothing_is_live() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(48);
        let paths = write_phase_artifacts(root, phase);

        clean_phase(root, phase).expect("clean a phase with nothing live");

        for path in paths {
            assert!(
                !path.exists(),
                "cleaning a phase with nothing live must remove {}",
                path.display()
            );
        }
    }

    /// An older binary's `lock-NN` means nothing now, whatever pid it names, so it
    /// blocks no explicit clean. This one names a live pid with another start time.
    #[test]
    fn a_leftover_recycled_pid_lock_file_does_not_block_an_explicit_clean() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(50);
        let paths = write_phase_artifacts(root, phase);
        let observed =
            crate::agent::process_start_time(std::process::id()).expect("test pid start time");
        std::fs::write(
            root.join(format!(".devflow/lock-{}", phase.padded())),
            format!("{}\n{}", std::process::id(), observed + 1),
        )
        .unwrap();

        clean_phase(root, phase).expect("a leftover lock file must not block the clean");

        for path in paths {
            assert!(
                !path.exists(),
                "cleaning past a leftover lock file must remove {}",
                path.display()
            );
        }
    }

    /// WR-02 (48-REVIEW.md): gate files whose phase has no state file — left by
    /// a pre-`854bbce` sweep, or by `abort` for another stage — were never
    /// reached, because the sweep only cleaned gates inside its per-state loop.
    /// `clean_keeps_the_orphan_gate_files_of_a_registered_waiter` is the control.
    #[test]
    fn clean_removes_the_gate_files_of_a_phase_with_no_state() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(9);
        Gates::write_gate(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "orphaned",
        )
        .unwrap();
        let answered = Gates::open_gate(
            root,
            phase,
            Stage::Validate,
            crate::gates::GateKind::StageFailure,
            "orphaned",
        )
        .unwrap()
        .gate_id;
        let answer = Gates::answer(
            root,
            phase,
            Stage::Validate,
            &answered,
            &GateResponse {
                approved: true,
                abort: false,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        Gates::claim(root, phase, Stage::Validate, &answered).unwrap();
        assert!(!workflow::state_path(root, phase).exists());
        assert_eq!(Gates::list_open(root).len(), 1, "fixture: one open gate");

        clean(root).expect("clean");

        for path in [
            Gates::gate_path(root, phase, Stage::Code),
            Gates::gate_path(root, phase, Stage::Validate),
            answer,
            Gates::claim_path(root, phase, Stage::Validate, &answered),
        ] {
            assert!(
                !path.exists(),
                "the sweep must remove orphan gate file {}",
                path.display()
            );
        }
        assert!(Gates::list_open(root).is_empty());
    }

    /// Control: an orphan gate whose waiter is registered is left alone, like the gate
    /// of a phase with a state.
    #[test]
    fn clean_keeps_the_orphan_gate_files_of_a_registered_waiter() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(10);
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "held",
        )
        .unwrap();

        let warnings = clean(root).expect("clean");

        assert!(
            opened.path.exists(),
            "an orphan gate with a registered waiter must be kept"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains(&format!("phase {phase}")) && w.contains("waiter")),
            "the kept orphan gate must be reported with its waiter as the reason: {warnings:?}"
        );
        drop(registration);
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
        Gates::write_gate(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "behind corrupt state",
        )
        .unwrap();

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
    fn clean_report_flags_an_unremovable_state_temp_for_a_stale_phase() {
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

        let report = clean_report(root).expect("clean a stale phase");

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

    /// WR-03: a removal that fails is not a cleanup. The only artifact here is
    /// a state file that cannot be removed (a directory in its place).
    #[test]
    fn clean_phase_report_does_not_count_a_failed_removal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(8);
        std::fs::create_dir_all(workflow::state_path(root, phase)).unwrap();

        let report = clean_phase_report(root, phase).expect("clean");

        assert!(
            !report.removed_anything,
            "a state file that could not be removed must not count as cleaned"
        );
        assert!(
            report.removal_failed,
            "a failed removal must be flagged, not only warned about"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("could not clear phase 8's state")),
            "the failed removal must be reported: {:?}",
            report.warnings
        );
    }

    /// A caller must not report "nothing to clean" after moving an answer. The file
    /// has no gate beside it, so setting it aside is the only thing the clean did.
    #[test]
    fn clean_phase_report_counts_a_lone_answer_it_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(52);
        let answer = Gates::dir(root).join("52-code.response.json");
        std::fs::create_dir_all(answer.parent().unwrap()).unwrap();
        std::fs::write(&answer, "{}").unwrap();

        let report = clean_phase_report(root, phase).expect("clean");

        assert!(
            report.removed_anything,
            "an answer it set aside must count as removed from the live files"
        );
        assert!(!answer.exists());
        assert_eq!(Gates::quarantined(root).len(), 1);
    }

    /// D-05: an answer nobody consumed is set aside, never deleted, even when the
    /// phase it decided is being cleared. Its body stays readable under the
    /// quarantine name.
    #[test]
    fn recover_sets_an_unconsumed_answer_aside_when_it_clears_a_phase() {
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
        let gate = Gates::open_gate(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "left open",
        )
        .unwrap()
        .gate_id;
        let answer = Gates::answer(
            root,
            phase,
            Stage::Code,
            &gate,
            &GateResponse {
                approved: true,
                abort: false,
                note: Some("unclaimed".into()),
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        let body = std::fs::read(&answer).unwrap();

        clean(root).expect("clean");

        assert!(!workflow::state_path(root, phase).exists());
        assert!(!Gates::gate_path(root, phase, Stage::Code).exists());
        assert!(!answer.exists(), "no live answer is left behind");
        let set_aside = Gates::quarantined(root);
        assert_eq!(set_aside.len(), 1, "{set_aside:?}");
        assert_eq!(set_aside[0].label, gate.as_str());
        assert_eq!(std::fs::read(&set_aside[0].path).unwrap(), body);
    }

    /// Run `body` with `hook` installed for this thread.
    fn with_recover_hook<R>(
        hook: impl FnMut(RecoverPoint, PhaseId) + 'static,
        body: impl FnOnce() -> R,
    ) -> R {
        struct ClearOnDrop;
        impl Drop for ClearOnDrop {
            fn drop(&mut self) {
                RECOVER_HOOK.with(|slot| *slot.borrow_mut() = None);
            }
        }
        RECOVER_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
        let _clear = ClearOnDrop;
        body()
    }

    /// Save a stale state (old, its agent pid dead) after `edit` has changed it.
    fn save_stale(root: &Path, phase: PhaseId, edit: impl FnOnce(&mut State)) {
        let mut state =
            state_aged_phase(root, phase, STALE_THRESHOLD.as_secs() + 60, Some(DEAD_PID));
        edit(&mut state);
        workflow::save_state(&state).unwrap();
    }

    /// Sweep until `phase`'s state goes. A just-released flock can read held while
    /// another test's fork is between its fork and its exec, so a control that clears a
    /// phase right after a release polls, with a bound.
    fn sweep_until_cleared(root: &Path, phase: PhaseId) -> CleanReport {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let report = clean_report(root).expect("clean");
            if !workflow::state_path(root, phase).exists() {
                return report;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "phase {phase} was never cleared: {:?}",
                report.warnings
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A held gate registration alone keeps a phase, with its own reason.
    #[test]
    fn clean_keeps_a_phase_with_a_registered_waiter() {
        // A held gate registration alone.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2103);
        save_stale(root, phase, |_| {});
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            Stage::Plan,
            crate::gates::GateKind::StageFailure,
            "waiting",
        )
        .unwrap();
        let state_path = workflow::state_path(root, phase);
        let before = std::fs::read(&state_path).unwrap();

        let report = clean_report(root).expect("clean");

        assert_eq!(
            std::fs::read(&state_path).ok(),
            Some(before),
            "the phase's state must be byte-identical"
        );
        assert!(opened.path.exists(), "the waited-on gate file stays");
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("registered gate waiter")
                    && w.contains(&format!("devflow stop --phase {phase}"))),
            "{:?}",
            report.warnings
        );
        drop(registration);
        sweep_until_cleared(root, phase);
        assert!(!opened.path.exists());
    }

    /// D-17: a foreground `start`, `resume` or hand-run `advance` is
    /// what the long lock used to cover.
    #[test]
    fn clean_keeps_a_phase_whose_foreground_holder_is_live() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2104);
        save_stale(root, phase, |_| {});
        let hold = crate::liveness::claim_phase_holder(root, phase).unwrap();
        let state_path = workflow::state_path(root, phase);
        let before = std::fs::read(&state_path).unwrap();

        let report = clean_report(root).expect("clean");

        assert_eq!(
            std::fs::read(&state_path).ok(),
            Some(before),
            "the phase's state must be byte-identical"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("foreground holder")
                    && w.contains(&format!("pid {}", std::process::id()))
                    && w.contains(&format!("devflow stop --phase {phase}"))),
            "{:?}",
            report.warnings
        );

        // Control: with the hold gone the phase clears, and the holder file stays (a
        // removed one would let two claimers lock different inodes).
        drop(hold);
        sweep_until_cleared(root, phase);
        assert!(crate::liveness::phase_holder_path(root, phase).exists());
    }

    /// R2-03: what one write-lock hold can wait for, counted on both paths and then
    /// summed on the shipped constants. A decision probes no socket, so its one gate
    /// wait is all a hold is made of, and it fits [`workflow::WRITE_LOCK_REPAIR_WAIT`].
    #[test]
    fn a_recover_decisions_worst_case_is_counted_on_its_real_paths() {
        let phase_with_every_gate = |root: &Path, phase: PhaseId| {
            save_stale(root, phase, |_| {});
            for stage in crate::gates::STAGES {
                Gates::write_gate(
                    root,
                    phase,
                    stage,
                    crate::gates::GateKind::StageFailure,
                    "left open",
                )
                .unwrap();
            }
        };
        let phase = PhaseId::new(2105);

        let dir = tempfile::tempdir().unwrap();
        phase_with_every_gate(dir.path(), phase);
        let before = crate::gates::gate_lock_acquisitions();
        let report = clean_report(dir.path()).expect("clean");
        let after = crate::gates::gate_lock_acquisitions();
        assert_eq!(report.cleared, vec![phase], "{:?}", report.warnings);
        assert_eq!(after - before, 1, "sweep gate-lock acquisitions");

        let dir = tempfile::tempdir().unwrap();
        phase_with_every_gate(dir.path(), phase);
        let before = crate::gates::gate_lock_acquisitions();
        clean_phase_report(dir.path(), phase).expect("explicit clean");
        let after = crate::gates::gate_lock_acquisitions();
        assert!(!workflow::state_path(dir.path(), phase).exists());
        assert_eq!(after - before, 1, "explicit gate-lock acquisitions");

        let (gate, repair) = (
            crate::gates::GATE_LOCK_WAIT,
            workflow::WRITE_LOCK_REPAIR_WAIT,
        );
        assert!(gate < repair, "{gate:?} must fit {repair:?}");
        // Control: one gate wait per stage, the shape before R2-03, does not fit.
        let per_stage = gate * crate::gates::STAGES.len() as u32;
        assert!(per_stage >= repair, "{per_stage:?} against {repair:?}");
    }

    /// R-07: run A is stale when the sweep lists it, and run B has replaced it by the
    /// time the sweep decides. The decision must read B. Q is the control: the same
    /// sweep, nothing happening in the gap, removes its stale run and gate file.
    #[test]
    fn clean_keeps_a_run_that_reserved_after_the_sweep_listed_its_phase() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (replaced, control) = (PhaseId::new(2110), PhaseId::new(2111));
        for phase in [replaced, control] {
            save_stale(&root, phase, |_| {});
            Gates::write_gate(
                &root,
                phase,
                Stage::Code,
                crate::gates::GateKind::StageFailure,
                "run A",
            )
            .unwrap();
        }
        let gate = Gates::gate_path(&root, replaced, Stage::Code);
        // Kept alive to the end of the test: B's registration.
        let held = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let run_b = std::rc::Rc::new(std::cell::RefCell::new(None));
        let (hook_root, hook_held, hook_run_b) = (root.clone(), held.clone(), run_b.clone());
        let mut fired = false;

        let report = with_recover_hook(
            move |point, phase| {
                if point != RecoverPoint::SweepListed || phase != replaced || fired {
                    return;
                }
                fired = true;
                let root = hook_root.as_path();
                workflow::update_state_with(root, phase, |_| {
                    let run_b = state_aged_phase(root, phase, 0, None);
                    Ok::<_, WorkflowError>((Some(run_b), ()))
                })
                .unwrap();
                let (opened, registration) = Gates::open_registered(
                    root,
                    phase,
                    Stage::Code,
                    crate::gates::GateKind::StageFailure,
                    "run B",
                )
                .unwrap();
                *hook_run_b.borrow_mut() = Some((
                    std::fs::read(workflow::state_path(root, phase)).unwrap(),
                    std::fs::read(&opened.path).unwrap(),
                    Gates::waiter_path(root, phase, Stage::Code, &opened.gate_id),
                ));
                hook_held.borrow_mut().push(registration);
            },
            || clean_report(&root).expect("clean"),
        );

        let (state_b, gate_b, waiter) = run_b.borrow_mut().take().expect("the hook ran");
        assert_eq!(report.cleared, vec![control], "{:?}", report.warnings);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains(&format!("kept phase {replaced}"))),
            "{:?}",
            report.warnings
        );
        assert_eq!(
            std::fs::read(workflow::state_path(&root, replaced)).ok(),
            Some(state_b),
            "run B's state must be byte-identical to what it reserved"
        );
        assert_eq!(std::fs::read(&gate).ok(), Some(gate_b), "run B's gate file");
        assert!(waiter.exists(), "run B's registration stays");
        assert!(!workflow::state_path(&root, control).exists());
        assert!(!Gates::gate_path(&root, control, Stage::Code).exists());
    }

    /// R-07, the orphan path: P's gate files have no state when listed; a run starts
    /// before the decision. Q's orphan gate files, with nothing in the gap, go.
    #[test]
    fn clean_keeps_the_gate_files_of_a_run_that_started_after_the_orphan_listing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (started, control) = (PhaseId::new(2112), PhaseId::new(2113));
        let orphan = Gates::write_gate(
            &root,
            started,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "orphan",
        )
        .unwrap();
        let control_gate = Gates::write_gate(
            &root,
            control,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "orphan",
        )
        .unwrap();
        let held = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let run_b = std::rc::Rc::new(std::cell::RefCell::new(None));
        let (hook_root, hook_held, hook_run_b) = (root.clone(), held.clone(), run_b.clone());
        let mut fired = false;

        let report = with_recover_hook(
            move |point, phase| {
                if point != RecoverPoint::OrphanGatesListed || phase != started || fired {
                    return;
                }
                fired = true;
                let root = hook_root.as_path();
                workflow::update_state_with(root, phase, |_| {
                    let run_b = state_aged_phase(root, phase, 0, None);
                    Ok::<_, WorkflowError>((Some(run_b), ()))
                })
                .unwrap();
                let (opened, registration) = Gates::open_registered(
                    root,
                    phase,
                    Stage::Plan,
                    crate::gates::GateKind::StageFailure,
                    "run B",
                )
                .unwrap();
                *hook_run_b.borrow_mut() = Some((
                    std::fs::read(workflow::state_path(root, phase)).unwrap(),
                    opened.path,
                ));
                hook_held.borrow_mut().push(registration);
            },
            || clean_report(&root).expect("clean"),
        );

        let (state_b, gate_b) = run_b.borrow_mut().take().expect("the hook ran");
        assert_eq!(
            std::fs::read(workflow::state_path(&root, started)).ok(),
            Some(state_b),
            "run B's state is byte-identical"
        );
        assert!(gate_b.exists(), "run B's gate file stays");
        assert!(
            orphan.exists(),
            "the phase's earlier gate file stays with it"
        );
        assert_eq!(report.orphan_gates_cleared, vec![control]);
        assert!(!control_gate.exists());
    }

    /// Bound on a test's holder of the write lock, so that a clean which fails to give
    /// up fails its assertion instead of hanging.
    const WRITE_LOCK_HOLD_BOUND: Duration = Duration::from_secs(15);

    #[test]
    fn explicit_clean_refuses_a_phase_with_a_registered_waiter() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2122);
        workflow::save_state(&state_aged_phase(root, phase, 0, None)).unwrap();
        let (opened, registration) = Gates::open_registered(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "waiting",
        )
        .unwrap();
        let state_path = workflow::state_path(root, phase);
        let (state_before, gate_before) = (
            std::fs::read(&state_path).unwrap(),
            std::fs::read(&opened.path).unwrap(),
        );

        let report = clean_phase_report(root, phase).expect("a keep is reported, not an error");

        assert_eq!(std::fs::read(&state_path).ok(), Some(state_before));
        assert_eq!(std::fs::read(&opened.path).ok(), Some(gate_before));
        assert!(report.removal_failed, "a kept phase cannot read as cleaned");
        assert!(!report.removed_anything);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("registered gate waiter")
                    && w.contains(&format!("devflow stop --phase {phase}"))),
            "{:?}",
            report.warnings
        );

        // Control: with the registration released the same call clears it.
        drop(registration);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let report = clean_phase_report(root, phase).expect("explicit clean");
            if !state_path.exists() {
                assert!(report.removed_anything, "{:?}", report.warnings);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{:?}",
                report.warnings
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!opened.path.exists());
    }

    /// With no state to read, a state that does not parse, or both, a live waiter or
    /// holder still keeps the files: the lock this replaced refused whatever the state.
    #[test]
    fn explicit_clean_refuses_a_live_waiter_or_holder_without_a_loadable_state() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let unparsable = PhaseId::new(2124);
        let state_path = workflow::state_path(root, unparsable);
        std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        std::fs::write(&state_path, "{\"stage\":").unwrap();
        let hold = crate::liveness::claim_phase_holder(root, unparsable).unwrap();

        let report = clean_phase_report(root, unparsable).expect("explicit clean");

        assert_eq!(
            std::fs::read_to_string(&state_path).ok().as_deref(),
            Some("{\"stage\":"),
            "a live holder keeps an unparsable state"
        );
        assert!(report.removal_failed, "{:?}", report.warnings);
        drop(hold);

        let missing = PhaseId::new(2125);
        let (opened, registration) = Gates::open_registered(
            root,
            missing,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "waiting",
        )
        .unwrap();

        let report = clean_phase_report(root, missing).expect("explicit clean");

        assert!(opened.path.exists(), "a waited-on gate file stays");
        assert!(report.removal_failed, "{:?}", report.warnings);
        drop(registration);
    }

    /// A supervisor that has not published shows only through its held liveness file, and
    /// a state that does not load names no launch: an explicit clean must keep that state,
    /// or a later `start` over the missing state launches a second supervisor beside it.
    /// Control: the same lock, released, keeps nothing.
    #[test]
    fn explicit_clean_keeps_an_unloadable_state_beside_an_unrecorded_live_launch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2126);
        let state_path = workflow::state_path(root, phase);
        std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
        let write_unloadable = || std::fs::write(&state_path, "{\"stage\":").unwrap();
        write_unloadable();
        let launch_id = crate::liveness::new_launch_id();
        let lock = root
            .join(".devflow")
            .join(format!("supervisor-{launch_id}.live"));
        let hold = crate::liveness::Hold::take(&lock).unwrap();

        let report = clean_phase_report(root, phase).expect("a keep is reported, not an error");

        assert_eq!(
            std::fs::read_to_string(&state_path).ok().as_deref(),
            Some("{\"stage\":"),
            "an unrecorded live launch keeps the unloadable state"
        );
        assert!(report.removal_failed, "{:?}", report.warnings);
        let pid = std::process::id().to_string();
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains(&launch_id) && w.contains(&pid)),
            "the keep names the launch and its holder pid: {:?}",
            report.warnings
        );

        write_unloadable();
        drop(hold);
        clean_phase_report(root, phase).expect("explicit clean");
        assert!(
            !state_path.exists(),
            "control: a released lock keeps nothing"
        );
    }

    /// R-03: a repair verb never hangs on a holder of the write lock, and says who it is.
    #[test]
    fn explicit_clean_gives_up_on_a_held_write_lock_and_names_its_holder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let phase = PhaseId::new(2126);
        save_stale(root, phase, |_| {});
        let gate = Gates::write_gate(
            root,
            phase,
            Stage::Code,
            crate::gates::GateKind::StageFailure,
            "left open",
        )
        .unwrap();
        let state_path = workflow::state_path(root, phase);
        let (state_before, gate_before) = (
            std::fs::read(&state_path).unwrap(),
            std::fs::read(&gate).unwrap(),
        );
        let (entered, holder_is_inside) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel::<()>();

        std::thread::scope(|scope| {
            scope.spawn(move || {
                workflow::update_state(root, phase, |_| {
                    entered.send(()).unwrap();
                    let _ = released.recv_timeout(WRITE_LOCK_HOLD_BOUND);
                    Ok::<(), WorkflowError>(())
                })
                .unwrap();
            });
            holder_is_inside.recv().unwrap();

            let started = std::time::Instant::now();
            let result = clean_phase_report(root, phase);
            let took = started.elapsed();
            let state_then = std::fs::read(&state_path).ok();
            let gate_then = std::fs::read(&gate).ok();
            let _ = release.send(());

            let error = result.expect_err("a held write lock must end the clean with an error");
            assert!(
                matches!(
                    &error,
                    RecoverError::Workflow(WorkflowError::WriteLockTimeout {
                        holder: Some(pid),
                        ..
                    }) if *pid == std::process::id()
                ),
                "{error:?}"
            );
            assert!(
                error
                    .to_string()
                    .contains(&format!("pid {}", std::process::id())),
                "{error}"
            );
            assert!(
                took >= workflow::WRITE_LOCK_REPAIR_WAIT,
                "gave up after {took:?}"
            );
            assert!(
                took < workflow::WRITE_LOCK_REPAIR_WAIT + Duration::from_secs(2),
                "gave up after {took:?}"
            );
            assert_eq!(state_then, Some(state_before));
            assert_eq!(gate_then, Some(gate_before));
        });

        // Control: released, the same call clears the phase.
        let report = clean_phase_report(root, phase).expect("explicit clean");
        assert!(report.removed_anything, "{:?}", report.warnings);
        assert!(!state_path.exists());
        assert!(!gate.exists());
    }

    /// pi S3: `recover` keeps every set-aside answer (D-05) and lists it, so an operator
    /// learns it persists until removed by hand.
    #[test]
    fn clean_reports_quarantined_answers_and_keeps_them() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(clean_report(root).expect("clean").quarantined.is_empty());

        let (cleared, kept) = (PhaseId::new(2131), PhaseId::new(2132));
        save_stale(root, cleared, |_| {});
        workflow::save_state(&state_aged_phase(root, kept, 60, None)).unwrap();
        for phase in [cleared, kept] {
            let gate = Gates::open_gate(
                root,
                phase,
                Stage::Code,
                crate::gates::GateKind::StageFailure,
                "ctx",
            )
            .unwrap()
            .gate_id;
            Gates::answer(
                root,
                phase,
                Stage::Code,
                &gate,
                &GateResponse {
                    approved: true,
                    abort: false,
                    note: None,
                    responded_by: Some("test".into()),
                },
            )
            .unwrap();
            // Closing the gate sets its unconsumed answer aside.
            Gates::close(root, phase, Stage::Code).unwrap();
        }
        let set_aside = Gates::quarantined(root);
        assert_eq!(set_aside.len(), 2, "fixture: {set_aside:?}");
        let paths: Vec<_> = set_aside.into_iter().map(|answer| answer.path).collect();
        let before: Vec<_> = paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();

        let report = clean_report(root).expect("clean");

        assert_eq!(report.cleared, vec![cleared], "{:?}", report.warnings);
        assert_eq!(report.quarantined, paths);
        let after: Vec<_> = paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();
        assert_eq!(after, before);
    }

    /// Sweep until `gone` has gone, gathering what each sweep says it removed. A hold
    /// just dropped can read held while another test's fork is before its exec.
    fn sweep_until_gone(root: &Path, gone: &Path) -> Vec<std::path::PathBuf> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut removed = Vec::new();
        loop {
            let report = clean_report(root).expect("clean");
            removed.extend(report.markers_removed);
            if !gone.exists() {
                return removed;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{} was never removed: {:?}",
                gone.display(),
                report.warnings
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// R-06: an abandoned hold never reads live again and a hold's final name is never
    /// visible unheld while its creator lives, so the sweep needs no age bound, even run
    /// inside a take at each of its steps. The three lock files are never markers.
    #[test]
    fn clean_removes_unheld_liveness_files() {
        use crate::liveness::{Held, Hold, TakeStep, probe, take_with_hook};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let devflow = workflow::devflow_dir(root);
        std::fs::create_dir_all(&devflow).unwrap();
        let lock_files = [
            devflow.join(".holder-07"),
            devflow.join(".state-07.lock"),
            devflow.join(".gates-07.lock"),
        ];
        for path in &lock_files {
            std::fs::write(path, path.to_string_lossy().as_bytes()).unwrap();
        }
        let locks_are_untouched = || {
            for path in &lock_files {
                assert_eq!(
                    std::fs::read(path).ok().as_deref(),
                    Some(path.to_string_lossy().as_bytes()),
                    "{} must be byte-identical",
                    path.display()
                );
            }
        };
        let abandoned = devflow.join("supervisor-aaaa0000aaaa0001.live");
        drop(Hold::take(&abandoned).unwrap());
        let held_path = devflow.join("supervisor-aaaa0000aaaa0002.live");
        let held = Hold::take(&held_path).unwrap();

        let removed = sweep_until_gone(root, &abandoned);

        assert_eq!(removed, vec![abandoned.clone()], "whatever its age");
        assert!(held_path.exists(), "a held file must be kept");
        locks_are_untouched();

        let taking = devflow.join("supervisor-aaaa0000aaaa0003.live");
        let mut steps = Vec::new();
        let taken = take_with_hook(&taking, &mut |step, temp| {
            clean_report(root).expect("clean inside a take");
            assert!(
                temp.exists(),
                "{step:?}: a live creator's temp must be kept"
            );
            assert_eq!(
                taking.exists(),
                matches!(step, TakeStep::Published),
                "{step:?}: the final name appears only once published"
            );
            locks_are_untouched();
            steps.push(step);
        })
        .expect("a sweep in its gaps must not break the take");
        assert_eq!(steps.len(), 3, "{steps:?}");
        assert_eq!(probe(&taking).unwrap(), Held::Live);
        drop((held, taken));
    }

    /// R-06, pi 5: nothing else sweeps an abandoned `.waiter`, or a hold temp a crash
    /// left. The live registration and a live creator's temp stay. The phase has no
    /// state, so the orphan-gate pass keeps it for the live registration: this removal is
    /// the marker sweep's.
    #[test]
    fn clean_removes_abandoned_waiters_and_orphan_hold_temps_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let live_gate = crate::gates::GateId::new();
        let live = Gates::register_waiter(root, phase, stage, &live_gate).unwrap();
        let dead_gate = crate::gates::GateId::new();
        drop(Gates::register_waiter(root, phase, stage, &dead_gate).unwrap());
        let abandoned = Gates::waiter_path(root, phase, stage, &dead_gate);
        let mut exited = std::process::Command::new("true").spawn().unwrap();
        let exited_pid = exited.id();
        exited.wait().unwrap();
        let waiter_temp = Gates::dir(root).join(format!(
            ".07-code.{dead_gate}.waiter.{exited_pid}.0{}",
            crate::liveness::HOLD_TEMP_SUFFIX
        ));
        let live_temp = Gates::dir(root).join(format!(
            ".07-code.{dead_gate}.waiter.{}.0{}",
            std::process::id(),
            crate::liveness::HOLD_TEMP_SUFFIX
        ));
        let liveness_temp = workflow::devflow_dir(root).join(format!(
            ".supervisor-x.live.{exited_pid}.0{}",
            crate::liveness::HOLD_TEMP_SUFFIX
        ));
        for temp in [&waiter_temp, &live_temp, &liveness_temp] {
            std::fs::write(temp, "").unwrap();
        }

        let mut removed = sweep_until_gone(root, &abandoned);

        assert_eq!(
            Gates::waiter_status(root, phase, stage, &live_gate).unwrap(),
            crate::gates::WaiterStatus::Live,
            "the live registration must be kept"
        );
        assert!(live_temp.exists(), "a live creator's temp must be kept");
        removed.sort();
        let mut expected = vec![abandoned, waiter_temp, liveness_temp];
        expected.sort();
        assert_eq!(removed, expected);
        drop(live);
    }
}
