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
        let paths = [
            Self::gate_path(project_root, phase, stage),
            Self::response_path(project_root, phase, stage),
            Self::ack_path(project_root, phase, stage),
        ];
        let mut removed = false;
        for path in &paths {
            if path.exists() {
                std::fs::remove_file(path)?;
                removed = true;
            }
        }
        let dir = Self::dir(project_root);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(removed);
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
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.ends_with(".tmp") && prefixes.iter().any(|prefix| name.starts_with(prefix)) {
                std::fs::remove_file(entry.path())?;
                removed = true;
            }
        }
        Ok(removed)
    }
}

/// Fire the operator-configured gate notify hook, if any.
///
/// Reads `DEVFLOW_GATE_NOTIFY_CMD`; if unset or empty, this is a silent no-op
/// (no notify command configured). Otherwise delegates to
/// [`run_notify_command`]. `unexpected` marks a gate fired on a stage the
/// active [`crate::mode::Mode`] would not normally gate (e.g. a Define/Plan/Code
/// failure in Auto mode) — a never-silent gate per WR-11.
pub fn fire_gate_notify(phase: PhaseId, stage: Stage, context: &str, unexpected: bool) {
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
}
