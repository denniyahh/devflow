---
phase: 25-end-to-end-dogfood-blockers
reviewed: 2026-07-28T00:00:00Z
depth: standard
files_reviewed: 4
files_reviewed_list:
  - crates/devflow-cli/src/test_support.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/staleness.rs
findings:
  critical: 1
  warning: 3
  info: 4
  total: 8
status: issues_found
---

# Phase 25: Code Review Report (re-review after gap-closure round 4)

**Reviewed:** 2026-07-28T00:00:00Z
**Depth:** standard
**Files Reviewed:** 4
**Status:** issues_found

> **This revision re-adjudicates only the diff `c956bf78..HEAD`**, which touches exactly the
> four files listed above (plans 25-17, 25-18). The prior revision's WR-03/WR-05/WR-06 are
> re-adjudicated below against this diff and found CLOSED. Its WR-01, WR-02, WR-04 and
> IN-01–IN-04 were **not** in scope for this round (their files are untouched by this diff) and
> are carried forward unchanged, re-verified against the current tree, not re-derived from
> scratch.

## Summary

Round 4 (25-17, 25-18) converts the five test sites that drive a real `launch_stage_inner` /
`monitor::spawn_monitor` call from a plain trailing `reap_spawned_monitor(&state)` statement to
an RAII guard, `ReapMonitorOnDrop`, bound immediately after each test's final `&mut state` use.
I traced all five binding sites individually (`pipeline_launch.rs:426`, `staleness.rs:776`,
`preflight.rs:1587`, `preflight.rs:1677`, `preflight.rs:1793`) and confirmed each is bound
strictly after `state.monitor_pid` is populated by the real launch and strictly before every
subsequent panicking checkpoint (`.unwrap()`, `.expect()`, `assert!`, `assert_eq!`) in that test
— WR-06 is genuinely closed, not just relocated. The third, previously-unplanned site
(`run_preflight_advance_skips_recheck_on_idempotently_failing_check`) is a real leak: I traced
`run_preflight`'s `GateAction::Advance` arm (`preflight.rs:935-943`) and confirmed it calls
`launch_stage_inner` **unconditionally** — no recursive re-check to fail first — so with a
working `codex`+`sh` stub on `PATH` this test genuinely spawns a detached monitor wrapper; its
fix matches the other four sites exactly. I also confirmed, by reading every other
`launch_stage`/`launch_stage_inner` call site inside these four files' test modules, that no
sixth leak site was missed: `launch_stage_inner_clears_monitor_pid_on_early_failure`
(`pipeline_launch.rs:606`) neutralizes `PATH` to a `git`-only directory specifically so
`ensure_agent_binary` fails before `spawn_monitor` ever runs, and
`run_preflight_failing_check_gates_and_never_reaches_spawn_monitor` /
`run_preflight_adapter_hook_override_fires` / `run_preflight_loopback_bounds_recursion` all
resolve to `Abort` or hit the retry ceiling before any `GateAction::Advance`/`LoopBack` arm is
reached — none of the three spawns a monitor, so none needed conversion.

The discriminating test pair (`reap_guard_reaps_the_monitor_when_a_later_assertion_panics` /
`trailing_reap_call_is_skipped_when_a_later_assertion_panics`) is genuinely non-vacuous: I
traced `catch_unwind`'s interaction with local-variable drop order in both tests and confirmed
the guard-bound closure reaps its real `sleep 300` child during the unwind (locals drop before
`catch_unwind` returns `Err`), while the control's plain trailing call — placed after the same
failing assertion — never executes, leaving its child alive at the point the control's own
assertion checks it (the control's own outer `ReapMonitorOnDrop`, bound before the closure,
still hasn't dropped at that point, since it drops only at function end). All edits are
test-only: `crates/devflow-cli/src/test_support.rs` is included only via `#[cfg(test)] mod
test_support;` (`main.rs:7-8`), and every other diff hunk is inside a pre-existing `#[cfg(test)]
mod tests` block. `cargo check -p devflow --tests` compiles clean against the current tree.

One new gap survives this round's fix: the double-panic interlock in `ReapMonitorOnDrop::drop`
guards its own `panic!` call behind `std::thread::panicking()`, but the `eprintln!` in the
*other* branch of that same `if` is not itself panic-safe (see CR-01). All other
`Drop`-path panic sources (the `assert!`/`panic!`s inside `reap_monitor_pid`,
`agent_running`, `terminate_and_verify`) were traced into `devflow-core/src/agent.rs` and
confirmed to contain no `unwrap`/`expect`/`assert`/panicking-format call on any path — that
half of the interlock holds.

## Critical Issues

### CR-01: `ReapMonitorOnDrop::drop`'s "safe" branch can itself panic during an in-flight unwind

**File:** `crates/devflow-cli/src/test_support.rs:404-409`

**Issue:** The whole point of `std::thread::panicking()` in this `Drop` impl is to avoid ever
calling something that can panic while a panic is already unwinding — a second panic during
unwind calls `abort()`, killing the entire test binary (~694 tests) instead of failing the one
in flight. The `panicking()` branch chosen specifically to be "safe" is:

```rust
if std::thread::panicking() {
    eprintln!(
        "ReapMonitorOnDrop: monitor wrapper pid {pid} still alive after reap \
         during an unwind — not re-panicking because a panic is already in flight"
    );
}
```

`eprintln!` is not safe here: `std::io::_eprint` (the function the macro expands to) calls
`panic!("failed printing to stderr: {e}")` if the underlying `write_fmt` returns an `Err` —
e.g. a closed or broken stderr file descriptor. That is a second panic while
`std::thread::panicking()` is already `true`, which is exactly the `abort()` path this branch
exists to avoid. In practice this requires an external I/O failure on `stderr` (not reachable
under a normal `cargo test` run, where the harness's output-capture buffer essentially cannot
fail to write), but the interlock's own doc comment claims this is "the ONLY thing standing
between an assertion failed and the whole test binary aborted" — that guarantee is not actually
airtight while the fallback branch itself can panic. Given how narrow but real this window is,
and that its blast radius is the entire test binary rather than one test, this is flagged as
Critical rather than downgraded for low likelihood.

**Fix:** Use a write path that cannot panic on failure, e.g.:

```rust
if std::thread::panicking() {
    use std::io::Write;
    let _ = writeln!(
        std::io::stderr(),
        "ReapMonitorOnDrop: monitor wrapper pid {pid} still alive after reap \
         during an unwind — not re-panicking because a panic is already in flight"
    );
}
```

## Warnings

### WR-01 (carried forward, unresolved, out of scope for this round): `release_range_start` cannot distinguish "not an ancestor" from "git failed"

**File:** `crates/devflow-core/src/version.rs:338-349`

**Issue:** `.map(|out| out.status.success()).unwrap_or(false)` folds a genuine `git
merge-base --is-ancestor` error (exit 128, or a spawn failure) into the same `false` as a
legitimate "not an ancestor" (exit 1) answer, both of which anchor the release range at the
current candidate. Because the walk is oldest-first, a spurious `false` anchors *earlier*
than correct, producing an over-inclusive range that can inflate `preflight_major_bump_check`
into a spurious MAJOR gate. Unchanged this round (`version.rs` is outside this round's diff).
See the round-3 revision for the full fix.

### WR-02 (carried forward, unresolved, out of scope for this round): `wait_for_exec_visibility`'s guard (ii) compares against the caller, not the parent

**File:** `crates/devflow-core/src/test_support.rs:101,120`

**Issue:** `self_cmdline` is read from the *caller's* `/proc/self/cmdline`, not the target
pid's actual parent. Every current call site happens to be the direct parent, so the guard
holds today, but the doc comment's "unambiguous" claim does not survive a future caller
waiting on a *grandchild* (e.g. the monitor's trailing `devflow advance` invocation, itself
spawned by a `devflow` process, not by the test binary) — in that shape guard (i) and (ii)
can both pass during the grandchild's own fork/exec window. Unchanged this round — note this
is `crates/devflow-core/src/test_support.rs` (a different file/crate from
`crates/devflow-cli/src/test_support.rs`, which this round's diff does touch); the core crate's
copy is untouched. See the round-3 revision for the full fix (compare against `ppid` from
`/proc/<pid>/stat`, or rename the function to make the parent-only invariant explicit).

### WR-04 (carried forward, unresolved, out of scope for this round): the `TooYoung` regression test is flaky by construction

**File:** `crates/devflow-cli/src/commands.rs:3861-3895` (`reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age`)

**Issue:** The fixture crosses `wait_for_exec_visibility` with a 10-second ceiling
(`EXEC_VISIBILITY_WAIT`, `crates/devflow-core/src/test_support.rs:61`) and then asserts the
fixture is younger than `STRAY_MIN_AGE` = 2 seconds (`agent.rs:287`) — the barrier's bound is
five times the assertion's budget. Under a loaded machine (the exact shape
`25-CI-OBSERVATION.md` already measured for a related defect class) the barrier can
legitimately take long enough that the fixture ages past the 2s floor before the assertion
runs, at which point the test both fails *and* SIGKILLs its own fixture. Unchanged this round
— `commands.rs` is outside this round's diff. See the round-3 revision for the full fix (assert
the age premise explicitly before relying on it, or use a large synthetic `min_age`).

## Info

### IN-01 (carried forward, unresolved): `process_age` can panic where its contract promises `None`

**File:** `crates/devflow-core/src/agent.rs:257-267`

**Issue:** `Duration::from_secs_f64` panics on a non-finite value; `.max(0.0)` absorbs `NaN`
but not `+inf`, so a `/proc/uptime` whose first field parses as `inf` would abort the
process, contradicting the doc comment's promise of `None` on any unparseable input. Not
reachable through a real Linux kernel; unchanged this round. Fix:
`Duration::try_from_secs_f64(age_secs).ok()`, or guard with `age_secs.is_finite()`.

### IN-02 (carried forward, unresolved): `gate_sweep`'s `TooYoung` message prints the constant, not the floor actually applied

**File:** `crates/devflow-cli/src/commands.rs:1175` (call site), `crates/devflow-cli/src/commands.rs:1228-1237` (message)

**Issue:** The message at `:1232-1236` interpolates `agent::STRAY_MIN_AGE` directly rather
than the `min_age` value actually passed to `reap_stray_candidates` at `:1175`. They agree
today only because the one call site passes the constant — exactly the implicit coupling the
`min_age` parameter exists to remove, per `reap_stray_candidates`'s own doc comment. Unchanged
this round. Fix: bind `min_age` once above the call and interpolate that binding in the message
instead of the constant.

### IN-03 (carried forward, unresolved): `breaking_commit_subjects` uses a different breaking-change rule than the classifier it explains

**File:** `crates/devflow-cli/src/preflight.rs:772-810` (subject-detection logic at `:800-804`)

**Issue:** This diagnostic scans `subject.split_once(':')` for `!` in the prefix plus a bare
substring search for `"BREAKING CHANGE:"`/`"BREAKING-CHANGE:"` anywhere in the message, while
`version::classify_commit_message` delegates to `git_conventional::Commit::parse().breaking()`
(footer-aware). The two can disagree in both directions — a body that merely mentions
`BREAKING CHANGE:` mid-paragraph is listed as a "deciding commit" without being one, and a
footer form `git_conventional` accepts but this substring check misses yields "classified
bump is MAJOR" with an empty deciding-commit list in the gate message. This region of
`preflight.rs` is outside this round's diff (this round's edits are confined to
`mod tests`, lines 1568-1827) — line numbers re-verified unchanged. Fix: reuse
`git_conventional::Commit::parse(message).map(|c| c.breaking()).unwrap_or(false)` instead of
re-deriving the rule.

### IN-04 (carried forward, unresolved): `test-support` feature comment is stale

**File:** `crates/devflow-core/Cargo.toml:13-16`

**Issue:** The comment still describes the feature as exposing only "hermetic git command
construction, 999.37." Since 25-11 the same gate also exposes `wait_for_exec_visibility`,
`EXEC_VISIBILITY_WAIT`, and `EXEC_VISIBILITY_POLL` (used cross-crate by
`crates/devflow-cli/tests/reap_strays_e2e.rs:106-111`). Unchanged this round. Fix: extend the
comment to name both hazards, matching `devflow-core/src/test_support.rs:1-31`'s own module
doc.

---

_Reviewed: 2026-07-28T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
