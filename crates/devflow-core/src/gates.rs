//! Gate file protocol — the handoff between DevFlow and a human (via Hermes).
//!
//! A *gate* is a pause point where DevFlow writes a request to
//! `.devflow/gates/` and waits for a human (or the Hermes cron poller) to drop
//! a response file. Phase and stage are a gate's location; its gate id, fresh for
//! every gate opened, is its identity. The protocol is:
//!
//! - `NN-{stage}.json` — the gate request DevFlow writes (a [`GateFile`], which
//!   names the gate id).
//! - `NN-{stage}.{gate_id}.response.json` — the answer to that one gate (a
//!   [`GateAnswer`], whose body names the same id).
//! - `NN-{stage}.{gate_id}.ack.json` — the consumption claim (a [`GateAck`]); the
//!   first process to publish it consumes the answer.
//!
//! - `NN-{stage}.{gate_id}.waiter` — the waiter's registration: a file whose held
//!   lock says something still waits on that gate ([`Gates::register_waiter`]).
//!
//! The unkeyed `NN-{stage}.response.json` and `NN-{stage}.ack.json` of
//! [`Gates::respond`], [`Gates::poll_response`] and [`Gates::ack`] stay until the
//! CLI has moved off them.
//!
//! Writes are atomic and fsynced (`crate::durable`) so a reader never sees a
//! partial file and a power loss cannot lose a published one. Polling uses exponential backoff so a long human wait costs little, and never moves a file.

use crate::liveness;
use crate::phase_id::PhaseId;
use crate::stage::Stage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// The gate request DevFlow writes when it pauses for a human decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateFile {
    /// Phase the gate belongs to.
    pub phase: PhaseId,
    /// Stage that fired the gate.
    pub stage: Stage,
    /// Human-readable context explaining what is being asked.
    pub context: String,
    /// Unix timestamp (seconds) when the gate was written.
    pub timestamp: String,
    /// The gate's identity. Empty only in a gate file written before gate ids
    /// existed, which no writer can answer through [`Gates::answer`].
    #[serde(default)]
    pub gate_id: String,
}

/// The human's response to a gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateResponse {
    /// Whether the gated work is approved to advance.
    pub approved: bool,
    /// Optional free-text note (e.g. what to fix on a rejection).
    #[serde(default)]
    pub note: Option<String>,
    /// Who responded (human name, or "hermes").
    #[serde(default)]
    pub responded_by: Option<String>,
}

/// An answer as stored on disk: a [`GateResponse`] bound to the gate it decides.
/// `gate_id` has no default, so a body without one does not parse as an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateAnswer {
    /// The gate this answer decides.
    pub gate_id: String,
    /// The decision.
    #[serde(flatten)]
    pub response: GateResponse,
}

/// DevFlow's receipt that it has read a [`GateResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateAck {
    /// Always `true` — presence of the file is the signal.
    pub received: bool,
}

/// What DevFlow should do after reading a [`GateResponse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateAction {
    /// Approved — advance to the next stage.
    Advance,
    /// Rejected with fixable feedback — loop back to the given stage.
    LoopBack(Stage),
    /// Rejected and aborted — stop the workflow with a reason.
    Abort(String),
}

impl GateAction {
    /// Decide the action from a response: approval advances, a rejection loops
    /// back to Code unless the note asks to abort.
    pub fn from_response(response: &GateResponse) -> GateAction {
        if response.approved {
            return GateAction::Advance;
        }
        match response.note.as_deref() {
            Some(note) if note.to_ascii_lowercase().contains("abort") => {
                GateAction::Abort(note.to_string())
            }
            _ => GateAction::LoopBack(Stage::Code),
        }
    }
}

/// Errors produced by the gate protocol.
///
/// Whether a file was published matters to a caller deciding what to do next.
/// Nothing was published on [`GateError::GateLockTimeout`],
/// [`GateError::GateIdMismatch`], [`GateError::NoOpenGate`] and
/// [`GateError::AlreadyResponded`]. A file was on
/// [`GateError::GateClosedAfterPublish`] (it was then set aside) and on
/// [`GateError::PublishedUnconfirmed`] (it was left in place). [`GateError::Io`]
/// and [`GateError::Json`] say neither: from `answer` they can come from either
/// side of the publish.
#[derive(Debug, thiserror::Error)]
pub enum GateError {
    /// Filesystem operation failed.
    #[error("gate I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse or serialization failed.
    #[error("gate JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    /// Responding to a gate that was never fired (or already resolved).
    #[error("no open gate for phase {phase} stage {stage} — see `devflow gate list`")]
    NoOpenGate { phase: PhaseId, stage: Stage },
    /// Responding to a gate that already has a response on disk.
    #[error("gate for phase {phase} stage {stage} already has a response awaiting pickup")]
    AlreadyResponded { phase: PhaseId, stage: Stage },
    /// A gate id that does not have a gate id's format.
    #[error("`{0}` is not a gate id: expected 16 lowercase hex digits")]
    InvalidGateId(String),
    /// The caller observed a gate that is no longer the one open at this stage.
    #[error(
        "phase {phase} stage {stage} is at gate {current}, not gate {observed} — see `devflow gate list`"
    )]
    GateIdMismatch {
        phase: PhaseId,
        stage: Stage,
        observed: String,
        current: String,
    },
    /// The answer was published for a gate that closed while it was being written.
    /// It cannot answer the next gate. It was set aside, unless that gate's waiter
    /// consumed it before the re-check ran.
    #[error(
        "the answer for gate {observed} at phase {phase} stage {stage} was published after that gate closed, and it was set aside unless its waiter had already consumed it"
    )]
    GateClosedAfterPublish {
        phase: PhaseId,
        stage: Stage,
        observed: String,
    },
    /// The gate lock stayed held for the whole bounded wait. Returned only before
    /// anything is published.
    #[error(
        "phase {phase}'s gate lock `.devflow/.gates-{padded}.lock` is still held after {waited:?}; nothing was changed", padded = phase.padded()
    )]
    GateLockTimeout { phase: PhaseId, waited: Duration },
    /// The answer was published, but the re-check after it could not take the gate
    /// lock, so whether its gate was still open is not known. The file stays where
    /// it is: `GateLockTimeout`'s "nothing was changed" would be false here.
    #[error(
        "the answer for gate {observed} at phase {phase}'s {stage} gate was published and left in place, but the gate lock `.devflow/.gates-{padded}.lock` was still held after {waited:?}, so whether that gate was still open could not be confirmed; its waiter acts on it if the gate is still open — run `devflow gate list`", padded = phase.padded()
    )]
    PublishedUnconfirmed {
        phase: PhaseId,
        stage: Stage,
        observed: String,
        waited: Duration,
    },
}

/// How long a gate operation waits for the phase's gate lock before giving up.
/// It sits below [`crate::workflow::WRITE_LOCK_REPAIR_WAIT`], so a fenced side
/// effect stuck behind a stopped gate-lock holder releases the state write lock
/// before a repair verb's bounded wait gives up.
pub const GATE_LOCK_WAIT: Duration = Duration::from_secs(5);

/// A gate's identity: 16 lowercase hex digits, fresh for every gate opened and
/// never reused. Holding one means the id was checked, so it is safe in a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateId(String);

impl GateId {
    /// A fresh gate id.
    #[must_use]
    pub fn new() -> Self {
        Self(crate::supervisor::new_launch_id())
    }

    /// Check an id read from elsewhere; anything [`is_gate_id`] refuses is
    /// [`GateError::InvalidGateId`].
    pub fn parse(id: &str) -> Result<GateId, GateError> {
        if is_gate_id(id) {
            Ok(GateId(id.to_string()))
        } else {
            Err(GateError::InvalidGateId(id.to_string()))
        }
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for GateId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for GateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `id` has a gate id's format. No path is built from an id that fails
/// this: the id in an answer body is untrusted input.
#[must_use]
pub fn is_gate_id(id: &str) -> bool {
    crate::supervisor::is_launch_id(id)
}

/// A gate [`Gates::open_gate`] opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedGate {
    /// The new gate's identity.
    pub gate_id: GateId,
    /// The gate file.
    pub path: PathBuf,
}

/// What [`Gates::read_answer`] found in one gate's answer slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerRead {
    /// Nothing is there.
    None,
    /// An answer that binds to this gate.
    Bound(GateAnswer),
    /// A file that does not bind to this gate, with the reason.
    Unbound(String),
}

/// How [`Gates::claim`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// This call consumed the answer.
    Claimed,
    /// Another claim got there first; this caller must not act on the answer.
    AlreadyConsumed,
}

/// A claimed answer whose files a close removed: the consumer claimed it, and the
/// caller knows from the run's state whether the decision was ever recorded (R-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedAnswer {
    /// The gate the claim was for.
    pub gate_id: String,
    /// The answer as read, or `None` when it was already gone or did not bind.
    pub answer: Option<GateAnswer>,
}

/// What [`Gates::close`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloseReport {
    /// Whether this call removed any file (not counting what it set aside).
    pub removed: bool,
    /// Where each answer it set aside now lives.
    pub quarantined: Vec<PathBuf>,
    /// Every claimed answer whose files it removed.
    pub claimed: Vec<ClaimedAnswer>,
}

/// An answer that was set aside, as [`Gates::quarantined`] lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedAnswer {
    /// The phase of the gate it was answering.
    pub phase: PhaseId,
    /// The stage of that gate.
    pub stage: Stage,
    /// The gate id it named, or `noid`.
    pub label: String,
    /// Where it now lives.
    pub path: PathBuf,
}

/// An open gate: a request the workflow wrote that has no response yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGate {
    /// Phase the gate belongs to.
    pub phase: PhaseId,
    /// Stage that fired the gate.
    pub stage: Stage,
    /// Human-readable context from the request.
    pub context: String,
    /// Unix timestamp (seconds) when the gate was written.
    pub timestamp: String,
    /// The gate's identity, empty for a gate file from before gate ids.
    pub gate_id: String,
}

/// A waiter's registration: a held lock on `NN-{stage}.{gate_id}.waiter` (D-04).
///
/// The lock is the registration. It dies with the process, so a crashed waiter's
/// file reads [`WaiterStatus::Absent`] although it is still there. Dropping this
/// without [`finish`](Self::finish) leaves that unheld file for the stage's next
/// [`Gates::close`] to remove.
#[derive(Debug)]
pub struct WaiterRegistration {
    hold: liveness::Hold,
}

impl WaiterRegistration {
    /// End the registration: remove the waiter file, then release the lock. Only
    /// the owner removes a live registration (R-06). A failed removal is logged and
    /// leaves an unheld file, which the stage's next close removes.
    pub fn finish(self) {
        let Self { hold } = self;
        if let Err(error) = crate::durable::remove_durably(hold.path()) {
            warn!(
                "could not remove the waiter registration {}: {error}",
                hold.path().display()
            );
        }
        drop(hold);
    }
}

/// Whether a gate has a waiter. Only the held registration lock counts, never a
/// pid, a socket or a file's existence (D-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaiterStatus {
    /// A process holds the gate's registration.
    Live,
    /// Nothing holds it: no registration, or its waiter finished or died.
    Absent,
}

/// How [`Gates::await_answer`] ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AwaitOutcome {
    /// The gate's own answer, bound to its id.
    Answered(GateAnswer),
    /// A file in the gate's slot that does not bind to it, left where it is. The
    /// caller moves it with [`Gates::set_aside_unbound`] and waits again with the
    /// time left.
    Unbound {
        /// Why it does not bind.
        reason: String,
    },
    /// The wait ran out with nothing in the slot.
    TimedOut,
    /// The caller's `cancel` answered `true`.
    Cancelled,
}

/// The gate-file protocol, scoped to a project's `.devflow/gates/` directory.
pub struct Gates;

impl Gates {
    /// The `.devflow/gates/` directory for a project.
    pub fn dir(project_root: &Path) -> PathBuf {
        project_root.join(".devflow").join("gates")
    }

    /// Path to the gate request file for a phase + stage.
    pub fn gate_path(project_root: &Path, phase: PhaseId, stage: Stage) -> PathBuf {
        Self::dir(project_root).join(format!("{padded}-{stage}.json", padded = phase.padded()))
    }

    /// Path to the response file for a phase + stage.
    pub fn response_path(project_root: &Path, phase: PhaseId, stage: Stage) -> PathBuf {
        Self::dir(project_root).join(format!(
            "{padded}-{stage}.response.json",
            padded = phase.padded()
        ))
    }

    /// Path to the ack file for a phase + stage.
    pub fn ack_path(project_root: &Path, phase: PhaseId, stage: Stage) -> PathBuf {
        Self::dir(project_root).join(format!(
            "{padded}-{stage}.ack.json",
            padded = phase.padded()
        ))
    }

    /// Path to the answer file for one gate.
    pub fn answer_path(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
    ) -> PathBuf {
        Self::dir(project_root).join(format!(
            "{padded}-{stage}.{gate}.response.json",
            padded = phase.padded()
        ))
    }

    /// Path to the consumption claim for one gate.
    pub fn claim_path(project_root: &Path, phase: PhaseId, stage: Stage, gate: &GateId) -> PathBuf {
        Self::dir(project_root).join(format!(
            "{padded}-{stage}.{gate}.ack.json",
            padded = phase.padded()
        ))
    }

    /// Path to the waiter registration for one gate.
    pub fn waiter_path(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
    ) -> PathBuf {
        Self::dir(project_root).join(format!(
            "{padded}-{stage}.{gate}{WAITER_SUFFIX}",
            padded = phase.padded()
        ))
    }

    /// Path to a stage's pre-answer, which exists only in test builds. See
    /// [`crate::test_support::preanswer_gate`].
    #[cfg(any(test, feature = "test-support"))]
    pub fn preanswer_path(project_root: &Path, phase: PhaseId, stage: Stage) -> PathBuf {
        Self::dir(project_root).join(format!(
            "{padded}-{stage}.preanswer.json",
            padded = phase.padded()
        ))
    }

    /// Open a gate under a fresh gate id, replacing any earlier gate file for the
    /// stage. Whatever the earlier gate left is disposed of first, in one gate-lock
    /// critical section with the write: an unclaimed answer is set aside, and a
    /// claimed pair is removed and recorded as an event.
    ///
    /// For fixtures and any opener that is not a waiter. A waiter opens through
    /// [`open_registered`](Self::open_registered), so its gate file is never visible
    /// before its registration.
    pub fn open_gate(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        context: &str,
    ) -> Result<OpenedGate, GateError> {
        Self::open_gate_as(project_root, phase, stage, &GateId::new(), context)
    }

    /// Open a gate and register its waiter, in that order on disk: the registration
    /// exists before the gate file does, so a writer that sees the gate file finds
    /// the waiter registered, or registered and since dead (D-04).
    ///
    /// A crash between the two leaves no gate file, so no writer can learn the id,
    /// and an unheld registration the stage's next close removes.
    pub fn open_registered(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        context: &str,
    ) -> Result<(OpenedGate, WaiterRegistration), GateError> {
        Self::open_registered_with_hooks(project_root, phase, stage, context, || {}, || {})
    }

    /// [`open_registered`](Self::open_registered)'s whole sequence, so the tests run
    /// the order a release build ships. The hooks run outside the gate lock. A failed
    /// publish finishes the registration, leaving neither file.
    fn open_registered_with_hooks(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        context: &str,
        before_publish: impl FnOnce(),
        after_publish: impl FnOnce(),
    ) -> Result<(OpenedGate, WaiterRegistration), GateError> {
        let gate_id = GateId::new();
        let registration = Self::register_waiter(project_root, phase, stage, &gate_id)?;
        before_publish();
        let opened = match Self::open_gate_as(project_root, phase, stage, &gate_id, context) {
            Ok(opened) => opened,
            Err(error) => {
                registration.finish();
                return Err(error);
            }
        };
        after_publish();
        Ok((opened, registration))
    }

    /// The order [`open_registered`](Self::open_registered) must not have, as the
    /// control of the tests that pin its order.
    #[cfg(test)]
    fn publish_then_register(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        context: &str,
        before_publish: impl FnOnce(),
        after_publish: impl FnOnce(),
    ) -> Result<(OpenedGate, WaiterRegistration), GateError> {
        let gate_id = GateId::new();
        before_publish();
        let opened = Self::open_gate_as(project_root, phase, stage, &gate_id, context)?;
        after_publish();
        let registration = Self::register_waiter(project_root, phase, stage, &gate_id)?;
        Ok((opened, registration))
    }

    /// [`open_gate`](Self::open_gate) under the id the caller chose. The disposal
    /// never touches a `.waiter` file: the registration for the gate being opened may
    /// already exist.
    fn open_gate_as(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate_id: &GateId,
        context: &str,
    ) -> Result<OpenedGate, GateError> {
        let _lock = gate_lock(project_root, phase)?;
        let (_, _, disposal) =
            Self::set_aside_stage_answers(project_root, phase, stage, &mut |_| {});
        disposal?;
        let opened = Self::write_gate_file(project_root, phase, stage, gate_id, context)?;
        consume_preanswer(project_root, phase, stage, gate_id)?;
        Ok(opened)
    }

    /// Register as `gate`'s waiter by taking a lock on its `.waiter` file, held until
    /// [`WaiterRegistration::finish`] or this process's death (D-04). A waiter with no
    /// supervisor socket registers the same way. It records this process's identity
    /// for an operator; only the lock decides liveness.
    ///
    /// # Errors
    /// [`GateError::Io`], including when `gate` already has a registration file, which
    /// is never replaced.
    pub fn register_waiter(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
    ) -> Result<WaiterRegistration, GateError> {
        crate::workflow::ensure_devflow_dir(&Self::dir(project_root))?;
        let hold = liveness::Hold::take(&Self::waiter_path(project_root, phase, stage, gate))?;
        Ok(WaiterRegistration { hold })
    }

    /// Whether `gate` has a waiter, read from its registration lock alone.
    ///
    /// # Errors
    /// [`GateError::Io`] when the registration exists but cannot be probed.
    pub fn waiter_status(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
    ) -> Result<WaiterStatus, GateError> {
        let path = Self::waiter_path(project_root, phase, stage, gate);
        Ok(match liveness::probe(&path)? {
            liveness::Held::Live => WaiterStatus::Live,
            liveness::Held::Abandoned | liveness::Held::Missing => WaiterStatus::Absent,
        })
    }

    /// The pid and start time of every process holding a waiter registration, for
    /// the stray reaper's safety set. A holder whose record cannot be read is logged
    /// and left out.
    pub fn live_waiter_holders(project_root: &Path) -> Vec<(u32, Option<u64>)> {
        let Ok(entries) = std::fs::read_dir(Self::dir(project_root)) else {
            return Vec::new();
        };
        let mut holders = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.to_string_lossy().ends_with(WAITER_SUFFIX) {
                continue;
            }
            match liveness::live_holder(&path) {
                Ok(Some(holder)) => holders.push(holder),
                Ok(None) => {}
                Err(error) => warn!(
                    "could not read the waiter registration {}: {error}",
                    path.display()
                ),
            }
        }
        holders
    }

    /// Wait for `gate`'s answer, reading only that gate's slot. Never writes, renames
    /// or removes a file (R-09), so a waiter that outlives its gate cannot touch the
    /// current gate's files.
    ///
    /// A caller receiving [`AwaitOutcome::Unbound`] moves the body with
    /// [`set_aside_unbound`](Self::set_aside_unbound) and waits again with the time
    /// left. `cancel` is checked before every read.
    pub fn await_answer(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
        timeout_secs: u64,
        cancel: impl Fn() -> bool,
    ) -> AwaitOutcome {
        let end = backoff_wait(Duration::from_secs(timeout_secs), cancel, || {
            #[cfg(test)]
            SLOT_READS.with(|reads| reads.set(reads.get() + 1));
            match Self::read_answer(project_root, phase, stage, gate) {
                AnswerRead::None => None,
                AnswerRead::Bound(answer) => Some(AwaitOutcome::Answered(answer)),
                AnswerRead::Unbound(reason) => Some(AwaitOutcome::Unbound { reason }),
            }
        });
        match end {
            WaitEnd::Found(outcome) => outcome,
            WaitEnd::TimedOut => AwaitOutcome::TimedOut,
            WaitEnd::Cancelled => AwaitOutcome::Cancelled,
        }
    }

    /// Answer the gate the caller observed: publish a [`GateAnswer`] for that id,
    /// exactly once.
    pub fn answer(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        observed: &GateId,
        response: &GateResponse,
    ) -> Result<PathBuf, GateError> {
        Self::answer_with_hook(project_root, phase, stage, observed, response, || {})
    }

    /// Answer an abandoned gate with a rejection, for the gate the caller observed.
    /// Mitigation for T-23-41 (Elevation of Privilege): this function takes **no**
    /// boolean parameter and hard-codes `approved: false` at the literal below, so no
    /// caller can ever make it write an approval.
    ///
    /// The caller-supplied `note`'s lowercase form MUST contain the abort keyword
    /// [`GateAction::from_response`] matches on (`"abort"`), so the reap resolves to
    /// `GateAction::Abort` and never relaunches an agent on an abandoned run. It
    /// publishes the way [`answer`](Self::answer) does, with the same checks.
    pub fn reap_answer(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        observed: &GateId,
        note: &str,
        responded_by: &str,
    ) -> Result<PathBuf, GateError> {
        let response = GateResponse {
            approved: false,
            note: Some(note.to_string()),
            responded_by: Some(responded_by.to_string()),
        };
        Self::answer_with_hook(project_root, phase, stage, observed, &response, || {})
    }

    /// Every answer that was set aside, for `status` and `doctor`. A file whose name
    /// does not parse is skipped.
    pub fn quarantined(project_root: &Path) -> Vec<QuarantinedAnswer> {
        let Ok(entries) = std::fs::read_dir(Self::dir(project_root)) else {
            return Vec::new();
        };
        let mut listed: Vec<QuarantinedAnswer> = entries
            .flatten()
            .filter_map(|entry| parse_quarantine_name(&entry.path()))
            .collect();
        listed.sort_by(|a, b| a.path.cmp(&b.path));
        listed
    }

    /// Close a stage's gate. Every unconsumed answer is set aside, never deleted;
    /// every consumed answer/claim pair is removed and reported in
    /// [`CloseReport::claimed`] (R-02); then the gate file and orphaned durable temps
    /// go. Idempotent.
    pub fn close(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> Result<CloseReport, GateError> {
        let (report, result) = Self::close_with_partial_result(project_root, phase, stage);
        result.map(|()| report)
    }

    /// [`close`](Self::close), retaining what happened before an error.
    pub(crate) fn close_with_partial_result(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> (CloseReport, Result<(), GateError>) {
        Self::close_with_partial_result_with(project_root, phase, stage, &mut |_| {})
    }

    /// The gate file goes last (R2-05), so a crash inside a close leaves it and every
    /// pair not yet removed for the next close to report: a pair is reported twice at
    /// worst, never zero times.
    fn close_with_partial_result_with(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        hook: &mut dyn FnMut(DisposalStep),
    ) -> (CloseReport, Result<(), GateError>) {
        let mut report = CloseReport::default();
        if !Self::dir(project_root).is_dir() {
            return (report, Ok(()));
        }
        let _lock = match gate_lock(project_root, phase) {
            Ok(lock) => lock,
            Err(error) => return (report, Err(error)),
        };
        let (quarantined, claimed, disposal) =
            Self::set_aside_stage_answers(project_root, phase, stage, hook);
        report.quarantined = quarantined.into_iter().map(|moved| moved.to).collect();
        report.removed = !claimed.is_empty();
        report.claimed = claimed;
        if let Err(error) = disposal {
            return (report, Err(error));
        }
        let gate_file =
            crate::durable::remove_durably(&Self::gate_path(project_root, phase, stage));
        match gate_file {
            Ok(removed) => report.removed |= removed,
            Err(error) => return (report, Err(error.into())),
        }
        let temps = Self::remove_stage_temps(project_root, phase, stage);
        match temps {
            Ok(removed) => report.removed |= removed,
            Err(error) => return (report, Err(error)),
        }
        match Self::remove_abandoned_waiters(project_root, phase, stage) {
            Ok(removed) => report.removed |= removed,
            Err(error) => return (report, Err(error)),
        }
        (report, Ok(()))
    }

    /// The only stage-wide disposal, called only by [`open_gate`](Self::open_gate) and
    /// a close (R-09), both under the gate lock.
    ///
    /// Unclaimed answers are set aside first. Each claimed pair's event then goes out
    /// before that pair's files are removed, so a crash reports a pair again rather
    /// than losing it. What was done before an error is returned with it.
    fn set_aside_stage_answers(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        hook: &mut dyn FnMut(DisposalStep),
    ) -> (Vec<Quarantined>, Vec<ClaimedAnswer>, Result<(), GateError>) {
        let mut quarantined = Vec::new();
        let mut claimed = Vec::new();
        let result = Self::dispose_stage_answers(
            project_root,
            phase,
            stage,
            hook,
            &mut quarantined,
            &mut claimed,
        );
        (quarantined, claimed, result)
    }

    fn dispose_stage_answers(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        hook: &mut dyn FnMut(DisposalStep),
        quarantined: &mut Vec<Quarantined>,
        claimed: &mut Vec<ClaimedAnswer>,
    ) -> Result<(), GateError> {
        let files = StageFiles::scan(project_root, phase, stage)?;
        for slot in &files.unclaimed_answers {
            let moved = set_aside_unclaimed(project_root, phase, stage, slot)?;
            quarantined.extend(moved);
        }
        for gate in &files.claimed_gates {
            Self::remove_claimed_pair(
                project_root,
                phase,
                stage,
                gate,
                hook,
                quarantined,
                claimed,
            )?;
        }
        if let Some(legacy_ack) = &files.legacy_ack {
            crate::durable::remove_durably(legacy_ack)?;
        }
        Ok(())
    }

    /// Record one claimed pair, then remove it. A body that never bound to its gate is
    /// set aside instead of removed: a claim does not license deleting it (D-05).
    fn remove_claimed_pair(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
        hook: &mut dyn FnMut(DisposalStep),
        quarantined: &mut Vec<Quarantined>,
        claimed: &mut Vec<ClaimedAnswer>,
    ) -> Result<(), GateError> {
        let path = Self::answer_path(project_root, phase, stage, gate);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let bound = bytes
            .as_deref()
            .map(|bytes| classify_answer(bytes, gate.as_str()));
        let answer = bound.clone().and_then(Result::ok);
        let response = answer.as_ref().map(|answer| &answer.response);
        warn!("removing claimed answer for phase {phase} {stage} gate {gate}");
        crate::events::emit(
            project_root,
            phase,
            "gate_claimed_answer_removed",
            serde_json::json!({
                "stage": stage.to_string(),
                "gate_id": gate.as_str(),
                "approved": response.map(|response| response.approved),
                "note": response.and_then(|response| response.note.as_deref()),
                "responded_by": response.and_then(|response| response.responded_by.as_deref()),
            }),
        );
        hook(DisposalStep::ClaimedEventEmitted(gate.to_string()));
        claimed.push(ClaimedAnswer {
            gate_id: gate.to_string(),
            answer,
        });
        match (bound, bytes) {
            (Some(Err(reason)), Some(bytes)) => {
                let moved = set_aside_if_present(
                    project_root,
                    phase,
                    stage,
                    &path,
                    &body_label(&bytes),
                    reason,
                )?;
                quarantined.extend(moved);
            }
            _ => {
                crate::durable::remove_durably(&path)?;
            }
        }
        crate::durable::remove_durably(&Self::claim_path(project_root, phase, stage, gate))?;
        Ok(())
    }

    /// Remove the stage's orphaned durable temps. A `*.hold-tmp` never matches, so a
    /// waiter mid-registration is untouched.
    fn remove_stage_temps(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> Result<bool, GateError> {
        let prefix = format!(".{}-{stage}.", phase.padded());
        let mut removed = false;
        for entry in std::fs::read_dir(Self::dir(project_root))? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with(&prefix) && name.ends_with(".tmp") {
                removed |= crate::durable::remove_durably(&entry.path())?;
            }
        }
        Ok(removed)
    }

    /// Remove the stage's waiter registrations that read abandoned. An abandoned
    /// registration never reads live again, because nothing takes a lock through an
    /// existing hold name, so the probe and the removal cannot race a waiter. A live
    /// one is never removed here: only its owner does that, through
    /// [`WaiterRegistration::finish`] (R-06).
    fn remove_abandoned_waiters(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> Result<bool, GateError> {
        let prefix = format!("{}-{stage}.", phase.padded());
        let mut removed = false;
        for entry in std::fs::read_dir(Self::dir(project_root))? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with(&prefix)
                && name.ends_with(WAITER_SUFFIX)
                && liveness::probe(&entry.path())? == liveness::Held::Abandoned
            {
                removed |= crate::durable::remove_durably(&entry.path())?;
            }
        }
        Ok(removed)
    }

    /// Move an answer that does not bind to `gate` out of that gate's slot, and
    /// return its new name. This is how a waiter disposes of one (reading never
    /// moves anything). It touches `gate`'s slot and nothing else, and only while
    /// the gate file names `gate` (R-09).
    pub fn set_aside_unbound(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
    ) -> Result<Option<PathBuf>, GateError> {
        if !Self::dir(project_root).is_dir() {
            return Err(GateError::NoOpenGate { phase, stage });
        }
        let _lock = gate_lock(project_root, phase)?;
        Self::require_open(project_root, phase, stage, gate)?;
        let moved = Self::set_aside_slot_if_unbound(project_root, phase, stage, gate)?;
        Ok(moved.map(|quarantined| quarantined.to))
    }

    /// [`answer`](Self::answer) with a seam between the pre-check and the publish,
    /// where no lock is held.
    ///
    /// Under the gate lock: the gate file must name `observed`, and a body in its
    /// slot that does not bind is moved, so a refused answer never blocks a valid
    /// one (D-05). Then the exclusive publish, outside the lock. Then under the gate
    /// lock again: if the gate closed meanwhile, this writer sets its own publish
    /// aside, so a decision about one gate can never be read as a decision about the
    /// next.
    fn answer_with_hook(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        observed: &GateId,
        response: &GateResponse,
        before_publish: impl FnOnce(),
    ) -> Result<PathBuf, GateError> {
        if !Self::dir(project_root).is_dir() {
            return Err(GateError::NoOpenGate { phase, stage });
        }
        {
            let _lock = gate_lock(project_root, phase)?;
            Self::require_open(project_root, phase, stage, observed)?;
            Self::set_aside_slot_if_unbound(project_root, phase, stage, observed)?;
        }
        before_publish();
        run_answer_publish_hook();
        let path = Self::answer_path(project_root, phase, stage, observed);
        let answer = GateAnswer {
            gate_id: observed.to_string(),
            response: response.clone(),
        };
        publish_response_exclusive(&path, &serde_json::to_string_pretty(&answer)?, phase, stage)?;
        info!(
            "gate answer written for phase {phase} {stage} gate {observed}: approved={}",
            response.approved
        );

        // The answer is published: "nothing was changed" is no longer true, and
        // without the lock this writer cannot tell an open gate from a closed one,
        // so a timeout here leaves the file where it is (R2-04).
        let _lock = gate_lock(project_root, phase).map_err(|error| match error {
            GateError::GateLockTimeout { waited, .. } => GateError::PublishedUnconfirmed {
                phase,
                stage,
                observed: observed.to_string(),
                waited,
            },
            other => other,
        })?;
        if Self::current_gate_id(project_root, phase, stage)?.as_deref() == Some(observed.as_str())
        {
            return Ok(path);
        }
        set_aside_if_present(
            project_root,
            phase,
            stage,
            &path,
            observed.as_str(),
            REASON_GATE_CLOSED,
        )?;
        Err(GateError::GateClosedAfterPublish {
            phase,
            stage,
            observed: observed.to_string(),
        })
    }

    /// The id the stage's gate file names, or `None` with no gate file.
    pub(crate) fn current_gate_id(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> Result<Option<String>, GateError> {
        match std::fs::read_to_string(Self::gate_path(project_root, phase, stage)) {
            Ok(contents) => Ok(Some(serde_json::from_str::<GateFile>(&contents)?.gate_id)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Refuse unless the stage's gate file names `gate`.
    fn require_open(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
    ) -> Result<(), GateError> {
        match Self::current_gate_id(project_root, phase, stage)? {
            None => Err(GateError::NoOpenGate { phase, stage }),
            Some(current) if current == gate.as_str() => Ok(()),
            Some(current) => Err(GateError::GateIdMismatch {
                phase,
                stage,
                observed: gate.to_string(),
                current,
            }),
        }
    }

    /// Set aside what is in `gate`'s slot when it does not bind to `gate`. A bound
    /// answer stays: the first answer wins.
    fn set_aside_slot_if_unbound(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
    ) -> std::io::Result<Option<Quarantined>> {
        let path = Self::answer_path(project_root, phase, stage, gate);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        match classify_answer(&bytes, gate.as_str()) {
            Ok(_) => Ok(None),
            Err(reason) => {
                let label = body_label(&bytes);
                set_aside_if_present(project_root, phase, stage, &path, &label, reason)
            }
        }
    }

    /// What is in one gate's answer slot. Only reads: a file that does not bind is
    /// reported and left where it is.
    pub fn read_answer(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
    ) -> AnswerRead {
        let path = Self::answer_path(project_root, phase, stage, gate);
        match std::fs::read(&path) {
            Ok(bytes) => match classify_answer(&bytes, gate.as_str()) {
                Ok(answer) => AnswerRead::Bound(answer),
                Err(reason) => AnswerRead::Unbound(reason.to_string()),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => AnswerRead::None,
            Err(error) => {
                warn!("could not read gate answer {}: {error}", path.display());
                AnswerRead::Unbound(REASON_UNPARSABLE.to_string())
            }
        }
    }

    /// Claim a gate's answer for consumption: the first caller gets
    /// [`Claim::Claimed`] and acts on the answer, any later one
    /// [`Claim::AlreadyConsumed`].
    ///
    /// The claim succeeds only while the gate file still names `gate`, checked in one
    /// gate-lock critical section with the publish (R-08). A gate id is never reused
    /// (fresh per open), so once a gate's file is removed or replaced no claim of it
    /// can succeed. On [`GateError::NoOpenGate`] or [`GateError::GateIdMismatch`]
    /// nothing was published, and the caller must not act on the answer.
    pub fn claim(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate: &GateId,
    ) -> Result<Claim, GateError> {
        if !Self::dir(project_root).is_dir() {
            return Err(GateError::NoOpenGate { phase, stage });
        }
        let _lock = gate_lock(project_root, phase)?;
        Self::require_open(project_root, phase, stage, gate)?;
        let path = Self::claim_path(project_root, phase, stage, gate);
        let ack = serde_json::to_string_pretty(&GateAck { received: true })?;
        match crate::durable::publish_exclusive_durably(&path, ack.as_bytes())? {
            crate::durable::Publication::Published => Ok(Claim::Claimed),
            crate::durable::Publication::AlreadyExists => Ok(Claim::AlreadyConsumed),
        }
    }

    /// Every open gate (request written, no response yet), sorted by phase
    /// then stage. Request files are `NN-{stage}.json`; a name with more after the
    /// stage (a keyed answer, a claim, a quarantined answer) is a protocol artifact,
    /// not a request, and any unparsable file is skipped — listing must degrade, not
    /// die. A gate is hidden once its own live answer, or a legacy unkeyed one, is
    /// there. Only reads.
    pub fn list_open(project_root: &Path) -> Vec<OpenGate> {
        let mut open = Vec::new();
        let Ok(entries) = std::fs::read_dir(Self::dir(project_root)) else {
            return open;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            // The phase may hold a dot (`50.1-code.json`), so look past it first.
            let after_phase = stem.split_once('-').map_or(stem, |(_, rest)| rest);
            if after_phase.contains('.') {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok(gate) = serde_json::from_str::<GateFile>(&contents) else {
                continue;
            };
            let answered = GateId::parse(&gate.gate_id).is_ok_and(|id| {
                matches!(
                    Self::read_answer(project_root, gate.phase, gate.stage, &id),
                    AnswerRead::Bound(_)
                )
            });
            if answered || Self::response_path(project_root, gate.phase, gate.stage).exists() {
                continue;
            }
            open.push(OpenGate {
                phase: gate.phase,
                stage: gate.stage,
                context: gate.context,
                timestamp: gate.timestamp,
                gate_id: gate.gate_id,
            });
        }
        open.sort_by_key(|g| (g.phase, g.stage.to_string()));
        open
    }

    /// Every phase that has a gate request, response, ack, or orphaned write
    /// temp on disk, whether or not the gate is still open — so a reset can
    /// reach gate files that outlived their phase's state. A waiter's in-flight take
    /// temp is not one: it is never a gate write, and the sweep of orphaned hold temps
    /// removes one a dead creator left.
    pub fn phases_on_disk(project_root: &Path) -> std::collections::BTreeSet<PhaseId> {
        let mut phases = std::collections::BTreeSet::new();
        let Ok(entries) = std::fs::read_dir(Self::dir(project_root)) else {
            return phases;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.ends_with(liveness::HOLD_TEMP_SUFFIX) {
                continue;
            }
            let name = name.strip_prefix('.').unwrap_or(name);
            if let Some((phase, _)) = name.split_once('-')
                && let Ok(phase) = phase.parse::<PhaseId>()
            {
                phases.insert(phase);
            }
        }
        phases
    }

    /// Answer an open gate by writing its response file atomically — the
    /// programmatic form of what a human previously hand-edited. Refuses
    /// when no gate request is open for the phase+stage, and when a
    /// response is already on disk awaiting the workflow's poller (silently
    /// replacing an unconsumed answer would race the poll).
    pub fn respond(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        response: &GateResponse,
    ) -> Result<PathBuf, GateError> {
        if !Self::gate_path(project_root, phase, stage).exists() {
            return Err(GateError::NoOpenGate { phase, stage });
        }
        let path = Self::response_path(project_root, phase, stage);
        if path.exists() {
            return Err(GateError::AlreadyResponded { phase, stage });
        }
        publish_response_exclusive(
            &path,
            &serde_json::to_string_pretty(response)?,
            phase,
            stage,
        )?;
        info!(
            "gate response written for phase {phase} {stage}: approved={}",
            response.approved
        );
        Ok(path)
    }

    /// Answer an abandoned gate with a rejection — the reaping half of 23b's
    /// aged-gate sweep. Mitigation for T-23-41 (Elevation of Privilege): this
    /// function takes **no** boolean parameter and hard-codes `approved:
    /// false` at the literal below, so no caller — buggy or refactored — can
    /// ever make it write an approval. It is the sweep's ONLY write path.
    ///
    /// The caller-supplied `note`'s lowercase form MUST contain the abort
    /// keyword [`GateAction::from_response`] matches on (`"abort"`), so the
    /// reap resolves to `GateAction::Abort` rather than
    /// `GateAction::LoopBack(Stage::Code)` — a loop-back would relaunch an
    /// agent on an abandoned run, the opposite of what a reap should do.
    pub fn reap(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        note: &str,
        responded_by: &str,
    ) -> Result<PathBuf, GateError> {
        let response = GateResponse {
            approved: false,
            note: Some(note.to_string()),
            responded_by: Some(responded_by.to_string()),
        };
        Self::respond(project_root, phase, stage, &response)
    }

    /// Write a gate request, creating the gates directory if needed.
    pub fn write_gate(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        context: &str,
    ) -> Result<PathBuf, GateError> {
        let opened = Self::write_gate_file(project_root, phase, stage, &GateId::new(), context)?;
        consume_preanswer(project_root, phase, stage, &opened.gate_id)?;
        Ok(opened.path)
    }

    /// Write a gate file under `gate_id`. Disposes of nothing.
    fn write_gate_file(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        gate_id: &GateId,
        context: &str,
    ) -> Result<OpenedGate, GateError> {
        let gate = GateFile {
            phase,
            stage,
            context: context.to_string(),
            timestamp: unix_now(),
            gate_id: gate_id.to_string(),
        };
        let path = Self::gate_path(project_root, phase, stage);
        info!("writing gate {} for phase {phase}", stage);
        write_atomic(&path, &serde_json::to_string_pretty(&gate)?)?;
        Ok(OpenedGate {
            gate_id: gate_id.clone(),
            path,
        })
    }

    /// Poll for a response, reading at backoff boundaries (1 s doubling to 60 s) and
    /// at the timeout, giving up after `timeout_secs`. Returns the parsed response
    /// when it appears, or `None` on timeout. Only reads (R-09).
    ///
    /// Transition bridge, deleted in 50.1-15: it also reads the live gate's keyed
    /// answer.
    pub fn poll_response(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        timeout_secs: u64,
    ) -> Option<GateResponse> {
        let path = Self::response_path(project_root, phase, stage);
        debug!("polling for gate response at {}", path.display());
        backoff_wait(
            Duration::from_secs(timeout_secs),
            || false,
            || Self::read_live_or_unkeyed(project_root, phase, stage),
        )
        .found()
    }

    /// [`poll_response`](Self::poll_response) that a caller can cancel (50-13, R-2):
    /// the same reads, with `cancel` checked at least once a second, so a cancel ends
    /// the wait within about a second of being set.
    ///
    /// Returns the response when it appears, or `None` on a timeout or a cancel. A
    /// caller tells the two apart by asking its own `cancel` again: a `None` returned
    /// before the timeout means `cancel` answered `true`. `cancel` is checked before
    /// every read, so a cancel wins over a response already on disk.
    ///
    /// Transition bridge, deleted in 50.1-15: it also reads the live gate's keyed
    /// answer.
    pub fn poll_response_cancellable(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        timeout_secs: u64,
        cancel: impl Fn() -> bool,
    ) -> Option<GateResponse> {
        let path = Self::response_path(project_root, phase, stage);
        debug!(
            "polling (cancellable) for gate response at {}",
            path.display()
        );
        backoff_wait(Duration::from_secs(timeout_secs), cancel, || {
            Self::read_live_or_unkeyed(project_root, phase, stage)
        })
        .found()
    }

    /// One read of an unkeyed poll: the live gate's bound answer, else the unkeyed
    /// response. Moves nothing (R-09). Transition bridge, deleted in 50.1-15.
    fn read_live_or_unkeyed(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> Option<GateResponse> {
        let live = Self::current_gate_id(project_root, phase, stage)
            .ok()
            .flatten()
            .and_then(|id| GateId::parse(&id).ok());
        if let Some(gate) = live
            && let AnswerRead::Bound(answer) = Self::read_answer(project_root, phase, stage, &gate)
        {
            return Some(answer.response);
        }
        read_unkeyed_response(&Self::response_path(project_root, phase, stage))
    }

    /// Write an ack file signalling the response was read.
    pub fn ack(project_root: &Path, phase: PhaseId, stage: Stage) -> Result<PathBuf, GateError> {
        let path = Self::ack_path(project_root, phase, stage);
        write_atomic(
            &path,
            &serde_json::to_string_pretty(&GateAck { received: true })?,
        )?;
        Ok(path)
    }

    /// Remove the gate, response, and ack files for a stage, and their
    /// orphaned write temps. Idempotent. Returns whether this call removed
    /// any file, so a caller can report only what it actually cleaned.
    ///
    /// Transition bridge, deleted in 50.1-15: it also removes the stage's keyed
    /// answer and claim files, and its waiter files that read abandoned (R-06).
    pub fn cleanup(project_root: &Path, phase: PhaseId, stage: Stage) -> Result<bool, GateError> {
        let (removed, result) = Self::cleanup_with_partial_result(project_root, phase, stage);
        result.map(|()| removed)
    }

    /// Clean up a gate while retaining removals that happened before an error.
    pub(crate) fn cleanup_with_partial_result(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> (bool, Result<(), GateError>) {
        let paths = [
            Self::gate_path(project_root, phase, stage),
            Self::response_path(project_root, phase, stage),
            Self::ack_path(project_root, phase, stage),
        ];
        let mut removed = false;
        for path in &paths {
            if path.exists() {
                match crate::durable::remove_durably(path) {
                    Ok(was_removed) => removed |= was_removed,
                    Err(error) => return (removed, Err(error.into())),
                }
            }
        }
        let dir = Self::dir(project_root);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return (removed, Ok(()));
        };
        let prefixes = paths
            .iter()
            .map(|path| {
                format!(
                    ".{}.",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            })
            .collect::<Vec<_>>();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => return (removed, Err(error.into())),
            };
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.ends_with(".tmp") && prefixes.iter().any(|prefix| name.starts_with(prefix)) {
                match crate::durable::remove_durably(&entry.path()) {
                    Ok(was_removed) => removed |= was_removed,
                    Err(error) => return (removed, Err(error.into())),
                }
            }
        }
        match Self::remove_keyed_stage_files(project_root, phase, stage) {
            Ok(was_removed) => removed |= was_removed,
            Err(error) => return (removed, Err(error)),
        }
        match Self::remove_abandoned_waiters(project_root, phase, stage) {
            Ok(was_removed) => removed |= was_removed,
            Err(error) => return (removed, Err(error)),
        }
        (removed, Ok(()))
    }

    /// Remove the stage's keyed answer and claim files. Transition bridge, deleted in
    /// 50.1-15.
    fn remove_keyed_stage_files(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
    ) -> Result<bool, GateError> {
        let prefix = format!("{}-{stage}.", phase.padded());
        let mut removed = false;
        for entry in std::fs::read_dir(Self::dir(project_root))? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with(&prefix)
                && (name.ends_with(".response.json") || name.ends_with(".ack.json"))
            {
                removed |= crate::durable::remove_durably(&entry.path())?;
            }
        }
        Ok(removed)
    }
}

/// Publish the stage's pre-answer, if there is one, as `gate`'s answer, and remove it.
///
/// Not re-entrant, by design: this runs inside [`Gates::open_gate`]'s gate-lock critical
/// section, so it calls only the unlocked publish step and never [`Gates::answer`] or
/// anything else that takes the gate lock. A second open file description in this
/// process cannot take the `flock` the first holds, so `gate_lock` would poll until
/// [`GATE_LOCK_WAIT`] and fail with [`GateError::GateLockTimeout`]. It never takes the
/// state write lock either.
#[cfg(any(test, feature = "test-support"))]
fn consume_preanswer(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    gate: &GateId,
) -> Result<(), GateError> {
    let path = Gates::preanswer_path(project_root, phase, stage);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let answer = GateAnswer {
        gate_id: gate.to_string(),
        response: serde_json::from_slice(&bytes)?,
    };
    publish_response_exclusive(
        &Gates::answer_path(project_root, phase, stage, gate),
        &serde_json::to_string_pretty(&answer)?,
        phase,
        stage,
    )?;
    crate::durable::remove_durably(&path)?;
    Ok(())
}

/// A release build has no pre-answer: nothing can turn a stray file into an answer
/// (T-50.1-30).
#[cfg(not(any(test, feature = "test-support")))]
fn consume_preanswer(
    _project_root: &Path,
    _phase: PhaseId,
    _stage: Stage,
    _gate: &GateId,
) -> Result<(), GateError> {
    Ok(())
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// What the next answer this thread publishes runs, after its id check and before
    /// its publish. See [`crate::test_support::with_answer_before_publish`].
    static ANSWER_PUBLISH_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Install `hook`, replacing any earlier one, or clear the slot with `None`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn set_answer_publish_hook(hook: Option<Box<dyn FnOnce()>>) {
    ANSWER_PUBLISH_HOOK.with(|slot| *slot.borrow_mut() = hook);
}

/// Take the installed hook out of its slot and run it, so it runs once, and with the
/// slot free in case it answers a gate itself.
#[cfg(any(test, feature = "test-support"))]
fn run_answer_publish_hook() {
    let hook = ANSWER_PUBLISH_HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

/// A release build runs no closure inside an answer (T-50.1-104).
#[cfg(not(any(test, feature = "test-support")))]
fn run_answer_publish_hook() {}

#[cfg(test)]
thread_local! {
    /// How many times this thread's [`Gates::await_answer`] read its slot.
    static SLOT_READS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// How [`backoff_wait`] ended.
enum WaitEnd<T> {
    /// `read` found something.
    Found(T),
    /// The timeout ran out.
    TimedOut,
    /// `cancel` answered `true`.
    Cancelled,
}

impl<T> WaitEnd<T> {
    /// What `read` found, if it did.
    fn found(self) -> Option<T> {
        match self {
            WaitEnd::Found(found) => Some(found),
            WaitEnd::TimedOut | WaitEnd::Cancelled => None,
        }
    }
}

/// The gap between a wait's first two reads, doubling up to [`READ_BACKOFF_CAP`].
const READ_BACKOFF_START: Duration = Duration::from_secs(1);
/// The longest gap between two reads of a wait.
const READ_BACKOFF_CAP: Duration = Duration::from_secs(60);
/// The longest a wait sleeps before it checks `cancel` again.
const CANCEL_CHECK_SLICE: Duration = Duration::from_secs(1);

/// The wait loop every gate wait shares: `read` until it finds something, `cancel`
/// answers `true`, or `timeout` runs out.
///
/// `cancel` is checked first and at least once a second, so a stop accepted in the
/// same second as an answer wins and a wait of days stays cancellable. `read` runs
/// only at backoff boundaries, so such a wait costs few reads (D-13 items 4 and 7).
/// The timeout is a boundary too, so an answer that lands in the last gap is an
/// answer rather than a timeout.
fn backoff_wait<T>(
    timeout: Duration,
    cancel: impl Fn() -> bool,
    mut read: impl FnMut() -> Option<T>,
) -> WaitEnd<T> {
    let started = std::time::Instant::now();
    let mut next_read = Duration::ZERO;
    let mut backoff = READ_BACKOFF_START;
    loop {
        if cancel() {
            return WaitEnd::Cancelled;
        }
        if started.elapsed() >= next_read {
            if let Some(found) = read() {
                return WaitEnd::Found(found);
            }
            next_read = next_read.saturating_add(backoff).min(timeout);
            backoff = (backoff * 2).min(READ_BACKOFF_CAP);
        }
        let waited = started.elapsed();
        if waited >= timeout {
            return WaitEnd::TimedOut;
        }
        std::thread::sleep(CANCEL_CHECK_SLICE.min(next_read.saturating_sub(waited)));
    }
}

/// The body at the unkeyed response path, when it parses as a response.
fn read_unkeyed_response(path: &Path) -> Option<GateResponse> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Fire the operator-configured gate notify hook, if any.
///
/// Reads `DEVFLOW_GATE_NOTIFY_CMD`; if unset or empty, this is a silent no-op
/// (no notify command configured). Otherwise delegates to
/// [`run_notify_command`]. `unexpected` marks a gate fired on a stage the
/// active [`crate::mode::Mode`] would not normally gate (e.g. a Define/Plan/Code
/// failure in Auto mode) — a never-silent gate per WR-11.
///
/// This ignores a run's persisted command (`State::gate_notify_cmd`). DevFlow's
/// own gates call [`fire_gate_notify_with_fallback`]; this entry point stays
/// for API compatibility (49-REVIEW.md IN-02).
pub fn fire_gate_notify(phase: PhaseId, stage: Stage, context: &str, unexpected: bool) {
    fire_gate_notify_with_fallback(phase, stage, context, unexpected, None);
}

/// Fire the operator-configured gate notify hook, resolved by
/// [`resolve_gate_notify_cmd`] from `DEVFLOW_GATE_NOTIFY_CMD` and the run's
/// persisted `fallback_cmd`.
pub fn fire_gate_notify_with_fallback(
    phase: PhaseId,
    stage: Stage,
    context: &str,
    unexpected: bool,
    fallback_cmd: Option<&str>,
) {
    let env = std::env::var_os(GATE_NOTIFY_CMD_ENV);
    let Some(cmd) = resolve_gate_notify_cmd(env.as_deref(), fallback_cmd) else {
        return;
    };
    run_notify_command(&cmd, phase, stage, context, unexpected);
}

/// The environment variable that configures the gate notify hook.
pub const GATE_NOTIFY_CMD_ENV: &str = "DEVFLOW_GATE_NOTIFY_CMD";

/// Apply the operator's `DEVFLOW_GATE_NOTIFY_CMD` to a run's persisted notify
/// command. `devflow start`, `resume` and `advance` all call this, so the
/// state file is the one place a run's command lives (49-REVIEW.md CR-03):
/// - unset (or not valid UTF-8): keep the persisted command;
/// - set and empty: clear it, disabling notification for the run;
/// - set and non-empty: replace it.
///
/// The detached monitor chain never carries the variable, so an `advance` it
/// runs keeps whatever the state file holds.
pub fn persisted_notify_cmd_after_env(
    persisted: Option<String>,
    env: Option<&std::ffi::OsStr>,
) -> Option<String> {
    match env.map(|value| value.to_str()) {
        Some(Some("")) => None,
        Some(Some(value)) => Some(value.to_owned()),
        Some(None) | None => persisted,
    }
}

/// [`persisted_notify_cmd_after_env`] applied to this process's environment.
pub fn apply_notify_env(persisted: Option<String>) -> Option<String> {
    persisted_notify_cmd_after_env(persisted, std::env::var_os(GATE_NOTIFY_CMD_ENV).as_deref())
}

/// Resolve the gate notify command from the environment value and a run's
/// persisted command (49-REVIEW.md WR-01).
///
/// The environment has three states:
/// - set and non-empty: an operator override, used as is;
/// - set and empty: notification explicitly disabled, even when a command is
///   persisted;
/// - unset: the persisted `fallback_cmd`, if it is non-empty.
///
/// A value that is not valid UTF-8 counts as unset, as it did before this
/// resolver existed, rather than being run with replacement characters.
pub fn resolve_gate_notify_cmd(
    env: Option<&std::ffi::OsStr>,
    fallback_cmd: Option<&str>,
) -> Option<String> {
    match env.map(|value| value.to_str()) {
        Some(Some("")) => None,
        Some(Some(value)) => Some(value.to_owned()),
        Some(None) | None => fallback_cmd.filter(|s| !s.is_empty()).map(str::to_owned),
    }
}

/// Run the notify `cmd` via `sh -c`, passing gate metadata to the child as
/// environment variables — never interpolated into the command string
/// (WR-01 argv-not-shell precedent; `context` may contain agent-generated,
/// untrusted text). Fail-soft: a non-zero exit or spawn error is logged via
/// `warn!` and otherwise ignored — this must never propagate an error that
/// could abort `run_gate`.
fn run_notify_command(cmd: &str, phase: PhaseId, stage: Stage, context: &str, unexpected: bool) {
    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .env("DEVFLOW_GATE_PHASE", phase.to_string())
        .env("DEVFLOW_GATE_STAGE", stage.to_string())
        .env("DEVFLOW_GATE_CONTEXT", context)
        .env(
            "DEVFLOW_NON_SILENT_GATE",
            if unexpected { "1" } else { "0" },
        )
        .output();
    match output {
        Ok(out) if out.status.success() => {
            debug!("gate notify hook ran successfully");
        }
        Ok(out) => warn!(
            "gate notify hook exited with status {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(err) => warn!("gate notify hook could not be spawned: {err}"),
    }
}

/// The point inside a close's disposal that a kill-point fixture stops at.
enum DisposalStep {
    /// A claimed pair's event is out and its files are not yet removed.
    ClaimedEventEmitted(
        #[cfg_attr(not(test), expect(dead_code, reason = "only the fixture reads the id"))] String,
    ),
}

/// A file moved out of the way instead of being consumed or deleted.
#[derive(Debug)]
pub(crate) struct Quarantined {
    from: PathBuf,
    to: PathBuf,
    label: String,
    reason: &'static str,
}

/// Move `from` to a quarantine name beside it, log it and record it: the one way
/// an answer is set aside. The name is unique across processes and the move never
/// replaces anything (R-11), so two same-second quarantines both survive. `label`
/// names the gate in the file name, so it must be a checked gate id or `noid`.
fn set_aside(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    from: &Path,
    label: &str,
    reason: &'static str,
    secs: u64,
) -> std::io::Result<Quarantined> {
    let to = crate::durable::rename_to_unique_durably(
        from,
        &Gates::dir(project_root),
        &format!("{padded}-{stage}.{label}.{secs}", padded = phase.padded()),
        ".quarantined.json",
    )?;
    let quarantined = Quarantined {
        from: from.to_path_buf(),
        to,
        label: label.to_string(),
        reason,
    };
    warn!(
        "gate answer set aside ({}): {} -> {}",
        quarantined.reason,
        quarantined.from.display(),
        quarantined.to.display()
    );
    crate::events::emit(
        project_root,
        phase,
        "gate_answer_quarantined",
        serde_json::json!({
            "stage": stage.to_string(),
            "from": quarantined.from.display().to_string(),
            "to": quarantined.to.display().to_string(),
            "reason": quarantined.reason,
            "gate_id": quarantined.label,
        }),
    );
    Ok(quarantined)
}

/// [`set_aside`] at the current time, where a source already gone is not an error:
/// another process set it aside, and it owns the record.
fn set_aside_if_present(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    from: &Path,
    label: &str,
    reason: &'static str,
) -> std::io::Result<Option<Quarantined>> {
    match set_aside(project_root, phase, stage, from, label, reason, unix_secs()) {
        Ok(quarantined) => Ok(Some(quarantined)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// What a stage-wide disposal has to deal with, in name order so a report and a
/// kill-point fixture are deterministic.
struct StageFiles {
    unclaimed_answers: Vec<UnclaimedAnswer>,
    claimed_gates: Vec<GateId>,
    legacy_ack: Option<PathBuf>,
}

/// An answer file with no claim beside it.
struct UnclaimedAnswer {
    path: PathBuf,
    /// The id in its file name, `None` for the legacy unkeyed name.
    slot_id: Option<String>,
}

impl StageFiles {
    fn scan(project_root: &Path, phase: PhaseId, stage: Stage) -> Result<StageFiles, GateError> {
        let dir = Gates::dir(project_root);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StageFiles {
                    unclaimed_answers: Vec::new(),
                    claimed_gates: Vec::new(),
                    legacy_ack: None,
                });
            }
            Err(error) => return Err(error.into()),
        };
        let prefix = format!("{}-{stage}.", phase.padded());
        let mut names = Vec::new();
        for entry in entries {
            if let Some(name) = entry?.file_name().to_str()
                && let Some(rest) = name.strip_prefix(&prefix)
            {
                names.push(rest.to_string());
            }
        }
        names.sort();
        let claimed_gates: Vec<GateId> = names
            .iter()
            .filter_map(|rest| rest.strip_suffix(".ack.json"))
            .filter_map(|id| GateId::parse(id).ok())
            .collect();
        let unclaimed_answers = names
            .iter()
            .filter_map(|rest| {
                let slot_id = match rest.as_str() {
                    "response.json" => None,
                    keyed => Some(keyed.strip_suffix(".response.json")?.to_string()),
                };
                let claimed = slot_id
                    .as_deref()
                    .is_some_and(|id| claimed_gates.iter().any(|gate| gate.as_str() == id));
                (!claimed).then(|| UnclaimedAnswer {
                    path: dir.join(format!("{prefix}{rest}")),
                    slot_id,
                })
            })
            .collect();
        let legacy_ack = names
            .iter()
            .any(|rest| rest == "ack.json")
            .then(|| Gates::ack_path(project_root, phase, stage));
        Ok(StageFiles {
            unclaimed_answers,
            claimed_gates,
            legacy_ack,
        })
    }
}

/// Set aside one unclaimed answer. The reason says what the file is, the same at
/// open, close and read: a legacy name is always `legacy_unkeyed`, a body that binds
/// to its slot `gate_closed`, and any other the reason it does not bind.
fn set_aside_unclaimed(
    project_root: &Path,
    phase: PhaseId,
    stage: Stage,
    answer: &UnclaimedAnswer,
) -> std::io::Result<Option<Quarantined>> {
    let bytes = match std::fs::read(&answer.path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let reason = match answer.slot_id.as_deref() {
        None => REASON_LEGACY_UNKEYED,
        Some(id) => match classify_answer(&bytes, id) {
            Ok(_) if is_gate_id(id) => REASON_GATE_CLOSED,
            Ok(_) => REASON_OTHER_GATE,
            Err(reason) => reason,
        },
    };
    let label = body_label(&bytes);
    set_aside_if_present(project_root, phase, stage, &answer.path, &label, reason)
}

/// Parse `NN-{stage}.{label}.{secs}.{pid}.{seq}.quarantined.json`.
fn parse_quarantine_name(path: &Path) -> Option<QuarantinedAnswer> {
    let name = path.file_name()?.to_str()?;
    let (phase, rest) = name.strip_suffix(".quarantined.json")?.split_once('-')?;
    let fields: Vec<&str> = rest.split('.').collect();
    let [stage, label, secs, pid, seq] = fields[..] else {
        return None;
    };
    secs.parse::<u64>().ok()?;
    pid.parse::<u32>().ok()?;
    seq.parse::<u64>().ok()?;
    Some(QuarantinedAnswer {
        phase: phase.parse().ok()?,
        stage: stage.parse().ok()?,
        label: label.to_string(),
        path: path.to_path_buf(),
    })
}

/// The label for a quarantined body: the id it names when that is a gate id, so
/// that a hostile id never reaches a path, else [`NO_GATE_ID_LABEL`].
fn body_label(bytes: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .as_ref()
        .and_then(|body| body.get("gate_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|id| is_gate_id(id))
        .unwrap_or(NO_GATE_ID_LABEL)
        .to_string()
}

/// The phase's gate lock, released on drop. The file is never removed or replaced,
/// so two lockers can never hold different inodes.
struct GateLock {
    _file: std::fs::File,
}

/// Take the phase's gate lock, `.devflow/.gates-NN.lock`, waiting at most
/// [`GATE_LOCK_WAIT`].
///
/// It is held only across a few file operations, and `gates.rs` never takes the
/// state write lock, so a caller holding the state write lock may take the gate
/// lock inside it (50.1-19's owner fence) and nothing takes them in the other
/// order. `gates_never_takes_the_state_write_lock` holds that order for direct
/// `workflow::` paths only: a path through a helper outside `workflow` is not
/// scanned and rests on review.
fn gate_lock(project_root: &Path, phase: PhaseId) -> Result<GateLock, GateError> {
    let devflow = project_root.join(".devflow");
    crate::workflow::ensure_devflow_dir(&devflow)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(devflow.join(format!(".gates-{}.lock", phase.padded())))?;
    let started = std::time::Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(GateLock { _file: file }),
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
            Err(std::fs::TryLockError::WouldBlock) => {}
        }
        let waited = started.elapsed();
        let Some(remaining) = GATE_LOCK_WAIT
            .checked_sub(waited)
            .filter(|left| !left.is_zero())
        else {
            return Err(GateError::GateLockTimeout { phase, waited });
        };
        std::thread::sleep(crate::workflow::WRITE_LOCK_POLL.min(remaining));
    }
}

/// The suffix of a waiter registration's file name.
const WAITER_SUFFIX: &str = ".waiter";
/// An answer body that is not JSON, or lacks what an answer needs.
const REASON_UNPARSABLE: &str = "unparsable";
/// An answer body with no gate id.
const REASON_MISSING_GATE_ID: &str = "missing_gate_id";
/// An answer body naming a gate other than the one whose slot holds it.
const REASON_OTHER_GATE: &str = "other_gate";
/// An answer that bound to its gate, but the gate is no longer live.
const REASON_GATE_CLOSED: &str = "gate_closed";
/// An answer under the unkeyed per-stage name.
const REASON_LEGACY_UNKEYED: &str = "legacy_unkeyed";
/// The label of a quarantined body that names no usable gate id.
const NO_GATE_ID_LABEL: &str = "noid";

/// Whether a file in the answer slot of gate `slot_id` binds to that gate: its body
/// parses as a [`GateAnswer`] naming `slot_id`. Otherwise, why not.
fn classify_answer(bytes: &[u8], slot_id: &str) -> Result<GateAnswer, &'static str> {
    let Ok(body) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Err(REASON_UNPARSABLE);
    };
    match body.get("gate_id").and_then(serde_json::Value::as_str) {
        None | Some("") if body.is_object() => Err(REASON_MISSING_GATE_ID),
        None | Some("") => Err(REASON_UNPARSABLE),
        Some(id) if id != slot_id => Err(REASON_OTHER_GATE),
        Some(_) => serde_json::from_value(body).map_err(|_| REASON_UNPARSABLE),
    }
}

/// Publish a fully-written response exactly once. Filesystems that cannot
/// create hard links fail loudly through [`GateError::Io`] rather than falling
/// back to a rename that could overwrite the first response.
fn publish_response_exclusive(
    path: &Path,
    contents: &str,
    phase: PhaseId,
    stage: Stage,
) -> Result<(), GateError> {
    if let Some(parent) = path.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }
    match crate::durable::publish_exclusive_durably(path, contents.as_bytes())? {
        crate::durable::Publication::Published => Ok(()),
        crate::durable::Publication::AlreadyExists => {
            Err(GateError::AlreadyResponded { phase, stage })
        }
    }
}

/// Write `contents` to `path` through [`crate::durable::replace_durably`], so
/// readers never see a partial write and a power loss cannot lose the rename.
fn write_atomic(path: &Path, contents: &str) -> Result<(), GateError> {
    if let Some(parent) = path.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }
    crate::durable::replace_durably(path, contents.as_bytes())?;
    Ok(())
}

fn unix_now() -> String {
    unix_secs().to_string()
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate process-global env vars (`set_var`/
    /// `remove_var` are process-wide and `cargo test` runs in parallel by
    /// default) so they don't race each other.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn gate_file_round_trips_through_serde() {
        let gate = GateFile {
            phase: PhaseId::new(11),
            stage: Stage::Validate,
            context: "review the validation".into(),
            timestamp: "1750000000".into(),
            gate_id: GateId::new().to_string(),
        };
        let json = serde_json::to_string(&gate).unwrap();
        let back: GateFile = serde_json::from_str(&json).unwrap();
        assert_eq!(gate, back);
    }

    #[test]
    fn write_gate_creates_file_with_correct_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = Gates::write_gate(dir.path(), PhaseId::new(11), Stage::Validate, "ctx").unwrap();
        assert!(path.ends_with(".devflow/gates/11-validate.json"));
        assert!(path.exists());
        let gate: GateFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(gate.phase, PhaseId::new(11));
        assert_eq!(gate.stage, Stage::Validate);
        assert_eq!(gate.context, "ctx");
    }

    #[test]
    fn poll_response_returns_when_file_appears() {
        let dir = tempfile::tempdir().unwrap();
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: Some("human".into()),
        };
        let path = Gates::response_path(dir.path(), PhaseId::new(11), Stage::Validate);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&response).unwrap()).unwrap();

        let got = Gates::poll_response(dir.path(), PhaseId::new(11), Stage::Validate, 1).unwrap();
        assert_eq!(got, response);
    }

    #[test]
    fn poll_response_returns_immediately_at_full_timeout() {
        const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;

        let dir = tempfile::tempdir().unwrap();
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: Some("human".into()),
        };
        let path = Gates::response_path(dir.path(), PhaseId::new(11), Stage::Validate);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&response).unwrap()).unwrap();

        let started = std::time::Instant::now();
        let got = Gates::poll_response(dir.path(), PhaseId::new(11), Stage::Validate, SEVEN_DAYS)
            .unwrap();

        assert_eq!(got, response);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn poll_response_times_out_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Gates::poll_response(dir.path(), PhaseId::new(11), Stage::Ship, 0).is_none());
    }

    #[test]
    fn ack_writes_received_true() {
        let dir = tempfile::tempdir().unwrap();
        let path = Gates::ack(dir.path(), PhaseId::new(11), Stage::Ship).unwrap();
        let ack: GateAck = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(ack.received);
    }

    #[test]
    fn cleanup_removes_all_three_files_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        Gates::write_gate(dir.path(), PhaseId::new(11), Stage::Validate, "ctx").unwrap();
        Gates::ack(dir.path(), PhaseId::new(11), Stage::Validate).unwrap();
        std::fs::write(
            Gates::response_path(dir.path(), PhaseId::new(11), Stage::Validate),
            "{\"approved\":true}",
        )
        .unwrap();

        Gates::cleanup(dir.path(), PhaseId::new(11), Stage::Validate).unwrap();
        assert!(!Gates::gate_path(dir.path(), PhaseId::new(11), Stage::Validate).exists());
        assert!(!Gates::response_path(dir.path(), PhaseId::new(11), Stage::Validate).exists());
        assert!(!Gates::ack_path(dir.path(), PhaseId::new(11), Stage::Validate).exists());
        // Idempotent: cleaning again with nothing present succeeds.
        Gates::cleanup(dir.path(), PhaseId::new(11), Stage::Validate).unwrap();
    }

    #[test]
    fn cleanup_retains_an_earlier_removal_when_a_later_path_fails() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(12);
        let stage = Stage::Validate;
        let gate = Gates::gate_path(dir.path(), phase, stage);
        let response = Gates::response_path(dir.path(), phase, stage);
        Gates::write_gate(dir.path(), phase, stage, "ctx").unwrap();
        std::fs::create_dir_all(&response).unwrap();

        let (removed, result) = Gates::cleanup_with_partial_result(dir.path(), phase, stage);

        assert!(
            removed,
            "the gate file was removed before the response failed"
        );
        assert!(result.is_err(), "the response directory must fail removal");
        assert!(!gate.exists(), "the earlier gate removal must persist");
        assert!(
            response.is_dir(),
            "the response directory is the negative control"
        );
    }

    #[test]
    fn cleanup_removes_orphan_temps_for_its_gate_only() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(48);
        let stage = Stage::Code;
        let paths = [
            Gates::gate_path(dir.path(), phase, stage),
            Gates::response_path(dir.path(), phase, stage),
            Gates::ack_path(dir.path(), phase, stage),
        ];
        std::fs::create_dir_all(paths[0].parent().unwrap()).unwrap();
        let code_orphans = paths.map(|path| {
            let orphan = path.with_file_name(format!(
                ".{}.1.0.tmp",
                path.file_name().unwrap().to_string_lossy()
            ));
            std::fs::write(&orphan, "orphan").unwrap();
            orphan
        });
        let validate = Gates::gate_path(dir.path(), phase, Stage::Validate);
        let validate_orphan = validate.with_file_name(format!(
            ".{}.1.0.tmp",
            validate.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&validate_orphan, "orphan").unwrap();

        Gates::cleanup(dir.path(), phase, stage).unwrap();

        assert!(code_orphans.iter().all(|path| !path.exists()));
        assert!(validate_orphan.exists());
        Gates::cleanup(dir.path(), phase, stage).unwrap();
    }

    /// 15a: `devflow gate list` — a gate is open until its response lands;
    /// response/ack protocol files must never be mistaken for requests.
    #[test]
    fn list_open_shows_unanswered_gates_only() {
        let dir = tempfile::tempdir().unwrap();
        Gates::write_gate(dir.path(), PhaseId::new(7), Stage::Ship, "approve merge?").unwrap();
        Gates::write_gate(dir.path(), PhaseId::new(8), Stage::Validate, "review gaps").unwrap();
        // Phase 8's gate gets answered; its response/ack must hide it.
        Gates::respond(
            dir.path(),
            PhaseId::new(8),
            Stage::Validate,
            &GateResponse {
                approved: true,
                note: None,
                responded_by: Some("test".into()),
            },
        )
        .unwrap();
        Gates::ack(dir.path(), PhaseId::new(8), Stage::Validate).unwrap();
        // Corrupt junk in the gates dir is skipped, not fatal.
        std::fs::write(Gates::dir(dir.path()).join("junk.json"), "{nope").unwrap();

        let open = Gates::list_open(dir.path());

        assert_eq!(open.len(), 1);
        assert_eq!(open[0].phase, PhaseId::new(7));
        assert_eq!(open[0].stage, Stage::Ship);
        assert_eq!(open[0].context, "approve merge?");
    }

    #[test]
    fn list_open_is_empty_without_gates_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Gates::list_open(dir.path()).is_empty());
    }

    #[test]
    fn list_open_ignores_a_planted_temp() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(48);
        Gates::write_gate(dir.path(), phase, Stage::Code, "real gate").unwrap();
        let gate = Gates::gate_path(dir.path(), phase, Stage::Code);
        let orphan = gate.with_file_name(format!(
            ".{}.1.0.tmp",
            gate.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&orphan, "orphan").unwrap();

        let open = Gates::list_open(dir.path());

        assert_eq!(open.len(), 1);
        assert_eq!(open[0].phase, phase);
        assert_eq!(open[0].stage, Stage::Code);
        assert!(orphan.exists());
    }

    #[test]
    fn write_gate_still_overwrites_a_leftover_request() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(48);
        Gates::write_gate(dir.path(), phase, Stage::Code, "old request").unwrap();

        Gates::write_gate(dir.path(), phase, Stage::Code, "replacement request").unwrap();

        let gate: GateFile = serde_json::from_str(
            &std::fs::read_to_string(Gates::gate_path(dir.path(), phase, Stage::Code)).unwrap(),
        )
        .unwrap();
        assert_eq!(gate.context, "replacement request");
    }

    #[test]
    fn a_gate_write_publish_and_cleanup_fsync_the_gates_directory() {
        use crate::durable::{Synced, take_synced};
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(48);
        let stage = Stage::Code;
        let gates_dir = Synced::Dir(Gates::dir(dir.path()));
        let response = Gates::response_path(dir.path(), phase, stage);

        Gates::write_gate(dir.path(), phase, stage, "ctx").unwrap();
        let synced = take_synced();
        assert!(synced.contains(&gates_dir), "write_gate: {synced:?}");

        publish_response_exclusive(&response, "{}", phase, stage).unwrap();
        let synced = take_synced();
        assert!(synced.contains(&gates_dir), "publish: {synced:?}");

        assert!(Gates::cleanup(dir.path(), phase, stage).unwrap());
        assert_eq!(
            take_synced(),
            [gates_dir.clone(), gates_dir],
            "one directory fsync per removed file: the gate and the response"
        );
    }

    #[test]
    fn second_publisher_past_the_existence_check_gets_already_responded() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(48);
        let stage = Stage::Code;
        let path = Gates::response_path(dir.path(), phase, stage);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let rejected = serde_json::to_string(&GateResponse {
            approved: false,
            note: Some("first answer".into()),
            responded_by: None,
        })
        .unwrap();
        let approved = serde_json::to_string(&GateResponse {
            approved: true,
            note: Some("second answer".into()),
            responded_by: None,
        })
        .unwrap();

        publish_response_exclusive(&path, &rejected, phase, stage).unwrap();
        let error = publish_response_exclusive(&path, &approved, phase, stage).unwrap_err();

        assert!(
            matches!(error, GateError::AlreadyResponded { phase: actual, stage: actual_stage } if actual == phase && actual_stage == stage)
        );
        let response: GateResponse =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(!response.approved);
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(
            std::fs::read_dir(path.parent().unwrap())
                .unwrap()
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .all(|entry| !entry.starts_with(&format!(".{name}."))),
            "exclusive publication cleans up its temp on both outcomes"
        );
    }

    #[test]
    fn single_responder_still_publishes() {
        let dir = tempfile::tempdir().unwrap();
        let phase = PhaseId::new(48);
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: None,
        };
        Gates::write_gate(dir.path(), phase, Stage::Code, "ctx").unwrap();

        Gates::respond(dir.path(), phase, Stage::Code, &response).unwrap();

        let actual: GateResponse = serde_json::from_str(
            &std::fs::read_to_string(Gates::response_path(dir.path(), phase, Stage::Code)).unwrap(),
        )
        .unwrap();
        assert_eq!(actual, response);
    }

    /// 15a: `respond` is the programmatic answer path — it must round-trip
    /// through the same file `poll_response` reads.
    #[test]
    fn respond_writes_a_response_poll_response_consumes() {
        let dir = tempfile::tempdir().unwrap();
        Gates::write_gate(dir.path(), PhaseId::new(9), Stage::Ship, "ctx").unwrap();
        let response = GateResponse {
            approved: false,
            note: Some("abort: nope".into()),
            responded_by: Some("cli".into()),
        };

        Gates::respond(dir.path(), PhaseId::new(9), Stage::Ship, &response).unwrap();

        let polled = Gates::poll_response(dir.path(), PhaseId::new(9), Stage::Ship, 1).unwrap();
        assert_eq!(polled, response);
        assert!(matches!(
            GateAction::from_response(&polled),
            GateAction::Abort(_)
        ));
    }

    #[test]
    fn respond_refuses_when_no_gate_is_open() {
        let dir = tempfile::tempdir().unwrap();
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: None,
        };
        let err = Gates::respond(dir.path(), PhaseId::new(3), Stage::Ship, &response).unwrap_err();
        assert!(matches!(err, GateError::NoOpenGate { phase, .. } if phase == PhaseId::new(3)));
    }

    #[test]
    fn respond_refuses_to_clobber_unconsumed_response() {
        let dir = tempfile::tempdir().unwrap();
        Gates::write_gate(dir.path(), PhaseId::new(4), Stage::Validate, "ctx").unwrap();
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: None,
        };
        Gates::respond(dir.path(), PhaseId::new(4), Stage::Validate, &response).unwrap();

        let err =
            Gates::respond(dir.path(), PhaseId::new(4), Stage::Validate, &response).unwrap_err();
        assert!(
            matches!(err, GateError::AlreadyResponded { phase, .. } if phase == PhaseId::new(4))
        );
    }

    #[test]
    fn gate_action_advances_on_approval() {
        let response = GateResponse {
            approved: true,
            note: None,
            responded_by: None,
        };
        assert_eq!(GateAction::from_response(&response), GateAction::Advance);
    }

    #[test]
    fn gate_action_loops_back_on_fixable_rejection() {
        let response = GateResponse {
            approved: false,
            note: Some("fix the failing test".into()),
            responded_by: None,
        };
        assert_eq!(
            GateAction::from_response(&response),
            GateAction::LoopBack(Stage::Code)
        );
    }

    #[test]
    fn gate_action_aborts_when_note_says_abort() {
        let response = GateResponse {
            approved: false,
            note: Some("abort: requirements changed".into()),
            responded_by: None,
        };
        assert!(matches!(
            GateAction::from_response(&response),
            GateAction::Abort(_)
        ));
    }

    /// `run_notify_command` takes the command string as an argument (not an
    /// env var), so this test needs no env mutation and cannot race other
    /// tests.
    #[test]
    fn notify_hook_runs_configured_command() {
        let dir = tempfile::tempdir().unwrap();
        let sentinel = dir.path().join("sentinel");
        let cmd = format!("touch {}", sentinel.display());
        run_notify_command(&cmd, PhaseId::new(11), Stage::Ship, "ctx", false);
        assert!(sentinel.exists());
    }

    #[test]
    fn notify_hook_failure_is_fail_soft() {
        // A command that always fails must not panic or otherwise abort the
        // caller — fail-soft per T-13-02.
        run_notify_command("exit 1", PhaseId::new(11), Stage::Ship, "ctx", false);
    }

    #[test]
    fn notify_hook_sets_non_silent_flag() {
        let dir = tempfile::tempdir().unwrap();

        let sentinel_unexpected = dir.path().join("unexpected");
        let cmd_unexpected = format!(
            "echo -n \"$DEVFLOW_NON_SILENT_GATE\" > {}",
            sentinel_unexpected.display()
        );
        run_notify_command(&cmd_unexpected, PhaseId::new(11), Stage::Code, "ctx", true);
        assert_eq!(std::fs::read_to_string(&sentinel_unexpected).unwrap(), "1");

        let sentinel_expected = dir.path().join("expected");
        let cmd_expected = format!(
            "echo -n \"$DEVFLOW_NON_SILENT_GATE\" > {}",
            sentinel_expected.display()
        );
        run_notify_command(&cmd_expected, PhaseId::new(11), Stage::Ship, "ctx", false);
        assert_eq!(std::fs::read_to_string(&sentinel_expected).unwrap(), "0");
    }

    /// CR-03 (49-REVIEW.md): one persistence rule for start, resume, advance.
    #[test]
    fn persisted_notify_cmd_follows_the_environment_rule() {
        use std::ffi::OsStr;
        let saved = || Some("saved".to_string());
        assert_eq!(
            persisted_notify_cmd_after_env(saved(), None).as_deref(),
            Some("saved")
        );
        assert_eq!(
            persisted_notify_cmd_after_env(saved(), Some(OsStr::new(""))),
            None
        );
        assert_eq!(
            persisted_notify_cmd_after_env(saved(), Some(OsStr::new("new"))).as_deref(),
            Some("new")
        );
        assert_eq!(
            persisted_notify_cmd_after_env(None, Some(OsStr::new("new"))).as_deref(),
            Some("new")
        );
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            persisted_notify_cmd_after_env(saved(), Some(OsStr::from_bytes(b"\xff"))).as_deref(),
            Some("saved")
        );
    }

    /// WR-01 (49-REVIEW.md): set-and-empty disables even when a command is
    /// persisted; a set value overrides; only an unset variable falls back.
    #[test]
    fn notify_cmd_resolution_has_three_environment_states() {
        use std::ffi::OsStr;
        let persisted = Some("persisted");
        assert_eq!(
            resolve_gate_notify_cmd(Some(OsStr::new("")), persisted),
            None
        );
        assert_eq!(
            resolve_gate_notify_cmd(Some(OsStr::new("override")), persisted).as_deref(),
            Some("override")
        );
        assert_eq!(
            resolve_gate_notify_cmd(None, persisted).as_deref(),
            Some("persisted")
        );
        assert_eq!(resolve_gate_notify_cmd(None, Some("")), None);
        assert_eq!(resolve_gate_notify_cmd(None, None), None);
        // IN-08: non-UTF-8 counts as unset.
        use std::os::unix::ffi::OsStrExt;
        let invalid = OsStr::from_bytes(b"touch \xff");
        assert_eq!(
            resolve_gate_notify_cmd(Some(invalid), persisted).as_deref(),
            Some("persisted")
        );
    }

    /// This test mutates process-global env, so it acquires `ENV_MUTEX` to
    /// avoid racing any other env-touching test in this module.
    #[test]
    // D-09 (48-09): mutates `DEVFLOW_GATE_NOTIFY_CMD` process-wide.
    // Deferred deliberately, not overlooked: THIS process reads the value, so
    // per-`Command` scoping would not reach the reader. ENV_MUTEX bounds the
    // race and the value is restored on every exit path, unwinding included.
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn notify_hook_unset_is_noop() {
        let _guard = ENV_MUTEX.lock().unwrap();
        // SAFETY: serialized under ENV_MUTEX — no other thread in this
        // process reads/writes DEVFLOW_GATE_NOTIFY_CMD concurrently.
        unsafe {
            std::env::remove_var("DEVFLOW_GATE_NOTIFY_CMD");
        }
        // Must return normally without touching the filesystem or panicking.
        fire_gate_notify(PhaseId::new(11), Stage::Ship, "ctx", false);
    }

    #[test]
    #[expect(clippy::disallowed_methods, reason = "test-only; ENV_MUTEX-guarded")]
    fn notify_hook_fallback_runs_when_env_unset() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::remove_var("DEVFLOW_GATE_NOTIFY_CMD");
        }
        let dir = tempfile::tempdir().unwrap();
        let sentinel = dir.path().join("sentinel");
        let cmd = format!("touch {}", sentinel.display());
        fire_gate_notify_with_fallback(PhaseId::new(11), Stage::Ship, "ctx", false, Some(&cmd));
        assert!(sentinel.exists());
    }

    /// R-2 (50-13): a cancel set during the wait ends it within a second,
    /// even once the backoff has grown past a second. The cancel is set
    /// 3.2 s in, where `poll_response`'s own schedule (sleeps of 1, 2, then
    /// 4 s) would sleep until 7 s; the 60 s timeout rules out a timeout.
    #[test]
    fn poll_response_cancellable_returns_abort_within_a_second_of_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let setter = std::sync::Arc::clone(&cancelled);
        let set_at = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(3200));
            setter.store(true, std::sync::atomic::Ordering::SeqCst);
            std::time::Instant::now()
        });
        let got = Gates::poll_response_cancellable(
            dir.path(),
            PhaseId::new(13),
            Stage::Define,
            60,
            || cancelled.load(std::sync::atomic::Ordering::SeqCst),
        );
        let returned = std::time::Instant::now();
        let set_at = set_at.join().unwrap();
        assert!(got.is_none(), "a cancelled wait has no response: {got:?}");
        let after_cancel = returned.saturating_duration_since(set_at);
        assert!(
            after_cancel < std::time::Duration::from_millis(1500),
            "returned {after_cancel:?} after the cancel, not within 1.5 s"
        );
    }

    /// The control: never cancelled, the wait returns the response that
    /// appears, and with no response it times out like `poll_response`.
    #[test]
    fn poll_response_cancellable_returns_the_response_when_not_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(13), Stage::Define);
        Gates::write_gate(dir.path(), phase, stage, "ctx").unwrap();
        let response = GateResponse {
            approved: true,
            note: Some("go".into()),
            responded_by: Some("human".into()),
        };
        let root = dir.path().to_path_buf();
        let answer = response.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1300));
            Gates::respond(&root, phase, stage, &answer).unwrap();
        });
        let got = Gates::poll_response_cancellable(dir.path(), phase, stage, 30, || false);
        writer.join().unwrap();
        assert_eq!(got, Some(response));

        let empty = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();
        assert!(
            Gates::poll_response_cancellable(empty.path(), phase, Stage::Ship, 1, || false)
                .is_none()
        );
        assert!(
            started.elapsed() >= std::time::Duration::from_secs(1),
            "an uncancelled wait runs to its timeout"
        );
    }

    fn approved() -> GateResponse {
        GateResponse {
            approved: true,
            note: None,
            responded_by: Some("human".into()),
        }
    }

    /// The events named `name` in the project's event log, in order.
    fn events_named(root: &Path, name: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(crate::events::events_path(root))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|event| event["event"] == name)
            .collect()
    }

    /// Every quarantine file under `root`, sorted by name.
    fn quarantine_files(root: &Path) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(Gates::dir(root))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().ends_with(".quarantined.json"))
            .collect();
        files.sort();
        files
    }

    /// Hold the phase's gate lock through a descriptor of the test's own, as
    /// another process would.
    fn hold_gate_lock(root: &Path, phase: PhaseId) -> std::fs::File {
        let path = root
            .join(".devflow")
            .join(format!(".gates-{}.lock", phase.padded()));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .unwrap();
        file.lock().unwrap();
        file
    }

    fn read_gate(root: &Path, phase: PhaseId, stage: Stage) -> GateFile {
        serde_json::from_str(
            &std::fs::read_to_string(Gates::gate_path(root, phase, stage)).unwrap(),
        )
        .unwrap()
    }

    /// D-04, D-05: a gate has an id, an answer names it, and consuming the answer
    /// is one first-wins claim.
    #[test]
    fn a_gate_answered_for_its_own_id_is_read_bound_and_claimed_once() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);

        let opened = Gates::open_gate(dir.path(), phase, stage, "ctx").unwrap();
        let gate = opened.gate_id.clone();
        assert!(is_gate_id(gate.as_str()), "{gate} is not a gate id");
        assert_eq!(opened.path, Gates::gate_path(dir.path(), phase, stage));
        assert_eq!(
            read_gate(dir.path(), phase, stage).gate_id,
            gate.as_str(),
            "the gate file must name the id open_gate returned"
        );

        let path = Gates::answer(dir.path(), phase, stage, &gate, &approved()).unwrap();
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            format!("07-code.{gate}.response.json")
        );
        let body: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(body["gate_id"], gate.as_str());
        assert_eq!(body["approved"], true);

        assert_eq!(
            Gates::read_answer(dir.path(), phase, stage, &gate),
            AnswerRead::Bound(GateAnswer {
                gate_id: gate.to_string(),
                response: approved(),
            })
        );
        assert_eq!(
            Gates::claim(dir.path(), phase, stage, &gate).unwrap(),
            Claim::Claimed
        );
        assert!(Gates::claim_path(dir.path(), phase, stage, &gate).exists());
        assert_eq!(
            Gates::claim(dir.path(), phase, stage, &gate).unwrap(),
            Claim::AlreadyConsumed,
            "a second claim of one answer must not consume it again"
        );
    }

    #[test]
    fn two_gates_opened_at_one_stage_get_different_ids() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);

        let first = Gates::open_gate(dir.path(), phase, stage, "first").unwrap();
        let second = Gates::open_gate(dir.path(), phase, stage, "second").unwrap();

        assert_ne!(first.gate_id, second.gate_id);
        assert_eq!(
            read_gate(dir.path(), phase, stage).gate_id,
            second.gate_id.as_str()
        );
    }

    #[test]
    fn write_gate_still_returns_its_path_and_now_writes_an_id() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);

        let path = Gates::write_gate(dir.path(), phase, stage, "ctx").unwrap();

        assert_eq!(path, Gates::gate_path(dir.path(), phase, stage));
        let gate = read_gate(dir.path(), phase, stage);
        assert!(
            is_gate_id(&gate.gate_id),
            "write_gate must write a valid id, wrote {:?}",
            gate.gate_id
        );
        Gates::respond(dir.path(), phase, stage, &approved()).unwrap();
        assert_eq!(
            Gates::poll_response(dir.path(), phase, stage, 1),
            Some(approved())
        );
    }

    /// V-15, both halves: an id-less answer is set aside under a name of its own,
    /// byte for byte, and a valid answer to the live gate is then accepted.
    #[test]
    fn an_answer_without_a_gate_id_is_quarantined_and_a_valid_one_is_then_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer_path(dir.path(), phase, stage, &gate);
        let body = "{\"approved\": true}";
        std::fs::write(&slot, body).unwrap();
        assert!(matches!(
            Gates::read_answer(dir.path(), phase, stage, &gate),
            AnswerRead::Unbound(_)
        ));

        let moved = Gates::set_aside_unbound(dir.path(), phase, stage, &gate).unwrap();

        let moved = moved.expect("an id-less answer must be set aside");
        let name = moved.file_name().unwrap().to_string_lossy().into_owned();
        let fields: Vec<&str> = name
            .strip_prefix("07-code.noid.")
            .and_then(|rest| rest.strip_suffix(".quarantined.json"))
            .unwrap_or_else(|| panic!("not a noid quarantine name: {name}"))
            .split('.')
            .collect();
        assert_eq!(fields.len(), 3, "secs, pid and seq: {name}");
        assert_eq!(fields[1], std::process::id().to_string(), "{name}");
        assert!(!slot.exists());
        assert_eq!(std::fs::read_to_string(&moved).unwrap(), body);
        let events = events_named(dir.path(), "gate_answer_quarantined");
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0]["reason"], "missing_gate_id");

        Gates::answer(dir.path(), phase, stage, &gate, &approved()).unwrap();
        assert!(matches!(
            Gates::read_answer(dir.path(), phase, stage, &gate),
            AnswerRead::Bound(_)
        ));
    }

    /// D-05, the writer's half: a refused answer never blocks a valid one, and a
    /// bound answer is never moved: the first answer wins.
    #[test]
    fn answer_sets_aside_an_unbound_answer_in_its_own_slot_before_publishing() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer_path(dir.path(), phase, stage, &gate);
        let id_less = "{\"approved\": false}";
        std::fs::write(&slot, id_less).unwrap();

        let answered = Gates::answer(dir.path(), phase, stage, &gate, &approved());

        assert!(answered.is_ok(), "{answered:?}");
        let quarantined = quarantine_files(dir.path());
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(std::fs::read_to_string(&quarantined[0]).unwrap(), id_less);
        assert_eq!(
            events_named(dir.path(), "gate_answer_quarantined")[0]["reason"],
            "missing_gate_id"
        );
        assert_eq!(
            Gates::read_answer(dir.path(), phase, stage, &gate),
            AnswerRead::Bound(GateAnswer {
                gate_id: gate.to_string(),
                response: approved(),
            })
        );

        let bound = std::fs::read(&slot).unwrap();
        let rejection = GateResponse {
            approved: false,
            note: Some("second answer".into()),
            responded_by: None,
        };
        let second = Gates::answer(dir.path(), phase, stage, &gate, &rejection);
        assert!(
            matches!(second, Err(GateError::AlreadyResponded { .. })),
            "{second:?}"
        );
        assert_eq!(std::fs::read(&slot).unwrap(), bound);
        assert_eq!(quarantine_files(dir.path()).len(), 1);
    }

    /// R-09: setting an answer aside touches the given gate's slot and nothing else.
    #[test]
    fn an_answer_for_another_gate_id_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let old = GateId::new();
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer_path(dir.path(), phase, stage, &gate);
        let wrong_body = serde_json::to_string(&GateAnswer {
            gate_id: old.to_string(),
            response: approved(),
        })
        .unwrap();
        std::fs::write(&slot, &wrong_body).unwrap();
        let old_slot = Gates::answer_path(dir.path(), phase, stage, &old);
        std::fs::write(&old_slot, &wrong_body).unwrap();
        let legacy = Gates::response_path(dir.path(), phase, stage);
        std::fs::write(&legacy, "{\"approved\":true}").unwrap();

        let moved = Gates::set_aside_unbound(dir.path(), phase, stage, &gate).unwrap();

        let moved = moved.expect("an answer naming another gate must be set aside");
        let name = moved.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with(&format!("07-code.{old}.")), "{name}");
        assert!(!slot.exists());
        let events = events_named(dir.path(), "gate_answer_quarantined");
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0]["reason"], "other_gate");
        assert_eq!(events[0]["gate_id"], old.as_str());
        assert_eq!(std::fs::read_to_string(&old_slot).unwrap(), wrong_body);
        assert_eq!(
            std::fs::read_to_string(&legacy).unwrap(),
            "{\"approved\":true}"
        );

        let refused = Gates::set_aside_unbound(dir.path(), phase, stage, &old);
        assert!(
            matches!(&refused, Err(GateError::GateIdMismatch { observed, current, .. })
                if observed == old.as_str() && current == gate.as_str()),
            "{refused:?}"
        );
        assert_eq!(std::fs::read_to_string(&old_slot).unwrap(), wrong_body);
        assert_eq!(quarantine_files(dir.path()).len(), 1);
    }

    #[test]
    fn answer_refuses_a_gate_id_the_gate_file_does_not_name() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let old = GateId::new();
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;

        let refused = Gates::answer(dir.path(), phase, stage, &old, &approved());

        assert!(
            matches!(&refused, Err(GateError::GateIdMismatch { observed, current, .. })
                if observed == old.as_str() && current == gate.as_str()),
            "{refused:?}"
        );
        assert!(!Gates::answer_path(dir.path(), phase, stage, &old).exists());

        let no_gate = Gates::answer(dir.path(), phase, Stage::Validate, &old, &approved());
        assert!(
            matches!(no_gate, Err(GateError::NoOpenGate { .. })),
            "{no_gate:?}"
        );
        let no_gates_dir = tempfile::tempdir().unwrap();
        let no_dir = Gates::answer(no_gates_dir.path(), phase, stage, &old, &approved());
        assert!(
            matches!(no_dir, Err(GateError::NoOpenGate { .. })),
            "{no_dir:?}"
        );
    }

    /// V-18 (AR-48-04 path b): a writer past its pre-check while the gate closes and
    /// the next opens cannot decide the next gate.
    #[test]
    fn a_late_respond_after_cleanup_cannot_decide_the_next_gate() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let first = Gates::open_gate(dir.path(), phase, stage, "first")
            .unwrap()
            .gate_id;
        let mut next = None;

        let late = Gates::answer_with_hook(dir.path(), phase, stage, &first, &approved(), || {
            next = Some(
                Gates::open_gate(dir.path(), phase, stage, "next")
                    .unwrap()
                    .gate_id,
            );
        });

        assert!(
            matches!(&late, Err(GateError::GateClosedAfterPublish { observed, .. })
                if observed == first.as_str()),
            "{late:?}"
        );
        let quarantined = quarantine_files(dir.path());
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        let name = quarantined[0].file_name().unwrap().to_string_lossy();
        assert!(name.starts_with(&format!("07-code.{first}.")), "{name}");
        assert_eq!(
            events_named(dir.path(), "gate_answer_quarantined")[0]["reason"],
            "gate_closed"
        );
        assert!(!Gates::answer_path(dir.path(), phase, stage, &first).exists());
        assert_eq!(
            Gates::read_answer(dir.path(), phase, stage, &next.unwrap()),
            AnswerRead::None
        );
    }

    /// T-50.1-20: an id in a body is untrusted, so no path is built from one that
    /// fails `is_gate_id`.
    #[test]
    fn a_hostile_gate_id_in_an_answer_is_quarantined_as_noid() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        std::fs::write(
            Gates::answer_path(dir.path(), phase, stage, &gate),
            "{\"gate_id\": \"../../x\", \"approved\": true}",
        )
        .unwrap();

        let moved = Gates::set_aside_unbound(dir.path(), phase, stage, &gate).unwrap();

        let moved = moved.expect("a hostile id must be set aside");
        assert_eq!(moved.parent().unwrap(), Gates::dir(dir.path()));
        let name = moved.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("07-code.noid."), "{name}");
        let at_root: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(at_root, [".devflow"], "nothing outside .devflow");
        assert!(!dir.path().join(".devflow").join("x").exists());
    }

    /// R-11: a counter private to each process would give two fresh processes the
    /// same quarantine name in one second, and the second would overwrite the first.
    #[test]
    fn two_processes_quarantining_in_one_second_keep_both_answers() {
        const NAME: &str =
            "gates::tests::two_processes_quarantining_in_one_second_keep_both_answers";
        const ONE_SECOND: u64 = 1_700_000_000;
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        if crate::test_support::in_child_test(NAME) {
            let root = PathBuf::from(std::env::var_os("QUARANTINE_CHILD_ROOT").unwrap());
            let legacy = Gates::response_path(&root, phase, stage);
            set_aside(
                &root,
                phase,
                stage,
                &legacy,
                "noid",
                "legacy_unkeyed",
                ONE_SECOND,
            )
            .unwrap();
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let legacy = Gates::response_path(dir.path(), phase, stage);
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        let path_dir = tempfile::tempdir().unwrap();
        let run_child = || {
            let output = crate::test_support::run_test_in_child(
                NAME,
                path_dir.path(),
                &[("QUARANTINE_CHILD_ROOT", dir.path().as_os_str())],
            );
            crate::test_support::assert_child_ran_exactly_one_passing_test(&output, NAME);
        };

        std::fs::write(&legacy, "first").unwrap();
        run_child();
        std::fs::write(&legacy, "second").unwrap();
        run_child();

        let files = quarantine_files(dir.path());
        let mut bodies: Vec<String> = files
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect();
        bodies.sort();
        assert_eq!(bodies, ["first", "second"], "{files:?}");
        for file in &files {
            let name = file.file_name().unwrap().to_string_lossy();
            let fields = name
                .strip_prefix(&format!("07-code.noid.{ONE_SECOND}."))
                .and_then(|rest| rest.strip_suffix(".quarantined.json"))
                .unwrap_or_else(|| panic!("not a quarantine name: {name}"));
            assert_eq!(fields.split('.').count(), 2, "pid and seq: {name}");
        }
    }

    /// T-50.1-118: a stopped gate-lock holder cannot hang a gate operation.
    #[test]
    fn a_gate_operation_gives_up_on_a_held_gate_lock_after_its_bound() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer_path(dir.path(), phase, stage, &gate);
        std::fs::write(&slot, "{\"approved\": true}").unwrap();
        let holder = hold_gate_lock(dir.path(), phase);

        let started = std::time::Instant::now();
        let timed_out = Gates::set_aside_unbound(dir.path(), phase, stage, &gate);
        let took = started.elapsed();

        assert!(
            matches!(timed_out, Err(GateError::GateLockTimeout { .. })),
            "{timed_out:?}"
        );
        assert!(took >= GATE_LOCK_WAIT, "gave up after {took:?}");
        assert!(
            took < GATE_LOCK_WAIT + Duration::from_secs(2),
            "waited {took:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&slot).unwrap(),
            "{\"approved\": true}"
        );

        drop(holder);
        let moved = Gates::set_aside_unbound(dir.path(), phase, stage, &gate).unwrap();
        assert!(moved.is_some(), "the control: a free lock moves the body");
        assert!(!slot.exists());
    }

    /// R2-04: after the publish, "nothing was changed" would be false.
    #[test]
    fn an_answer_whose_recheck_cannot_take_the_gate_lock_is_published_unconfirmed() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let mut parked = None;

        let started = std::time::Instant::now();
        let unconfirmed =
            Gates::answer_with_hook(dir.path(), phase, stage, &gate, &approved(), || {
                parked = Some(hold_gate_lock(dir.path(), phase));
            });
        let took = started.elapsed();

        let error = unconfirmed.unwrap_err();
        assert!(
            matches!(&error, GateError::PublishedUnconfirmed { observed, .. }
                if observed == gate.as_str()),
            "{error:?}"
        );
        assert!(took >= GATE_LOCK_WAIT, "gave up after {took:?}");
        assert!(
            took < GATE_LOCK_WAIT + Duration::from_secs(2),
            "waited {took:?}"
        );
        let message = error.to_string();
        assert!(message.contains("published"), "{message}");
        assert!(message.contains(gate.as_str()), "{message}");
        assert!(!message.contains("nothing was changed"), "{message}");
        assert!(Gates::answer_path(dir.path(), phase, stage, &gate).exists());
        assert!(quarantine_files(dir.path()).is_empty());
        drop(parked);
        assert!(matches!(
            Gates::read_answer(dir.path(), phase, stage, &gate),
            AnswerRead::Bound(answer) if answer.response == approved()
        ));

        // The control: held before the call, the writer publishes nothing and says so.
        let other = Stage::Validate;
        let second = Gates::open_gate(dir.path(), phase, other, "ctx")
            .unwrap()
            .gate_id;
        let holder = hold_gate_lock(dir.path(), phase);
        let error = Gates::answer(dir.path(), phase, other, &second, &approved()).unwrap_err();
        assert!(
            matches!(error, GateError::GateLockTimeout { .. }),
            "{error:?}"
        );
        assert!(error.to_string().contains("nothing was changed"), "{error}");
        assert!(!Gates::answer_path(dir.path(), phase, other, &second).exists());
        drop(holder);
    }

    /// D-08 operator ruling (2026-10-26): the bound stays below the repair verbs'.
    #[test]
    fn the_gate_lock_wait_stays_below_the_write_lock_repair_wait() {
        let repair = crate::workflow::WRITE_LOCK_REPAIR_WAIT;
        assert!(
            GATE_LOCK_WAIT < repair,
            "GATE_LOCK_WAIT {GATE_LOCK_WAIT:?} must stay below WRITE_LOCK_REPAIR_WAIT {repair:?}"
        );
    }

    /// The `workflow` items `gates.rs` may name, none of which takes the state write
    /// lock.
    const LOCK_FREE_WORKFLOW_ITEMS: &[&str] = &[
        // Creates a directory and its marker file.
        "ensure_devflow_dir",
        // A constant: the gate lock's poll interval.
        "WRITE_LOCK_POLL",
    ];

    /// Every `workflow` path in `source`'s non-test, non-comment lines that is not
    /// on [`LOCK_FREE_WORKFLOW_ITEMS`]; a glob, a brace import or a rename counts,
    /// since what it brings in cannot be read off the line.
    fn state_lock_paths(source: &str) -> Vec<String> {
        let mut paths = Vec::new();
        for line in source.lines().take_while(|line| *line != "mod tests {") {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if line.contains("workflow as") {
                paths.push(line.trim().to_string());
            }
            for (at, _) in line.match_indices("workflow::") {
                let rest = &line[at + "workflow::".len()..];
                let end = rest
                    .find(|ch: char| !ch.is_alphanumeric() && ch != '_')
                    .unwrap_or(rest.len());
                if !LOCK_FREE_WORKFLOW_ITEMS.contains(&&rest[..end]) {
                    paths.push(line[at..].trim().to_string());
                }
            }
        }
        paths
    }

    /// D-08 operator ruling (2026-10-26): the order is always write lock, then gate
    /// lock, so nothing holding the gate lock may take the write lock.
    ///
    /// Limit: this sees only `workflow::` paths written in `gates.rs`'s own text. A
    /// call into a helper outside `workflow` (or a re-export under another name) that
    /// itself takes the state write lock passes this scan; that case rests on review
    /// of each gate-lock holder's body, which stays file operations only.
    #[test]
    fn gates_never_takes_the_state_write_lock() {
        let source = include_str!("gates.rs");
        assert!(
            source.lines().any(|line| line == "mod tests {"),
            "the scan stops at this file's test module"
        );
        assert_eq!(state_lock_paths(source), Vec::<String>::new());

        assert_eq!(
            state_lock_paths("crate::workflow::update_state(root, phase, |s| Ok(s))\n").len(),
            1
        );
        assert_eq!(
            state_lock_paths("use crate::workflow::update_owned_state;\n").len(),
            1
        );
        assert_eq!(state_lock_paths("use crate::workflow::{a, b};\n").len(), 1);
        assert_eq!(state_lock_paths("use crate::workflow::*;\n").len(), 1);
        assert_eq!(state_lock_paths("use crate::workflow as w;\n").len(), 1);
        assert_eq!(
            state_lock_paths("    // crate::workflow::update_state(root)\n").len(),
            0,
            "a whole-line comment is not code"
        );
        assert_eq!(
            state_lock_paths("mod tests {\ncrate::workflow::update_state(root)\n").len(),
            0,
            "test code is out of scope"
        );
    }

    fn rejection() -> GateResponse {
        GateResponse {
            approved: false,
            note: Some("rejected".into()),
            responded_by: None,
        }
    }

    /// Plant an answered and claimed pair for `gate`, as a consumer leaves it.
    fn plant_claimed_pair(root: &Path, phase: PhaseId, stage: Stage, gate: &GateId) -> GateAnswer {
        let answer = GateAnswer {
            gate_id: gate.to_string(),
            response: rejection(),
        };
        std::fs::write(
            Gates::answer_path(root, phase, stage, gate),
            serde_json::to_string(&answer).unwrap(),
        )
        .unwrap();
        std::fs::write(
            Gates::claim_path(root, phase, stage, gate),
            serde_json::to_string(&GateAck { received: true }).unwrap(),
        )
        .unwrap();
        answer
    }

    #[test]
    fn closing_a_gate_sets_aside_unconsumed_answers_and_removes_consumed_ones() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        Gates::answer(dir.path(), phase, stage, &gate, &approved()).unwrap();
        let old = GateId::new();
        let old_answer = plant_claimed_pair(dir.path(), phase, stage, &old);
        let gates_dir = Gates::dir(dir.path());
        let temps = [
            gates_dir.join(format!(".07-code.{gate}.response.json.1.0.tmp")),
            gates_dir.join(".07-code.json.1.0.tmp"),
        ];
        let hold_temp = gates_dir.join(format!(".07-code.{gate}.waiter.1.0.hold-tmp"));
        let other_stage_temp = gates_dir.join(".07-validate.json.1.0.tmp");
        for path in temps.iter().chain([&hold_temp, &other_stage_temp]) {
            std::fs::write(path, "temp").unwrap();
        }

        let report = Gates::close(dir.path(), phase, stage).unwrap();

        assert!(report.removed);
        assert_eq!(report.quarantined.len(), 1, "{report:?}");
        let name = report.quarantined[0].file_name().unwrap().to_string_lossy();
        assert!(name.starts_with(&format!("07-code.{gate}.")), "{name}");
        assert_eq!(
            events_named(dir.path(), "gate_answer_quarantined")[0]["reason"],
            "gate_closed"
        );
        assert_eq!(
            report.claimed,
            [ClaimedAnswer {
                gate_id: old.to_string(),
                answer: Some(old_answer),
            }]
        );
        assert!(!Gates::answer_path(dir.path(), phase, stage, &old).exists());
        assert!(!Gates::claim_path(dir.path(), phase, stage, &old).exists());
        assert!(!Gates::answer_path(dir.path(), phase, stage, &gate).exists());
        assert!(!Gates::gate_path(dir.path(), phase, stage).exists());
        assert!(temps.iter().all(|path| !path.exists()), "{temps:?}");
        assert!(hold_temp.exists(), "a hold temp is never a durable temp");
        assert!(other_stage_temp.exists(), "another stage's temp stays");

        let again = Gates::close(dir.path(), phase, stage).unwrap();
        assert_eq!(
            again,
            CloseReport::default(),
            "a second close finds nothing"
        );
    }

    /// D-05 over R-02: a claim does not license deleting a body that never bound.
    #[test]
    fn a_claimed_body_that_does_not_bind_is_reported_and_set_aside_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        std::fs::write(
            Gates::answer_path(dir.path(), phase, stage, &gate),
            "{\"approved\": true}",
        )
        .unwrap();
        assert_eq!(
            Gates::claim(dir.path(), phase, stage, &gate).unwrap(),
            Claim::Claimed
        );

        let report = Gates::close(dir.path(), phase, stage).unwrap();

        assert_eq!(
            report.claimed,
            [ClaimedAnswer {
                gate_id: gate.to_string(),
                answer: None,
            }]
        );
        let quarantined = quarantine_files(dir.path());
        assert_eq!(quarantined.len(), 1, "{quarantined:?}");
        assert_eq!(
            std::fs::read_to_string(&quarantined[0]).unwrap(),
            "{\"approved\": true}"
        );
        assert!(!Gates::claim_path(dir.path(), phase, stage, &gate).exists());
    }

    /// R-08: a gate id is never reused, so once its gate file is gone or replaced no
    /// claim of it can succeed: a paused `devflow ship` cannot finalize a second time.
    #[test]
    fn a_paused_consumer_cannot_claim_a_gate_that_closed_or_was_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let first = Gates::open_gate(dir.path(), phase, stage, "first")
            .unwrap()
            .gate_id;
        Gates::answer(dir.path(), phase, stage, &first, &approved()).unwrap();
        assert!(
            matches!(
                Gates::read_answer(dir.path(), phase, stage, &first),
                AnswerRead::Bound(_)
            ),
            "the reader pauses here, holding a bound answer"
        );
        assert_eq!(
            Gates::claim(dir.path(), phase, stage, &first).unwrap(),
            Claim::Claimed
        );
        Gates::close(dir.path(), phase, stage).unwrap();
        let next = Gates::open_gate(dir.path(), phase, stage, "next")
            .unwrap()
            .gate_id;
        let next_gate_file = std::fs::read(Gates::gate_path(dir.path(), phase, stage)).unwrap();

        let paused = Gates::claim(dir.path(), phase, stage, &first);

        assert!(
            matches!(&paused, Err(GateError::GateIdMismatch { observed, current, .. })
                if observed == first.as_str() && current == next.as_str()),
            "{paused:?}"
        );
        assert!(!Gates::claim_path(dir.path(), phase, stage, &first).exists());
        assert_eq!(
            std::fs::read(Gates::gate_path(dir.path(), phase, stage)).unwrap(),
            next_gate_file
        );
        assert!(!Gates::claim_path(dir.path(), phase, stage, &next).exists());
        assert_eq!(
            Gates::read_answer(dir.path(), phase, stage, &next),
            AnswerRead::None
        );

        // A gate closed with none opened after it.
        let other = Stage::Validate;
        let closed = Gates::open_gate(dir.path(), phase, other, "ctx")
            .unwrap()
            .gate_id;
        Gates::close(dir.path(), phase, other).unwrap();
        let gone = Gates::claim(dir.path(), phase, other, &closed);
        assert!(
            matches!(gone, Err(GateError::NoOpenGate { .. })),
            "{gone:?}"
        );
        assert!(!Gates::claim_path(dir.path(), phase, other, &closed).exists());
    }

    /// R-02, a kill-point fixture: a consumer killed after its claim and before it
    /// recorded its decision leaves a record the next close reports.
    #[test]
    fn a_claim_whose_consumer_died_is_reported_when_its_gate_closes() {
        const NAME: &str =
            "gates::tests::a_claim_whose_consumer_died_is_reported_when_its_gate_closes";
        const KILLED_AFTER_CLAIM: i32 = 73;
        const MARKER: &str = "claimed-marker";
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let kept = GateResponse {
            approved: true,
            note: Some("kept".into()),
            responded_by: None,
        };
        if crate::test_support::in_child_test(NAME) {
            let root = PathBuf::from(std::env::var_os("CLAIM_KILL_ROOT").unwrap());
            let gate = Gates::open_gate(&root, phase, stage, "ctx")
                .unwrap()
                .gate_id;
            Gates::answer(&root, phase, stage, &gate, &kept).unwrap();
            assert_eq!(
                Gates::claim(&root, phase, stage, &gate).unwrap(),
                Claim::Claimed
            );
            std::fs::write(root.join(MARKER), "claimed").unwrap();
            std::process::exit(KILLED_AFTER_CLAIM);
        }
        let dir = tempfile::tempdir().unwrap();
        let path_dir = tempfile::tempdir().unwrap();

        let output = crate::test_support::run_test_in_child(
            NAME,
            path_dir.path(),
            &[("CLAIM_KILL_ROOT", dir.path().as_os_str())],
        );

        assert_eq!(output.status.code(), Some(KILLED_AFTER_CLAIM), "{output:?}");
        assert!(dir.path().join(MARKER).exists(), "the child died elsewhere");
        let gate = GateId::parse(&read_gate(dir.path(), phase, stage).gate_id).unwrap();
        let report = Gates::close(dir.path(), phase, stage).unwrap();
        assert_eq!(
            report.claimed,
            [ClaimedAnswer {
                gate_id: gate.to_string(),
                answer: Some(GateAnswer {
                    gate_id: gate.to_string(),
                    response: kept,
                }),
            }]
        );
        assert!(report.quarantined.is_empty(), "{report:?}");
        assert!(!Gates::answer_path(dir.path(), phase, stage, &gate).exists());
        assert!(!Gates::claim_path(dir.path(), phase, stage, &gate).exists());
        let events = events_named(dir.path(), "gate_claimed_answer_removed");
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0]["gate_id"], gate.as_str());
        assert_eq!(events[0]["approved"], true);
        assert_eq!(events[0]["note"], "kept");

        // The control: an answer nobody claimed is never reported as claimed.
        let unclaimed = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        Gates::answer(dir.path(), phase, stage, &unclaimed, &approved()).unwrap();
        let report = Gates::close(dir.path(), phase, stage).unwrap();
        assert_eq!(report.quarantined.len(), 1, "{report:?}");
        assert!(report.claimed.is_empty(), "{report:?}");
    }

    /// R2-05, a kill-point fixture: the gate file goes last and each pair's event
    /// precedes its removal, so a crash inside a close reports a pair twice at worst,
    /// never zero times.
    #[test]
    fn a_close_killed_mid_disposal_leaves_the_gate_file_and_the_unremoved_pair() {
        const NAME: &str =
            "gates::tests::a_close_killed_mid_disposal_leaves_the_gate_file_and_the_unremoved_pair";
        const KILLED_MID_DISPOSAL: i32 = 74;
        // Fixed and ordered, so the child removes the first pair before the second.
        const FIRST: &str = "00000000000000a0";
        const SECOND: &str = "00000000000000a1";
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let first = GateId::parse(FIRST).unwrap();
        let second = GateId::parse(SECOND).unwrap();
        if crate::test_support::in_child_test(NAME) {
            let root = PathBuf::from(std::env::var_os("CLOSE_KILL_ROOT").unwrap());
            Gates::open_gate(&root, phase, stage, "ctx").unwrap();
            plant_claimed_pair(&root, phase, stage, &first);
            plant_claimed_pair(&root, phase, stage, &second);
            let mut hook = |step: DisposalStep| {
                let DisposalStep::ClaimedEventEmitted(id) = step;
                if id == SECOND {
                    std::process::exit(KILLED_MID_DISPOSAL);
                }
            };
            let _ = Gates::close_with_partial_result_with(&root, phase, stage, &mut hook);
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path_dir = tempfile::tempdir().unwrap();

        let output = crate::test_support::run_test_in_child(
            NAME,
            path_dir.path(),
            &[("CLOSE_KILL_ROOT", dir.path().as_os_str())],
        );

        assert_eq!(
            output.status.code(),
            Some(KILLED_MID_DISPOSAL),
            "{output:?}"
        );
        assert!(
            Gates::gate_path(dir.path(), phase, stage).exists(),
            "the gate file must outlive the pairs"
        );
        assert!(Gates::answer_path(dir.path(), phase, stage, &second).exists());
        assert!(Gates::claim_path(dir.path(), phase, stage, &second).exists());
        assert!(!Gates::answer_path(dir.path(), phase, stage, &first).exists());
        let reported = |root: &Path| {
            events_named(root, "gate_claimed_answer_removed")
                .iter()
                .filter(|event| event["gate_id"] == SECOND)
                .count()
        };
        assert_eq!(
            reported(dir.path()),
            1,
            "the event comes before the removal"
        );

        let report = Gates::close(dir.path(), phase, stage).unwrap();

        assert_eq!(report.claimed.len(), 1, "{report:?}");
        assert_eq!(report.claimed[0].gate_id, SECOND);
        assert!(report.claimed[0].answer.is_some(), "{report:?}");
        assert_eq!(
            reported(dir.path()),
            2,
            "a duplicate report, never a lost one"
        );
        assert!(!Gates::gate_path(dir.path(), phase, stage).exists());
        assert!(!Gates::answer_path(dir.path(), phase, stage, &second).exists());
        assert!(!Gates::claim_path(dir.path(), phase, stage, &second).exists());
    }

    #[test]
    fn opening_a_gate_sets_aside_the_previous_gates_unconsumed_answer() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let first = Gates::open_gate(dir.path(), phase, stage, "first")
            .unwrap()
            .gate_id;
        Gates::answer(dir.path(), phase, stage, &first, &approved()).unwrap();
        let legacy = Gates::response_path(dir.path(), phase, stage);
        std::fs::write(&legacy, "{\"approved\":true}").unwrap();

        let next = Gates::open_gate(dir.path(), phase, stage, "next")
            .unwrap()
            .gate_id;

        assert_eq!(
            Gates::read_answer(dir.path(), phase, stage, &next),
            AnswerRead::None
        );
        assert!(!Gates::answer_path(dir.path(), phase, stage, &first).exists());
        assert!(!legacy.exists());
        let mut reasons: Vec<String> = events_named(dir.path(), "gate_answer_quarantined")
            .iter()
            .map(|event| event["reason"].as_str().unwrap().to_string())
            .collect();
        reasons.sort();
        assert_eq!(reasons, ["gate_closed", "legacy_unkeyed"]);
        assert_eq!(quarantine_files(dir.path()).len(), 2);
    }

    #[test]
    fn quarantined_lists_every_set_aside_answer() {
        let dir = tempfile::tempdir().unwrap();
        let decimal: PhaseId = "50.1".parse().unwrap();
        let first = Gates::open_gate(dir.path(), PhaseId::new(7), Stage::Code, "ctx")
            .unwrap()
            .gate_id;
        Gates::answer(
            dir.path(),
            PhaseId::new(7),
            Stage::Code,
            &first,
            &approved(),
        )
        .unwrap();
        Gates::close(dir.path(), PhaseId::new(7), Stage::Code).unwrap();
        let second = Gates::open_gate(dir.path(), decimal, Stage::Validate, "ctx")
            .unwrap()
            .gate_id;
        std::fs::write(
            Gates::answer_path(dir.path(), decimal, Stage::Validate, &second),
            "not json",
        )
        .unwrap();
        Gates::set_aside_unbound(dir.path(), decimal, Stage::Validate, &second).unwrap();
        std::fs::write(
            Gates::dir(dir.path()).join("07-code.noid.x.quarantined.json"),
            "a name that does not parse",
        )
        .unwrap();

        let listed = Gates::quarantined(dir.path());

        assert_eq!(listed.len(), 2, "{listed:?}");
        let code = listed.iter().find(|q| q.stage == Stage::Code).unwrap();
        assert_eq!(code.phase, PhaseId::new(7));
        assert_eq!(code.label, first.as_str());
        assert_eq!(
            std::fs::read_to_string(&code.path).unwrap(),
            serde_json::to_string_pretty(&GateAnswer {
                gate_id: first.to_string(),
                response: approved(),
            })
            .unwrap()
        );
        let validate = listed.iter().find(|q| q.stage == Stage::Validate).unwrap();
        assert_eq!(validate.phase, decimal);
        assert_eq!(validate.label, "noid");
        assert_eq!(std::fs::read_to_string(&validate.path).unwrap(), "not json");
    }

    #[test]
    fn reap_answer_writes_a_rejection_for_the_observed_gate_only() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let other = GateId::new();

        let refused = Gates::reap_answer(dir.path(), phase, stage, &other, "abort: x", "sweep");
        assert!(
            matches!(refused, Err(GateError::GateIdMismatch { .. })),
            "{refused:?}"
        );
        assert!(!Gates::answer_path(dir.path(), phase, stage, &other).exists());

        Gates::reap_answer(dir.path(), phase, stage, &gate, "abort: x", "sweep").unwrap();

        let AnswerRead::Bound(answer) = Gates::read_answer(dir.path(), phase, stage, &gate) else {
            panic!("the reap must be read back as an answer to its gate");
        };
        assert!(!answer.response.approved);
        assert_eq!(answer.response.responded_by.as_deref(), Some("sweep"));
        assert!(matches!(
            GateAction::from_response(&answer.response),
            GateAction::Abort(_)
        ));
    }

    #[test]
    fn list_open_reports_the_gate_id_and_hides_a_gate_its_live_answer_decided() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let open = Gates::list_open(dir.path());
        assert_eq!(open.len(), 1, "{open:?}");
        assert_eq!(open[0].gate_id, gate.as_str());

        // What sits beside a gate file must not read as another request, even with a
        // gate-shaped body, in a decimal phase whose own name holds a dot.
        let decimal: PhaseId = "50.1".parse().unwrap();
        let decimal_gate = Gates::open_gate(dir.path(), decimal, stage, "decimal").unwrap();
        let gate_shaped = std::fs::read_to_string(&decimal_gate.path).unwrap();
        for name in [
            "50.1-code.noid.1.1.0.quarantined.json",
            "50.1-code.0000000000000000.ack.json",
            "50.1-code.0000000000000000.extra.json",
        ] {
            std::fs::write(Gates::dir(dir.path()).join(name), &gate_shaped).unwrap();
        }
        assert_eq!(Gates::list_open(dir.path()).len(), 2, "{gate_shaped}");

        let other = GateId::new();
        std::fs::write(
            Gates::answer_path(dir.path(), phase, stage, &other),
            serde_json::to_string(&GateAnswer {
                gate_id: other.to_string(),
                response: approved(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            Gates::list_open(dir.path()).len(),
            2,
            "an answer for another id must not hide the gate"
        );

        Gates::answer(dir.path(), phase, stage, &gate, &approved()).unwrap();
        let open = Gates::list_open(dir.path());
        assert_eq!(open.len(), 1, "{open:?}");
        assert_eq!(open[0].phase, decimal);

        // A legacy unkeyed response still hides its gate until the CLI moves off it.
        std::fs::write(
            Gates::response_path(dir.path(), decimal, stage),
            "{\"approved\":true}",
        )
        .unwrap();
        assert!(Gates::list_open(dir.path()).is_empty());
    }

    #[test]
    fn close_reports_removed_for_a_lone_gate_file_and_for_a_lone_temp() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        Gates::open_gate(dir.path(), phase, stage, "ctx").unwrap();
        assert!(Gates::close(dir.path(), phase, stage).unwrap().removed);

        std::fs::write(Gates::dir(dir.path()).join(".07-code.json.1.0.tmp"), "temp").unwrap();
        assert!(Gates::close(dir.path(), phase, stage).unwrap().removed);

        assert!(!Gates::close(dir.path(), phase, stage).unwrap().removed);
    }

    #[test]
    fn close_removes_a_lone_legacy_ack() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let ack = Gates::ack(dir.path(), phase, stage).unwrap();

        Gates::close(dir.path(), phase, stage).unwrap();

        assert!(!ack.exists());
    }

    #[test]
    fn a_claim_with_its_answer_already_gone_is_reported_without_one() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        assert_eq!(
            Gates::claim(dir.path(), phase, stage, &gate).unwrap(),
            Claim::Claimed
        );

        let report = Gates::close(dir.path(), phase, stage).unwrap();

        assert_eq!(
            report.claimed,
            [ClaimedAnswer {
                gate_id: gate.to_string(),
                answer: None,
            }]
        );
        assert!(!Gates::claim_path(dir.path(), phase, stage, &gate).exists());
    }

    /// A read that fails for any reason but absence stops the disposal, with the
    /// claim and the gate file kept for the next close.
    #[test]
    fn a_claimed_answer_that_cannot_be_read_stops_the_close_and_keeps_the_gate_file() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer_path(dir.path(), phase, stage, &gate);
        std::fs::create_dir(&slot).unwrap();
        assert_eq!(
            Gates::claim(dir.path(), phase, stage, &gate).unwrap(),
            Claim::Claimed
        );

        let closed = Gates::close(dir.path(), phase, stage);

        assert!(matches!(closed, Err(GateError::Io(_))), "{closed:?}");
        assert!(Gates::gate_path(dir.path(), phase, stage).exists());
        assert!(Gates::claim_path(dir.path(), phase, stage, &gate).exists());
        assert!(events_named(dir.path(), "gate_claimed_answer_removed").is_empty());
    }

    /// Only absence reads as "no gate": any other failure must not.
    #[test]
    fn a_gate_file_that_cannot_be_read_is_an_error_not_a_closed_gate() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = GateId::new();
        std::fs::create_dir_all(Gates::gate_path(dir.path(), phase, stage)).unwrap();

        let current = Gates::claim(dir.path(), phase, stage, &gate);

        assert!(matches!(current, Err(GateError::Io(_))), "{current:?}");
    }

    /// Only absence reads as an empty slot: a slot that cannot be read is neither
    /// empty nor moved.
    #[test]
    fn an_answer_slot_that_cannot_be_read_is_not_an_empty_slot() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer_path(dir.path(), phase, stage, &gate);
        std::fs::create_dir(&slot).unwrap();

        assert_eq!(
            Gates::read_answer(dir.path(), phase, stage, &gate),
            AnswerRead::Unbound("unparsable".to_string())
        );
        let moved = Gates::set_aside_unbound(dir.path(), phase, stage, &gate);
        assert!(matches!(moved, Err(GateError::Io(_))), "{moved:?}");
        assert!(slot.is_dir());
    }

    /// Another process setting a file aside first is not an error; any other failure
    /// to move it is.
    #[test]
    fn setting_aside_a_file_already_gone_is_a_no_op_and_any_other_failure_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gates_dir = Gates::dir(dir.path());
        std::fs::create_dir_all(&gates_dir).unwrap();
        let missing = gates_dir.join("missing.json");
        let directory = gates_dir.join("a-directory");
        std::fs::create_dir(&directory).unwrap();

        let gone = set_aside_if_present(dir.path(), phase, stage, &missing, "noid", "unparsable");
        assert!(matches!(gone, Ok(None)), "{gone:?}");
        let failed =
            set_aside_if_present(dir.path(), phase, stage, &directory, "noid", "unparsable");
        assert!(failed.is_err(), "{failed:?}");

        let unclaimed = |path: &Path| UnclaimedAnswer {
            path: path.to_path_buf(),
            slot_id: None,
        };
        let gone = set_aside_unclaimed(dir.path(), phase, stage, &unclaimed(&missing));
        assert!(matches!(gone, Ok(None)), "{gone:?}");
        let failed = set_aside_unclaimed(dir.path(), phase, stage, &unclaimed(&directory));
        assert!(failed.is_err(), "{failed:?}");
        assert!(quarantine_files(dir.path()).is_empty());
    }

    #[test]
    fn a_gates_path_that_is_not_a_directory_fails_the_stage_scan() {
        let dir = tempfile::tempdir().unwrap();
        let gates_dir = Gates::dir(dir.path());
        std::fs::create_dir_all(gates_dir.parent().unwrap()).unwrap();
        std::fs::write(&gates_dir, "not a directory").unwrap();

        let scanned = StageFiles::scan(dir.path(), PhaseId::new(7), Stage::Code);

        assert!(matches!(scanned, Err(GateError::Io(_))));
    }

    /// A name that passes as an answer file but holds no gate id is never reported as
    /// that gate's answer, however its body reads.
    #[test]
    fn an_answer_file_named_with_no_gate_id_is_set_aside_as_another_gates() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gates_dir = Gates::dir(dir.path());
        std::fs::create_dir_all(&gates_dir).unwrap();
        std::fs::write(
            gates_dir.join("07-code.garbage.response.json"),
            "{\"gate_id\": \"garbage\", \"approved\": true}",
        )
        .unwrap();

        let report = Gates::close(dir.path(), phase, stage).unwrap();

        assert_eq!(report.quarantined.len(), 1, "{report:?}");
        let name = report.quarantined[0].file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("07-code.noid."), "{name}");
        assert_eq!(
            events_named(dir.path(), "gate_answer_quarantined")[0]["reason"],
            "other_gate"
        );
    }

    #[test]
    fn classifying_an_answer_names_the_reason_it_does_not_bind() {
        let id = "00000000000000a0";
        let classify = |body: &str| classify_answer(body.as_bytes(), id).map(|_| ());

        assert_eq!(classify("not json"), Err("unparsable"));
        assert_eq!(classify("[1]"), Err("unparsable"), "not an object");
        assert_eq!(classify("{\"approved\": true}"), Err("missing_gate_id"));
        assert_eq!(
            classify("{\"gate_id\": \"\", \"approved\": true}"),
            Err("missing_gate_id")
        );
        assert_eq!(
            classify("{\"gate_id\": 7, \"approved\": true}"),
            Err("missing_gate_id")
        );
        assert_eq!(
            classify("{\"gate_id\": \"00000000000000a1\", \"approved\": true}"),
            Err("other_gate")
        );
        assert_eq!(
            classify("{\"gate_id\": \"00000000000000a0\"}"),
            Err("unparsable"),
            "its own id, but no decision"
        );
        assert_eq!(
            classify("{\"gate_id\": \"00000000000000a0\", \"approved\": true}"),
            Ok(())
        );
    }

    #[test]
    fn a_gate_and_a_quarantine_carry_the_current_time() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer_path(dir.path(), phase, stage, &gate);
        std::fs::write(&slot, "not json").unwrap();

        let moved = Gates::set_aside_unbound(dir.path(), phase, stage, &gate)
            .unwrap()
            .unwrap();

        let opened_at: u64 = read_gate(dir.path(), phase, stage)
            .timestamp
            .parse()
            .unwrap();
        assert!(opened_at >= before, "{opened_at} is before {before}");
        let name = moved.file_name().unwrap().to_string_lossy().into_owned();
        let secs: u64 = name
            .strip_prefix("07-code.noid.")
            .unwrap()
            .split('.')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(secs >= before, "{secs} is before {before}");
    }

    /// A read path and a refusal must leave a project without a gates directory as
    /// they found it.
    #[test]
    fn reading_or_refusing_without_a_gates_dir_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = GateId::new();

        assert!(Gates::list_open(dir.path()).is_empty());
        assert!(Gates::quarantined(dir.path()).is_empty());
        assert_eq!(
            Gates::read_answer(dir.path(), phase, stage, &gate),
            AnswerRead::None
        );
        assert_eq!(
            Gates::close(dir.path(), phase, stage).unwrap(),
            CloseReport::default()
        );
        let refusals = [
            Gates::answer(dir.path(), phase, stage, &gate, &approved()).map(|_| ()),
            Gates::claim(dir.path(), phase, stage, &gate).map(|_| ()),
            Gates::set_aside_unbound(dir.path(), phase, stage, &gate).map(|_| ()),
        ];
        for refusal in refusals {
            assert!(
                matches!(refusal, Err(GateError::NoOpenGate { .. })),
                "{refusal:?}"
            );
        }

        assert!(!dir.path().join(".devflow").exists());
    }

    #[test]
    fn only_the_gate_id_format_parses_as_a_gate_id() {
        let good = GateId::new();
        assert_eq!(GateId::parse(good.as_str()).unwrap(), good);
        assert_eq!(good.to_string(), good.as_str());
        for bad in [
            "",
            "../../x",
            "00000000000000A0",
            "00000000000000a",
            "00000000000000a00",
            "noid",
        ] {
            let parsed = GateId::parse(bad);
            assert!(
                matches!(&parsed, Err(GateError::InvalidGateId(id)) if id == bad),
                "{bad:?}: {parsed:?}"
            );
        }
    }

    // ---- D-04: a waiter is live exactly while it holds its registration ----

    /// Run `body` in a child test process of its own. Another test's fork holds a
    /// duplicate of every descriptor here until its exec, so under `cargo test` a lock
    /// read right after its release could still read held.
    fn in_own_process(test_name: &str, body: impl FnOnce()) {
        if crate::test_support::in_child_test(test_name) {
            body();
            return;
        }
        let path_dir = tempfile::tempdir().unwrap();
        let output = crate::test_support::run_test_in_child(test_name, path_dir.path(), &[]);
        crate::test_support::assert_child_ran_exactly_one_passing_test(&output, test_name);
    }

    /// Every `*.waiter` file under `root`'s gates directory, sorted by name.
    fn waiter_files(root: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(Gates::dir(root)) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().ends_with(WAITER_SUFFIX))
            .collect();
        files.sort();
        files
    }

    /// The gate id in the name of a waiter file at `phase`'s `stage`.
    fn waiter_file_gate(path: &Path, phase: PhaseId, stage: Stage) -> GateId {
        let name = path.file_name().unwrap().to_str().unwrap();
        let id = name
            .strip_prefix(&format!("{}-{stage}.", phase.padded()))
            .and_then(|rest| rest.strip_suffix(WAITER_SUFFIX))
            .unwrap();
        GateId::parse(id).unwrap()
    }

    #[test]
    fn a_registered_waiter_reads_live_and_its_answer_is_awaited_and_claimed() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let registration = Gates::register_waiter(dir.path(), phase, stage, &gate).unwrap();
        assert_eq!(
            Gates::waiter_status(dir.path(), phase, stage, &gate).unwrap(),
            WaiterStatus::Live
        );

        let root = dir.path().to_path_buf();
        let answered = gate.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            Gates::answer(&root, phase, stage, &answered, &approved()).unwrap();
        });
        let outcome = Gates::await_answer(dir.path(), phase, stage, &gate, 30, || false);
        writer.join().unwrap();

        assert_eq!(
            outcome,
            AwaitOutcome::Answered(GateAnswer {
                gate_id: gate.to_string(),
                response: approved(),
            })
        );
        assert_eq!(
            Gates::claim(dir.path(), phase, stage, &gate).unwrap(),
            Claim::Claimed
        );
        registration.finish();
        assert_eq!(
            Gates::waiter_status(dir.path(), phase, stage, &gate).unwrap(),
            WaiterStatus::Absent
        );
        assert!(!Gates::waiter_path(dir.path(), phase, stage, &gate).exists());
    }

    #[test]
    fn a_crashed_waiters_registration_reads_absent() {
        const NAME: &str = "gates::tests::a_crashed_waiters_registration_reads_absent";
        const DIED_REGISTERED: i32 = 75;
        const MARKER: &str = "registered-marker";
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        if crate::test_support::in_child_test(NAME) {
            let root = PathBuf::from(std::env::var_os("WAITER_CRASH_ROOT").unwrap());
            let gate = GateId::parse(&std::env::var("WAITER_CRASH_GATE").unwrap()).unwrap();
            let _registration = Gates::register_waiter(&root, phase, stage, &gate).unwrap();
            std::fs::write(root.join(MARKER), "registered").unwrap();
            std::process::exit(DIED_REGISTERED);
        }
        let dir = tempfile::tempdir().unwrap();
        let path_dir = tempfile::tempdir().unwrap();
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;

        let output = crate::test_support::run_test_in_child(
            NAME,
            path_dir.path(),
            &[
                ("WAITER_CRASH_ROOT", dir.path().as_os_str()),
                ("WAITER_CRASH_GATE", std::ffi::OsStr::new(gate.as_str())),
            ],
        );

        assert_eq!(output.status.code(), Some(DIED_REGISTERED), "{output:?}");
        assert!(dir.path().join(MARKER).exists(), "the child died elsewhere");
        assert!(
            Gates::waiter_path(dir.path(), phase, stage, &gate).exists(),
            "a crash leaves the file behind"
        );
        assert_eq!(
            Gates::waiter_status(dir.path(), phase, stage, &gate).unwrap(),
            WaiterStatus::Absent,
            "a marker file outliving its process must not read live"
        );

        let held = GateId::new();
        let registration = Gates::register_waiter(dir.path(), phase, stage, &held).unwrap();
        assert_eq!(
            Gates::waiter_status(dir.path(), phase, stage, &held).unwrap(),
            WaiterStatus::Live,
            "the control: a registration still held reads live"
        );
        registration.finish();
    }

    /// R-09: the wait hands an unbound body back untouched, and the caller moves it.
    #[test]
    fn await_answer_returns_an_unbound_answer_without_moving_it() {
        const ID_LESS_BODY: &str = "{\"approved\":true}";
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer_path(dir.path(), phase, stage, &gate);
        std::fs::write(&slot, ID_LESS_BODY).unwrap();

        let started = std::time::Instant::now();
        let outcome = Gates::await_answer(dir.path(), phase, stage, &gate, 5, || false);

        assert_eq!(
            outcome,
            AwaitOutcome::Unbound {
                reason: REASON_MISSING_GATE_ID.to_string(),
            }
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "returned at its first read"
        );
        assert_eq!(std::fs::read_to_string(&slot).unwrap(), ID_LESS_BODY);
        assert_eq!(
            events_named(dir.path(), "gate_answer_quarantined").len(),
            0,
            "the wait records nothing because it moved nothing"
        );

        let moved = Gates::set_aside_unbound(dir.path(), phase, stage, &gate).unwrap();
        assert!(moved.is_some());
        assert!(!slot.exists());
        assert_eq!(events_named(dir.path(), "gate_answer_quarantined").len(), 1);

        let root = dir.path().to_path_buf();
        let answered = gate.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(1));
            Gates::answer(&root, phase, stage, &answered, &approved()).unwrap();
        });
        let outcome = Gates::await_answer(dir.path(), phase, stage, &gate, 10, || false);
        writer.join().unwrap();
        assert_eq!(
            outcome,
            AwaitOutcome::Answered(GateAnswer {
                gate_id: gate.to_string(),
                response: approved(),
            })
        );
    }

    /// R-06. Own process: the abandoned registration's lock must read released.
    #[test]
    fn close_removes_only_an_abandoned_waiter_registration() {
        in_own_process(
            "gates::tests::close_removes_only_an_abandoned_waiter_registration",
            || {
                let dir = tempfile::tempdir().unwrap();
                let (phase, stage) = (PhaseId::new(7), Stage::Code);
                let live = GateId::new();
                let abandoned = GateId::new();
                let registration = Gates::register_waiter(dir.path(), phase, stage, &live).unwrap();
                drop(Gates::register_waiter(dir.path(), phase, stage, &abandoned).unwrap());
                // Another stage's abandoned registration is not this close's to remove.
                let elsewhere = GateId::new();
                drop(
                    Gates::register_waiter(dir.path(), phase, Stage::Validate, &elsewhere).unwrap(),
                );
                let live_path = Gates::waiter_path(dir.path(), phase, stage, &live);
                let abandoned_path = Gates::waiter_path(dir.path(), phase, stage, &abandoned);

                let report = Gates::close(dir.path(), phase, stage).unwrap();

                assert!(report.removed, "the abandoned registration was removed");
                assert!(!abandoned_path.exists(), "an abandoned registration goes");
                assert!(live_path.exists(), "a live registration stays");
                assert_eq!(
                    Gates::waiter_status(dir.path(), phase, stage, &live).unwrap(),
                    WaiterStatus::Live
                );
                assert!(
                    Gates::waiter_path(dir.path(), phase, Stage::Validate, &elsewhere).exists()
                );

                registration.finish();
                assert!(!live_path.exists(), "the owner removes its own");
            },
        );
    }

    /// R-06: a take in flight is not a gate file of any kind.
    #[test]
    fn a_waiter_hold_temp_does_not_make_its_phase_appear_on_disk() {
        use crate::liveness::{HOLD_TEMP_SUFFIX, TakeStep};
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        std::fs::create_dir_all(Gates::dir(dir.path())).unwrap();
        let gate = GateId::new();
        let mut paused = false;

        let hold = liveness::take_with_hook(
            &Gates::waiter_path(dir.path(), phase, stage, &gate),
            &mut |step, temp| {
                if step != TakeStep::Locked {
                    return;
                }
                paused = true;
                assert!(temp.to_string_lossy().ends_with(HOLD_TEMP_SUFFIX));
                assert_eq!(
                    std::fs::read_dir(Gates::dir(dir.path())).unwrap().count(),
                    1,
                    "the temp is the phase's only entry"
                );
                assert!(!Gates::phases_on_disk(dir.path()).contains(&phase));
                assert_eq!(Gates::list_open(dir.path()), Vec::new());
                assert_eq!(Gates::live_waiter_holders(dir.path()), Vec::new());
                assert_eq!(
                    crate::test_support::gate_answer_files(dir.path(), phase, stage),
                    Vec::<PathBuf>::new()
                );
                Gates::close(dir.path(), phase, stage).unwrap();
                assert!(temp.exists(), "a close leaves a take in flight alone");
            },
        )
        .unwrap();
        assert!(paused, "the take never reached the point under test");
        drop(hold);

        let control = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(Gates::dir(control.path())).unwrap();
        std::fs::write(Gates::dir(control.path()).join(".07-code.json.1.0.tmp"), "").unwrap();
        assert!(
            Gates::phases_on_disk(control.path()).contains(&phase),
            "the control: a durable temp marks a gate write that outlived its phase"
        );
    }

    #[test]
    fn live_waiter_holders_names_the_registered_waiter() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = GateId::new();
        let pid = std::process::id();
        let holds = |root: &Path| {
            Gates::live_waiter_holders(root)
                .iter()
                .any(|(holder, _)| *holder == pid)
        };
        let registration = Gates::register_waiter(dir.path(), phase, stage, &gate).unwrap();
        assert!(
            holds(dir.path()),
            "{:?}",
            Gates::live_waiter_holders(dir.path())
        );
        registration.finish();
        assert!(!holds(dir.path()));
    }

    /// The writer a CLI runs: learn the id from the gate file, and answer only a gate
    /// with a registered waiter. Returns what it read.
    fn answer_if_registered(root: &Path, phase: PhaseId, stage: Stage) -> WaiterStatus {
        let gate = GateId::parse(&read_gate(root, phase, stage).gate_id).unwrap();
        let status = Gates::waiter_status(root, phase, stage, &gate).unwrap();
        if status == WaiterStatus::Live {
            Gates::answer(root, phase, stage, &gate, &approved()).unwrap();
        }
        status
    }

    /// D-04's order, through the private fn that is `open_registered`'s whole body,
    /// with the publish-first order as its control.
    #[test]
    fn a_writer_answering_the_instant_the_gate_file_appears_finds_its_waiter_registered() {
        let (phase, stage) = (PhaseId::new(7), Stage::Code);

        let shipped = tempfile::tempdir().unwrap();
        let mut shipped_read = None;
        let (opened, registration) = Gates::open_registered_with_hooks(
            shipped.path(),
            phase,
            stage,
            "ctx",
            || {},
            || shipped_read = Some(answer_if_registered(shipped.path(), phase, stage)),
        )
        .unwrap();
        assert_eq!(shipped_read, Some(WaiterStatus::Live));
        assert_eq!(
            Gates::await_answer(shipped.path(), phase, stage, &opened.gate_id, 5, || false),
            AwaitOutcome::Answered(GateAnswer {
                gate_id: opened.gate_id.to_string(),
                response: approved(),
            })
        );
        registration.finish();

        let control = tempfile::tempdir().unwrap();
        let mut control_read = None;
        let (opened, registration) = Gates::publish_then_register(
            control.path(),
            phase,
            stage,
            "ctx",
            || {},
            || control_read = Some(answer_if_registered(control.path(), phase, stage)),
        )
        .unwrap();
        assert_eq!(control_read, Some(WaiterStatus::Absent));
        assert_eq!(
            Gates::read_answer(control.path(), phase, stage, &opened.gate_id),
            AnswerRead::None,
            "the writer refused"
        );
        registration.finish();
        assert_ne!(
            shipped_read, control_read,
            "the two orders must be told apart, or the test proves nothing"
        );
    }

    /// A kill-point fixture for each side of the publish, in the shipped sequence.
    #[test]
    fn a_waiter_killed_between_registering_and_publishing_leaves_no_answerable_gate() {
        const NAME: &str = "gates::tests::a_waiter_killed_between_registering_and_publishing_leaves_no_answerable_gate";
        const KILLED: i32 = 76;
        const MARKER: &str = "kill-point-marker";
        const BEFORE_PUBLISH: &str = "before_publish";
        const AFTER_PUBLISH: &str = "after_publish";
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        if crate::test_support::in_child_test(NAME) {
            let root = PathBuf::from(std::env::var_os("GATE_KILL_ROOT").unwrap());
            let point = std::env::var("GATE_KILL_POINT").unwrap();
            let kill_at = |here: &str| {
                if point == here {
                    std::fs::write(root.join(MARKER), here).unwrap();
                    std::process::exit(KILLED);
                }
            };
            let opened = Gates::open_registered_with_hooks(
                &root,
                phase,
                stage,
                "ctx",
                || kill_at(BEFORE_PUBLISH),
                || kill_at(AFTER_PUBLISH),
            );
            panic!("the kill point {point} never fired: {opened:?}");
        }
        let killed_at = |point: &str| {
            let dir = tempfile::tempdir().unwrap();
            let path_dir = tempfile::tempdir().unwrap();
            let output = crate::test_support::run_test_in_child(
                NAME,
                path_dir.path(),
                &[
                    ("GATE_KILL_ROOT", dir.path().as_os_str()),
                    ("GATE_KILL_POINT", std::ffi::OsStr::new(point)),
                ],
            );
            assert_eq!(output.status.code(), Some(KILLED), "{output:?}");
            assert_eq!(
                std::fs::read_to_string(dir.path().join(MARKER)).unwrap(),
                point,
                "the child died elsewhere"
            );
            dir
        };

        let before = killed_at(BEFORE_PUBLISH);
        assert!(!Gates::gate_path(before.path(), phase, stage).exists());
        assert_eq!(Gates::list_open(before.path()), Vec::new());
        let registrations = waiter_files(before.path());
        assert_eq!(registrations.len(), 1, "{registrations:?}");
        let gate = waiter_file_gate(&registrations[0], phase, stage);
        assert_eq!(
            Gates::waiter_status(before.path(), phase, stage, &gate).unwrap(),
            WaiterStatus::Absent
        );
        Gates::close(before.path(), phase, stage).unwrap();
        assert_eq!(waiter_files(before.path()), Vec::<PathBuf>::new());

        let after = killed_at(AFTER_PUBLISH);
        assert!(Gates::gate_path(after.path(), phase, stage).exists());
        let open = Gates::list_open(after.path());
        assert_eq!(open.len(), 1, "{open:?}");
        let gate = GateId::parse(&open[0].gate_id).unwrap();
        assert!(Gates::waiter_path(after.path(), phase, stage, &gate).exists());
        assert_eq!(
            Gates::waiter_status(after.path(), phase, stage, &gate).unwrap(),
            WaiterStatus::Absent,
            "a waiter that died at its gate"
        );
    }

    // ---- D-13 items 4 and 7, R-09: the wait backs off, cancels first, moves nothing ----

    /// Reads at 0 s, 1 s and 3 s, the next one due at 7 s, with `cancel` honoured at
    /// the next whole slice after 4.5 s.
    #[test]
    fn await_answer_backs_off_between_reads_and_still_cancels_within_a_second() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = GateId::new();
        SLOT_READS.with(|reads| reads.set(0));
        let started = std::time::Instant::now();

        let outcome = Gates::await_answer(dir.path(), phase, stage, &gate, 60, || {
            started.elapsed() >= std::time::Duration::from_millis(4500)
        });

        let elapsed = started.elapsed();
        assert_eq!(outcome, AwaitOutcome::Cancelled);
        assert_eq!(SLOT_READS.with(std::cell::Cell::get), 3);
        assert!(
            elapsed <= std::time::Duration::from_millis(5600),
            "cancelled {elapsed:?} in, more than a second after the cancel"
        );
    }

    /// A stop accepted in the same second as an answer wins (D-13 item 7).
    #[test]
    fn await_answer_checks_cancellation_before_reading_an_answer() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let slot = Gates::answer(dir.path(), phase, stage, &gate, &approved()).unwrap();
        let body = std::fs::read(&slot).unwrap();

        let outcome = Gates::await_answer(dir.path(), phase, stage, &gate, 30, || true);

        assert_eq!(outcome, AwaitOutcome::Cancelled);
        assert_eq!(
            std::fs::read(&slot).unwrap(),
            body,
            "the answer is untouched"
        );
    }

    /// The deadline is a read boundary too: an answer that lands in the last backoff
    /// interval is an answer, not a timeout.
    #[test]
    fn await_answer_reads_once_more_at_its_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let gate = Gates::open_gate(dir.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        let root = dir.path().to_path_buf();
        let answered = gate.clone();
        // Past the read at 1 s and before the deadline at 2 s; the next backoff
        // boundary would be 3 s.
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1400));
            Gates::answer(&root, phase, stage, &answered, &approved()).unwrap();
        });

        let outcome = Gates::await_answer(dir.path(), phase, stage, &gate, 2, || false);

        writer.join().unwrap();
        assert_eq!(
            outcome,
            AwaitOutcome::Answered(GateAnswer {
                gate_id: gate.to_string(),
                response: approved(),
            })
        );

        let silent = GateId::new();
        let started = std::time::Instant::now();
        assert_eq!(
            Gates::await_answer(dir.path(), phase, stage, &silent, 0, || false),
            AwaitOutcome::TimedOut,
            "a zero timeout still ends"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    /// Every file under `root`'s gates directory, name and bytes.
    fn gates_dir_snapshot(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(Gates::dir(root))
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    std::fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }

    /// R-09: a waiter that outlives its gate, a superseded supervisor among them, can
    /// never move the current gate's files.
    #[test]
    fn a_waiter_whose_gate_was_replaced_never_touches_another_gates_files() {
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        const MORE_READS_SEEN: u32 = 3;
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let first = Gates::open_gate(dir.path(), phase, stage, "first")
            .unwrap()
            .gate_id;
        let registration = Gates::register_waiter(dir.path(), phase, stage, &first).unwrap();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let reads_seen = std::sync::Arc::new(AtomicU32::new(0));
        let waiter = {
            let (root, first) = (dir.path().to_path_buf(), first.clone());
            let (stop, reads_seen) = (stop.clone(), reads_seen.clone());
            std::thread::spawn(move || {
                Gates::await_answer(&root, phase, stage, &first, 30, || {
                    reads_seen.store(SLOT_READS.with(std::cell::Cell::get), Ordering::SeqCst);
                    stop.load(Ordering::SeqCst)
                })
            })
        };

        let current = Gates::open_gate(dir.path(), phase, stage, "second")
            .unwrap()
            .gate_id;
        Gates::answer(dir.path(), phase, stage, &current, &approved()).unwrap();
        std::fs::write(
            Gates::response_path(dir.path(), phase, stage),
            serde_json::to_string(&approved()).unwrap(),
        )
        .unwrap();
        let third = GateId::new();
        std::fs::write(
            Gates::answer_path(dir.path(), phase, stage, &third),
            serde_json::to_string(&GateAnswer {
                gate_id: third.to_string(),
                response: approved(),
            })
            .unwrap(),
        )
        .unwrap();
        let snapshot = gates_dir_snapshot(dir.path());
        let quarantined_before = events_named(dir.path(), "gate_answer_quarantined").len();
        let taken_at = std::time::Instant::now();
        let reads_then = reads_seen.load(Ordering::SeqCst);

        // `cancel` publishes the count before each read, so it lags by one read: three
        // more seen means at least two more made after the snapshot.
        while reads_seen.load(Ordering::SeqCst) < reads_then + MORE_READS_SEEN
            || taken_at.elapsed() < std::time::Duration::from_millis(3500)
        {
            assert!(
                taken_at.elapsed() < std::time::Duration::from_secs(20),
                "the waiter stopped reading"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        assert_eq!(gates_dir_snapshot(dir.path()), snapshot);
        assert_eq!(
            events_named(dir.path(), "gate_answer_quarantined").len(),
            quarantined_before
        );
        stop.store(true, Ordering::SeqCst);
        assert_eq!(waiter.join().unwrap(), AwaitOutcome::Cancelled);
        registration.finish();
    }

    // ---- The test-only pre-answer, and the unkeyed bridge ----

    #[test]
    fn a_preanswer_is_published_for_the_gate_that_opens_next() {
        const PREANSWER_NAME: &str = "07-code.preanswer.json";
        use crate::test_support::preanswer_gate;
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let bound = |gate: &GateId| GateAnswer {
            gate_id: gate.to_string(),
            response: rejection(),
        };

        let opened = tempfile::tempdir().unwrap();
        preanswer_gate(opened.path(), phase, stage, &rejection());
        let gate = Gates::open_gate(opened.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        assert_eq!(
            Gates::read_answer(opened.path(), phase, stage, &gate),
            AnswerRead::Bound(bound(&gate))
        );
        assert!(!Gates::dir(opened.path()).join(PREANSWER_NAME).exists());

        let registered = tempfile::tempdir().unwrap();
        preanswer_gate(registered.path(), phase, stage, &rejection());
        let (opened_gate, registration) =
            Gates::open_registered(registered.path(), phase, stage, "ctx").unwrap();
        assert_eq!(
            Gates::await_answer(
                registered.path(),
                phase,
                stage,
                &opened_gate.gate_id,
                5,
                || false
            ),
            AwaitOutcome::Answered(bound(&opened_gate.gate_id))
        );
        registration.finish();

        let unkeyed = tempfile::tempdir().unwrap();
        preanswer_gate(unkeyed.path(), phase, stage, &rejection());
        Gates::write_gate(unkeyed.path(), phase, stage, "ctx").unwrap();
        assert_eq!(
            Gates::poll_response(unkeyed.path(), phase, stage, 5),
            Some(rejection())
        );

        // The control: a pre-answer for another stage decides nothing here.
        let elsewhere = tempfile::tempdir().unwrap();
        preanswer_gate(elsewhere.path(), phase, Stage::Validate, &rejection());
        let gate = Gates::open_gate(elsewhere.path(), phase, stage, "ctx")
            .unwrap()
            .gate_id;
        assert_eq!(
            Gates::read_answer(elsewhere.path(), phase, stage, &gate),
            AnswerRead::None
        );
        assert!(
            Gates::dir(elsewhere.path())
                .join("07-validate.preanswer.json")
                .exists()
        );
    }

    #[test]
    fn the_unkeyed_poll_also_reads_the_live_gates_keyed_answer() {
        let dir = tempfile::tempdir().unwrap();
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        Gates::write_gate(dir.path(), phase, stage, "ctx").unwrap();

        // The control: another gate's answer in its own slot is not this gate's.
        let other = GateId::new();
        std::fs::write(
            Gates::answer_path(dir.path(), phase, stage, &other),
            serde_json::to_string(&GateAnswer {
                gate_id: other.to_string(),
                response: rejection(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(Gates::poll_response(dir.path(), phase, stage, 0), None);

        let live = GateId::parse(&read_gate(dir.path(), phase, stage).gate_id).unwrap();
        Gates::answer(dir.path(), phase, stage, &live, &approved()).unwrap();
        assert_eq!(
            Gates::poll_response(dir.path(), phase, stage, 1),
            Some(approved())
        );
        assert_eq!(
            Gates::poll_response_cancellable(dir.path(), phase, stage, 1, || false),
            Some(approved())
        );
        assert_eq!(
            Gates::read_answer(dir.path(), phase, stage, &live),
            AnswerRead::Bound(GateAnswer {
                gate_id: live.to_string(),
                response: approved(),
            }),
            "the polls only read"
        );
    }

    /// Own process: the dropped registration's lock must read released.
    #[test]
    fn the_unkeyed_cleanup_also_removes_the_stages_keyed_files() {
        in_own_process(
            "gates::tests::the_unkeyed_cleanup_also_removes_the_stages_keyed_files",
            || {
                let dir = tempfile::tempdir().unwrap();
                let (phase, stage) = (PhaseId::new(7), Stage::Code);
                Gates::write_gate(dir.path(), phase, stage, "ctx").unwrap();
                let live = GateId::parse(&read_gate(dir.path(), phase, stage).gate_id).unwrap();
                Gates::answer(dir.path(), phase, stage, &live, &approved()).unwrap();
                assert_eq!(
                    Gates::claim(dir.path(), phase, stage, &live).unwrap(),
                    Claim::Claimed
                );
                drop(Gates::register_waiter(dir.path(), phase, stage, &live).unwrap());
                let held = GateId::new();
                let registration = Gates::register_waiter(dir.path(), phase, stage, &held).unwrap();
                let set_aside = Gates::dir(dir.path()).join(format!(
                    "07-code.noid.1.{}.0.quarantined.json",
                    std::process::id()
                ));
                std::fs::write(&set_aside, "{}").unwrap();

                assert!(Gates::cleanup(dir.path(), phase, stage).unwrap());

                assert_eq!(
                    crate::test_support::gate_answer_files(dir.path(), phase, stage),
                    Vec::<PathBuf>::new()
                );
                assert!(!Gates::claim_path(dir.path(), phase, stage, &live).exists());
                assert!(!Gates::waiter_path(dir.path(), phase, stage, &live).exists());
                assert_eq!(
                    Gates::waiter_status(dir.path(), phase, stage, &held).unwrap(),
                    WaiterStatus::Live,
                    "R-06: a live registration is its owner's to remove"
                );
                assert!(
                    Gates::waiter_path(dir.path(), phase, stage, &held).exists(),
                    "R-06: a live registration stays"
                );
                assert!(set_aside.exists(), "a set-aside answer is never removed");
                registration.finish();
            },
        );
    }

    const PREANSWER_SEAM_NOT_FOUND: &str =
        "no gated fn consume_preanswer( or no gated item naming preanswer.json";
    const TEST_BUILD_ONLY: &str = "#[cfg(any(test, feature = \"test-support\"))]";
    const RELEASE_BUILD_ONLY: &str = "#[cfg(not(any(test, feature = \"test-support\")))]";

    /// Each line of `source` before the first line that is exactly `mod tests {`,
    /// numbered from 1, without its whole-line `//` comments.
    fn numbered_non_test_lines(source: &str) -> Vec<(usize, &str)> {
        source
            .lines()
            .enumerate()
            .take_while(|(_, line)| *line != "mod tests {")
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
            .map(|(index, line)| (index + 1, line))
            .collect()
    }

    /// Where a gated item ends: the first later line that is the header's indent
    /// followed by `}`, or the header itself when it is a whole item.
    fn gated_item_end(lines: &[(usize, &str)], header: usize) -> usize {
        let text = lines[header].1;
        if text.ends_with('}') || text.ends_with(';') {
            return header;
        }
        let indent = &text[..text.len() - text.trim_start().len()];
        let closing = format!("{indent}}}");
        lines[header + 1..]
            .iter()
            .position(|(_, line)| *line == closing)
            .map_or(lines.len() - 1, |offset| header + 1 + offset)
    }

    /// T-50.1-30 as a pure scan: every line naming the pre-answer sits inside an item
    /// gated to test builds, and a gated `consume_preanswer` exists. Each violation is
    /// `line {n}: {trimmed line}`, then [`PREANSWER_SEAM_NOT_FOUND`].
    fn preanswer_seam_violations(source: &str) -> Vec<String> {
        let lines = numbered_non_test_lines(source);
        let under =
            |attribute: &str, index: usize| index > 0 && lines[index - 1].1.trim() == attribute;
        let mut gated = vec![false; lines.len()];
        for index in 0..lines.len() {
            if under(TEST_BUILD_ONLY, index) {
                gated[index..=gated_item_end(&lines, index)].fill(true);
            }
        }
        let mut violations = Vec::new();
        for (index, (number, line)) in lines.iter().enumerate() {
            let names_seam = line.contains("preanswer.json") || line.contains("preanswer_path(");
            let bad_path_header =
                line.contains("fn preanswer_path(") && !under(TEST_BUILD_ONLY, index);
            let bad_consume_header = line.contains("fn consume_preanswer(")
                && !under(TEST_BUILD_ONLY, index)
                && !under(RELEASE_BUILD_ONLY, index);
            if (names_seam && !gated[index]) || bad_path_header || bad_consume_header {
                violations.push(format!("line {number}: {}", line.trim()));
            }
        }
        let gated_consume = (0..lines.len()).any(|index| {
            under(TEST_BUILD_ONLY, index) && lines[index].1.contains("fn consume_preanswer(")
        });
        let gated_name =
            (0..lines.len()).any(|index| gated[index] && lines[index].1.contains("preanswer.json"));
        if !gated_consume || !gated_name {
            violations.push(PREANSWER_SEAM_NOT_FOUND.to_string());
        }
        violations
    }

    /// T-50.1-30. Limit: this reads `gates.rs` only, where the seam lives. A reader in
    /// another file, or one that builds the name without the `preanswer.json` literal,
    /// is not seen, and the answer-publish hook slot is not scanned.
    #[test]
    fn the_preanswer_seam_is_compiled_only_in_test_builds() {
        let source = include_str!("gates.rs");
        assert!(
            source.lines().any(|line| line == "mod tests {"),
            "the scan would read its own plants"
        );
        assert_eq!(preanswer_seam_violations(source), Vec::<String>::new());

        let good = [
            "impl Gates {",
            "    #[cfg(any(test, feature = \"test-support\"))]",
            "    pub fn preanswer_path(root: &Path) -> PathBuf {",
            "        root.join(\"07-code.preanswer.json\")",
            "    }",
            "}",
            "#[cfg(any(test, feature = \"test-support\"))]",
            "fn consume_preanswer(root: &Path) {",
            "    let _path = Gates::preanswer_path(root);",
            "}",
            "#[cfg(not(any(test, feature = \"test-support\")))]",
            "fn consume_preanswer(_root: &Path) {}",
            "fn open_gate_as(root: &Path) {",
            "    consume_preanswer(root);",
            "}",
        ];
        assert_eq!(good.len(), 15);
        assert_eq!(
            preanswer_seam_violations(&good.join("\n")),
            Vec::<String>::new()
        );

        let mut ungated_copy = good.to_vec();
        ungated_copy.remove(6);
        assert_eq!(
            preanswer_seam_violations(&ungated_copy.join("\n")),
            [
                "line 7: fn consume_preanswer(root: &Path) {",
                "line 8: let _path = Gates::preanswer_path(root);",
                PREANSWER_SEAM_NOT_FOUND,
            ]
        );

        let mut ungated_reader = good.to_vec();
        ungated_reader.extend([
            "fn read_preanswer(root: &Path) -> Option<String> {",
            "    std::fs::read_to_string(root.join(\"07-code.preanswer.json\")).ok()",
            "}",
        ]);
        assert_eq!(
            preanswer_seam_violations(&ungated_reader.join("\n")),
            ["line 17: std::fs::read_to_string(root.join(\"07-code.preanswer.json\")).ok()"]
        );

        let carrying =
            "fn consume_preanswer(root: &Path) { let _path = Gates::preanswer_path(root); }";
        let mut no_op_carrying_the_seam = good.to_vec();
        no_op_carrying_the_seam[11] = carrying;
        assert_eq!(
            preanswer_seam_violations(&no_op_carrying_the_seam.join("\n")),
            [format!("line 12: {carrying}")]
        );

        let mut comment_and_tests = good.to_vec();
        comment_and_tests.extend([
            "// reads NN-{stage}.preanswer.json through preanswer_path(",
            "mod tests {",
            "    fn plant(root: &Path) { let _ = Gates::preanswer_path(root); }",
        ]);
        assert_eq!(
            preanswer_seam_violations(&comment_and_tests.join("\n")),
            Vec::<String>::new()
        );
    }

    /// What an operator reads must say what happened and where to look next.
    #[test]
    fn a_gate_id_error_names_the_ids_and_where_to_look() {
        let (phase, stage) = (PhaseId::new(7), Stage::Code);
        let mismatch = GateError::GateIdMismatch {
            phase,
            stage,
            observed: "old-id".into(),
            current: "new-id".into(),
        }
        .to_string();
        for needle in ["old-id", "new-id", "devflow gate list"] {
            assert!(mismatch.contains(needle), "{needle}: {mismatch}");
        }
        let closed = GateError::GateClosedAfterPublish {
            phase,
            stage,
            observed: "old-id".into(),
        }
        .to_string();
        for needle in ["old-id", "published", "set aside"] {
            assert!(closed.contains(needle), "{needle}: {closed}");
        }
        let unconfirmed = GateError::PublishedUnconfirmed {
            phase,
            stage,
            observed: "old-id".into(),
            waited: GATE_LOCK_WAIT,
        }
        .to_string();
        for needle in ["left in place", ".gates-07.lock", "devflow gate list"] {
            assert!(unconfirmed.contains(needle), "{needle}: {unconfirmed}");
        }
        let timeout = GateError::GateLockTimeout {
            phase,
            waited: GATE_LOCK_WAIT,
        }
        .to_string();
        for needle in [".gates-07.lock", "nothing was changed"] {
            assert!(timeout.contains(needle), "{needle}: {timeout}");
        }
    }
}
