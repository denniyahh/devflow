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

/// Trimmed, non-blank, non-comment lines. Guards that count occurrences must
/// use this: a bare search over the raw source counts prose, so documentation
/// mentioning a forbidden value would fail a guard about code.
fn code_lines(source: &str) -> Vec<&str> {
    source
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
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

/// CR-01 for `devcontainer.yml`'s `runCmd`, which runs under `bash -c`:
/// without `set -e` as the first line, only the LAST command's exit code
/// counts and a failing check cannot fail the job.
#[test]
fn devcontainer_runcmd_fails_fast_before_any_check() {
    let path = repo_root().join(".github/workflows/devcontainer.yml");
    let workflow = read(&path);

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
        .expect("find `runCmd: |` in devcontainer.yml");

    let mut cmd_lines = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if line.len() - line.trim_start().len() <= block_indent {
            break;
        }
        cmd_lines.push(line.trim());
    }

    assert!(
        !cmd_lines.is_empty(),
        "could not locate command lines inside `runCmd: |` in {}",
        path.display()
    );
    assert_eq!(
        cmd_lines[0], "set -e",
        "`runCmd`'s first command line must be exactly `set -e` (15-REVIEW.md \
         CR-01). Found: {:?}\nfull runCmd: {cmd_lines:#?}",
        cmd_lines[0]
    );
    assert!(
        cmd_lines.iter().any(|l| l.contains("scripts/check.sh")),
        "devcontainer.yml must delegate to scripts/check.sh so it does not \
         become a second definition of green. Found: {cmd_lines:#?}"
    );
}

/// This workflow's job name is a REQUIRED status check on `develop`, declared
/// in the `develop-merge-or-squash` ruleset — which classic branch protection
/// does not report. Deleting the workflow or renaming this job makes a
/// required check that can never report, wedging every merge to develop. That
/// happened on 2026-07-26; this guard exists so it cannot happen silently.
#[test]
fn devcontainer_job_name_matches_the_required_status_check() {
    let path = repo_root().join(".github/workflows/devcontainer.yml");
    let workflow = read(&path);
    assert!(
        workflow.contains("name: Build + test in devcontainer"),
        "{} must define a job named exactly `Build + test in devcontainer` — \
         it is a required status check on develop. Verify with:\n  \
         gh api repos/denniyahh/devflow/rules/branches/develop",
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

/// CI must carry the **load shape** the local pre-push gate already runs:
/// `fmt` → `clippy` → `test` sequentially, in one job, squeezed onto two
/// CPUs. Every reproduction of the 999.47 `/proc` fork-inheritance race
/// needed the sequential ordering *under CPU pressure*; `devcontainer.yml`
/// supplies the ordering but runs unpinned on the whole runner, so the pin is
/// the missing ingredient. See 46-CONTEXT.md D-01/D-02 (a new job in `ci.yml`
/// inside the pinned image, not a change to the required devcontainer job),
/// D-03 (the pin has ONE definition site, sourced by both consumers — a
/// re-typed literal is exactly the drift this forbids), and D-04 (the job
/// prints the shape it actually got).
///
/// Deleting the job, dropping the `taskset` wrapper, or re-typing the CPU
/// list into the workflow all fail here rather than silently reverting CI to
/// the three-parallel-jobs shape that has rejected 0 of the pushes the local
/// gate rejected 2 of 2.
#[test]
fn ci_workflow_runs_the_sequential_check_under_a_cpu_pin() {
    let root = repo_root();
    let path = root.join(".github/workflows/ci.yml");
    let workflow = read(&path);

    assert!(
        workflow.contains(&format!("name: {SEQUENTIAL_JOB_NAME}")),
        "{} must define a job named exactly `Sequential 2-CPU check` — it is \
         the only place CI runs the local gate's sequential-under-CPU-pressure \
         load shape (46-CONTEXT.md D-01/D-02). Without it CI runs three \
         parallel jobs on an unpinned runner and cannot see the interleaving \
         the local gate sees.",
        path.display()
    );

    let pinned: Vec<&str> = workflow
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("taskset -c") && l.contains("scripts/check.sh all"))
        .collect();
    assert!(
        !pinned.is_empty(),
        "{} must run `scripts/check.sh all` wrapped in `taskset -c` — CPU \
         affinity is inherited across fork/exec, so the pin is what puts \
         rustc and the test harness threads under the same mask \
         (46-CONTEXT.md D-02). `--test-threads=N` is not a substitute: it \
         throttles the harness only, leaving compilation unpinned. \
         Found: {pinned:?}",
        path.display()
    );

    assert!(
        workflow.contains("scripts/lib/ci-cpus.sh"),
        "{} must read the CPU list from `scripts/lib/ci-cpus.sh` rather than \
         re-typing it — the local gate reads the same file, and a second \
         definition site is the drift 46-CONTEXT.md D-03 exists to prevent.",
        path.display()
    );

    let fragment = root.join("scripts/lib/ci-cpus.sh");
    assert!(
        fragment.is_file(),
        "{} must exist — it is the single definition site both \
         scripts/check-in-container.sh and .github/workflows/ci.yml source \
         (46-CONTEXT.md D-03). A missing fragment makes the CI sourcing step \
         fail loudly under `set -e` rather than running the suite unpinned.",
        fragment.display()
    );
}

/// The CPU list the pinned CI job and the local pre-push gate share must have
/// exactly ONE definition site (46-CONTEXT.md D-03). Two copies let the gate
/// and CI measure different load shapes while both report green — and the
/// whole reason the pinned job exists is that its load shape matches the
/// gate's.
///
/// This repository has already been bitten by precisely this class once: the
/// container image tag is duplicated into `ci.yml` because GitHub Actions
/// cannot interpolate `env` into `jobs.*.container.image`, and the fix was
/// `scripts/assert-image-parity.sh` — a mechanism, not a "keep in sync"
/// comment. A comment cannot fail a build. This test can.
///
/// Comment lines are stripped before counting, so prose that merely *mentions*
/// the value cannot trip the guard; the third demonstration in 46-01 confirms
/// that direction passes for the right reason.
#[test]
fn cpu_pin_has_exactly_one_definition_site() {
    let root = repo_root();

    // (a) The fragment defines the value exactly once.
    let fragment_path = root.join("scripts/lib/ci-cpus.sh");
    let fragment = read(&fragment_path);
    let definitions: Vec<&str> = code_lines(&fragment)
        .into_iter()
        .filter(|l| l.starts_with("CPUS="))
        .collect();
    assert_eq!(
        definitions.len(),
        1,
        "{} must contain exactly one `CPUS=` assignment — it is the single \
         definition site for the CI CPU pin (46-CONTEXT.md D-03), and a \
         second assignment makes which one wins depend on line order. \
         Found: {definitions:?}",
        fragment_path.display()
    );

    // (b) The local gate reads it rather than re-declaring it.
    let gate_path = root.join("scripts/check-in-container.sh");
    let gate = read(&gate_path);
    let redeclared: Vec<&str> = code_lines(&gate)
        .into_iter()
        .filter(|l| l.starts_with("CPUS="))
        .collect();
    assert!(
        redeclared.is_empty(),
        "{} must not assign `CPUS=` itself — it sources \
         scripts/lib/ci-cpus.sh, and re-typing the value here is exactly the \
         drift that lets the local gate and CI pin different core counts \
         while both look green (46-CONTEXT.md D-03). Found: {redeclared:?}",
        gate_path.display()
    );
    assert!(
        gate.contains("scripts/lib/ci-cpus.sh"),
        "{} must source scripts/lib/ci-cpus.sh — without it the gate runs \
         with CPUS unset, and `set -u` fails it rather than silently \
         unpinning, but the shared definition site is gone either way.",
        gate_path.display()
    );

    // (c) CI reads it rather than re-typing the literal.
    let ci_path = root.join(".github/workflows/ci.yml");
    let ci = read(&ci_path);
    let retyped: Vec<&str> = code_lines(&ci)
        .into_iter()
        .filter(|l| l.contains("0,1"))
        .collect();
    assert!(
        retyped.is_empty(),
        "{} must not re-type the CPU list literal — the \
         `Sequential 2-CPU check` job sources scripts/lib/ci-cpus.sh so the \
         pin has one definition site (46-CONTEXT.md D-03). A literal here \
         drifts from the local gate the moment either is bumped. \
         Found: {retyped:?}",
        ci_path.display()
    );
    assert!(
        ci.contains("scripts/lib/ci-cpus.sh"),
        "{} must source scripts/lib/ci-cpus.sh in the \
         `Sequential 2-CPU check` job. Dropping the source line while keeping \
         `taskset -c \"$CPUS\"` would expand to an empty list and change what \
         the job measures.",
        ci_path.display()
    );
}

/// The four status contexts BOTH `develop` and `main` require, declared in the
/// `develop-merge-or-squash` and `main-squash-only` rulesets. Three come from
/// `.github/workflows/ci.yml`; the fourth lives in `devcontainer.yml`.
const REQUIRED_STATUS_CONTEXTS: [&str; 4] =
    ["Test", "Clippy", "Format", "Build + test in devcontainer"];

/// The advisory sequential job added in 46-01. Deliberately absent from
/// `REQUIRED_STATUS_CONTEXTS`.
const SEQUENTIAL_JOB_NAME: &str = "Sequential 2-CPU check";

/// `Sequential 2-CPU check` is ADVISORY, and it is advisory for exactly one
/// reason: its name is not in the required-check set the branch rulesets
/// declare. Nothing in the workflow marks it advisory — there is no such key —
/// so a rename in EITHER direction changes its merge-blocking status silently,
/// and only this guard makes that loud. See 46-CONTEXT.md D-05.
///
/// Renaming it INTO the required set would not add a check; it would HIJACK an
/// existing required context, so the real `Test` (or whichever) would stop
/// reporting and every merge to that branch would wedge. That has happened
/// here before, on 2026-07-26, when deleting `devcontainer.yml` orphaned a
/// required context.
///
/// The `continue-on-error` assertion encodes the other half of D-05. That key
/// was considered and explicitly REJECTED: it makes a job report SUCCESS on a
/// real suite failure, which would poison every `gh pr checks` reading this
/// repository's own rules require as the acceptance evidence — a false-green
/// generator aimed squarely at the one signal used to accept the job.
#[test]
fn sequential_job_name_is_not_a_required_status_check() {
    let path = repo_root().join(".github/workflows/ci.yml");
    let workflow = read(&path);

    let job_name_line = format!("name: {SEQUENTIAL_JOB_NAME}");
    assert!(
        workflow.lines().map(str::trim).any(|l| l == job_name_line),
        "{} must define a job named exactly `{SEQUENTIAL_JOB_NAME}`. If it was \
         renamed, check whether the new name collides with a REQUIRED status \
         context before shipping — classic branch protection under-reports \
         them, so check the rulesets:\n  \
         gh api repos/denniyahh/devflow/rules/branches/develop\n  \
         gh api repos/denniyahh/devflow/rules/branches/main",
        path.display()
    );

    assert!(
        !REQUIRED_STATUS_CONTEXTS.contains(&SEQUENTIAL_JOB_NAME),
        "`{SEQUENTIAL_JOB_NAME}` must NOT be one of the required status \
         contexts {REQUIRED_STATUS_CONTEXTS:?} (46-CONTEXT.md D-05). Sharing a \
         name hijacks that required context rather than adding an advisory \
         one, so the real job stops reporting and merges wedge. Promotion is \
         deferred past this milestone and is a ruleset change, not a rename. \
         Verify the current set with:\n  \
         gh api repos/denniyahh/devflow/rules/branches/develop\n  \
         gh api repos/denniyahh/devflow/rules/branches/main"
    );

    let masking: Vec<&str> = code_lines(&workflow)
        .into_iter()
        .filter(|l| l.contains("continue-on-error"))
        .collect();
    assert!(
        masking.is_empty(),
        "{} must not set `continue-on-error` on any job — it reports SUCCESS \
         on a real suite failure, which is worse than no job at all because \
         `gh pr checks` then shows green for a red suite (46-CONTEXT.md D-05 \
         rejected it explicitly). Advisory-ness comes from being outside the \
         required-check set, not from masking the exit code. Found: {masking:?}",
        path.display()
    );
}
