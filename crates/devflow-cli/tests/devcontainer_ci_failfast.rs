//! Regression guard for CR-01 (15-REVIEW.md, this round): `.github/workflows/
//! devcontainer.yml`'s `runCmd` block ran multiple commands via `bash -c`
//! with no `set -e`, so an earlier failing command (e.g. `cargo test`) would
//! not fail the CI job — only the exit code of the *last* command counted.
//! Fixed in commit `3918792` by adding `set -e` as the first line inside
//! `runCmd`.
//!
//! A plain substring grep for `set -e` anywhere in the file is not a
//! sufficient regression guard: it would still pass if a future edit moved
//! `set -e` below a `cargo` invocation (reintroducing the exact bug this
//! commit fixed) or added a new command before it. This test asserts `set
//! -e` is literally the first non-blank command line inside `runCmd: |`,
//! and that it precedes every `cargo` invocation.

use std::path::PathBuf;

/// Cargo test binaries run with cwd = the crate dir, but the workflow file
/// lives at the repo root.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

/// Extract the literal lines of the `runCmd: |` block from the workflow
/// YAML by line-matching indentation, rather than pulling in a YAML parser
/// dependency just for this one check.
fn run_cmd_lines(workflow: &str) -> Vec<&str> {
    let mut lines = workflow.lines();
    for line in lines.by_ref() {
        if line.trim_start() == "runCmd: |" {
            break;
        }
    }
    let block_indent = workflow
        .lines()
        .find(|l| l.trim_start() == "runCmd: |")
        .map(|l| l.len() - l.trim_start().len())
        .expect("find runCmd: | line");

    let mut cmd_lines = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        // Block scalar ends once indentation drops back to (or below) the
        // `runCmd: |` key's own indentation.
        if indent <= block_indent {
            break;
        }
        cmd_lines.push(line.trim());
    }
    cmd_lines
}

#[test]
fn devcontainer_runcmd_fails_fast_before_any_cargo_invocation() {
    let path = repo_root().join(".github/workflows/devcontainer.yml");
    let workflow =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let cmd_lines = run_cmd_lines(&workflow);
    assert!(
        !cmd_lines.is_empty(),
        "could not locate any command lines inside `runCmd: |` in {}",
        path.display()
    );

    assert_eq!(
        cmd_lines[0], "set -e",
        "`runCmd`'s first command line must be exactly `set -e`, so an \
         earlier failing command (e.g. `cargo test`) fails the CI job \
         instead of being masked by a later command's exit code — see \
         15-REVIEW.md CR-01 (commit 3918792). Found first line: {:?}\n\
         full runCmd lines: {cmd_lines:#?}",
        cmd_lines[0]
    );

    let first_cargo_idx = cmd_lines
        .iter()
        .position(|l| l.starts_with("cargo "))
        .expect("expected at least one `cargo` invocation in runCmd");
    let set_e_idx = cmd_lines
        .iter()
        .position(|l| *l == "set -e")
        .expect("expected `set -e` in runCmd");
    assert!(
        set_e_idx < first_cargo_idx,
        "`set -e` (line {set_e_idx}) must precede every `cargo` invocation \
         (first at line {first_cargo_idx}) — see 15-REVIEW.md CR-01.\n\
         full runCmd lines: {cmd_lines:#?}"
    );
}
