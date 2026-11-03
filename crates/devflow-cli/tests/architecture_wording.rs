//! ARCHITECTURE.md states the stream supervisor's drain as the code does it (D-13 item 8).
//! The bound is computed here from the code's own constants, and the source lines it rests
//! on are checked, so a change to either side fails this test rather than leaving the doc
//! stale.

use std::path::Path;

/// Read a file at a path relative to this crate.
fn read(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Whitespace collapsed, so a sentence wrapped across lines still matches.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn architecture_states_the_stream_reader_drain_bound() {
    let monitor = read("../devflow-core/src/monitor.rs");
    // The two waits the bound adds up: the join's own, and at most one more to take the
    // capture from a reader whose write is blocked.
    for source_line in [
        "const READER_DRAIN_WAIT: Duration = crate::agent::TERMINATE_VERIFY_WAIT;",
        "threads.join_within(READER_DRAIN_WAIT)",
        "capture_closed: close_shared_capture(&capture, wait),",
    ] {
        assert!(
            monitor.contains(source_line),
            "the bound below is computed from this line, which is gone: {source_line}"
        );
    }
    let reader_drain_secs = devflow_core::agent::TERMINATE_VERIFY_WAIT.as_secs();
    let bound_secs = 2 * reader_drain_secs;

    let architecture = one_line(&read("../../ARCHITECTURE.md"));
    assert!(
        !architecture.contains("stops reading when the agent"),
        "the reader keeps draining after the agent exits"
    );
    assert!(
        !architecture.contains("at most 3 s"),
        "3 s is one of the two waits, not the bound"
    );
    let sentence = format!(
        "waits up to `READER_DRAIN_WAIT` ({reader_drain_secs} s) for its pipe threads, then \
         takes the capture file from the reader, within one more `READER_DRAIN_WAIT` when a \
         capture write is blocked, so at most {bound_secs} s in all"
    );
    assert!(
        architecture.contains(&sentence),
        "ARCHITECTURE.md must state the bound the code computes: {sentence}"
    );
}
