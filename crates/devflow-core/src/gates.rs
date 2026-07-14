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
    pub phase: u32,
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
}

/// The gate-file protocol, scoped to a project's `.devflow/gates/` directory.
pub struct Gates;

impl Gates {
    /// The `.devflow/gates/` directory for a project.
    pub fn dir(project_root: &Path) -> PathBuf {
        project_root.join(".devflow").join("gates")
    }

    /// Path to the gate request file for a phase + stage.
    pub fn gate_path(project_root: &Path, phase: u32, stage: Stage) -> PathBuf {
        Self::dir(project_root).join(format!("{phase:02}-{stage}.json"))
    }

    /// Path to the response file for a phase + stage.
    pub fn response_path(project_root: &Path, phase: u32, stage: Stage) -> PathBuf {
        Self::dir(project_root).join(format!("{phase:02}-{stage}.response.json"))
    }

    /// Path to the ack file for a phase + stage.
    pub fn ack_path(project_root: &Path, phase: u32, stage: Stage) -> PathBuf {
        Self::dir(project_root).join(format!("{phase:02}-{stage}.ack.json"))
    }

    /// Write a gate request, creating the gates directory if needed.
    pub fn write_gate(
        project_root: &Path,
        phase: u32,
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
        phase: u32,
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

    /// Write an ack file signalling the response was read.
    pub fn ack(project_root: &Path, phase: u32, stage: Stage) -> Result<PathBuf, GateError> {
        let path = Self::ack_path(project_root, phase, stage);
        write_atomic(
            &path,
            &serde_json::to_string_pretty(&GateAck { received: true })?,
        )?;
        Ok(path)
    }

    /// Remove the gate, response, and ack files for a stage. Idempotent.
    pub fn cleanup(project_root: &Path, phase: u32, stage: Stage) -> Result<(), GateError> {
        for path in [
            Self::gate_path(project_root, phase, stage),
            Self::response_path(project_root, phase, stage),
            Self::ack_path(project_root, phase, stage),
        ] {
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

/// Fire the operator-configured gate notify hook, if any.
///
/// Reads `DEVFLOW_GATE_NOTIFY_CMD`; if unset or empty, this is a silent no-op
/// (no notify command configured). Otherwise delegates to
/// [`run_notify_command`]. `unexpected` marks a gate fired on a stage the
/// active [`crate::mode::Mode`] would not normally gate (e.g. a Define/Plan/Code
/// failure in Auto mode) — a never-silent gate per WR-11.
pub fn fire_gate_notify(phase: u32, stage: Stage, context: &str, unexpected: bool) {
    let cmd = match std::env::var("DEVFLOW_GATE_NOTIFY_CMD") {
        Ok(cmd) if !cmd.is_empty() => cmd,
        _ => return,
    };
    run_notify_command(&cmd, phase, stage, context, unexpected);
}

/// Run the notify `cmd` via `sh -c`, passing gate metadata to the child as
/// environment variables — never interpolated into the command string
/// (WR-01 argv-not-shell precedent; `context` may contain agent-generated,
/// untrusted text). Fail-soft: a non-zero exit or spawn error is logged via
/// `warn!` and otherwise ignored — this must never propagate an error that
/// could abort `run_gate`.
fn run_notify_command(cmd: &str, phase: u32, stage: Stage, context: &str, unexpected: bool) {
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

/// Write `contents` to `path` atomically: write a temp file in the same
/// directory, then rename over the target so readers never see a partial write.
fn write_atomic(path: &Path, contents: &str) -> Result<(), GateError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
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
            phase: 11,
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
        let path = Gates::write_gate(dir.path(), 11, Stage::Validate, "ctx").unwrap();
        assert!(path.ends_with(".devflow/gates/11-validate.json"));
        assert!(path.exists());
        let gate: GateFile =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(gate.phase, 11);
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
        let path = Gates::response_path(dir.path(), 11, Stage::Validate);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&response).unwrap()).unwrap();

        let got = Gates::poll_response(dir.path(), 11, Stage::Validate, 1).unwrap();
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
        let path = Gates::response_path(dir.path(), 11, Stage::Validate);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&response).unwrap()).unwrap();

        let started = std::time::Instant::now();
        let got = Gates::poll_response(dir.path(), 11, Stage::Validate, SEVEN_DAYS).unwrap();

        assert_eq!(got, response);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn poll_response_times_out_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Gates::poll_response(dir.path(), 11, Stage::Ship, 0).is_none());
    }

    #[test]
    fn ack_writes_received_true() {
        let dir = tempfile::tempdir().unwrap();
        let path = Gates::ack(dir.path(), 11, Stage::Ship).unwrap();
        let ack: GateAck = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(ack.received);
    }

    #[test]
    fn cleanup_removes_all_three_files_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        Gates::write_gate(dir.path(), 11, Stage::Validate, "ctx").unwrap();
        Gates::ack(dir.path(), 11, Stage::Validate).unwrap();
        std::fs::write(
            Gates::response_path(dir.path(), 11, Stage::Validate),
            "{\"approved\":true}",
        )
        .unwrap();

        Gates::cleanup(dir.path(), 11, Stage::Validate).unwrap();
        assert!(!Gates::gate_path(dir.path(), 11, Stage::Validate).exists());
        assert!(!Gates::response_path(dir.path(), 11, Stage::Validate).exists());
        assert!(!Gates::ack_path(dir.path(), 11, Stage::Validate).exists());
        // Idempotent: cleaning again with nothing present succeeds.
        Gates::cleanup(dir.path(), 11, Stage::Validate).unwrap();
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
        run_notify_command(&cmd, 11, Stage::Ship, "ctx", false);
        assert!(sentinel.exists());
    }

    #[test]
    fn notify_hook_failure_is_fail_soft() {
        // A command that always fails must not panic or otherwise abort the
        // caller — fail-soft per T-13-02.
        run_notify_command("exit 1", 11, Stage::Ship, "ctx", false);
    }

    #[test]
    fn notify_hook_sets_non_silent_flag() {
        let dir = tempfile::tempdir().unwrap();

        let sentinel_unexpected = dir.path().join("unexpected");
        let cmd_unexpected = format!(
            "echo -n \"$DEVFLOW_NON_SILENT_GATE\" > {}",
            sentinel_unexpected.display()
        );
        run_notify_command(&cmd_unexpected, 11, Stage::Code, "ctx", true);
        assert_eq!(
            std::fs::read_to_string(&sentinel_unexpected).unwrap(),
            "1"
        );

        let sentinel_expected = dir.path().join("expected");
        let cmd_expected = format!(
            "echo -n \"$DEVFLOW_NON_SILENT_GATE\" > {}",
            sentinel_expected.display()
        );
        run_notify_command(&cmd_expected, 11, Stage::Ship, "ctx", false);
        assert_eq!(std::fs::read_to_string(&sentinel_expected).unwrap(), "0");
    }

    /// This test mutates process-global env, so it acquires `ENV_MUTEX` to
    /// avoid racing any other env-touching test in this module.
    #[test]
    fn notify_hook_unset_is_noop() {
        let _guard = ENV_MUTEX.lock().unwrap();
        // SAFETY: serialized under ENV_MUTEX — no other thread in this
        // process reads/writes DEVFLOW_GATE_NOTIFY_CMD concurrently.
        unsafe {
            std::env::remove_var("DEVFLOW_GATE_NOTIFY_CMD");
        }
        // Must return normally without touching the filesystem or panicking.
        fire_gate_notify(11, Stage::Ship, "ctx", false);
    }
}
