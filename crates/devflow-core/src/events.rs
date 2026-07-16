//! Append-only workflow event log — `.devflow/events.jsonl`.
//!
//! One JSON object per line, schema v1:
//!
//! ```json
//! {"v":1,"ts":1752600000,"phase":14,"event":"transition","from":"code","to":"validate"}
//! ```
//!
//! Every line carries `v`, `ts` (unix seconds), `phase`, and `event`; the
//! remaining fields are kind-specific. The log exists so any frontend (TUI,
//! Hermes plugin, web) can observe a running loop by tailing one file instead
//! of integrating with DevFlow internals — it is the read side of the gate
//! notify hook's push side.
//!
//! Emission is **fail-soft**: an unwritable log warns and returns — recording
//! an event must never abort the workflow it records. Appends are a single
//! `write_all` of a complete line on an `O_APPEND` handle, so concurrent
//! phase monitors' lines interleave without tearing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

/// Path of a project's event log.
pub fn events_path(project_root: &Path) -> PathBuf {
    project_root.join(".devflow").join("events.jsonl")
}

/// Schema version stamped on every line.
const SCHEMA_VERSION: u32 = 1;

/// Append one event line. `fields` supplies the kind-specific payload and
/// must be a JSON object (anything else is recorded under a `"data"` key).
pub fn emit(project_root: &Path, phase: u32, event: &str, fields: serde_json::Value) {
    let mut line = serde_json::json!({
        "v": SCHEMA_VERSION,
        "ts": unix_now(),
        "phase": phase,
        "event": event,
    });
    match fields {
        serde_json::Value::Object(map) => {
            let base = line.as_object_mut().expect("line is an object");
            for (key, value) in map {
                // Envelope keys win — a payload must not be able to forge
                // another phase's identity or a different event kind.
                base.entry(key).or_insert(value);
            }
        }
        serde_json::Value::Null => {}
        other => {
            line["data"] = other;
        }
    }
    let path = events_path(project_root);
    if let Some(parent) = path.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        warn!("could not create events dir: {err}");
        return;
    }
    let result = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(format!("{line}\n").as_bytes()));
    if let Err(err) = result {
        warn!("could not append to {}: {err}", path.display());
    }
}

/// Read the last event line recorded for `phase`, if any. Used by
/// `devflow status` to show a phase's most recent action.
pub fn last_event_for_phase(project_root: &Path, phase: u32) -> Option<serde_json::Value> {
    let contents = std::fs::read_to_string(events_path(project_root)).ok()?;
    contents
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| event.get("phase").and_then(|p| p.as_u64()) == Some(phase as u64))
}

/// Render an event as a short human-readable summary ("gate_fired (ship)").
pub fn describe(event: &serde_json::Value) -> String {
    let kind = event
        .get("event")
        .and_then(|e| e.as_str())
        .unwrap_or("unknown");
    let detail = ["to", "stage", "status", "hook", "reason"]
        .iter()
        .find_map(|key| event.get(*key).and_then(|v| v.as_str()));
    match detail {
        Some(detail) => format!("{kind} ({detail})"),
        None => kind.to_string(),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_lines(root: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(events_path(root))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).expect("every line parses as JSON"))
            .collect()
    }

    #[test]
    fn emit_appends_parseable_lines_with_envelope_fields() {
        let dir = tempfile::tempdir().unwrap();
        emit(
            dir.path(),
            14,
            "transition",
            serde_json::json!({"from": "code", "to": "validate"}),
        );
        emit(
            dir.path(),
            15,
            "gate_fired",
            serde_json::json!({"stage": "ship"}),
        );

        let lines = read_lines(dir.path());
        assert_eq!(lines.len(), 2);
        for line in &lines {
            assert_eq!(line["v"], 1);
            assert!(line["ts"].as_u64().is_some());
            assert!(line["phase"].as_u64().is_some());
            assert!(line["event"].as_str().is_some());
        }
        assert_eq!(lines[0]["phase"], 14);
        assert_eq!(lines[0]["from"], "code");
        assert_eq!(lines[1]["phase"], 15);
        assert_eq!(lines[1]["stage"], "ship");
    }

    #[test]
    fn emit_never_lets_payload_forge_envelope_keys() {
        let dir = tempfile::tempdir().unwrap();
        emit(
            dir.path(),
            7,
            "transition",
            serde_json::json!({"phase": 99, "event": "forged", "note": "kept"}),
        );

        let lines = read_lines(dir.path());
        assert_eq!(lines[0]["phase"], 7, "envelope phase must win");
        assert_eq!(lines[0]["event"], "transition", "envelope event must win");
        assert_eq!(lines[0]["note"], "kept");
    }

    #[test]
    fn last_event_for_phase_filters_by_phase() {
        let dir = tempfile::tempdir().unwrap();
        emit(dir.path(), 1, "workflow_started", serde_json::Value::Null);
        emit(dir.path(), 2, "workflow_started", serde_json::Value::Null);
        emit(
            dir.path(),
            1,
            "transition",
            serde_json::json!({"to": "plan"}),
        );

        let last = last_event_for_phase(dir.path(), 1).expect("phase 1 events exist");
        assert_eq!(last["event"], "transition");
        let other = last_event_for_phase(dir.path(), 2).expect("phase 2 events exist");
        assert_eq!(other["event"], "workflow_started");
        assert!(last_event_for_phase(dir.path(), 3).is_none());
    }

    #[test]
    fn last_event_skips_corrupt_lines() {
        let dir = tempfile::tempdir().unwrap();
        emit(dir.path(), 4, "workflow_started", serde_json::Value::Null);
        let path = events_path(dir.path());
        let mut contents = std::fs::read_to_string(&path).unwrap();
        contents.push_str("{truncated\n");
        std::fs::write(&path, contents).unwrap();

        let last = last_event_for_phase(dir.path(), 4).expect("valid line still found");
        assert_eq!(last["event"], "workflow_started");
    }

    #[test]
    fn describe_prefers_detail_fields() {
        assert_eq!(
            describe(&serde_json::json!({"event": "transition", "to": "ship"})),
            "transition (ship)"
        );
        assert_eq!(
            describe(&serde_json::json!({"event": "workflow_finished"})),
            "workflow_finished"
        );
        assert_eq!(describe(&serde_json::json!({})), "unknown");
    }

    #[test]
    fn emit_is_fail_soft_on_unwritable_path() {
        // A file where the .devflow directory should be makes create_dir_all
        // fail; emit must not panic.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".devflow"), "not a dir").unwrap();
        emit(dir.path(), 1, "transition", serde_json::Value::Null);
    }
}
