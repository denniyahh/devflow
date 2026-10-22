//! Gate file protocol — the handoff between DevFlow and a human (via Hermes).
//!
//! A *gate* is a pause point where DevFlow writes a request to
//! `.devflow/gates/` and waits for a human (or the Hermes cron poller) to drop
//! a response file. The protocol is three files per gated stage:
//!
//! - `NN-{stage}.json` — the gate request DevFlow writes (a [`GateFile`]).
//! - `NN-{stage}.response.json` — the human's answer (a [`GateResponse`]).
//! - `NN-{stage}.ack.json` — DevFlow's receipt (a [`GateAck`]) so the poller can
//!   clean up.
//!
//! Writes are atomic (write-to-temp + rename) so a reader never sees a partial
//! file. Polling uses exponential backoff so a long human wait costs little.

use crate::phase_id::PhaseId;
use crate::stage::Stage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

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

    /// Every open gate (request written, no response yet), sorted by phase
    /// then stage. Request files are `NN-{stage}.json`; `.response.json` and
    /// `.ack.json` siblings are protocol artifacts, not requests, and any
    /// unparsable file is skipped — listing must degrade, not die.
    pub fn list_open(project_root: &Path) -> Vec<OpenGate> {
        let mut open = Vec::new();
        let Ok(entries) = std::fs::read_dir(Self::dir(project_root)) else {
            return open;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.ends_with(".json")
                || name.ends_with(".response.json")
                || name.ends_with(".ack.json")
            {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok(gate) = serde_json::from_str::<GateFile>(&contents) else {
                continue;
            };
            if Self::response_path(project_root, gate.phase, gate.stage).exists() {
                continue;
            }
            open.push(OpenGate {
                phase: gate.phase,
                stage: gate.stage,
                context: gate.context,
                timestamp: gate.timestamp,
            });
        }
        open.sort_by_key(|g| (g.phase, g.stage.to_string()));
        open
    }

    /// Every phase that has a gate request, response, ack, or orphaned write
    /// temp on disk, whether or not the gate is still open — so a reset can
    /// reach gate files that outlived their phase's state.
    pub fn phases_on_disk(project_root: &Path) -> std::collections::BTreeSet<PhaseId> {
        let mut phases = std::collections::BTreeSet::new();
        let Ok(entries) = std::fs::read_dir(Self::dir(project_root)) else {
            return phases;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
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
        let gate = GateFile {
            phase,
            stage,
            context: context.to_string(),
            timestamp: unix_now(),
        };
        let path = Self::gate_path(project_root, phase, stage);
        info!("writing gate {} for phase {phase}", stage);
        write_atomic(&path, &serde_json::to_string_pretty(&gate)?)?;
        Ok(path)
    }

    /// Poll for a response with exponential backoff (1s → 2s → 4s … capped at
    /// 60s), giving up after `timeout_secs`. Returns the parsed response when it
    /// appears, or `None` on timeout.
    pub fn poll_response(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        timeout_secs: u64,
    ) -> Option<GateResponse> {
        let path = Self::response_path(project_root, phase, stage);
        let deadline = Duration::from_secs(timeout_secs);
        let mut waited = Duration::ZERO;
        let mut backoff = Duration::from_secs(1);
        let cap = Duration::from_secs(60);
        debug!("polling for gate response at {}", path.display());
        loop {
            if let Ok(contents) = std::fs::read_to_string(&path)
                && let Ok(response) = serde_json::from_str::<GateResponse>(&contents)
            {
                return Some(response);
            }
            if waited >= deadline {
                return None;
            }
            let sleep = backoff.min(deadline - waited);
            std::thread::sleep(sleep);
            waited += sleep;
            backoff = (backoff * 2).min(cap);
        }
    }

    /// [`poll_response`](Self::poll_response) that a caller can cancel
    /// (50-13, R-2): the same loop, except that every sleep is at most one
    /// second and `cancel` is checked on every iteration, so a cancel ends
    /// the wait within about a second of being set.
    ///
    /// Returns the response when it appears, or `None` on a timeout or a
    /// cancel. A caller tells the two apart by asking its own `cancel`
    /// again: a `None` returned before the timeout means `cancel` answered
    /// `true`. A response already on disk wins over a cancel.
    pub fn poll_response_cancellable(
        project_root: &Path,
        phase: PhaseId,
        stage: Stage,
        timeout_secs: u64,
        cancel: impl Fn() -> bool,
    ) -> Option<GateResponse> {
        let path = Self::response_path(project_root, phase, stage);
        let deadline = Duration::from_secs(timeout_secs);
        let mut waited = Duration::ZERO;
        let mut backoff = Duration::from_secs(1);
        let cap = Duration::from_secs(60);
        let slice = Duration::from_secs(1);
        debug!(
            "polling (cancellable) for gate response at {}",
            path.display()
        );
        loop {
            if let Ok(contents) = std::fs::read_to_string(&path)
                && let Ok(response) = serde_json::from_str::<GateResponse>(&contents)
            {
                return Some(response);
            }
            if waited >= deadline || cancel() {
                return None;
            }
            let sleep = backoff.min(deadline - waited).min(slice);
            std::thread::sleep(sleep);
            waited += sleep;
            backoff = (backoff * 2).min(cap);
        }
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
                if let Err(error) = std::fs::remove_file(path) {
                    return (removed, Err(error.into()));
                }
                removed = true;
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
                if let Err(error) = std::fs::remove_file(entry.path()) {
                    return (removed, Err(error.into()));
                }
                removed = true;
            }
        }
        (removed, Ok(()))
    }
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
    let (tmp, mut file) = crate::workflow::create_unique_temp(path, std::process::id(), &TEMP_SEQ)?;
    if let Err(error) = std::io::Write::write_all(&mut file, contents.as_bytes()) {
        drop(file);
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    drop(file);

    let publish = std::fs::hard_link(&tmp, path);
    let cleanup = std::fs::remove_file(&tmp);
    match publish {
        Ok(()) => cleanup.map_err(GateError::Io),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = cleanup;
            Err(GateError::AlreadyResponded { phase, stage })
        }
        Err(error) => {
            let _ = cleanup;
            Err(error.into())
        }
    }
}

/// Write `contents` to `path` atomically: write a temp file in the same
/// directory, then rename over the target so readers never see a partial write.
fn write_atomic(path: &Path, contents: &str) -> Result<(), GateError> {
    if let Some(parent) = path.parent() {
        crate::workflow::ensure_devflow_dir(parent)?;
    }
    let (tmp, mut file) = crate::workflow::create_unique_temp(path, std::process::id(), &TEMP_SEQ)?;
    if let Err(error) = std::io::Write::write_all(&mut file, contents.as_bytes()) {
        drop(file);
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    drop(file);
    if let Err(error) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    Ok(())
}

fn unix_now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
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
}
