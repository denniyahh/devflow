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
/// Deleting the job, dropping the pin, or re-typing the CPU list into the
/// workflow all fail here rather than silently reverting CI to the
/// three-parallel-jobs shape that has rejected 0 of the pushes the local gate
/// rejected 2 of 2.
///
/// **The pin is invoked through `cpu_pin_prefix` (46-04), not through a
/// literal `taskset` line.** Do not go looking for `taskset -c` in `ci.yml` —
/// the suite step expands `"${CPU_PIN[@]}"`, which the fragment sets to
/// `(taskset -c "$CPUS")` or, under `DEVFLOW_CI_CPUS=all`, to an empty array.
///
/// **Job-scoped since 46-04 (46-REVIEWS.md C-02).** The three assertions used
/// to search the whole file independently, so a comment or a decoy job could
/// supply all three tokens while `sequential` itself carried none. Both
/// bypasses were demonstrated. The logic now lives in
/// `recognise_pinned_sequential_job`, a function over `&str` whose negative
/// cases are exercised against fixtures in `tests/fixtures/ci-parity/` —
/// without those, the guard can only ever be observed passing on the live
/// file, which cannot be made to fail on demand.
#[test]
fn ci_workflow_runs_the_sequential_check_under_a_cpu_pin() {
    let root = repo_root();
    let path = root.join(".github/workflows/ci.yml");
    let workflow = read(&path);

    if let Err(why) = recognise_pinned_sequential_job(&workflow) {
        panic!(
            "{}: {why}\n\n\
             The `{SEQUENTIAL_JOB_NAME}` job is the only place CI runs the local \
             gate's sequential-under-CPU-pressure load shape (46-CONTEXT.md \
             D-01/D-02). CPU affinity is inherited across fork/exec, so the pin \
             is what puts rustc AND the test harness threads under the same \
             mask; `--test-threads=N` is not a substitute, because it throttles \
             the harness only and leaves compilation unpinned. The CPU list \
             itself must come from scripts/lib/ci-cpus.sh rather than being \
             re-typed (46-CONTEXT.md D-03).",
            path.display()
        );
    }

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

/// A `run:` step in a **container** job defaults to `sh -e {0}`, not
/// `bash -e {0}` — GitHub only uses bash on a bare runner. This repo's images
/// are Debian-based, so `/bin/sh` is dash, which has no `set -o pipefail`.
///
/// This is not hypothetical. PR #208 shipped the `Sequential 2-CPU check` job
/// with two `set -euo pipefail` steps and no shell declaration. Both died on
/// their FIRST line — `set: Illegal option -o pipefail`, exit 2, 42 seconds —
/// so `scripts/check.sh` never executed, and the job reported as a red check
/// rather than as the misconfiguration it was. A reviewer reading only the
/// check name would conclude the suite failed under the pin. It had not run.
///
/// The failure mode this guards is therefore *not* "the job is red" — red was
/// at least visible. It is the inverse case: a bashism whose failure happens
/// to be benign under dash (`[[`, `${x,,}`, arrays) would let the step exit 0
/// having silently done the wrong thing, which is the same false-green class
/// as `${PIPESTATUS[0]}` expanding to nothing under the executor's zsh
/// (CLAUDE.md, verification habits).
///
/// Job-scoped rather than file-scoped on purpose: a bare `workflow.contains
/// ("shell: bash")` would pass while the declaration sat on some *other* job
/// than the one using the bashism.
#[test]
fn container_jobs_using_bash_syntax_declare_a_bash_shell() {
    let path = repo_root().join(".github/workflows/ci.yml");
    let workflow = read(&path);

    // Bash-only constructs that dash either rejects outright or mishandles.
    const BASHISMS: &[&str] = &["pipefail", "[[", "PIPESTATUS", "declare -A"];

    let jobs = split_jobs(&workflow);
    let keys: Vec<&str> = jobs.iter().map(|(k, _)| k.as_str()).collect();

    // ANTI-VACUITY, replacing the `jobs.len() >= 4` this used to carry
    // (46-REVIEWS.md C-02). That count was satisfiable by two NON-jobs: the
    // old splitter anchored to nothing, so `push:` and `pull_request:` under
    // the top-level `on:` block parsed as job headers and it reported 6
    // headers for 4 real jobs. Asserting the exact key list is what makes a
    // broken splitter loud — a broken splitter would otherwise pass this test
    // vacuously by finding no bashisms anywhere.
    for expected in LIVE_CI_JOB_KEYS {
        assert!(
            keys.contains(&expected),
            "expected to parse job `{expected}` out of {}, got {keys:?} — the \
             block splitter is broken",
            path.display()
        );
    }
    for not_a_job in ["push", "pull_request"] {
        assert!(
            !keys.contains(&not_a_job),
            "`{not_a_job}` is a key under the top-level `on:` block, not a job, \
             but the splitter returned it in {keys:?}. That is the exact \
             miscount 46-REVIEWS.md C-02 found: it let `jobs.len() >= 4` pass \
             on two non-jobs."
        );
    }

    for (name, body) in &jobs {
        let used: Vec<&str> = BASHISMS
            .iter()
            .copied()
            .filter(|b| body.contains(b))
            .collect();
        if used.is_empty() {
            continue;
        }
        assert!(
            body.contains("shell: bash"),
            "job `{name}` in {} uses bash-only syntax {used:?} but does not declare \
             `shell: bash`. Container jobs get `sh -e {{0}}` (dash), where \
             `set -o pipefail` is an ERROR and other bashisms fail quietly. Add:\n    \
             defaults:\n      run:\n        shell: bash\n  \
             to that job. See PR #208, where this cost the job its entire first run.",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// 46-04 / 46-REVIEWS.md C-02: a job-scoped, fixture-driven parity recogniser.
// ---------------------------------------------------------------------------

/// The four real jobs in `.github/workflows/ci.yml`. `push` and
/// `pull_request` are deliberately absent — they are keys under the top-level
/// `on:` block, and the pre-46-04 splitter counted them as jobs.
const LIVE_CI_JOB_KEYS: [&str; 4] = ["test", "clippy", "fmt", "sequential"];

/// The argv prefix the pinned suite step expands. `cpu_pin_prefix` in
/// `scripts/lib/ci-cpus.sh` sets `CPU_PIN` to `(taskset -c "$CPUS")`, or to an
/// empty array under `DEVFLOW_CI_CPUS=all`.
const PIN_TOKEN: &str = "\"${CPU_PIN[@]}\"";

/// Read one of the recogniser's fixtures. These exist so the negative cases
/// actually run: a guard that only ever inspects the live workflow cannot be
/// made to fail on demand, and one that has only been observed passing is not
/// evidence of anything.
fn ci_parity_fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ci-parity")
        .join(name);
    read(&path)
}

/// Split a workflow's top-level `jobs:` mapping into `(job_key, body)` pairs.
///
/// **Anchored to `jobs:` (46-04, 46-REVIEWS.md C-02).** The previous version
/// anchored to nothing and treated any two-space key as a job header, so
/// `push:` and `pull_request:` under the top-level `on:` block parsed as jobs.
/// Measured on the live file at the time: six headers for four real jobs,
/// which is what let `assert!(jobs.len() >= 4)` be satisfied by two non-jobs.
///
/// Operates on RAW lines on purpose. Indentation is the ONLY signal that
/// separates a top-level key from a nested one, and `code_lines` maps
/// `str::trim` over everything — it has destroyed that signal before this
/// function could use it. `code_lines` stays correct for the
/// occurrence-COUNTING guards that use it; it is simply the wrong tool here.
fn split_jobs(workflow: &str) -> Vec<(String, String)> {
    let mut jobs: Vec<(String, String)> = Vec::new();
    let mut inside_jobs_mapping = false;

    for line in workflow.lines() {
        let blank = line.trim().is_empty();
        let indent = line.len() - line.trim_start().len();

        if !inside_jobs_mapping {
            if indent == 0 && line.trim_end() == "jobs:" {
                inside_jobs_mapping = true;
            }
            continue;
        }

        // A non-blank line back at column 0 ends the `jobs:` mapping.
        if !blank && indent == 0 {
            break;
        }

        let is_job_header = !blank
            && indent == 2
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');

        if is_job_header {
            jobs.push((line.trim().trim_end_matches(':').to_string(), String::new()));
        } else if let Some(last) = jobs.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        }
    }

    jobs
}

/// Decide whether a workflow really runs the pinned sequential suite, scoped
/// to the `Sequential 2-CPU check` job's OWN body.
///
/// Returns the job's key on success. Every failure names what the job itself
/// is missing, not what the file is missing — that distinction is the whole of
/// 46-REVIEWS.md C-02, which demonstrated two bypasses against the previous
/// whole-file `contains` checks.
///
/// Within that body, after dropping whole comment lines, it requires:
///   (a) a line mentioning `scripts/lib/ci-cpus.sh` — the single definition
///       site for the CPU list AND for the `all` special-case (D-03, C-06);
///   (b) a line whose TRIMMED content STARTS WITH [`PIN_TOKEN`] and also
///       contains `scripts/check.sh all`.
///
/// **The starts-with rule in (b) is the entire echoed-command defence**, and
/// `tests/fixtures/ci-parity/echoed-command.yml` is the case that proves it:
/// `- run: echo '"${CPU_PIN[@]}" scripts/check.sh all'` trims to a line
/// starting with `- run: echo`, so it is rejected. A `contains` check would
/// accept it — mentioning the pinned command is not running it.
///
/// Comment stripping is WHOLE-LINE only (drop lines whose first non-space
/// character is `#`), and is applied only AFTER the splitter has used
/// indentation. A mid-line `#` is not reliably a comment in YAML, so a
/// stripper that assumed it was would corrupt the very lines being recognised
/// — `- run: echo "a#b"` is one value, not a command plus a comment.
fn recognise_pinned_sequential_job(workflow: &str) -> Result<String, String> {
    let name_line = format!("name: {SEQUENTIAL_JOB_NAME}");

    let (key, body) = split_jobs(workflow)
        .into_iter()
        .find(|(_, body)| body.lines().map(str::trim).any(|l| l == name_line))
        .ok_or_else(|| {
            format!(
                "no job body contains `{name_line}` — the job is missing, \
                 renamed, or the line sits outside any job block"
            )
        })?;

    let code: Vec<&str> = body
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();

    if !code.iter().any(|l| l.contains("scripts/lib/ci-cpus.sh")) {
        return Err(format!(
            "job `{key}` does not source `scripts/lib/ci-cpus.sh` in its own \
             body — the CPU list and the `all` override both live there, and a \
             mention elsewhere in the file (another job, or a comment) does not \
             put the pin on this job"
        ));
    }

    if !code.iter().any(|l| {
        let trimmed = l.trim();
        trimmed.starts_with(PIN_TOKEN) && trimmed.contains("scripts/check.sh all")
    }) {
        return Err(format!(
            "job `{key}` has no line whose trimmed content STARTS WITH \
             `{PIN_TOKEN}` and runs `scripts/check.sh all` — the pinned \
             invocation is missing, commented out, or merely echoed"
        ));
    }

    Ok(key)
}

/// POSITIVE CONTROL. Without it a recogniser that rejected every input would
/// satisfy all four negative fixtures and still report green.
#[test]
fn recogniser_accepts_a_valid_pinned_sequential_job() {
    let workflow = ci_parity_fixture("valid.yml");
    assert_eq!(
        recognise_pinned_sequential_job(&workflow),
        Ok("sequential".to_string()),
        "valid.yml carries the job name, the ci-cpus.sh source line and a \
         command line starting with the pin token — the recogniser must accept \
         it, or every negative case below passes for the wrong reason."
    );
}

/// agy's demonstrated bypass (46-REVIEWS.md C-02). Measured against the
/// pre-46-04 guard: commenting the suite step out left it reporting
/// `1 passed`.
#[test]
fn recogniser_rejects_a_commented_out_pinned_suite_step() {
    let workflow = ci_parity_fixture("commented-out.yml");
    let err = recognise_pinned_sequential_job(&workflow)
        .expect_err("a commented-out pinned suite step must not satisfy the guard");
    assert!(
        err.contains("pinned"),
        "the error must name the missing pinned invocation. Found: {err:?}"
    );
}

/// codex's demonstrated bypass (46-REVIEWS.md C-02): the tokens are all
/// present in the file, just not in the job that has to carry them.
#[test]
fn recogniser_rejects_a_decoy_job_carrying_the_pin() {
    let workflow = ci_parity_fixture("decoy-job.yml");
    let err = recognise_pinned_sequential_job(&workflow)
        .expect_err("tokens in a DIFFERENT job must not satisfy the guard");
    assert!(
        err.contains("ci-cpus.sh") || err.contains("pinned"),
        "the error must name what the sequential job itself is missing. \
         Found: {err:?}"
    );
}

/// Mentioning the pinned command is not running it. This is the whole of the
/// starts-with rule's justification.
#[test]
fn recogniser_rejects_an_echoed_pin_command() {
    let workflow = ci_parity_fixture("echoed-command.yml");
    let err = recognise_pinned_sequential_job(&workflow)
        .expect_err("an echoed pin command must not satisfy the guard");
    assert!(
        err.contains("pinned"),
        "the error must name the missing pinned invocation. Found: {err:?}"
    );
}

/// Without the pin the job is just a fourth unpinned runner, which is the
/// exact shape 46-01 added it to escape.
#[test]
fn recogniser_rejects_an_unpinned_suite_step() {
    let workflow = ci_parity_fixture("no-pin.yml");
    let err = recognise_pinned_sequential_job(&workflow)
        .expect_err("an unpinned `scripts/check.sh all` must not satisfy the guard");
    assert!(
        err.contains("pinned"),
        "the error must name the missing pinned invocation. Found: {err:?}"
    );
}

/// The splitter must anchor to the top-level `jobs:` key. `push:` and
/// `pull_request:` sit at the same two-space indent under `on:`, which is why
/// indentation alone is not enough — and why these lines must NOT be routed
/// through `code_lines`, whose `str::trim` destroys the only signal that
/// distinguishes a top-level key from a nested one.
#[test]
fn job_splitter_excludes_on_block_keys() {
    let workflow = ci_parity_fixture("valid.yml");
    let keys: Vec<String> = split_jobs(&workflow).into_iter().map(|(k, _)| k).collect();
    assert_eq!(
        keys,
        vec!["test".to_string(), "sequential".to_string()],
        "valid.yml has exactly two jobs; `push` and `pull_request` are `on:` \
         keys. Got: {keys:?}"
    );
}

/// The same assertion against the live workflow, pinned to the exact key list
/// rather than to a count. Measured before 46-04: the old splitter returned
/// six keys — `push`, `pull_request`, `test`, `clippy`, `fmt`, `sequential` —
/// for four real jobs, so `assert!(jobs.len() >= 4)` was satisfiable by two
/// non-jobs.
#[test]
fn job_splitter_finds_exactly_the_live_ci_jobs() {
    let workflow = read(&repo_root().join(".github/workflows/ci.yml"));
    let keys: Vec<String> = split_jobs(&workflow).into_iter().map(|(k, _)| k).collect();
    assert_eq!(
        keys,
        LIVE_CI_JOB_KEYS
            .iter()
            .map(|k| k.to_string())
            .collect::<Vec<_>>(),
        "the splitter must return exactly the four real jobs. Got: {keys:?}"
    );
}
