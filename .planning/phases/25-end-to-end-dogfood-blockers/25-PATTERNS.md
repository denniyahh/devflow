# Phase 25: End-to-End Dogfood Blockers - Pattern Map

**Mapped:** 2026-07-27
**Files analyzed:** 9 (all modified, none newly created as source files)
**Analogs found:** 9 / 9 (all analogs are in-file precedent — this phase edits
existing modules far more than it adds new ones; see note below)

**Note on this phase's shape:** Per the orchestrator's framing, almost every
unit in Phase 25 modifies an existing function/module rather than creating a
new file. Accordingly this map is dominated by "surrounding convention"
excerpts (error types, test-fixture idioms, doc-comment style, existing
sibling functions) rather than cross-file analog pairs. The one place a
genuinely new artifact is added — `preflight_major_bump_check` in
`preflight.rs` (25c) — gets a full existing-check-as-template treatment.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/devflow-core/src/version.rs` (`compute_version` rewrite, 25c) | service (library, git-derived computation) | transform (git plumbing → typed `Version`) | itself — `count_git_tags`/`origin_main_ancestor_status` (git.rs) for subprocess idiom | exact (in-file + git.rs precedent) |
| `crates/devflow-cli/src/preflight.rs` (new `preflight_major_bump_check`, 25c) | named preflight check (gate-integrated predicate) | request-response (Result<(), String> consumed by `run_preflight`) | `preflight_gh_auth_check` / `gh_auth_check_applies` (same file) | exact |
| `crates/devflow-core/src/git.rs` (new `reachable_semver_tags`-style helper, 25c) | utility (git subprocess wrapper) | transform | `origin_main_ancestor_status` / `GitFlow::cleanup_merged` (`branch --merged`) | exact |
| `crates/devflow-cli/src/commands.rs` (`start`, hoist `enforce_build_staleness`, 25b) | controller (CLI subcommand orchestration) | request-response | itself — surrounding `start()` body | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (remove call at `:93`, 25b) | controller (stage-launch orchestration) | request-response | itself | exact |
| `crates/devflow-core/src/agent.rs` (`terminate_and_verify` addition, 25d; `#[deprecated]` on `looks_like_devflow_process`, 25e) | utility (process primitives) | event-driven (signal + poll) | itself — `terminate`/`agent_running`/`is_same_process` (same file) | exact |
| `crates/devflow-cli/src/commands.rs` (25d reaper reusing `stop_via_lock`'s identity match; 25e retargeted test) | controller | event-driven | `stop_via_lock` (same file) | exact |
| `crates/devflow-cli/src/staleness.rs` (999.38 fix: per-`Command` `env_remove`) | test-fixture / utility | transform | `crates/devflow-core/src/test_support.rs::git_command`/`hermetic_command` | exact |
| new 25d test spawning a real child + deleting its root | test (integration) | event-driven | `crates/devflow-cli/tests/stop_e2e.rs` (spawns real `devflow advance` child, tracks pid) + `staleness.rs::worktree_staleness_fixture` (tempdir + real git repo shape) | strong role-match |

## Pattern Assignments

### `crates/devflow-core/src/version.rs` (service, transform) — 25c

**Analog:** itself (current `VersionError`, `count_git_tags`, `compute_version`) + `git.rs::origin_main_ancestor_status`/`GitFlow::cleanup_merged` for the `--merged` idiom.

**Existing error type to keep verbatim** (`version.rs:32-44`):
```rust
/// Errors produced by version operations.
#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    /// Filesystem operation failed.
    #[error("version file I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// Version field could not be found or parsed.
    #[error("version parse failed: {0}")]
    Parse(String),
    /// A git command failed.
    #[error("git command failed: {0}")]
    Git(String),
}
```
D-10's "refuse when the highest tag is unreachable" case is a new *kind* of
failure (not I/O, not parse, not a bare git-command failure) — the planner
should decide whether it fits `VersionError::Parse`/`Git` as-is or needs a
new named variant (e.g. `VersionError::UnreachableBaseline { tag: String }`)
so the `Display` message can name the offending tag per D-10's requirement
("refuse, naming the unreachable tag and the command that repairs it").
Follow the existing `#[error("...: {0}")]` `thiserror` shape either way.

**Existing subprocess-shelling idiom to copy** (`version.rs:90-106`,
`count_git_tags` — this is the pattern every new git-derived helper in this
file must match: `Command::new("git")`, `.current_dir(project_root)`,
`.output()`, map spawn error to `VersionError::Git`, check
`output.status.success()`, parse stdout lines):
```rust
pub fn count_git_tags(project_root: &Path) -> Result<u32, VersionError> {
    let output = Command::new("git")
        .arg("tag")
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;
    if !output.status.success() {
        return Err(VersionError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let count = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    Ok(count as u32)
}
```
This exact shape (`Command::new("git")` → `.current_dir` → `.output()` →
`map_err(VersionError::Git)` → success check → `String::from_utf8_lossy`
line-parsing) is what `count_git_tags`/`commits_since_last_minor_tag` both
use today and what D-07's `reachable_semver_tags` replacement should
continue using — it is the file's one, consistent subprocess convention.

**`--merged` precedent to copy verbatim (git.rs, not version.rs)** —
RESEARCH.md's Pattern 1 already gives the exact new-helper shape for tags,
modeled on this existing branch-`--merged` usage:
```rust
// git.rs:243 (GitFlow::cleanup_merged) — the in-repo precedent for
// "which refs are already reachable" via one `--merged` spawn instead of
// an O(n) per-ref `--is-ancestor` loop:
let output = self.git_output(["branch", "--merged", &self.config.develop])?;
```
And the direct 25a template, `origin_main_ancestor_status` (`git.rs:462-508`
— read in full, reproduced here since it is the load-bearing analog for
25a's chosen option):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AncestorStatus {
    Ancestor,
    Diverged,
    RefAbsent,
}

pub fn origin_main_ancestor_status(project_root: &Path) -> AncestorStatus {
    let ref_exists = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "origin/main"])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !ref_exists {
        return AncestorStatus::RefAbsent;
    }
    let is_ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", "origin/main", "HEAD"])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if is_ancestor {
        AncestorStatus::Ancestor
    } else {
        AncestorStatus::Diverged
    }
}
```
For 25a, repoint at `origin/develop`/`develop` per CONTEXT.md's D-17 options.
Note this function's error handling deliberately collapses spawn failure and
non-zero exit into the same `.unwrap_or(false)`/`RefAbsent` branch — a
softer failure mode than `version.rs`'s `VersionError::Git` propagation.
25a's chosen option should decide which of the two failure-handling styles
(propagate a typed error vs. fail-open to a status enum) fits its refusal
semantics; `origin_main_ancestor_status`'s fail-open shape matches
`phase_reachability_on_base`'s existing "fail open where blind" contract in
`preflight.rs`, which is the more likely sibling for a *preflight* refusal
check.

**Test fixture style to copy** (`version.rs:559-586`, the `#[cfg(test)] mod
tests` git-repo construction idiom — every new test for the rewritten
`compute_version` should build its temp repo this way, through
`crate::test_support::git_command`, never a bare `Command::new("git")`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let ok = crate::test_support::git_command(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    fn init_repo(root: &Path) {
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        git(root, &["config", "commit.gpgsign", "false"]);
        git(root, &["config", "tag.gpgsign", "false"]);
        git(root, &["config", "core.hooksPath", "/dev/null"]);
    }

    fn commit(root: &Path, name: &str) {
        std::fs::write(root.join(name), name).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", &format!("add {name}")]);
    }
```
A representative existing test using this fixture, showing the
`tempfile::tempdir()` → `init_repo` → write version file → `commit`/`tag` →
assert-on-`compute_version` shape the D-07/D-08 rewrite's new tests should
follow (`version.rs:659-689`):
```rust
#[test]
fn count_tags_and_commits_drive_minor_and_patch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    std::fs::write(root.join("Cargo.toml"), "[package]\nversion = \"2.0.0\"\n").unwrap();
    commit(root, "a.txt");
    assert_eq!(count_git_tags(root).unwrap(), 0);
    let v = compute_version(root).unwrap();
    assert_eq!(v.major, 2);
    assert_eq!(v.minor, 0);
    assert!(v.patch >= 1);

    git(root, &["tag", "v2.0.0"]);
    commit(root, "b.txt");
    commit(root, "c.txt");
    assert_eq!(count_git_tags(root).unwrap(), 1);
    assert_eq!(commits_since_last_minor_tag(root).unwrap(), 2);

    let v = compute_version(root).unwrap();
    assert_eq!(v, Version { major: 2, minor: 1, patch: 2 });
    assert_eq!(v.to_string(), "2.1.2");
}
```
This whole test will need rewriting under D-07/D-08 (its assertions encode
the OLD algorithm) — reuse the fixture helpers (`git`/`init_repo`/`commit`),
add a `tag(root, name)` helper of the same one-line shape if useful, but
replace the assertions with baseline-tag + conventional-commit-classification
expectations. Note `init_repo` already sets `tag.gpgsign=false`, so tagging
in tests needs no further signing setup.

**`read_version`'s distinct-role doc comment** (`version.rs:156-165`) is the
exact prose to preserve/extend when documenting why `compute_version`
changing under D-11 does NOT touch `read_version`:
```rust
/// Read the full [`Version`] (major/minor/patch) out of whatever version file
/// `detect_version_file` resolves, mirroring [`write_version`]'s format
/// handling (including `[workspace.package]`).
///
/// Unlike [`compute_version`], this never touches git — it reports exactly
/// what was last written to the version file, not a freshly recomputed
/// minor/patch. Callers that need the version a prior [`write_version`] call
/// actually wrote (e.g. after a tag was just cut) must use this instead of
/// `compute_version`, which would see the new tag and return a different,
/// larger version.
```

---

### `crates/devflow-cli/src/preflight.rs` (`preflight_major_bump_check`, request-response) — 25c/D-09

**Analog:** `preflight_gh_auth_check` + `gh_auth_check_applies` (same file, `preflight.rs:305-341`).

**Full existing named check as template** — same file, verbatim:
```rust
/// D-14 (universal, generic layer): whether the gh-auth credential probe
/// applies to `stage` — hardcoded to `Stage::Ship` rather than a dynamic
/// hook-scan (review Plan 05 MEDIUM, Codex+OpenCode): Ship's terminal hooks
/// (`hooks::hooks_after_ship()` = Merge/VersionBump/ChangelogAppend/BranchCleanup,
/// `hooks.rs:99-106`) are the only hooks that push to a remote. Split out as
/// its own pure predicate so "does not run for a non-Ship stage" is directly
/// unit-testable without shelling out to `gh`.
fn gh_auth_check_applies(stage: Stage) -> bool {
    stage == Stage::Ship
}

/// D-14 (universal, generic layer): external credential validity via `gh
/// auth status`, run ONLY when [`gh_auth_check_applies`] (Ship). Fails soft
/// to a warning when the `gh` binary itself is absent — a missing optional
/// tool must not hard-fail the pipeline (T-17-14). Fails preflight only when
/// `gh` is present and reports unauthenticated. Records only a boolean
/// pass/fail plus a short reason string — raw `gh auth status` stdout/stderr
/// is NEVER captured or logged (T-17-13, Information Disclosure).
fn preflight_gh_auth_check(state: &State) -> Result<(), String> {
    if !gh_auth_check_applies(state.stage) {
        return Ok(());
    }
    match std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err("gh auth status reports not authenticated".to_string()),
        Err(_) => {
            println!(
                "warning: `gh` binary not found — cannot verify GitHub credential validity \
                 before Ship (fail-soft, not a preflight failure)"
            );
            Ok(())
        }
    }
}
```
`preflight_major_bump_check(project_root: &Path, state: &State) -> Result<(),
String>` should mirror this exactly: a `*_applies(stage)` pure predicate
gated on `Stage::Ship` (same as `gh_auth_check_applies`), a check function
that early-returns `Ok(())` when inapplicable, and an `Err(reason: String)`
on failure — never a rich enum, matching this file's existing verdict type.

**How it reaches `run_gate`** — the composition point and full dispatch
(`preflight.rs:343-452`, `generic_preflight_checks` + `run_preflight`):
```rust
/// The generic (universal) preflight checks (D-14) — the adapter-specific
/// hook is composed separately in [`run_preflight`].
fn generic_preflight_checks(project_root: &Path, state: &State) -> Result<(), String> {
    preflight_interactivity_check(project_root, state)?;
    preflight_gh_auth_check(state)
}
```
Add `.and_then(|()| preflight_major_bump_check(project_root, state))` (or a
third `?`-chained call) to this composition — everything downstream
(`run_preflight`'s ceiling check, `run_gate` call, `GateAction` dispatch) is
shared machinery and needs no change. The full `run_preflight` body
(`preflight.rs:381-452`) is the "how it reaches `run_gate`" answer in full;
key excerpt:
```rust
pub(crate) fn run_preflight(
    project_root: &Path,
    state: &mut State,
    adapter: &dyn agents::AgentAdapter,
) -> Result<bool, CliError> {
    let stage = state.stage;
    if let Err(reason) =
        generic_preflight_checks(project_root, state).and_then(|()| adapter.preflight(state))
    {
        if state.preflight_retries >= mode::MAX_PREFLIGHT_RETRIES {
            /* ... abort, return Ok(false) ... */
        }
        state.preflight_retries = state.preflight_retries.saturating_add(1);
        workflow::save_state(state)?;
        let context = format!(
            "[never-silent] preflight failed for stage {stage}: {} — human review needed \
             (retry, loop-to-code, or abort)",
            truncate_reason(&reason)
        );
        match run_gate(project_root, state, stage, &context)? {
            GateAction::Advance => { /* skip recheck, launch_stage_inner */ }
            GateAction::LoopBack(_) => { /* full launch_stage re-entry, bounded */ }
            GateAction::Abort(reason) => abort(project_root, state, &reason)?,
        }
        return Ok(false);
    }
    /* preflight passed: reset preflight_retries, persist */
    Ok(true)
}
```

**Test for the new check** — model on `gh_auth_check_applies_only_to_ship_stage`
(`preflight.rs:530-536`, a pure-predicate table test) plus
`run_preflight_failing_check_gates_and_never_reaches_spawn_monitor`
(`preflight.rs:543-575`, the full gate+abort integration shape — seed a
rejection `GateResponse` at `Gates::response_path`, call `run_preflight`,
assert `!should_continue` and that `workflow::load_state` errors because
`abort()` cleared it). Both are directly copyable skeletons for
`preflight_major_bump_check_applies_only_to_ship_stage` and a
`run_preflight_major_bump_gates_and_never_ships_unattended` integration test.

---

### `crates/devflow-cli/src/commands.rs` + `pipeline_launch.rs` (D-03 hoist) — 25b

**Analog:** itself — `enforce_build_staleness`'s existing signature and call site.

**Exact signature (unchanged, only its call site moves)**
(`staleness.rs:329-334`):
```rust
pub(crate) fn enforce_build_staleness(
    project_root: &Path,
    state: &State,
    embedded_commit: &str,
    build_dirty: bool,
) -> Result<(), CliError>
```
**Current call site to remove** (`pipeline_launch.rs:93-98`):
```rust
enforce_build_staleness(
    &project_root,
    state,
    env!("DEVFLOW_BUILD_COMMIT"),
    env!("DEVFLOW_BUILD_DIRTY") == "true",
)?;
```
**New call site**: insert the identical call in `commands.rs`, between
`state.worktree_path` being set (`commands.rs:199`) and
`launch_stage(&mut state, None, None)` (`commands.rs:236`) — `use
crate::staleness::run_git_stdout;` already makes `staleness` an in-scope
module for `commands.rs` (`commands.rs:21`), and `enforce_build_staleness`
is `pub(crate)`, so only an import of the function name is needed, no
visibility change. No other reordering — `workflow::save_state` and the
`workflow_started` event emission currently sit in that span and are
unaffected.

---

### `crates/devflow-core/src/agent.rs` + `crates/devflow-cli/src/commands.rs` (identity reuse, escalation) — 25d/25e

**Analog:** `stop_via_lock` (`commands.rs:1160-1226`, abbreviated to the
identity-match block CONTEXT.md points at, lines ~1191-1200) — the `(pid,
starttime)` match the 25d reaper must reuse, not reinvent:
```rust
match lock::holder_identity(project_root, phase) {
    Some((recorded_pid, Some(recorded_start))) if recorded_pid == pid => {
        if !agent::is_same_process(pid, recorded_start) {
            return Err(CliError::Message(format!(
                "refusing to signal pid {pid} for phase {phase} — it is not the \
                 process that took the lock. The lock recorded start time \
                 {recorded_start}, but pid {pid} now reports {:?}, so the pid has \
                 been recycled and belongs to something else. Inspect it manually \
                 (e.g. `ps -p {pid}`) before proceeding.",
                agent::process_start_time(pid)
            )));
        }
    }
    Some((_, None)) => {
        return Err(CliError::Message(format!(
            "refusing to signal pid {pid} for phase {phase} — the lock file records \
             no start time, so this process's identity cannot be confirmed. ..."
        )));
    }
    _ => {
        return Err(CliError::Message(format!(
            "refusing to signal pid {pid} for phase {phase} — the lock file's holder \
             could not be read back for identity confirmation. ..."
        )));
    }
}
if agent::terminate(pid) {
    println!("stop: signalled pid {pid}, phase {phase}'s lock holder");
} else {
    println!("stop: pid {pid} could not be signalled (it may have just exited)");
}
```
Note this is the LOCK-based path (works when `.devflow/lock-{phase:02}`
still exists). 25d's registry-independent discovery is new — there is no
existing in-repo analog for "scan `/proc/*/cmdline` with no lock/registry at
all"; RESEARCH.md's own proposed `terminate_and_verify` shape is the closest
thing to a template and should be treated as the plan's starting point, not
an existing pattern to copy verbatim.

**`terminate`/`agent_running` — the two primitives `terminate_and_verify`
extends** (`agent.rs`, already read in full):
```rust
pub fn agent_running(pid: u32) -> bool {
    let Ok(signed) = libc::pid_t::try_from(pid) else { return false; };
    if signed <= 0 || unsafe { libc::kill(signed, 0) } != 0 { return false; }
    !is_zombie(pid)
}

pub fn terminate(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return false; };
    pid > 0 && unsafe { libc::kill(pid, libc::SIGTERM) == 0 }
}

pub fn is_same_process(pid: u32, expected_start: u64) -> bool {
    process_start_time(pid) == Some(expected_start)
}
```
Every guard style here (`libc::pid_t::try_from`, `pid > 0` before signalling,
never trust an unchecked cast) must be replicated in any new escalation
function — this is the file's one, consistent process-safety idiom.

**`looks_like_devflow_process` — the function `#[deprecated]` targets**
(`agent.rs`, doc comment already documents its own unsoundness — 25e adds
only the attribute, does not rewrite the body):
```rust
/// ...
/// **UNSOUND ON ITS OWN — see 999.47.** ...
pub fn looks_like_devflow_process(pid: u32) -> bool { /* unchanged body */ }
```
Add `#[deprecated(note = "...")]` directly above `pub fn`, matching Rust's
standard attribute placement (no other project precedent for `#[deprecated]`
exists in this codebase — this is a first use, so follow std-library
convention: a short `note` string pointing at the replacement,
`is_same_process`).

---

### 999.38 — per-`Command` env pattern (D-14) — folds into 25b's module

**Analog:** `crates/devflow-core/src/test_support.rs::git_command`/`hermetic_command`.

**The exact idiom the fix must follow** (`test_support.rs:68-83`):
```rust
pub fn git_command(repo: &Path) -> Command {
    hermetic_command("git", repo)
}

pub fn hermetic_command(program: &str, dir: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir);
    for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
        cmd.env_remove(var);
    }
    cmd
}
```
This is per-`Command` `.env_remove(...)`, never process-global
`std::env::remove_var`. The staleness.rs flake (`ahead_build_from_descendant
_commit_warns_instead_of_blocking`, `staleness.rs:891`) needs the equivalent
treatment: scope `PATH`/env overrides to the specific `Command` a test
drives (e.g. via `.env("PATH", ...)` on that one `Command::new(...)` call,
or `monitor.rs:156`'s `Command::new("sh")...envs(...)` for anything driving
`monitor::spawn_monitor`), not `std::env::set_var` at the test level. Per
RESEARCH.md's Pitfall 3, confirm any given call site actually goes through a
`Command` before applying this idiom — `ensure_agent_binary`
(`preflight.rs:61-91`) reads `std::env::var_os("PATH")` directly with no
`Command` to attach to, so the per-Command idiom does not apply there (that
call site is out of the folded-in 999.38 scope per D-14, which targets only
`staleness.rs:891`).

---

### 25d test — spawn a real child, delete its root

**Analogs:** `crates/devflow-cli/tests/stop_e2e.rs` (real-child spawn +
pid tracking) and `crates/devflow-cli/src/staleness.rs::worktree_staleness_
fixture` (tempdir + real git repo construction shape).

**Real-child-spawn shape to copy** (`stop_e2e.rs:127-153`, abbreviated to
the load-bearing lines):
```rust
fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}
// ...
let mut child = Command::new(devflow_bin())
    .args(["advance", "--phase", &phase.to_string()])
    /* .current_dir(...), stdio, etc. */
    .spawn()
    .expect("spawn devflow advance");
```
And the poll-until-exit helper used throughout the same file
(`stop_e2e.rs:63-70`):
```rust
fn wait_for(mut predicate: impl FnMut() -> bool, timeout_secs: u64, what: &str) {
    let start = Instant::now();
    while !predicate() {
        assert!(
            start.elapsed() < Duration::from_secs(timeout_secs),
            "timed out after {timeout_secs}s waiting for: {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
```
And a `DEVFLOW_E2E_CHILD_TIMEOUT_SECS`-bounded death-wait (`stop_e2e.rs:
~100-120`, referenced via `child.try_wait()`/`child.id()`) — the shape a
"spawn, delete root, assert discovery still finds it, then verify death"
test should reuse for its own bounded wait.

**Tempdir + real git repo fixture shape to copy**
(`staleness.rs:507-545`, `worktree_staleness_fixture` — note this builds TWO
directories under one `tempfile::tempdir()`, exactly the shape a "delete the
root out from under a live child" test needs: spawn the child pointed at the
inner directory, then delete only that inner directory while the outer
tempdir (and the test process) survives):
```rust
fn worktree_staleness_fixture() -> (tempfile::TempDir, PathBuf, String) {
    let outer = tempfile::tempdir().unwrap();
    let project_root = outer.path().join("project");
    std::fs::create_dir_all(&project_root).unwrap();
    let worktree_path = outer.path().join("worktree");

    let git = |args: &[&str], cwd: &Path| {
        assert!(
            devflow_core::test_support::git_command(cwd)
                .args(args)
                .output()
                .unwrap()
                .status
                .success(),
            "git {args:?} in {cwd:?} failed"
        );
    };

    git(&["init", "-q", "-b", "develop"], &project_root);
    git(&["config", "user.email", "t@e.st"], &project_root);
    git(&["config", "user.name", "t"], &project_root);
    git(&["config", "commit.gpgsign", "false"], &project_root);
    git(&["config", "core.hooksPath", "/dev/null"], &project_root);
    /* ... commit, worktree add ... */
}
```
A new 25d test should combine these two: spawn a real child (`stop_e2e.rs`
shape, e.g. `sh -c 'trap "" TERM; sleep 30'` per RESEARCH.md's regression-test
recommendation) rooted under a `worktree_staleness_fixture`-style nested
tempdir, `std::fs::remove_dir_all` the inner root while the child is still
alive, then assert the registry-independent discovery mechanism still finds
and reaps it — and separately, `terminate_and_verify` clears a
`TERM`-ignoring child (bounded via the same `wait_for`/timeout idiom).

## Shared Patterns

### Hermetic git subprocess invocation (applies to ALL new git-shelling code in this phase)
**Source:** `crates/devflow-core/src/test_support.rs::git_command`/`hermetic_command`
**Apply to:** every test fixture across 25a/25b/25c/25d/999.38 that shells
out to `git` — never a bare `Command::new("git")` in test code; production
code (`version.rs`, `git.rs`) uses plain `Command::new("git")` (no
hermeticity concern outside the test binary), so this pattern is
test-fixture-only.
```rust
pub fn git_command(repo: &Path) -> Command {
    hermetic_command("git", repo)
}
```

### `thiserror`-derived error enums with `#[error("...: {0}")]`
**Source:** `crates/devflow-core/src/version.rs::VersionError` (also
`GitError`, `CliError` elsewhere in the workspace — this is the project-wide
convention).
**Apply to:** any new error variant 25c's D-10 refuse-on-unreachable case
needs.

### Fail-open where the probe cannot see (vs. fail-closed on identity)
**Source:** `preflight.rs::PhaseReachability::Undeterminable` /
`origin_main_ancestor_status::RefAbsent` (probes fail open) vs.
`commands.rs::stop_via_lock`'s identity match (fails closed — refuses to
signal on any uncertainty).
**Apply to:** 25a (a currency probe should fail open per the existing
`origin_main_ancestor_status`/`phase_reachability_on_base` convention when it
cannot resolve a ref) vs. 25d (a signalling decision must fail closed per
`stop_via_lock`'s convention — never signal on inferred identity). These are
deliberately different postures for different operations; do not cross them.

### Named preflight check → gate → notify (D-09's reuse target)
**Source:** `preflight.rs::generic_preflight_checks` + `run_preflight`.
**Apply to:** 25c's `preflight_major_bump_check` — see full excerpt above.

## No Analog Found

None — every file this phase touches has a strong in-file or same-crate
analog; 25d's registry-independent `/proc` scan is the one genuinely novel
mechanism (no existing discovery-without-a-lock-file code in this repo), but
RESEARCH.md's own `terminate_and_verify` sketch (Code Examples section) is
explicitly the planner's starting point for that piece, not a gap.

## Metadata

**Analog search scope:** `crates/devflow-core/src/{version,git,agent,test_support}.rs`,
`crates/devflow-cli/src/{preflight,commands,pipeline_launch,staleness}.rs`,
`crates/devflow-cli/tests/{stop_e2e,gate_sweep_e2e}.rs`
**Files scanned:** 9 source files read in full or via targeted ranges (all
non-overlapping reads); `.planning/codebase/` maps intentionally NOT used
(dated 2026-06-17, predates Phases 1-12 per orchestrator instruction)
**Pattern extraction date:** 2026-07-27
