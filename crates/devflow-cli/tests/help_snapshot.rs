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
