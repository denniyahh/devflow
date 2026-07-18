//! Read-only per-phase attempt history assembled from existing DevFlow stores.

use crate::{agent_result, events};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// One chronological point in a phase's history, with nearby retained
/// evidence attached to the event that produced it.
#[derive(Debug, Clone)]
pub struct AttemptEntry {
    pub timestamp: u64,
    pub event: Option<serde_json::Value>,
    pub capture_files: Vec<PathBuf>,
    pub review_files: Vec<PathBuf>,
}

/// The complete read-only attempt view for one phase.
#[derive(Debug, Clone)]
pub struct AttemptTimeline {
    pub phase: u32,
    pub entries: Vec<AttemptEntry>,
}

/// Correlate schema-v1 events, retained capture generations, and review
/// artifacts without creating a second history store.
pub fn attempt_timeline(project_root: &Path, phase: u32) -> AttemptTimeline {
    let mut indexed_events = std::fs::read_to_string(events::events_path(project_root))
        .unwrap_or_default()
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let event = serde_json::from_str::<serde_json::Value>(line).ok()?;
            (event.get("v").and_then(|v| v.as_u64()) == Some(1)
                && event.get("phase").and_then(|p| p.as_u64()) == Some(phase as u64))
            .then(|| {
                let timestamp = event.get("ts").and_then(|ts| ts.as_u64()).unwrap_or(0);
                (timestamp, index, event)
            })
        })
        .collect::<Vec<_>>();
    indexed_events.sort_by_key(|(timestamp, index, _)| (*timestamp, *index));

    let mut entries = indexed_events
        .into_iter()
        .map(|(timestamp, _, event)| AttemptEntry {
            timestamp,
            event: Some(event),
            capture_files: Vec::new(),
            review_files: Vec::new(),
        })
        .collect::<Vec<_>>();

    for (timestamp, files) in capture_generations(project_root, phase) {
        attach_artifacts(&mut entries, timestamp, files, true);
    }
    for review in review_files(project_root, phase) {
        let timestamp = modified_timestamp(&review);
        attach_artifacts(&mut entries, timestamp, vec![review], false);
    }
    entries.sort_by_key(|entry| entry.timestamp);

    AttemptTimeline { phase, entries }
}

/// Human-readable history output; event summaries deliberately reuse the
/// schema-v1 formatter used by `devflow status`.
pub fn render_timeline(timeline: &AttemptTimeline) -> String {
    if timeline.entries.is_empty() {
        return format!("no attempts recorded for phase {}", timeline.phase);
    }

    let mut rendered = format!("attempt history for phase {}\n", timeline.phase);
    for entry in &timeline.entries {
        let summary = entry
            .event
            .as_ref()
            .map(events::describe)
            .unwrap_or_else(|| "retained artifact".into());
        rendered.push_str(&format!("[{}] {summary}\n", entry.timestamp));
        for capture in &entry.capture_files {
            rendered.push_str(&format!("  capture: {}\n", capture.display()));
        }
        for review in &entry.review_files {
            rendered.push_str(&format!("  review: {}\n", review.display()));
        }
    }
    rendered.trim_end().to_string()
}

fn capture_generations(project_root: &Path, phase: u32) -> Vec<(u64, Vec<PathBuf>)> {
    let dir = agent_result::history_dir(project_root, phase);
    let Ok(files) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut generations: BTreeMap<String, (u64, Vec<PathBuf>)> = BTreeMap::new();
    for file in files.flatten() {
        let path = file.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let stamp = name
            .strip_suffix("-stdout")
            .or_else(|| name.strip_suffix("-exit"));
        let Some(stamp) = stamp else { continue };
        let Some(nanos) = stamp
            .split('-')
            .next()
            .and_then(|value| value.parse::<u128>().ok())
        else {
            continue;
        };
        let timestamp = (nanos / 1_000_000_000).min(u64::MAX as u128) as u64;
        generations
            .entry(stamp.to_string())
            .or_insert_with(|| (timestamp, Vec::new()))
            .1
            .push(path);
    }
    let mut generations = generations.into_values().collect::<Vec<_>>();
    for (_, files) in &mut generations {
        files.sort();
    }
    generations.sort_by_key(|(timestamp, _)| *timestamp);
    generations
}

fn review_files(project_root: &Path, phase: u32) -> Vec<PathBuf> {
    let phases = project_root.join(".planning").join("phases");
    let prefix = format!("{phase:02}-");
    let mut reviews = Vec::new();
    let Ok(dirs) = std::fs::read_dir(phases) else {
        return reviews;
    };
    for dir in dirs.flatten() {
        if !dir
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(&prefix))
        {
            continue;
        }
        collect_reviews(&dir.path(), &mut reviews);
    }
    reviews.sort_by_key(|path| (modified_timestamp(path), path.clone()));
    reviews
}

fn collect_reviews(dir: &Path, reviews: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            collect_reviews(&path, reviews);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("REVIEW.md"))
        {
            reviews.push(path);
        }
    }
}

fn modified_timestamp(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn attach_artifacts(
    entries: &mut Vec<AttemptEntry>,
    timestamp: u64,
    files: Vec<PathBuf>,
    captures: bool,
) {
    if entries.is_empty() {
        entries.push(AttemptEntry {
            timestamp,
            event: None,
            capture_files: Vec::new(),
            review_files: Vec::new(),
        });
    }
    let index = entries
        .iter()
        .rposition(|entry| entry.timestamp <= timestamp)
        .unwrap_or(0);
    if captures {
        entries[index].capture_files.extend(files);
    } else {
        entries[index].review_files.extend(files);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn seed_event_log(root: &Path) {
        let path = events::events_path(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            concat!(
                r#"{"v":1,"ts":30,"phase":16,"event":"hook_run","hook":"Merge"}"#,
                "\n",
                r#"{"v":1,"ts":10,"phase":16,"event":"transition","to":"code"}"#,
                "\n",
                r#"{"v":1,"ts":20,"phase":16,"event":"gate_fired","stage":"ship"}"#,
                "\n",
                r#"{"v":1,"ts":15,"phase":99,"event":"workflow_started"}"#,
                "\n",
            ),
        )
        .unwrap();
    }

    #[test]
    fn timeline_orders_events_and_correlates_retained_captures() {
        let dir = tempfile::tempdir().unwrap();
        seed_event_log(dir.path());
        let captures = agent_result::history_dir(dir.path(), 16);
        std::fs::create_dir_all(&captures).unwrap();
        std::fs::write(captures.join("20000000000-0-stdout"), "attempt output").unwrap();
        std::fs::write(captures.join("20000000000-0-exit"), "1").unwrap();

        let timeline = attempt_timeline(dir.path(), 16);

        assert_eq!(timeline.entries.len(), 3);
        assert_eq!(timeline.entries[0].timestamp, 10);
        assert_eq!(timeline.entries[1].timestamp, 20);
        assert_eq!(timeline.entries[2].timestamp, 30);
        assert_eq!(timeline.entries[1].capture_files.len(), 2);
        assert!(
            timeline.entries[1]
                .capture_files
                .iter()
                .all(|path| path.starts_with(&captures))
        );
        let rendered = render_timeline(&timeline);
        assert!(rendered.contains("transition (code)"));
        assert!(rendered.contains("gate_fired (ship)"));
        assert!(rendered.contains("capture:"));
    }

    #[test]
    fn empty_phase_has_clean_no_attempts_result() {
        let dir = tempfile::tempdir().unwrap();
        let timeline = attempt_timeline(dir.path(), 42);

        assert!(timeline.entries.is_empty());
        assert_eq!(
            render_timeline(&timeline),
            "no attempts recorded for phase 42"
        );
    }
}
