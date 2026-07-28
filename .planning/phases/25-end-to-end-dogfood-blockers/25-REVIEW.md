---
phase: 25-end-to-end-dogfood-blockers
reviewed: 2026-07-28T16:06:43Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-cli/tests/reap_strays_e2e.rs
  - crates/devflow-core/Cargo.toml
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/test_support.rs
  - crates/devflow-core/src/version.rs
findings:
  critical: 2
  warning: 4
  info: 4
  total: 10
status: issues_found
---

# Phase 25: Code Review Report

**Reviewed:** 2026-07-28T16:06:43Z
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

> **This review supersedes the 2026-07-28T02:03:59Z revision in full.** That
> revision predated plans 25-11/25-12/25-13 and `test_support.rs`; its
> `critical: 3 / warning: 3` counters were derived against the old tree and are
> not carried forward. Every finding below was re-derived against the current
> working tree at `39da531`.

## Summary

`cargo check --workspace --all-targets` and `cargo clippy --workspace
--all-targets -- -D warnings` are both clean. The three areas flagged for
hardest scrutiny largely hold up:

- **`wait_for_exec_visibility` (25-11)** — the timeout path is correct and
  bounded (`deadline` is checked after each read, and the poll sleeps, so no
  busy-spin and no unbounded wait); the dead-pid short-circuit is sound;
  `Instant`-based deadlines are monotonic. One latent unsoundness in the
  self-cmdline guard (WR-02).
- **`process_age` / `clock_ticks_per_second` (25-12)** — the classic
  `/proc/<pid>/stat` parsing bug is *correctly avoided*: `process_start_time`
  splits at `stat.rfind(')')`, not the first `)`, and the field index (`nth(19)`
  after the final paren = field 22) is right. Tick arithmetic is done in `f64`
  with a `.max(0.0)` clamp, so no integer underflow. `clock_ticks_per_second()`
  returning `None` propagates through `?` to `None`, and
  `reap_stray_candidates` treats `None` as refuse via `Option::is_some_and` —
  it genuinely fails **closed**. One residual panic path (IN-01).
- **`StrayReapOutcome::TooYoung` / `reap_stray_candidates` (25-12)** — the guard
  ordering is correct on every path: identity → age → `dry_run` → signal. There
  is **no** branch that can signal a too-young or unknown-age candidate, and the
  `is_same_process` recycling guard composes correctly with the age floor (they
  are checked in series, both fail-closed, and a pid recycled between them makes
  the age tiny, which refuses).

The two Critical findings are elsewhere, in the surfaces those primitives feed:
the `doctor` / `--reap-strays` census makes an **unverified** orphan-ness claim
about live processes and recommends SIGKILLing them (CR-01, reproduced live on
this machine), and `ensure_base_ref_current` performs its "safe fast-forward"
with `git update-ref`, which bypasses the very protections that make it safe
(CR-02, reproduced in a scratch repo).

## Critical Issues

### CR-01: `doctor` asserts orphan-ness it never checks, and points the operator at a SIGKILL

**File:** `crates/devflow-cli/src/commands.rs:3023-3049` (`build_stray_process_findings`), `crates/devflow-cli/src/commands.rs:1149-1232` (`gate_sweep`'s stray pass), `crates/devflow-core/src/agent.rs:393-446` (`discover_stray_devflow_processes`)

**Issue:** `discover_stray_devflow_processes` is a purely *structural* `/proc`
census — it matches `sh -c <script containing MONITOR_WRAPPER_MARKER>` and
`devflow advance`, filtered only by euid. It performs no orphan test of any
kind. `build_stray_process_findings` then emits, for every match:

```
severity: problem
detail:  "state-orphaned process: pid N (monitor wrapper) is running but
          reachable through no registry entry, lock file, or state file"
repair:  "devflow gate sweep --reap-strays"
```

That `detail` string states a fact the code never established. Reproduced live
against the current build on this machine:

```
$ ./target/debug/devflow doctor --json | jq '.stray_processes | length'
38
# cross-referenced against ~/.cache/devflow/roots/*.json and each root's
# .devflow/state-NN.json + .devflow/lock-NN:
stray pids that ARE named by a registered root's state/lock file: 14
  pid 596367  named in /tmp/.tmp4T6jFk/.devflow/state-12.json   (live monitor_pid)
  pid 602181  named in /tmp/.tmp4T6jFk/.devflow/lock-12         (live lock holder)
  pid 1664537 named in /tmp/.tmpNZddyv/.devflow/state-08.json
  pid 1667954 named in /tmp/.tmpNZddyv/.devflow/lock-08
```

14 of 38 findings are demonstrably false: the registry reaches them, their state
file names them, and one of them is the process **currently holding the phase
lock**. `doctor` is the read-only command operators trust, and it is now telling
them 38 healthy-or-not processes are orphans and naming a destructive repair.

`gate_sweep`'s `--reap-strays` then acts on that same unqualified census with
`terminate_and_verify` (TERM → SIGKILL). Concrete failure scenario, fully
reachable today with no race and no unusual state:

1. Two DevFlow phases are running (the normal dogfood shape on this machine).
2. Operator runs `devflow doctor`, sees 38 `problem` findings, and runs the
   repair `doctor` printed.
3. Every live monitor wrapper is SIGKILLed. SIGKILL is uncatchable, so the
   wrapper's `trap cleanup TERM INT` never fires — its backgrounded agent is
   orphaned and keeps running with nothing left to call `devflow advance`.
   That is *exactly* the orphan class 999.44 exists to eliminate; the reaper
   manufactures it.
4. Any `devflow advance` caught mid-transition (`AdvanceChild`, e.g. pid 602181
   above) is killed while holding the phase lock, leaving a stale lock and a
   half-written state machine.

`STRAY_MIN_AGE` does not mitigate this at all — a live monitor wrapper is
minutes to hours old, far above the 2s floor. The floor defends against
fork/exec false positives, not against "this process is alive and owned."

Two aggravating factors in the same pass:

- `gate_sweep`'s stray pass ignores `--root` entirely (`commands.rs:1149`, no
  reference to `roots`), so `devflow gate sweep --root /some/project
  --reap-strays` still reaps the whole machine. The `--root` flag's own help
  text says "Restrict the sweep to one project root."
- `main.rs:389-398`'s flag help describes the behaviour as "discover and clear
  STATE-ORPHANED processes (999.44)", which is the semantics the implementation
  does not have.

**Fix:** Filter the census against what the registry *can* reach before either
reporting it as orphaned or signalling it. `registry::load_roots()` already
yields `(project_root, phase)` pairs, and both `monitor_pid` and the lock
holder are readable from them:

```rust
/// Pids that a live registry entry still reaches — never "state-orphaned",
/// and never reaped. A stray by definition is NOT in this set, so filtering
/// it out preserves 999.44's deleted-root case exactly.
fn registry_reachable_pids() -> std::collections::HashSet<u32> {
    let mut reachable = std::collections::HashSet::new();
    for root in registry::load_roots() {
        if let Ok(state) = workflow::load_state(&root.project_root, root.phase)
            && let Some(pid) = state.monitor_pid
        {
            reachable.insert(pid);
        }
        if let Some(holder) = lock::holder(&root.project_root, root.phase) {
            reachable.insert(holder.pid);
        }
    }
    reachable
}

// in collect_stray_process_findings() and gate_sweep()'s stray pass:
let reachable = registry_reachable_pids();
let candidates: Vec<_> = agent::discover_stray_devflow_processes()
    .into_iter()
    .filter(|s| !reachable.contains(&s.pid))
    .collect();
```

Additionally: honour `--root` in the stray pass (scope `registry_reachable_pids`
to the given root, or document loudly at the call site why it cannot be scoped),
and reword `main.rs`'s `--reap-strays` help to match whatever the final scope
actually is.

### CR-02: `ensure_base_ref_current` rewrites `develop` with `git update-ref`, defeating both safety checks it relies on

**File:** `crates/devflow-cli/src/preflight.rs:456-473`

**Issue:** The `Behind` arm advances the local base branch with:

```rust
git update-ref refs/heads/develop refs/remotes/origin/develop
```

guarded only by `git symbolic-ref --short HEAD` read **in `project_root`**. The
doc comment argues this is safe because "`Behind` itself already establishes
losslessness" and "the not-checked-out precondition is sufficient." Both
arguments fail:

**(a) The checked-out test only sees one worktree.** `git update-ref` — unlike
`git branch -f` — has no checked-out-branch protection at all. Verified in a
scratch repo:

```
$ git update-ref refs/heads/develop $(git rev-parse HEAD)   # from worktree B
update-ref SUCCEEDED (no checked-out protection)
$ git branch -f develop $(git rev-parse HEAD)               # same operation, safe API
fatal: cannot force update the branch 'develop' used by worktree at '/tmp/urtest/repo'
```

Failure scenario: this repository routinely has several linked worktrees
(`.worktrees/phase-NN`, `.claude/worktrees/agent-*`). If `develop` is checked
out in *any* worktree other than the one `project_root` resolves to, `devflow
start` silently moves the ref out from under it. That worktree's HEAD now points
at a commit its index and working tree do not match: `git status` there reports
every intervening change as an uncommitted deletion/modification, and a commit
made there reverts them.

**(b) There is no old-value guard.** `git update-ref <ref> <new>` with no
`<oldvalue>` argument is an unconditional write — it will happily move a ref
*backwards* onto a non-descendant (demonstrated in the same scratch repo).
`base_ref_currency` establishes ancestry, then `ensure_base_ref_current` writes
without re-checking it. Any local commit landing on `develop` in that window (a
concurrent `devflow`, an operator, a hook) is silently discarded — recoverable
only via reflog, and the operator is told "advanced `develop` to
`origin/develop` (N commit(s) fast-forwarded)", which is a false description of
what happened.

**Fix:** Use the API that enforces both invariants, and pass the expected old
value so the write is atomic against the check:

```rust
BaseRefCurrency::Behind { count } => {
    let remote_ref = format!("{ORIGIN}/{base}");
    // Resolve BOTH endpoints that `base_ref_currency` just compared, so the
    // write is conditional on the state that was actually validated.
    let resolve = |rev: &str| {
        std::process::Command::new("git")
            .args(["rev-parse", "--verify", "--quiet", rev])
            .current_dir(project_root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let fast_forwarded = match (resolve(base), resolve(&remote_ref)) {
        (Some(old), Some(new)) => std::process::Command::new("git")
            .args([
                "update-ref",
                &format!("refs/heads/{base}"),
                &new,
                &old, // <oldvalue>: refuses if `base` moved since the check
            ])
            .current_dir(project_root)
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false),
        _ => false,
    };
    // ...
}
```

and replace the single-worktree `symbolic-ref` probe with a repository-wide
check (`git worktree list --porcelain` scanning for `branch
refs/heads/<base>`), or simply attempt `git branch -f <base> <remote_ref>` first
— it refuses on its own when the branch is checked out in *any* worktree, which
is precisely the precondition the current code is trying (and failing) to
establish. Both approaches must keep the existing "fall through to
`stale_base_message` on any failure" behaviour, which is already correct.

## Warnings

### WR-01: `release_range_start` cannot distinguish "not an ancestor" from "git failed", and errs toward an over-inclusive range

**File:** `crates/devflow-core/src/version.rs:338-349`

**Issue:**

```rust
let tag_is_ancestor_of_first_parent = Command::new("git")
    .args(["merge-base", "--is-ancestor", baseline_tag, &first_parent])
    ...
    .map(|out| out.status.success())
    .unwrap_or(false);

if !tag_is_ancestor_of_first_parent {
    return Ok(candidate.clone());   // anchor here
}
```

`merge-base --is-ancestor` exits 1 for "not an ancestor" and 128 for a genuine
error (bad object, corrupt repo); a spawn failure (EAGAIN/ENOMEM under the
concurrent-agent load this repository routinely runs) is folded into the same
`false`. All three collapse to "anchor at this candidate." Because the walk is
oldest-first, a spurious `false` anchors *earlier* than correct, producing an
**over-inclusive** range.

Failure scenario: one transient `git` spawn failure on the first candidate makes
`release_range_start` return C1 instead of the sync merge. The classified range
then re-admits pre-release `develop` history — exactly the 677-commit / 62-`feat`
condition the anchor exists to prevent. Downstream, `compute_version` computes an
inflated bump (e.g. `2.0.0 → 3.0.0` from an old `feat!:`), and
`preflight_major_bump_check` opens a spurious never-silent MAJOR gate. Note the
sibling helper `first_parent` (`version.rs:239-251`) already does this correctly,
propagating spawn errors via `?` and treating only non-zero exit as "no parent" —
this call site is inconsistent with it.

**Fix:** Propagate the spawn error, and treat only exit code 1 as a real
negative:

```rust
let out = Command::new("git")
    .args(["merge-base", "--is-ancestor", baseline_tag, &first_parent])
    .current_dir(project_root)
    .output()
    .map_err(|err| VersionError::Git(err.to_string()))?;
let tag_is_ancestor_of_first_parent = match out.status.code() {
    Some(0) => true,
    Some(1) => false,
    // 128 / signal / anything else is an error, not an answer — refuse
    // rather than silently anchoring the range in the wrong place.
    _ => {
        return Err(VersionError::Git(format!(
            "`git merge-base --is-ancestor {baseline_tag} {first_parent}` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
};
```

### WR-02: `wait_for_exec_visibility`'s second guard compares against the CALLER, not the PARENT

**File:** `crates/devflow-core/src/test_support.rs:101,120`

**Issue:** The fork-inheritance window makes `/proc/<pid>/cmdline` report the
**parent's** argv. Guard (ii) is:

```rust
let self_cmdline = std::fs::read(format!("/proc/{}/cmdline", std::process::id())).ok();
...
let differs_from_caller = self_cmdline.as_deref() != Some(raw.as_slice());
```

which compares against the **caller's** cmdline. Every current call site happens
to be the direct parent, so the guard holds today — but the doc comment claims
the barrier's answer is "unambiguous" rather than "probabilistically-correct,"
and that claim does not survive the first non-parent caller.

Failure scenario: a test waits on the monitor's trailing `devflow advance` child,
spawned by a `devflow` process (`monitor.rs:126-130`). During that child's
fork/exec window its cmdline is the parent `devflow`'s argv, so
`expected_argv0_basename == "devflow"` satisfies guard (i); the caller is the
test binary, so guard (ii) also passes. The barrier returns `true` *before* the
child has exec'd — the precise condition it exists to exclude — and the census
assertion that follows becomes vacuous again.

Secondary: if `/proc/self/cmdline` is unreadable, `self_cmdline` is `None` and
`differs_from_caller` is unconditionally `true`, silently degrading the function
to guard (i) alone. That degradation is undocumented.

**Fix:** Compare against the pid's actual parent, read from `/proc/<pid>/stat`
field 4 (`ppid`), rather than against `std::process::id()`; or, if the
parent-only invariant is intended, make it explicit in both the name and the doc
(e.g. `wait_for_child_exec_visibility`) so a non-parent caller cannot be written
by accident. Also handle the `None` case explicitly:

```rust
// A caller that cannot read its own cmdline has no guard (ii) — say so
// rather than silently proceeding on guard (i) alone.
let Some(self_cmdline) = std::fs::read(format!("/proc/{}/cmdline", std::process::id())).ok()
else {
    return false;
};
```

### WR-03: the new staleness regression test leaks a real, detached monitor wrapper on every run

**File:** `crates/devflow-cli/src/staleness.rs:689-786` (`mid_run_stage_transition_does_not_readjudicate_staleness`, spawn at `:767`)

**Issue:** `launch_stage_inner` calls `monitor::spawn_monitor`, which spawns a
detached `sh -c "...; trap cleanup TERM INT; ..."` (`monitor.rs:135-160`) with
stdin/stdout/stderr on `/dev/null`. The test asserts `result.expect(...)` and
then reads events — it never records the spawned pid and never kills or waits
it. The `TempDir` guard then unlinks the project root out from under the live
process.

Failure scenario: every `cargo test --workspace` leaves behind a live Layer-1
monitor wrapper whose project root has been deleted — which is *literally*
999.44's reproduction shape, manufactured by this phase's own test suite. On this
machine right now, `ps -eo args | grep -c "trap cleanup TERM INT"` reports 21,
and `devflow doctor` reports 38 stray findings, most of them rooted at deleted
`/tmp/.tmp*` paths. This also weakens the sibling census tests: a leaked wrapper
is a live Layer-1 match that `discover_stray_devflow_processes` will return in
every subsequent run.

(The same omission exists in the pre-existing
`launch_stage_persists_monitor_pid_for_reload` at `pipeline_launch.rs:395-424`
— noted, not attributed to this phase, but it should be fixed at the same time
since the fix is shared.)

**Fix:** Reap what the test spawns, on every exit path — the same constraint
`reap_strays_e2e.rs:219-223` already documents and follows:

```rust
result.expect("a mid-run stage transition must not re-invoke ...");

// 999.46: always reap what this test spawned. `launch_stage_inner`
// records the monitor pid on the state it was given.
if let Some(pid) = state.monitor_pid {
    devflow_core::agent::terminate_and_verify(
        pid,
        devflow_core::agent::TERMINATE_VERIFY_WAIT,
        devflow_core::agent::TERMINATE_VERIFY_POLL,
    );
}
```

Better still, extract that into a shared `test_support` helper so no future test
that drives a launch path can forget it.

### WR-04: `reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age` is flaky by construction

**File:** `crates/devflow-cli/src/commands.rs:3727-3762`

**Issue:** The test spawns a fixture, crosses `wait_for_exec_visibility` with a
ceiling of `EXEC_VISIBILITY_WAIT` = **10s** (`test_support.rs:61`), and then
asserts the fixture is younger than `STRAY_MIN_AGE` = **2s** (`agent.rs:287`).
The barrier's own bound is five times the assertion's budget.

Failure scenario: under the loaded, 2-core-pinned shape
`scripts/check-in-container.sh all` runs — the exact load profile
`25-CI-OBSERVATION.md` records as the environment where this defect class
manifests — the barrier takes >2s to resolve. `process_age` then reports ≥2s,
`reap_stray_candidates` returns `Reaped` instead of `TooYoung`, and the test both
fails *and* SIGKILLs its fixture, so the follow-up `agent_running(pid)` assertion
fails with a misleading message. The failure reads as "the age floor is broken"
when the floor worked correctly.

**Fix:** Make the assertion independent of wall-clock scheduling — assert the
premise explicitly before relying on it:

```rust
let age = agent::process_age(pid).expect("fixture age must resolve");
assert!(
    age < agent::STRAY_MIN_AGE,
    "fixture aged past the floor before the assertion could run ({age:?} >= {:?}) — \
     this test's premise is time-dependent and must be re-derived, not force-passed",
    agent::STRAY_MIN_AGE
);
let results = reap_stray_candidates(&[candidate], false, agent::STRAY_MIN_AGE);
```

or pass a large synthetic `min_age` (e.g. `Duration::from_secs(3600)`) so the
refusal is deterministic regardless of how long the barrier took — which is
exactly the parameterisation `min_age` was introduced for, per
`reap_stray_candidates`' own doc comment.

## Info

### IN-01: `process_age` can panic where its contract promises `None`

**File:** `crates/devflow-core/src/agent.rs:257-267`

**Issue:** `Duration::from_secs_f64` panics on a non-finite or overflowing
value. `"inf".parse::<f64>()` succeeds, and `f64::max` only absorbs `NaN`
(`NAN.max(0.0) == 0.0`), not infinity — so a `/proc/uptime` whose first field
reads `inf` yields `Duration::from_secs_f64(f64::INFINITY)` and aborts the
process. The doc comment explicitly promises `None` when "`/proc/uptime` is
unreadable or unparseable." Not reachable through a real Linux kernel, but the
function's whole value is its fail-closed posture.

**Fix:** `std::time::Duration::try_from_secs_f64(age_secs).ok()`, or guard with
`age_secs.is_finite()` before constructing the `Duration`.

### IN-02: `gate_sweep`'s `TooYoung` message prints the constant, not the floor that was applied

**File:** `crates/devflow-cli/src/commands.rs:1204-1214`

**Issue:** The message interpolates `agent::STRAY_MIN_AGE` directly rather than
the `min_age` value passed to `reap_stray_candidates` at `:1151`. They agree
today only because the call site passes the constant — which is exactly the
implicit coupling `reap_stray_candidates`' doc comment says the `min_age`
parameter exists to remove. Any future call site with a different floor prints a
message that contradicts the decision it is explaining.

**Fix:** Hoist `let min_age = agent::STRAY_MIN_AGE;` above the call at `:1151`,
pass it, and interpolate the same binding in the message.

### IN-03: `breaking_commit_subjects` uses a different breaking-change rule than the classifier it explains

**File:** `crates/devflow-cli/src/preflight.rs:404-408`

**Issue:** The diagnostic scans `subject.split_once(':')` for a `!` in the
prefix and `message.contains("BREAKING CHANGE:")` anywhere in the body, while
`version::classify_commit_message` (`version.rs:429-453`) delegates to
`git_conventional::Commit::parse().breaking()`, which is footer-aware. The two
can disagree in both directions: a body that merely mentions `BREAKING CHANGE:`
mid-paragraph is listed as a "deciding commit" without being one, and a footer
form `git_conventional` accepts but this substring check misses yields
"classified bump is MAJOR" with an empty deciding-commit list. The doc comment
claims it re-scans "the same range ... so a human ... can see which commit(s)
carry a breaking marker."

**Fix:** Reuse the classifier rather than re-deriving it —
`git_conventional::Commit::parse(message).map(|c| c.breaking()).unwrap_or(false)`
— so the diagnostic can never contradict the decision.

### IN-04: `test-support` feature doc no longer describes what the feature exposes

**File:** `crates/devflow-core/Cargo.toml:13-16`

**Issue:** The comment reads "Exposes `test_support` (hermetic git command
construction, 999.37)". As of 25-11 the same gate also exposes
`wait_for_exec_visibility`, `EXEC_VISIBILITY_WAIT` and `EXEC_VISIBILITY_POLL`,
which `crates/devflow-cli/tests/reap_strays_e2e.rs:106-111` depends on
cross-crate. A reader deciding whether the feature is still needed gets an
incomplete answer.

**Fix:** Extend the comment to name both hazards the module now covers (999.37
hermetic git, 999.47 exec-visibility barrier), matching the module doc at
`test_support.rs:1-31`.

---

_Reviewed: 2026-07-28T16:06:43Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
