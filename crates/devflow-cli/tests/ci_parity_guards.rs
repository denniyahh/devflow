//! Regression guards for the CI/local parity contract.
//!
//! **Structure changed (Phase 23 follow-up).** CI used to inline its cargo
//! commands in `.github/workflows/ci.yml`, and a second workflow
//! (`devcontainer.yml`) re-ran them inside the devcontainer via a `runCmd:
//! |` block. Both are gone: every CI job now runs the pinned devcontainer
//! image and invokes `scripts/check.sh`, the same script developers run via
//! `scripts/check-in-container.sh`. One definition of "green", one image.
//!
//! The historical findings these tests encode did not go away with that
//! restructure — they moved:
//!
//! * **CR-01 (15-REVIEW.md).** `devcontainer.yml`'s `runCmd` ran several
//!   commands under `bash -c` with no `set -e`, so only the *last* command's
//!   exit code counted and a failing `cargo test` could not fail the job.
//!   That block no longer exists; the equivalent exposure is now
//!   `scripts/check.sh` running several cargo commands in sequence, so this
//!   asserts the script fails fast before its first cargo invocation.
//!
//! * **WR-08 (17-REVIEW.md).** `cargo clippy -- -D warnings` does not compile
//!   test targets, so a lint firing only inside a `#[cfg(test)]` module
//!   passes it. WR-08 verified this empirically: injecting
//!   `format!("{}", (&r).to_string())` into a test module gives exit 0 under
//!   the narrow form and exit 101 under `--workspace --all-targets`. The
//!   clippy invocation now lives in `scripts/check.sh`, so that is where the
//!   scope is asserted — plus a check that the workflow really does delegate
//!   to the script, without which the scope guard would be guarding a file
//!   CI no longer runs.
//!
//! * **WR-10 (17-REVIEW.md).** `devflow test` is documented as the local
//!   mirror of CI, so its clippy scope must match. Unchanged.

use std::path::{Path, PathBuf};

/// Cargo test binaries run with cwd = the crate dir; these files live at the
/// repo root.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Every `cargo clippy` invocation must lint the whole workspace *including
/// test targets*, or the gate silently stops covering `#[cfg(test)]` code —
/// which is the majority of this repo's `unsafe` blocks. See WR-08.
fn assert_clippy_lines_are_workspace_wide(source: &str, path: &Path) {
    let clippy_lines: Vec<&str> = source
        .lines()
        .map(str::trim)
        .map(|l| l.trim_start_matches("- run:").trim())
        .filter(|l| l.starts_with("cargo clippy"))
        .collect();

    assert!(
        !clippy_lines.is_empty(),
        "expected at least one `cargo clippy` invocation in {}",
        path.display()
    );

    for line in &clippy_lines {
        assert!(
            line.contains("--workspace") && line.contains("--all-targets"),
            "clippy invocation in {} must include both `--workspace` and \
             `--all-targets` — the narrow `cargo clippy -- -D warnings` form \
             does not compile test targets, so lints inside `#[cfg(test)]` \
             modules go undetected (17-REVIEW.md WR-08). Found: {line:?}",
            path.display()
        );
        assert!(
            line.contains("-D warnings"),
            "clippy invocation in {} must fail on warnings (`-D warnings`). \
             Found: {line:?}",
            path.display()
        );
    }
}

/// CR-01, at its new home. `scripts/check.sh` runs fmt, clippy and test in
/// sequence; without fail-fast an early failure would be masked by a later
/// command's exit code, exactly the CI-lies-to-you bug CR-01 fixed.
#[test]
fn check_script_fails_fast_before_any_cargo_invocation() {
    let path = repo_root().join("scripts/check.sh");
    let script = read(&path);

    let lines: Vec<&str> = script
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    let set_idx = lines
        .iter()
        .position(|l| l.starts_with("set -") && l.contains('e'))
        .expect("expected a `set -e`-family line in scripts/check.sh");

    assert!(
        lines[set_idx].contains("-euo pipefail") || lines[set_idx].contains("pipefail"),
        "scripts/check.sh must use `set -euo pipefail`, not a weaker form — \
         an unset variable or a failing command in a pipe would otherwise \
         pass silently. Found: {:?}",
        lines[set_idx]
    );

    if let Some(first_cargo_idx) = lines.iter().position(|l| l.contains("cargo ")) {
        assert!(
            set_idx < first_cargo_idx,
            "fail-fast (line {set_idx}) must precede every `cargo` invocation \
             (first at line {first_cargo_idx}) — see 15-REVIEW.md CR-01."
        );
    }
}

/// WR-08, at its new home: the clippy scope now lives in the shared script.
#[test]
fn check_script_clippy_lints_test_targets() {
    let path = repo_root().join("scripts/check.sh");
    let script = read(&path);
    assert_clippy_lines_are_workspace_wide(&script, &path);
}

/// The scope guard above only means something if CI actually runs that
/// script. Without this, someone could reinline a narrow `cargo clippy` into
/// the workflow and both guards would still pass while CI linted nothing.
#[test]
fn ci_workflow_delegates_to_the_shared_check_script() {
    let path = repo_root().join(".github/workflows/ci.yml");
    let workflow = read(&path);

    for target in [
        "scripts/check.sh test",
        "scripts/check.sh clippy",
        "scripts/check.sh fmt",
    ] {
        assert!(
            workflow.contains(target),
            "{} must invoke `{target}` so local and CI runs execute the same \
             commands. If this moved, move the parity guards with it.",
            path.display()
        );
    }

    let inlined: Vec<&str> = workflow
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("- run: cargo ") || l.starts_with("run: cargo "))
        .collect();
    assert!(
        inlined.is_empty(),
        "{} must not invoke cargo directly — every check goes through \
         scripts/check.sh so CI and local cannot drift. Found: {inlined:?}",
        path.display()
    );
}

/// CI must run the same pinned image the devcontainer declares, or "local
/// parity" is a claim with nothing behind it.
#[test]
fn ci_workflow_runs_the_pinned_devcontainer_image() {
    let root = repo_root();
    let workflow = read(&root.join(".github/workflows/ci.yml"));
    let devcontainer = read(&root.join(".devcontainer/devcontainer.json"));

    let image = devcontainer
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("\"image\""))
        .and_then(|l| l.split('"').nth(3))
        .expect("read \"image\" from devcontainer.json")
        .to_string();

    assert!(
        !image.contains(":latest") && image.contains(':'),
        "devcontainer image must be pinned to an explicit tag, never a \
         floating one. Found: {image:?}"
    );

    let container_lines = workflow
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("image:"))
        .count();
    assert!(
        container_lines > 0,
        "ci.yml must run its jobs in a `container:` so the OS and toolchain \
         match the devcontainer"
    );

    for line in workflow
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("image:"))
    {
        let ci_image = line.trim_start_matches("image:").trim();
        assert_eq!(
            ci_image, image,
            "ci.yml runs {ci_image:?} but devcontainer.json declares \
             {image:?} — they must be identical or local checks stop \
             predicting CI (scripts/assert-image-parity.sh enforces this at \
             runtime too)."
        );
    }
}

/// `devflow test` is documented as the local mirror of CI's quality gates,
/// so its clippy invocation must match the workflows' scope. It ran the
/// narrow pre-Phase-17 form until this fix (17-REVIEW.md WR-10), which made
/// a local green weaker than a CI green — a false-green generator.
#[test]
fn devflow_test_clippy_matches_ci_scope() {
    // 19-09: `test_cmd` (the `devflow test` handler) moved from `main.rs`
    // into `commands.rs` as part of the main.rs decomposition — this
    // regression guard's target path moved with it.
    let path = repo_root().join("crates/devflow-cli/src/commands.rs");
    let src = read(&path);

    let has_narrow_form = src.contains("\"cargo clippy -- -D warnings\"");
    assert!(
        !has_narrow_form,
        "`devflow test` must not use the narrow `cargo clippy -- -D warnings` \
         form — it does not compile test targets, making a local green weaker \
         than a CI green (17-REVIEW.md WR-10). See {}",
        path.display()
    );
}
