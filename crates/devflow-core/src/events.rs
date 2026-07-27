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
        && let Err(err) = crate::workflow::ensure_devflow_dir(parent)
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

/// A phase's latest event plus its newest matching `stage_launched`
/// timestamp, both from the same one-pass read (999.30 / DEN-55 IN-01) —
/// `devflow status` needs the real stage-entry time (21a) alongside the
/// last-action line, and previously re-scanned the whole log per phase to
/// get it (`latest_stage_launched_ts`, now folded in here).
#[derive(Debug, Clone)]
pub struct PhaseEventSummary {
    pub event: serde_json::Value,
    pub stage_launched_ts: Option<u64>,
}

/// The most recent event per phase, from ONE read + parse pass over the log
/// (14-CR-10) — `devflow status` renders N phases without N full-file scans.
/// Each summary's `stage_launched_ts` is the newest matching `stage_launched`
/// event's `ts`, independent of what the latest event overall is: a later
/// `transition`, `gate_fired`, or corrupt line never clears an
/// already-recorded launch timestamp.
pub fn last_events_by_phase(
    project_root: &Path,
) -> std::collections::HashMap<u32, PhaseEventSummary> {
    use std::collections::hash_map::Entry;

    let mut latest: std::collections::HashMap<u32, PhaseEventSummary> =
        std::collections::HashMap::new();
    let Ok(contents) = std::fs::read_to_string(events_path(project_root)) else {
        return latest;
    };
    for event in contents
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    {
        let Some(phase) = event.get("phase").and_then(|p| p.as_u64()) else {
            continue;
        };
        let phase = phase as u32;
        let launch_ts = (event.get("event").and_then(|e| e.as_str()) == Some("stage_launched"))
            .then(|| event.get("ts").and_then(|t| t.as_u64()))
            .flatten();
        // Later lines overwrite the latest event by append order; the
        // launch timestamp only moves forward when THIS line is itself a
        // valid stage_launched event.
        match latest.entry(phase) {
            Entry::Occupied(mut occupied) => {
                let summary = occupied.get_mut();
                summary.event = event;
                if let Some(ts) = launch_ts {
                    summary.stage_launched_ts = Some(ts);
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(PhaseEventSummary {
                    event,
                    stage_launched_ts: launch_ts,
                });
            }
        }
    }
    latest
}

/// Read the last event line recorded for `phase`, if any.
pub fn last_event_for_phase(project_root: &Path, phase: u32) -> Option<serde_json::Value> {
    last_events_by_phase(project_root)
        .remove(&phase)
        .map(|summary| summary.event)
}

/// Read the last event line for `phase` whose `event` field equals `event`,
/// if any. Scans the log line by line, parsing each as JSON; an unparsable
/// line is skipped, not fatal — the log is append-only and a torn final line
/// must not make the whole history unreadable. A missing file returns `None`.
///
/// This is a targeted, single-event-kind scan, distinct from
/// [`last_events_by_phase`]'s one-pass "latest event of any kind per phase"
/// read — `ship_evidence::collect` needs the last occurrence of one
/// *specific* event name, which the latest-of-any-kind index does not
/// preserve once a later, different event overwrites it.
pub fn last_event_of_kind_for_phase(
    project_root: &Path,
    phase: u32,
    event: &str,
) -> Option<serde_json::Value> {
    let contents = std::fs::read_to_string(events_path(project_root)).ok()?;
    let mut last = None;
    for value in contents
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    {
        if value.get("phase").and_then(|p| p.as_u64()) == Some(phase as u64)
            && value.get("event").and_then(|e| e.as_str()) == Some(event)
        {
            last = Some(value);
        }
    }
    last
}

/// Whether `phase` has ever emitted an event named `event`. Implemented as
/// [`last_event_of_kind_for_phase`]`.is_some()` so there is one scanner, not
/// two.
pub fn has_event_for_phase(project_root: &Path, phase: u32, event: &str) -> bool {
    last_event_of_kind_for_phase(project_root, phase, event).is_some()
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

    /// 14-CR-10: one pass over the log yields every phase's latest event.
    #[test]
    fn last_events_by_phase_collects_latest_per_phase_in_one_pass() {
        let dir = tempfile::tempdir().unwrap();
        emit(dir.path(), 1, "workflow_started", serde_json::Value::Null);
        emit(dir.path(), 2, "workflow_started", serde_json::Value::Null);
        emit(
            dir.path(),
            1,
            "transition",
            serde_json::json!({"to": "plan"}),
        );
        emit(
            dir.path(),
            2,
            "gate_fired",
            serde_json::json!({"stage": "ship"}),
        );

        let latest = last_events_by_phase(dir.path());
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[&1].event["event"], "transition");
        assert_eq!(latest[&2].event["event"], "gate_fired");
        assert!(last_events_by_phase(&dir.path().join("empty")).is_empty());
    }

    /// 999.30 / DEN-55 IN-01: the summary's `stage_launched_ts` tracks the
    /// NEWEST matching `stage_launched` event from the same one-pass read,
    /// independent of the latest event overall — a later non-launch event
    /// or a corrupt line must never clear an already-recorded timestamp.
    #[test]
    fn last_events_by_phase_tracks_newest_stage_launched_ts_across_the_pass() {
        let dir = tempfile::tempdir().unwrap();

        // No stage_launched at all yet.
        emit(dir.path(), 1, "workflow_started", serde_json::Value::Null);
        assert_eq!(last_events_by_phase(dir.path())[&1].stage_launched_ts, None);

        // First stage_launched.
        emit(
            dir.path(),
            1,
            "stage_launched",
            serde_json::json!({"stage": "define"}),
        );
        let first_ts = last_events_by_phase(dir.path())[&1]
            .stage_launched_ts
            .expect("stage_launched recorded a timestamp");

        // A later, unrelated event must not clear the launch timestamp,
        // even though it becomes the new latest event.
        emit(
            dir.path(),
            1,
            "transition",
            serde_json::json!({"to": "plan"}),
        );
        let after_transition = last_events_by_phase(dir.path());
        assert_eq!(after_transition[&1].stage_launched_ts, Some(first_ts));
        assert_eq!(after_transition[&1].event["event"], "transition");

        // A newer stage_launched wins over the first.
        emit(
            dir.path(),
            1,
            "stage_launched",
            serde_json::json!({"stage": "plan"}),
        );
        let path = events_path(dir.path());
        let mut contents = std::fs::read_to_string(&path).unwrap();
        // A corrupt trailing line must not affect the parsed result.
        contents.push_str("{truncated\n");
        std::fs::write(&path, contents).unwrap();

        let latest = last_events_by_phase(dir.path());
        let newest_ts = latest[&1]
            .stage_launched_ts
            .expect("newest stage_launched timestamp present");
        assert!(newest_ts >= first_ts, "newest launch must not be older");
        assert_eq!(latest[&1].event["event"], "stage_launched");
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

    // These tests deliberately use a generic marker event name rather than
    // any production event literal, keeping this generic scanner's tests
    // decoupled from any one caller's naming choice. `ship_evidence::tests`
    // exercises this same scanner against its real, specific event name.
    const MARKER_EVENT: &str = "marker_event";

    #[test]
    fn last_event_of_kind_for_phase_filters_by_phase_and_event_name() {
        let dir = tempfile::tempdir().unwrap();
        emit(dir.path(), 1, "workflow_finished", serde_json::Value::Null);
        emit(
            dir.path(),
            1,
            MARKER_EVENT,
            serde_json::json!({"stage": "ship"}),
        );
        emit(
            dir.path(),
            2,
            MARKER_EVENT,
            serde_json::json!({"stage": "ship"}),
        );

        let phase1 = last_event_of_kind_for_phase(dir.path(), 1, MARKER_EVENT)
            .expect("phase 1 marker event exists");
        assert_eq!(phase1["stage"], "ship");
        assert!(has_event_for_phase(dir.path(), 1, MARKER_EVENT));
        assert!(!has_event_for_phase(dir.path(), 3, MARKER_EVENT));
        assert!(last_event_of_kind_for_phase(dir.path(), 3, MARKER_EVENT).is_none());
    }

    #[test]
    fn last_event_of_kind_for_phase_skips_corrupt_lines() {
        let dir = tempfile::tempdir().unwrap();
        emit(dir.path(), 4, MARKER_EVENT, serde_json::Value::Null);
        let path = events_path(dir.path());
        let mut contents = std::fs::read_to_string(&path).unwrap();
        contents.push_str("{truncated\n");
        std::fs::write(&path, contents).unwrap();

        assert!(has_event_for_phase(dir.path(), 4, MARKER_EVENT));
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
