# Phase 46: CI Load Shape and Operator Input Validation - Pattern Map

**Mapped:** 2026-09-04
**Files analyzed:** 9 (5 certain, 2 conditional on the D-03 mechanism, 2 documentation)
**Analogs found:** 9 / 9 (8 exact, 1 role-match)

Every path below was checked against `git ls-files` and is tracked source in this worktree —
none is a gitignored install/runtime mirror.

**Scope note carried from the orchestrator:** `crates/devflow-core/src/worktree.rs` (R-01
ref-spelling asymmetry), `evidence`/`sweep` `--root` convergence, branch-protection/ruleset
changes and `CLAUDE.md` are **out of scope** and are deliberately absent from this map. A plan
that touches them is outside the phase boundary.

---

## File Classification

| New/Modified File | Change | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|--------|------|-----------|----------------|---------------|
| `.github/workflows/ci.yml` | MODIFY — add one job | config (CI workflow) | batch / build pipeline | the `test:` job in the **same file**, `:31-53` | exact |
| `crates/devflow-cli/src/commands.rs` | MODIFY `ensure_base_is_a_local_branch` (`:147-170`) | utility / validator | request-response (subprocess to `git`) | **itself** — the current body is the pattern to extend | exact (self) |
| `crates/devflow-cli/src/commands.rs` | EXTEND test at `:5458-5495` | test (unit, in-crate) | request-response | **itself** — the existing loop + negative-control block | exact (self) |
| `crates/devflow-cli/src/main.rs` | MODIFY `Stop` variant (`:345-352`) + dispatch (`:700-703`) | route / CLI dispatcher | request-response | `Resume` (`:181-183`) and `Status` (`:255-259`) + `Command::Status` arm (`:662`) | exact |
| `crates/devflow-cli/tests/stop_e2e.rs` | ADD cases; EXTEND `stop_help_documents_phase_flag` (`:314-326`) | test (integration, spawns binary) | process-spawn / request-response | `stop_preserves_pre_existing_stop_reason` (`:258-287`), `stop_help_documents_phase_flag` (`:314-326`) | exact (self-file) |
| `crates/devflow-cli/tests/ci_parity_guards.rs` | ADD 3 guards | test (integration, static assertion) | file-I/O / transform | `devcontainer_job_name_matches_the_required_status_check` (`:215-226`), `ci_workflow_delegates_to_the_shared_check_script` (`:130-159`), `ci_workflow_runs_the_pinned_devcontainer_image` (`:230-275`) | exact (self-file) |
| `scripts/lib/ci-cpus.sh` | **CREATE — conditional** (only if the planner takes D-03 mechanism C) | config (shell fragment) | file-I/O | `scripts/assert-image-parity.sh:1-12` (header + `set -euo pipefail` idiom) | role-match |
| `scripts/check-in-container.sh` | MODIFY `:85-89` — **conditional** (mechanism C only) | config (shell) | file-I/O | **itself** — the comment block + `CPUS=` line | exact (self) |
| `.planning/user/DEV-SETUP-CHECKLIST.md` | MODIFY `:126-128` | docs | — | **itself** — the existing `- [ ] **[PROJECT]**` bullet form | exact (self) |
| `OPERATIONS.md` | MODIFY `:43` | docs | — | `:39` (`gate sweep`, the `[--root PATH]` row form) and `:43` itself | exact (self) |

**`scripts/lib/` does not exist** (`ls scripts/lib` → no such file). Mechanism C creates the
first file under it. That is the only genuinely new directory in the phase.

---

## Pattern Assignments

### `.github/workflows/ci.yml` — add one job (config, batch pipeline)

**Analog:** the `test:` job in the same file. **Not** `clippy`/`fmt` — those carry only
`safe.directory`, no image-parity step (D-01 as corrected by R-05).

**The full analog job, verbatim** (`ci.yml:31-53`):

```yaml
  test:
    name: Test
    runs-on: ubuntu-24.04
    container:
      image: mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v7
      # These jobs run as root inside a container while actions/checkout
      # leaves the workspace owned by the runner user (uid 1001), so git
      # refuses every command with "detected dubious ownership". checkout's
      # own safe.directory entry is scoped to its step and is not visible
      # here — GIT_CONFIG_GLOBAL is unset by the time tests run. Confirmed
      # from git's own stderr in run 30214005214.
      #
      # Must precede assert-image-parity.sh: that script resolves the repo
      # root via `git rev-parse` and would otherwise silently fall back to
      # `pwd` instead of failing loudly.
      - name: Trust the workspace
        run: git config --global --add safe.directory "$GITHUB_WORKSPACE"
      - name: Assert CI image matches the devcontainer definition
        run: scripts/assert-image-parity.sh "mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm"
      - run: scripts/check.sh test
```

**Copy exactly:** the `runs-on`/`container:`/`image:` block, the two named steps in that order,
and `actions/checkout@v7`. **Change:** job key, `name:`, `timeout-minutes:`, and the final `run:`
line. Do not duplicate the two long comments — they explain the `Trust the workspace` step that
already exists; a `# see the Test job above` pointer is enough.

**Three in-file constraints the new job must not break** (each is enforced by an existing test —
see the `ci_parity_guards.rs` section):

1. The image tag must be a **literal on its own `image:` line** — no anchor, no `env` reference.
   `ci_workflow_runs_the_pinned_devcontainer_image` `assert_eq!`s *every* line starting with
   `image:` against `.devcontainer/devcontainer.json`.
2. No trimmed line may start with `- run: cargo ` or `run: cargo `
   (`ci_workflow_delegates_to_the_shared_check_script`). `- run: taskset -c "$CPUS" scripts/check.sh all`
   passes; `- run: cargo test ...` does not.
3. The three existing `workflow.contains("scripts/check.sh <target>")` assertions
   (`test`/`clippy`/`fmt`) must still hold — adding an `all` line does not disturb them.

**The `concurrency` block at `:26-28` already covers the new job** — it is workflow-scoped, not
job-scoped. Nothing to add.

**Header-comment correction (discretionary, RESEARCH Open Question 2).** `ci.yml:9-14` recommends
a verification command that under-reports required checks. The correct dual-command form to copy
lives in `devcontainer.yml:12-14`:

```yaml
# Check BOTH before touching a workflow's job name or existence:
#   gh api repos/denniyahh/devflow/branches/<branch>/protection
#   gh api repos/denniyahh/devflow/rules/branches/<branch>
```

**Read-only analog, DO NOT MODIFY:** `.github/workflows/devcontainer.yml:34-52`. It is the
existing sequential `scripts/check.sh all` invocation and it is a **required** status check on
both trunks. It is shown here only so the planner sees the sequential shape already exists; D-01
says explicitly not to touch it.

```yaml
jobs:
  devcontainer:
    name: Build + test in devcontainer
    runs-on: ubuntu-24.04
    timeout-minutes: 40
    steps:
      - uses: actions/checkout@v7
      - name: Build devcontainer and run CI-parity checks
        uses: devcontainers/ci@v0.3
        with:
          push: never
          runCmd: |
            set -e
            git config --global --add safe.directory "$PWD"
            scripts/check.sh all
```

---

### `crates/devflow-cli/src/commands.rs` — `ensure_base_is_a_local_branch` (utility/validator, request-response)

**Analog:** the function itself. This is a body replacement inside a signature and doc comment
that stay.

**Current body, verbatim** (`commands.rs:147-170`):

```rust
pub(crate) fn ensure_base_is_a_local_branch(
    project_root: &Path,
    base: &str,
) -> Result<(), CliError> {
    let ok = git_command(project_root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{base}"),
        ])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if ok {
        return Ok(());
    }
    Err(CliError::Message(format!(
        "configured base branch `{base}` is not a local branch in this repository. \
         DevFlow forks phase worktrees from it and merges phase work back into it, so it \
         must be a branch — not a remote-tracking name, a `refs/heads/` path, `HEAD`, or a \
         commit SHA. Create it locally (e.g. `git branch {base} origin/{base}`) and re-run."
    )))
}
```

**Patterns to preserve verbatim:**

- **Subprocess idiom:** `git_command(project_root).args([...]).output().map(|out| out.status.success()).unwrap_or(false)`.
  `git_command` is the hermetic wrapper and is **already imported** at `commands.rs:30`:
  ```rust
  use devflow_core::git::{GitFlow, git_command, hermetic_command};
  ```
  Never `Command::new("git")`.
- **Error idiom:** `Err(CliError::Message(format!( ... )))` with a multi-line string continued by
  `\` and inline `{base}` interpolation. Both new messages use this exact shape.
- **The existing message text is D-08's "missing" arm verbatim** — including the
  `git branch {base} origin/{base}` advice. Do not reword it; the test discriminates on
  `contains("git branch ...")`.
- **The doc comment at `:125-146` stays** — D-09 preserves the caller-side scoping it documents.
  Its closing paragraph ("Scoped by the caller to a non-`Default` base, deliberately...") is the
  justification a reviewer will look for.

**Fail-open/fail-closed asymmetry the planner must state in the plan** (from RESEARCH):
`.unwrap_or(false)` on the `show-ref` existence call (an unspawnable git still refuses), but
`.unwrap_or(true)` on the `check-ref-format` classifier (an unspawnable git falls back to the
*existing* message, never to a new message it has no evidence for).

**The single call site stays as it is** (`commands.rs:326-332`) — copy nothing, change nothing:

```rust
    let resolved_base = config::base_branch(project_root).map_err(CliError::Message)?;
    let base = resolved_base.value.as_str();
    if resolved_base.source != config::BaseBranchSource::Default {
        // A commit-ish that is not a local branch passes `rev-parse --verify`
        // and is forwarded raw to `git worktree add` — scoped to an
        // explicitly configured base so the default path's existing
        // fall-open for a clone with no local `develop` is untouched.
        ensure_base_is_a_local_branch(project_root, base)?;
    }
```

That inline comment names `rev-parse --verify` and becomes stale the moment the body changes —
update it in the same edit.

---

### `crates/devflow-cli/src/commands.rs` — the D-10 test extension (test, request-response)

**Analog:** the test itself, `:5458-5495`. Extend in place; do not add a parallel test.

**The negative-control block that must survive unchanged** (`:5464-5470`):

```rust
        // NEGATIVE CONTROL: without this, every `Err` assertion below would
        // pass against a helper that returned `Err` unconditionally.
        assert!(
            ensure_base_is_a_local_branch(root, "workspace/example").is_ok(),
            "a real local branch must be accepted"
        );
        assert!(ensure_base_is_a_local_branch(root, "develop").is_ok());
```

**The existing reject loop — the idiom the new arms copy** (`:5478-5494`):

```rust
        for spelling in ["origin/main", "refs/heads/main", "HEAD", head_sha.as_str()] {
            // Each of these satisfies a bare `rev-parse --verify` — assert
            // that first, so the test proves the bypass exists rather than
            // merely that the helper is strict.
            let bare = devflow_core::test_support::git_command(root)
                .args(["rev-parse", "--verify", "--quiet", spelling])
                .output()
                .expect("rev-parse");
            assert!(
                bare.status.success(),
                "fixture is wrong: `{spelling}` must resolve as a commit-ish"
            );
            assert!(
                ensure_base_is_a_local_branch(root, spelling).is_err(),
                "`{spelling}` is not a local branch and must be refused"
            );
        }
```

**Copy from this exactly:**

- `devflow_core::test_support::git_command(root)` — **note this is a different path from the
  production code's `git_command`.** Test code in this file uses the `test_support` re-export
  (also used at `:5416`, `:5443`, `:5472`). Use `test_support::git_command` in the new arms.
- The **prove-the-bypass-first** structure: assert the bare probe *succeeds* before asserting the
  helper refuses. This is what makes the arm discriminate between "the fix works" and "the helper
  was already strict".
- `.expect("rev-parse")` on `.output()`, and the `"fixture is wrong: ..."` message prefix for a
  fixture-precondition failure.

**Two deviations the new arms require** (both from RESEARCH, both load-bearing):

1. The bypass probe for suffix spellings must use the **qualified** form
   `&format!("refs/heads/{spelling}")`, not the bare `spelling` the existing loop passes. The
   existing arms are bare-resolvable names; the new ones are only bypasses *after* the
   `refs/heads/` prefix is applied.
2. Use **`workspace/example~1`**, not `develop~1`. See the fixture below.

**The fixture and why `develop~1` is a dead arm** (`commands.rs:5414-5449`, excerpted):

```rust
    fn base_branch_fixture(root: &Path) {
        let run = |args: &[&str]| { /* ... assert git success ... */ };
        run(&["init", "-q", "-b", "main"]);
        // ... user.email / user.name / commit.gpgsign=false / core.hooksPath=/dev/null ...
        std::fs::write(root.join("README.md"), "x").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "init"]);
        run(&["branch", "develop"]);                       // <-- develop == ROOT commit
        run(&["checkout", "-q", "-b", "workspace/example"]);
        std::fs::create_dir_all(root.join(".planning")).unwrap();
        std::fs::write(root.join(".planning/config.json"), "{}").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "planning"]);          // <-- only workspace/example has a parent
        run(&["checkout", "-q", "develop"]);
        // ... update-ref refs/remotes/origin/main <sha of main> ...
    }
```

`develop` has no parent, so `develop~1` is *already* refused today — an arm using it passes
identically before and after the fix.

**Do not mutate this fixture.** It has a second consumer:
`no_worktree_start_forks_the_feature_branch_from_the_configured_base` at `:5565` (call site
`:5568`), which asserts a fork point against `workspace/example`. If a plan changes the fixture,
its acceptance block must run **both** tests.

**D-08's discriminator is a message-content pair, not `is_err()`** — the new arms assert
`!msg.contains("git branch")`, and a separate `nonexistent-xyz` arm asserts
`msg.contains("git branch nonexistent-xyz origin/nonexistent-xyz")`. Without both halves the test
cannot tell two messages from one.

---

### `crates/devflow-cli/src/main.rs` — `Stop` variant + dispatch (route/CLI, request-response)

**Analog:** `Resume` and `Status`. D-11 says "matching them exactly", so both are excerpted
verbatim.

**`Resume`'s positional** (`main.rs:181-183`):

```rust
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
```

**`Status`, the whole variant** (`main.rs:254-259`):

```rust
    /// Show current workflow state.
    Status {
        /// Project root.
        #[arg(default_value = ".")]
        project: PathBuf,
    },
```

**`Status`'s dispatch arm** — the one-line positional idiom (`main.rs:662`):

```rust
        Command::Status { project } => status(&project_root(project)?),
```

**`Stop` as it stands** (`main.rs:345-352`):

```rust
    Stop {
        /// Phase to stop.
        #[arg(long)]
        phase: PhaseId,
        /// Project root. Defaults to the current directory.
        #[arg(long)]
        root: Option<PathBuf>,
    },
```

**`Stop`'s dispatch arm as it stands** (`main.rs:700-703`):

```rust
        Command::Stop { phase, root } => stop(
            &project_root(root.unwrap_or_else(|| PathBuf::from(".")))?,
            phase,
        ),
```

**What changes:** append the `Resume`/`Status` positional block to the variant (keeping `root`,
D-12), and collapse the dispatch to `root.unwrap_or(project)`. The existing
`unwrap_or_else(|| PathBuf::from("."))` becomes redundant — the positional's `default_value = "."`
supplies it, which is exactly what makes the shape identical to `Resume`/`Status`. Update the
`root` doc comment from "Defaults to the current directory" to say it overrides the positional.

**No `conflicts_with` / `overrides_with`.** `conflicts_with` makes the both-supplied case an error,
which D-12 forbids; `overrides_with` relates repeated flags, not a flag to a positional. Both alter
`--help`. Plain `unwrap_or` is the precedence.

**`project_root` needs no change** — D-13 is a routing fix, not a message fix (`main.rs:718-724`):

```rust
fn project_root(project: PathBuf) -> Result<PathBuf, CliError> {
    if !project.exists() {
        return Err(CliError::Message(format!(
            "project path does not exist: {}",
            project.display()
        )));
    }
```

The remainder (`:726-738`) canonicalises and walks **up** to the nearest `.devflow` ancestor,
returning `start` unchanged if none is found — which is why the default `.` already works from
inside a phase worktree.

**Do not touch `Command::Evidence`'s arm** (`:704-714`), which uses the same
`root.unwrap_or_else(...)` shape. Converging it is explicitly deferred.

---

### `crates/devflow-cli/tests/stop_e2e.rs` — add cases (test, process-spawn)

**Analog:** the tests already in this file. The whole file drives the **real compiled binary** via
`Command::new`, because `commands::stop` is `pub(crate)` in a binary crate with no `lib.rs`.

**Harness helpers already present — reuse, do not re-create** (`:22-38`):

```rust
fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Hermetic `git` invocation for fixture setup (999.37) — never a bare
/// `Command::new("git")`.
fn git(root: &Path, args: &[&str]) {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

`init_repo(root, phase)` (`:44-60`) builds a repo with `develop` plus a `feature/phase-NN` branch,
hooks disabled.

**The `--root` invocation idiom, copied verbatim from `stop_leaves_stop_until_unchanged`**
(`:291-310`) — the shortest complete case in the file and the template for a new one:

```rust
#[test]
fn stop_leaves_stop_until_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = PhaseId::new(98);

    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stop_until = Some(Stage::Plan);
    devflow_core::workflow::save_state(&state).unwrap();

    let output = Command::new(devflow_bin())
        .args(["stop", "--phase", &phase.to_string(), "--root"])
        .arg(root)
        .output()
        .expect("run devflow stop");
    assert!(output.status.success());

    let reloaded = devflow_core::workflow::load_state(root, phase).unwrap();
    assert_eq!(reloaded.stop_until, Some(Stage::Plan));
}
```

Note the two-call argument idiom: `.args([... "--root"])` then `.arg(root)` — the `Path` is passed
as an `OsStr`, never stringified. **For the new positional case, use the same split**:
`.args(["stop", "--phase", &phase.to_string()]).arg(root)`.

Each test uses a **distinct `PhaseId`** (97, 98, 99, ...) against its own `tempdir`. New cases
should continue that.

**The failure-message idiom for a spawned command** (`:350-354`):

```rust
    assert!(
        first.status.success(),
        "first stop failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
```

For D-13's wrong-root case, invert it: assert `!output.status.success()` **and** that
`String::from_utf8_lossy(&output.stderr)` contains `project path does not exist:` plus the
offending path — a bare `!success()` would pass on a clap usage error, which is precisely the
failure D-13 exists to prevent.

**The help test to extend** (`:312-326`):

```rust
/// CLI discoverability: `--phase` must be documented in the subcommand's own
/// `--help`.
#[test]
fn stop_help_documents_phase_flag() {
    let output = Command::new(devflow_bin())
        .args(["stop", "--help"])
        .output()
        .expect("run devflow stop --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--phase"),
        "--help missing --phase:\n{stdout}"
    );
}
```

Add a second `assert!(stdout.contains("PROJECT"), ...)` in the same shape (message names what is
missing, then dumps the full `stdout`). The existing `--phase` assertion stays — it is the
negative control that stops the new one from passing against an empty help output.

**All 8 existing `--root` call sites in this file** (`:178`, `:234`, `:270`, `:302`, `:343`,
`:397`, `:424`, `:475`) **keep working unchanged** — D-12 is additive. Plus `reap_strays_e2e.rs:163`
= 9 total. `gate_sweep_e2e.rs:177,217` are `gate sweep --root`, a different subcommand.

---

### `crates/devflow-cli/tests/ci_parity_guards.rs` — add 3 guards (test, file-I/O)

**This is the highest-value analog in the phase.** The file already contains 7 tests that assert
over CI YAML, and all three new INFRA-01 guards go here in exactly this idiom.

**Shared helpers at the top of the file — reuse both** (`:33-46`):

```rust
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
```

**Guard idiom A — the required-check name guard.** This is the closest analog for the D-05 guard
("the new job's name is not one of the four required contexts"), and it is the one whose failure
message shows how to embed the *correct* `gh api` verification command (`:210-226`):

```rust
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
```

**Copy:** the doc comment that names the incident and its date; `repo_root().join(...)` + `read`;
`assert!` with `path.display()` as the trailing arg; a failure message that says what must be true
*and* gives the operator the command to check it. The D-05 guard inverts the predicate — the new
job's `name:` must **not** equal any of `Test`, `Clippy`, `Format`, `Build + test in devcontainer`.

**Guard idiom B — line filtering with a "found" dump.** This is the analog for the D-03 guard
("`ci.yml` does not re-type the `0,1` literal") and the taskset-shape guard, because it shows the
comment-stripping and the `Found: {...:?}` reporting that CLAUDE.md's C-3/C-4 traps demand
(`:127-159`):

```rust
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
```

**Copy:** `.lines().map(str::trim).filter(...)` collected into a `Vec<&str>`, then
`assert!(v.is_empty(), "... Found: {v:?}")`. **This is the pattern that replaces a shell `rg -c`
count** — it prints the offending lines rather than a count, which sidesteps C-3 entirely. For the
D-03 "no re-typed `0,1`" guard, add `!l.starts_with('#')` to the filter chain so a comment
mentioning `0,1` cannot fail it (C-4).

**Comment-stripping precedent, if the guard needs it** (`:91-95`, from
`check_script_fails_fast_before_any_cargo_invocation`):

```rust
    let lines: Vec<&str> = script
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
```

**Guard idiom C — the constraint the new job must satisfy, already enforced** (`:230-275`,
`ci_workflow_runs_the_pinned_devcontainer_image`). Excerpted because it is the test the new job
will trip if the image tag is not a bare literal:

```rust
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
```

**Every new guard needs a demonstrated failing direction** (CLAUDE.md C-7): mutate the YAML, run
`cargo test -p devflow --test ci_parity_guards`, capture a real `test result: FAILED`, restore. A
guard whose failing direction was never observed has not been shown to discriminate.

---

### `scripts/lib/ci-cpus.sh` — CREATE, conditional on D-03 mechanism C (config, file-I/O)

**No analog directory exists** — `scripts/lib/` is new. Closest role-match for a small POSIX-shell
file in this repo is `scripts/assert-image-parity.sh`.

**Header + failure idiom to copy** (`assert-image-parity.sh:1-19`):

```bash
#!/usr/bin/env bash
# Fail if the image CI runs in differs from the one .devcontainer/devcontainer.json
# declares. GitHub Actions cannot read a job's container image from a file, so
# the tag is necessarily duplicated in .github/workflows/ci.yml; this turns
# that duplication from a silent rot risk into a hard failure.
#
# Without this, bumping only one side reintroduces exactly the local-vs-CI
# divergence the container parity work was done to eliminate — and it would
# look green until something environment-sensitive broke.
set -euo pipefail

CI_IMAGE="${1:?usage: assert-image-parity.sh <image-used-by-ci>}"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
DEVCONTAINER="$REPO_ROOT/.devcontainer/devcontainer.json"

if [ ! -f "$DEVCONTAINER" ]; then
    echo "error: $DEVCONTAINER not found" >&2
    exit 1
fi
```

**The house style, verbatim from three scripts:** `#!/usr/bin/env bash`, then a comment block
opening with *why the file exists* and *what breaks without it*, then `set -euo pipefail`, then
code. `scripts/check.sh:1-9` and `scripts/check-in-container.sh:1-12` are the same shape.

**One deviation for this file:** it is **sourced**, not executed, so it must not carry
`set -euo pipefail` (that would leak shell options into both callers). Keep the shebang for
editor/shellcheck detection and the comment block; drop the `set` line. State this explicitly in
the plan so a reviewer comparing it to `assert-image-parity.sh` sees the deviation is deliberate.

**The single line it holds**, lifted from `check-in-container.sh:89`:

```bash
CPUS="${DEVFLOW_CI_CPUS:-0,1}"
```

---

### `scripts/check-in-container.sh` — MODIFY, conditional on D-03 mechanism C (config, file-I/O)

**Analog:** itself. The region is `:85-94`:

```bash
# CPU pinning: match CI's core count so test-thread interleaving is
# comparable. GitHub's standard hosted runners are 2-core; a 4-core host
# hides races that CI sees. Override with DEVFLOW_CI_CPUS=all to use every
# core (faster, less faithful).
CPUS="${DEVFLOW_CI_CPUS:-0,1}"
if [ "$CPUS" = "all" ]; then
    PIN=()
else
    PIN=(taskset -c "$CPUS")
fi
```

Mechanism C replaces line 89 with a `.`-source of the fragment; the comment block and the
`if`/`PIN=` construction stay. **Two documented corrections belong in the same edit:**

1. The comment's "GitHub's standard hosted runners are 2-core" is **stale for this repository** —
   public repos get 4 vCPU. The value `0,1` is still right; the stated reason is not.
2. If mechanism C moves the definition, D-03's `:89` reference moves with it. The plan must say so
   in prose — D-03's binding content is "one definition site", and C keeps exactly one.

**Editing this file triggers `scripts/hooks/post-commit`'s checklist warning** — see Shared
Patterns below.

---

### `.planning/user/DEV-SETUP-CHECKLIST.md` — MODIFY (docs)

**Analog:** the bullets already in § 4 (`:124-135`). Copy the `- [ ] **[PROJECT]**` /
`- [ ] **[PATTERN]**` tag convention and the two-space continuation indent:

```markdown
## 4. GitHub Actions CI

- [ ] **[PROJECT]** `.github/workflows/ci.yml` — three required jobs (`Test`, `Clippy`, `Format`),
  each running **inside the exact same pinned container image** the devcontainer and the
  pre-push hook use (§5) — the whole point being zero host/CI drift.
- [ ] **[PATTERN]** A dedicated `scripts/assert-image-parity.sh` step fails the build if the
  image tag in CI drifts from `.devcontainer/devcontainer.json` — because GitHub Actions can't
  interpolate `env` into `jobs.*.container.image`, so the tag is duplicated by hand and needs an
  explicit guard against silent rot.
- [ ] **[PROJECT]** `.github/workflows/devcontainer.yml` (separate workflow — check its trigger
  and purpose if replicating).
```

Line `:126` is wrong twice after this phase: `ci.yml` gains a fourth job, and the *required* set
already includes a fourth check (`Build + test in devcontainer`) that lives in `devcontainer.yml`.

**`.planning/` is exempt from the pre-commit personal-artifact refusal** (`scripts/hooks/pre-commit:60-65`),
so this file **is** committable from this worktree — unlike `CLAUDE.md`, which is not and which no
plan may edit.

---

### `OPERATIONS.md:43` — MODIFY (docs)

**Analog:** the `gate sweep` row at `:39`, which is the file's form for an optional `--root`:

```markdown
| `devflow gate sweep [--max-age-secs N] [--dry-run] [--root PATH]` | Answer or report aged, unattended gates ... |
```

and the `stop` row itself at `:43`:

```markdown
| `devflow stop --phase N [--root PATH]` | End a running phase cleanly (23c). Answers its open gate with a rejection if one is open ... |
```

Change the signature cell only — `[--root PATH]` becomes something like
`[PROJECT] [--root PATH]`, matching how `start`/`resume`/`status` rows spell their positional
elsewhere in the same table. The long description cell is unaffected by VALID-02.

---

## Shared Patterns

### Hermetic git invocation
**Source:** `crates/devflow-core/src/git.rs` (`git_command`), re-exported for tests as
`devflow_core::test_support::git_command`
**Apply to:** every git call in `commands.rs` and in `tests/*.rs`

```rust
// production (commands.rs:30 — already imported)
use devflow_core::git::{GitFlow, git_command, hermetic_command};

// tests (commands.rs:5416, stop_e2e.rs:29)
devflow_core::test_support::git_command(root)
    .args(args)
    .output()
    .expect("spawn git");
```

Never `Command::new("git")` (999.37). The wrapper strips redirecting `GIT_*` variables.

### CLI error construction
**Source:** `commands.rs:164-169`, `main.rs:720-723`
**Apply to:** both new D-08 messages

```rust
Err(CliError::Message(format!(
    "configured base branch `{base}` is not a local branch in this repository. \
     DevFlow forks phase worktrees from it and merges phase work back into it, so it \
     must be a branch — ... Create it locally (e.g. `git branch {base} origin/{base}`) and re-run."
)))
```

One `CliError::Message` holding a single continued string; the offending value in backticks;
actionable advice last. The revision-syntax message follows this shape but **must not** carry the
`git branch` advice.

### Assertion-with-diagnostic
**Source:** `ci_parity_guards.rs:153-158`, `stop_e2e.rs:322-325`, `commands.rs:5486-5489`
**Apply to:** every new assertion in all three test files

```rust
assert!(
    <predicate>,
    "<what must be true and why it matters>. Found: {actual:?}",
    <context such as path.display()>
);
```

Three consistent traits: the message states the *invariant*, not the mechanic; it dumps the actual
value (`Found: {x:?}`, or the whole `stdout`/`stderr`); and where a human must go check something
external, it embeds the exact command (`gh api repos/denniyahh/devflow/rules/branches/develop`).

### Regression-guard doc comments
**Source:** `ci_parity_guards.rs:1-32` (module header), `:83-85`, `:210-214`
**Apply to:** the three new `ci_parity_guards.rs` guards and the extended `commands.rs` test

Every guard carries a doc comment naming the incident it encodes, with its identifier and date
(`CR-01 (15-REVIEW.md)`, `WR-08 (17-REVIEW.md)`, `That happened on 2026-07-26`). New guards should
cite `46-CONTEXT.md` D-nn and, where applicable, `46-REVIEWS.md` R-nn — that is the citation form
this repo's guards already use.

### Shell script header
**Source:** `scripts/check.sh:1-9`, `scripts/check-in-container.sh:1-12`,
`scripts/assert-image-parity.sh:1-10`
**Apply to:** `scripts/lib/ci-cpus.sh` if mechanism C is taken

`#!/usr/bin/env bash` → comment block stating why the file exists and what breaks without it →
`set -euo pipefail` → code. (Sourced fragments omit the `set` line; see that file's section.)

### DEV-SETUP-CHECKLIST co-commit
**Source:** `scripts/hooks/post-commit:81`
**Apply to:** whichever plan touches `ci.yml` and whichever touches `check-in-container.sh`

The hook's trigger regex covers `^\.github/(workflows/|...)` **and**
`^scripts/(check\.sh|check-in-container\.sh|assert-image-parity\.sh)$` — both of this phase's
non-Rust edits fire it. `.planning/user/DEV-SETUP-CHECKLIST.md` must move in the **same commit**,
not a follow-up. The hook only warns; it never edits.

---

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| *(none)* | — | — | Every file this phase creates or modifies has a same-role analog in-tree. `scripts/lib/ci-cpus.sh` is the only new file and the only new directory; it has no directory analog but a clear file-level one (`scripts/assert-image-parity.sh`), recorded as role-match above. |

**Not a file, but the planner must know it:** D-06's acceptance — *the job actually ran and was
green on the PR's current `HEAD_SHA`* — has **no in-tree analog and cannot have one**. Nothing
`cargo test` can assert proves a GitHub job ran. That acceptance is a `gh pr checks <PR>`
observation (unfiltered, never `--required`) recorded in the phase's SUMMARY/VERIFICATION, not a
test file. Do not let a plan invent a test for it.

---

## Metadata

**Analog search scope:** `.github/workflows/`, `crates/devflow-cli/src/`,
`crates/devflow-cli/tests/`, `scripts/`, `.planning/user/`, `OPERATIONS.md`
**Files read this session:** `.github/workflows/ci.yml` (full, 77 lines),
`.github/workflows/devcontainer.yml` (full, 52), `crates/devflow-cli/tests/ci_parity_guards.rs`
(full, 297), `crates/devflow-cli/src/commands.rs` (`:20-40`, `:125-180`, `:320-345`, `:5405-5500`),
`crates/devflow-cli/src/main.rs` (`:175-195`, `:248-266`, `:336-360`, `:688-745`),
`crates/devflow-cli/tests/stop_e2e.rs` (`:1-60`, `:255-360`), `scripts/check.sh` (`:1-20`, `:50-65`),
`scripts/check-in-container.sh` (`:1-12`, `:80-100`), `scripts/assert-image-parity.sh` (`:1-30`),
`OPERATIONS.md` (`:35-50`), `.planning/user/DEV-SETUP-CHECKLIST.md` (`:120-138`)
**Tracked-source gate:** all 11 analog paths confirmed via `git ls-files`; no gitignored mirrors
**Pattern extraction date:** 2026-09-04
