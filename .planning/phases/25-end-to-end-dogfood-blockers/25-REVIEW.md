---
phase: 25-end-to-end-dogfood-blockers
reviewed: 2026-07-28T00:00:00Z
depth: standard
files_reviewed: 12
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-cli/src/test_support.rs
  - crates/devflow-cli/tests/reap_strays_e2e.rs
  - crates/devflow-core/Cargo.toml
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/test_support.rs
  - crates/devflow-core/src/version.rs
findings:
  critical: 0
  warning: 6
  info: 4
  total: 10
status: issues_found
---

# Phase 25: Code Review Report (re-review after gap-closure round 3)

**Reviewed:** 2026-07-28T00:00:00Z
**Depth:** standard
**Files Reviewed:** 12
**Status:** issues_found

> **This review supersedes the 2026-07-28T16:06:43Z revision.** That revision's two
> Critical findings (CR-01, CR-02) and one of its Warnings (WR-03) are re-adjudicated below
> against plans 25-14/25-15/25-16's gap-closure diff. Its remaining Warnings (WR-01, WR-02,
> WR-04) and Info items (IN-01–IN-04) were **not** in scope for this round and are carried
> forward unchanged — re-verified against the current tree, renumbered, not re-derived from
> scratch.

## Summary

`git show HEAD:.planning/phases/25-end-to-end-dogfood-blockers/25-REVIEW.md` (the prior
revision) raised CR-01, CR-02, and WR-03. Plans 25-14 (CR-02), 25-15 (CR-01), and 25-16
(WR-03) claim to close them. Adjudicated below, explicitly:

- **CR-02 — CLOSED.** `preflight.rs::ensure_base_ref_current`'s `Behind` arm now calls
  `fast_forward_base_ref`, a genuine `git update-ref refs/heads/<base> <new> <expected_old>`
  compare-and-swap (not the old unconditional two-argument write), gated by
  `base_is_checked_out_anywhere`, which genuinely parses `git worktree list --porcelain`
  across every worktree of the repository — not just `project_root`'s own `HEAD` (the old
  bug). Both halves are exercised by real-git fixtures, not mocks:
  `fast_forward_base_ref_refuses_a_stale_expected_old_value` proves the CAS refuses a stale
  `expected_old` and leaves the ref byte-identical; `currency_behind_refuses_when_base_is_
  checked_out_in_another_worktree` proves a `develop` checked out in a *linked* worktree
  (while `project_root`'s own HEAD sits on an unrelated branch) is still detected and the
  write is refused, leaving the ref unmoved. The one residual the doc comment documents (a
  worktree that checks out `base` in the window between the repo-wide scan and the CAS is
  not protected by the scan) is real but correctly scoped — the CAS still prevents a lost
  update in that window; it just cannot prevent that worktree from observing a moved HEAD.
  This is an accepted, explicitly-documented trade, not a silent gap.

- **CR-01 — CLOSED.** `commands.rs::doctor` and `commands.rs::gate_sweep`'s `--reap-strays`
  pass both now route through the single composition `unreachable_stray_candidates` →
  `retain_unreachable_strays` (filtering `agent::discover_stray_devflow_processes()`) against
  `registry_reachable_pids(&stray_safety_roots(extra_roots))`. Verified point by point:
  - `registry_reachable_pids` (`commands.rs:3050`) reads `state.monitor_pid` via
    `workflow::list_states` and the lock holder via `lock::holder_identity` — **never**
    `lock::holder` (which deletes an empty lock file) and **never**
    `registry::prune_missing` — so `doctor`'s read-only contract holds structurally, not by
    convention.
  - `stray_safety_roots` (`commands.rs:3100`) unions `registry::load_roots()` with the
    caller's extra roots and never narrows; `gate_sweep`'s `explicit_root` (from `--root`) is
    likewise only ever unioned in (`commands.rs:1067,1174`), never substituted, and prints an
    explicit note to that effect when `--root` is passed with `--reap-strays`
    (`commands.rs:1165-1173`) — confirmed this cannot narrow the protected set, so `gate
    sweep --root R --reap-strays` cannot un-protect other roots' live processes.
  - A live monitor's pid recorded in `state.monitor_pid`/the lock file is now provably
    excluded from both `doctor`'s "stray" finding and `gate sweep --reap-strays`'s kill list
    — the exact 14-of-38 false-positive reproduction the prior review measured live is
    structurally closed by the filter, not merely narrowed.

- **WR-03 — PARTIALLY CLOSED.** See WR-05 and WR-06 below: the shared
  `test_support::reap_spawned_monitor` helper is real, correct, and was wired into the two
  call sites `25-16-PLAN.md` names (`pipeline_launch.rs`'s
  `launch_stage_persists_monitor_pid_for_reload`, `staleness.rs`'s
  `mid_run_stage_transition_does_not_readjudicate_staleness`). But two more pre-existing
  tests that drive the identical real-monitor-spawning path were left untouched (WR-05), and
  the two sites that *were* fixed still leak on a panic between the spawn and the reap call,
  because the reap is a plain trailing statement rather than an unwind-safe guard (WR-06).

`cargo check --workspace --all-targets` was not re-run as part of this review (read-only,
static review); no changes were made to any reviewed file.

## Warnings

### WR-05: Two `preflight.rs` tests still leak a real detached monitor — WR-03 not fully closed

**File:** `crates/devflow-cli/src/preflight.rs:1543-1595` (`run_preflight_advance_gate_launches_agent_exactly_once`), `crates/devflow-cli/src/preflight.rs:1604-1657` (`run_preflight_loopback_gate_launches_agent_exactly_once`)

**Issue:** Both tests drive `run_preflight` with a `FailOnceAdapter` and a pre-seeded gate
response, forcing resolution via `GateAction::Advance` or `GateAction::LoopBack`. Traced
through the actual (not assumed) code path:

- The `Advance` arm (`preflight.rs:935-944`) calls `launch_stage_inner(state, None, None)?`
  directly.
- The `LoopBack` arm (`preflight.rs:945-950`) calls `launch_stage(state, None, None)?`, which
  re-resolves the **real** production adapter via `agents::adapter_for(state.agent)`
  (Claude — whose default preflight passes, as the test's own comment notes), re-runs
  `run_preflight` (which now passes against the real adapter), and falls through to
  `launch_stage_inner` anyway.

Either path reaches `monitor::spawn_monitor` for real (`pipeline_launch.rs:123` →
`monitor.rs:148-160`, `Command::new("sh").arg("-c").arg(&script)...spawn()`) — a live,
unwaited child process, exactly the shape `reap_spawned_monitor` exists to clean up. Both
tests use `stub_agent_binary("claude")` + `prepend_path`, the identical "real program name,
stubbed binary" construction `launch_stage_persists_monitor_pid_for_reload` uses to make its
own real spawn happen — there is no test-mode branch anywhere in `launch_stage_inner` or
`monitor::spawn_monitor` that would make this a no-op for these two tests specifically.

Neither test calls `reap_spawned_monitor` (or anything else) before its `TempDir` (`dir`)
drops and deletes the project root out from under the still-running wrapper — 999.44's exact
reproduction shape. This directly contradicts the premise that `reap_spawned_monitor` "now
covers both" real-launch call sites: it covers the two sites `25-16-PLAN.md` named, not these
two pre-existing tests (owned by no plan — they date to the 17-08 gap closure) that exercise
the same launch path via a different route (the `Advance`/`LoopBack` recursion, rather than a
direct `launch_stage` call).

This is empirically hard to catch by measuring leaked processes after the fact: the stubbed
`claude` binary exits in well under a millisecond, and the wrapper script's trailing
`devflow advance ...` invocation resolves `binary = current_exe()` to the **test binary
itself** under `cargo test` — which rejects the `advance`/`--phase` argument shape as an
invalid test-filter option and exits almost immediately. The whole `sh -c` wrapper therefore
tends to have already exited by the time anything checks for it on an unloaded machine — a
timing accident, not a structural guarantee, and exactly the kind of load-sensitivity this
project has already measured once for a related defect class (`25-CI-OBSERVATION.md`: 0
failures across 17 warm runs, 2 failures in 2 attempts under `scripts/check-in-container.sh
all`'s loaded, 2-core-pinned shape). On a slow or loaded CI host this remains a live
process-leak source, indistinguishable from the two sites this round fixed.

**Fix:** Add the same trailing call the sibling tests already use, right after the existing
assertions in both tests:

```rust
// after `assert_eq!(launches, 1, ...)` in both tests:
reap_spawned_monitor(&state);
```

`reap_spawned_monitor` is already `pub(crate)` in `test_support.rs` and already reachable via
this module's `use crate::test_support::*;` — no new import needed.

### WR-06: `reap_spawned_monitor` is called as a plain trailing statement — a panic between spawn and reap still leaks the process

**File:** `crates/devflow-cli/src/pipeline_launch.rs:414-441` (`launch_stage_persists_monitor_pid_for_reload`), `crates/devflow-cli/src/staleness.rs:689-803` (`mid_run_stage_transition_does_not_readjudicate_staleness`, reap call at `:802`), `crates/devflow-cli/src/test_support.rs:322-336` (`reap_spawned_monitor`)

**Issue:** `reap_spawned_monitor`'s own doc comment states: "Must be called BEFORE the
caller's `TempDir` guard drops — reaping after the project root has already been deleted is
999.44's reproduction shape with extra steps, not a fix for it" (`test_support.rs:313-315`).
At both fixed call sites, the reap call is a plain trailing statement — not a `Drop` guard,
not `scopeguard`, not inside a `catch_unwind` boundary — so it only runs if every prior
statement in the test body returns normally.

In `pipeline_launch.rs:414-441`, the reap call (line 440) is preceded by four separate
panicking checkpoints: `result.unwrap()` (`:423`), `assert!(state.monitor_pid.is_some(), ...)`
(`:425-428`), `workflow::load_state(root, phase).unwrap()` (`:429`), and
`assert_eq!(reloaded.monitor_pid, state.monitor_pid, ...)` (`:430-434`) — any one of which, on
failure, unwinds the test function and drops `dir` (the `TempDir`) without ever reaching the
reap call. The identical shape recurs in `staleness.rs:689-803`: `result.expect(...)` (`:777`)
then an `assert_eq!` on `blocked_count` (`:792-796`) both precede the reap call at `:802`.

This defeats the tests' purpose in exactly the scenario where reaping matters most: if a
future regression reintroduces the bug either test exists to catch (e.g. `launch_stage` stops
persisting `monitor_pid`, or the staleness check re-fires mid-run), the assertion that
detects the regression panics *before* the reap call runs, so the real monitor wrapper this
same test just spawned is left running against a soon-to-be-deleted project root. The
resulting CI run would report a genuine regression **and** manufacture a fresh orphan process
from the very test written to guard against orphans. A plain trailing statement cannot
satisfy "runs on every exit path, including panic paths" — Rust does not execute subsequent
statements once a panic has begun unwinding.

**Fix:** Reap unconditionally regardless of how the test body exits, e.g. via a small RAII
guard:

```rust
/// Reaps `state`'s spawned monitor on drop, including during an unwind —
/// unlike a trailing `reap_spawned_monitor(&state)` call, which never runs
/// if an assertion between the spawn and that call panics.
struct ReapMonitorOnDrop<'a>(&'a State);
impl Drop for ReapMonitorOnDrop<'_> {
    fn drop(&mut self) {
        reap_spawned_monitor(self.0);
    }
}
```

```rust
result.unwrap();
let _reap_guard = ReapMonitorOnDrop(&state); // reaps even if a later assertion panics
assert!(state.monitor_pid.is_some(), /* ... */);
// ... remaining assertions unchanged ...
```

(If a later line in either test needs `&mut state` while the guard is alive, hold the pid by
value — `let _reap_guard = ReapMonitorOnDrop(state.monitor_pid);` with a matching `Drop` that
takes `Option<u32>` — rather than borrowing the whole `State`.)

### WR-01 (carried forward, unresolved, out of scope for this round): `release_range_start` cannot distinguish "not an ancestor" from "git failed"

**File:** `crates/devflow-core/src/version.rs:338-349`

**Issue:** `.map(|out| out.status.success()).unwrap_or(false)` folds a genuine `git
merge-base --is-ancestor` error (exit 128, or a spawn failure) into the same `false` as a
legitimate "not an ancestor" (exit 1) answer, both of which anchor the release range at the
current candidate. Because the walk is oldest-first, a spurious `false` anchors *earlier*
than correct, producing an over-inclusive range that can inflate `preflight_major_bump_check`
into a spurious MAJOR gate. Unchanged from the prior review; not addressed by 25-14/25-15/
25-16 (none of which touch `version.rs`). See the prior revision
(`git show HEAD~1:.planning/phases/25-end-to-end-dogfood-blockers/25-REVIEW.md`, WR-01) for
the full fix.

### WR-02 (carried forward, unresolved, out of scope for this round): `wait_for_exec_visibility`'s guard (ii) compares against the caller, not the parent

**File:** `crates/devflow-core/src/test_support.rs:101,120`

**Issue:** `self_cmdline` is read from the *caller's* `/proc/self/cmdline`, not the target
pid's actual parent. Every current call site happens to be the direct parent, so the guard
holds today, but the doc comment's "unambiguous" claim does not survive a future caller
waiting on a *grandchild* (e.g. the monitor's trailing `devflow advance` invocation, itself
spawned by a `devflow` process, not by the test binary) — in that shape guard (i) and (ii)
can both pass during the grandchild's own fork/exec window. Unchanged from the prior review;
this file is otherwise unmodified by this round's diff. See the prior revision for the full
fix (compare against `ppid` from `/proc/<pid>/stat`, or rename the function to make the
parent-only invariant explicit).

### WR-04 (carried forward, unresolved, out of scope for this round): the `TooYoung` regression test is flaky by construction

**File:** `crates/devflow-cli/src/commands.rs:3861-3895` (`reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age`)

**Issue:** The fixture crosses `wait_for_exec_visibility` with a 10-second ceiling
(`EXEC_VISIBILITY_WAIT`, `test_support.rs:61`) and then asserts the fixture is younger than
`STRAY_MIN_AGE` = 2 seconds (`agent.rs:287`) — the barrier's bound is five times the
assertion's budget. Under a loaded machine (the exact shape `25-CI-OBSERVATION.md` already
measured for a related defect class) the barrier can legitimately take long enough that the
fixture ages past the 2s floor before the assertion runs, at which point the test both fails
*and* SIGKILLs its own fixture. Unchanged from the prior review; not addressed by this
round's diff. See the prior revision for the full fix (assert the age premise explicitly
before relying on it, or use a large synthetic `min_age`).

## Info

### IN-01 (carried forward, unresolved): `process_age` can panic where its contract promises `None`

**File:** `crates/devflow-core/src/agent.rs:257-267`

**Issue:** `Duration::from_secs_f64` panics on a non-finite value; `.max(0.0)` absorbs `NaN`
but not `+inf`, so a `/proc/uptime` whose first field parses as `inf` would abort the
process, contradicting the doc comment's promise of `None` on any unparseable input. Not
reachable through a real Linux kernel; unchanged from the prior review. Fix:
`Duration::try_from_secs_f64(age_secs).ok()`, or guard with `age_secs.is_finite()`.

### IN-02 (carried forward, unresolved): `gate_sweep`'s `TooYoung` message prints the constant, not the floor actually applied

**File:** `crates/devflow-cli/src/commands.rs:1175` (call site), `crates/devflow-cli/src/commands.rs:1228-1237` (message)

**Issue:** The message at `:1232-1236` interpolates `agent::STRAY_MIN_AGE` directly rather
than the `min_age` value actually passed to `reap_stray_candidates` at `:1175`. They agree
today only because the one call site passes the constant — exactly the implicit coupling the
`min_age` parameter exists to remove, per `reap_stray_candidates`'s own doc comment. Unchanged
from the prior review. Fix: bind `min_age` once above the call and interpolate that binding
in the message instead of the constant.

### IN-03 (carried forward, unresolved): `breaking_commit_subjects` uses a different breaking-change rule than the classifier it explains

**File:** `crates/devflow-cli/src/preflight.rs:772-810` (subject-detection logic at `:800-804`)

**Issue:** This diagnostic scans `subject.split_once(':')` for `!` in the prefix plus a bare
substring search for `"BREAKING CHANGE:"`/`"BREAKING-CHANGE:"` anywhere in the message, while
`version::classify_commit_message` delegates to `git_conventional::Commit::parse().breaking()`
(footer-aware). The two can disagree in both directions — a body that merely mentions
`BREAKING CHANGE:` mid-paragraph is listed as a "deciding commit" without being one, and a
footer form `git_conventional` accepts but this substring check misses yields "classified
bump is MAJOR" with an empty deciding-commit list in the gate message. Unchanged from the
prior review. Fix: reuse `git_conventional::Commit::parse(message).map(|c| c.breaking())
.unwrap_or(false)` instead of re-deriving the rule.

### IN-04 (carried forward, unresolved): `test-support` feature comment is stale

**File:** `crates/devflow-core/Cargo.toml:13-16`

**Issue:** The comment still describes the feature as exposing only "hermetic git command
construction, 999.37." Since 25-11 the same gate also exposes `wait_for_exec_visibility`,
`EXEC_VISIBILITY_WAIT`, and `EXEC_VISIBILITY_POLL` (used cross-crate by
`crates/devflow-cli/tests/reap_strays_e2e.rs:106-111`). Unchanged from the prior review. Fix:
extend the comment to name both hazards, matching `devflow-core/src/test_support.rs:1-31`'s
own module doc.

---

_Reviewed: 2026-07-28T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
