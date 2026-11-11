//! `--help` snapshot guard (15a): the committed snapshot is the contract
//! between the CLI surface and the docs (OPERATIONS.md, README). If this
//! test fails, the CLI changed — update the docs, then regenerate:
//!
//! ```bash
//! cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt
//! ```
//!
//! The child is built by `common::hermetic_devflow`, so it never reads the
//! operator's real config or cache (999.141); the help text does not depend
//! on either.

mod common;

/// Normalize trailing whitespace per line so editor/clap cosmetic drift
/// doesn't produce false diffs.
fn normalized(text: &str) -> String {
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

#[test]
fn help_output_matches_committed_snapshot() {
    let home = common::HermeticHome::new();
    let output = common::hermetic_devflow(&home)
        .arg("--help")
        .output()
        .expect("run devflow --help");
    assert!(output.status.success());
    let actual = normalized(&String::from_utf8_lossy(&output.stdout));
    let expected = normalized(include_str!("snapshots/devflow-help.txt"));

    assert_eq!(
        actual, expected,
        "\n`devflow --help` drifted from tests/snapshots/devflow-help.txt.\n\
         If the CLI change is intentional: update OPERATIONS.md (and README \
         once rewritten), then regenerate the snapshot:\n\
         cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt\n"
    );
}

/// The `stop` line of a `devflow --help` output.
fn stop_line(help: &str) -> &str {
    help.lines()
        .find(|line| line.trim_start().starts_with("stop "))
        .expect("the help lists stop")
}

/// Whether a `stop` help line names the holder `stop` signals, and not the per-phase lock
/// that D-17 replaced.
fn names_the_foreground_holder(line: &str) -> bool {
    line.contains("foreground holder") && !line.contains("per-phase lock holder")
}

#[test]
fn stop_help_names_the_foreground_holder_not_the_phase_lock() {
    /// The line as it stood before the holder arm replaced the lock.
    const LINE_AS_IT_STOOD: &str = "  stop       End a running phase cleanly: answers its open gate with a rejection when a live waiter holds it; otherwise addresses the run's supervisor socket — `shutdown` when ALIVE, the guarded backstop when STALE, the per-phase lock holder when GONE. `--launch <id>` tears down only the socket with that launch id (the form `devflow doctor` prints for an orphan). Never signals a pid recorded in state. Idempotent: safe to run against an already-stopped, never-started, or already-dead phase";
    assert!(
        !names_the_foreground_holder(LINE_AS_IT_STOOD),
        "control: the check fails on the old line"
    );
    let line = stop_line(include_str!("snapshots/devflow-help.txt"));
    assert!(names_the_foreground_holder(line), "{line}");
}

/// Help terms for surface v4.0 removed (D-01, D-03): the Ship stage, and the rate-limit
/// cron arm cut in 52-09/52-10. Matched case-insensitively as whole words or phrases, so
/// `shipped` or `relationship` is not a hit. Later cut plans append to this list.
const REMOVED_HELP_TERMS: &[&str] = &["Ship", "cron arm"];

/// The terms of [`REMOVED_HELP_TERMS`] that `text` names as a whole word or phrase.
fn removed_terms_in(text: &str) -> Vec<&'static str> {
    let haystack = text.to_lowercase();
    REMOVED_HELP_TERMS
        .iter()
        .copied()
        .filter(|term| {
            let needle = term.to_lowercase();
            haystack.match_indices(&needle).any(|(at, _)| {
                let before = haystack[..at].chars().next_back();
                let after = haystack[at + needle.len()..].chars().next();
                !before.is_some_and(char::is_alphanumeric)
                    && !after.is_some_and(char::is_alphanumeric)
            })
        })
        .collect()
}

#[test]
fn help_names_no_removed_surface() {
    for planted in [
        "Begin the workflow for a phase: Define → Plan → Code → Validate → Ship",
        "Pipeline mode: `auto` runs to Ship unattended; `supervise` gates at Validate",
        "a rate limit with no fallback takes the cron arm",
    ] {
        assert!(
            !removed_terms_in(planted).is_empty(),
            "control: the matcher must report the 3.0.0 line {planted:?}"
        );
    }
    for clean in ["shipped", "relationship", "finished"] {
        assert!(
            removed_terms_in(clean).is_empty(),
            "control: {clean:?} is not a removed term"
        );
    }

    let home = common::HermeticHome::new();
    for args in [&["--help"][..], &["start", "--help"], &["resume", "--help"]] {
        let output = common::hermetic_devflow(&home)
            .args(args)
            .output()
            .expect("run devflow help");
        assert!(output.status.success(), "devflow {args:?} failed");
        let help = String::from_utf8_lossy(&output.stdout);
        let found = removed_terms_in(&help);
        assert!(
            found.is_empty(),
            "`devflow {}` still names {found:?}:\n{help}",
            args.join(" ")
        );
    }
}
