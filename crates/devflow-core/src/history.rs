//! Read-only per-phase attempt history assembled from existing DevFlow stores.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{agent_result, events};
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
        assert!(timeline.entries[1]
            .capture_files
            .iter()
            .all(|path| path.starts_with(&captures)));
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
        assert_eq!(render_timeline(&timeline), "no attempts recorded for phase 42");
    }
}
