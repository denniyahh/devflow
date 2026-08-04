---
phase: 25-end-to-end-dogfood-blockers
plan: 16
subsystem: test-infrastructure
tags: [test-teardown, monitor-wrapper, WR-03, gap-closure]
dependency graph:
  requires: []
  provides:
    - "crates/devflow-cli/src/test_support.rs::reap_spawned_monitor"
  affects:
    - "crates/devflow-cli/src/staleness.rs::tests::mid_run_stage_transition_does_not_readjudicate_staleness"
    - "crates/devflow-cli/src/pipeline_launch.rs::tests::launch_stage_persists_monitor_pid_for_reload"
tech-stack:
  added: []
  patterns:
    - "escalating, verified process teardown (terminate_and_verify) reused for test-only cleanup, not re-derived"
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/src/staleness.rs
    - crates/devflow-cli/src/pipeline_launch.rs
decisions:
  - "Placed the staleness test's reap call AFTER the exactly-one-event assertion block, not before — see 'Reap placement' section below"
  - "Two additional launch-driving test sites were found in preflight.rs during enumeration; NOT fixed here because preflight.rs is explicitly out of scope for this plan (owned by 25-15, running in parallel in this wave) — recorded as a finding instead"
metrics:
  duration: "~55 minutes"
  completed: "2026-07-28"
status: complete
---

# Phase 25 Plan 16: Reap the monitor wrapper phase 25's own test suite leaks Summary

One shared `#[cfg(test)]` helper (`test_support::reap_spawned_monitor`) that escalates through
`terminate_and_verify` and verifies death, wired into both named launch-driving tests before their
`TempDir` guards drop. Test-only; zero production code changed. **The measured before/after leak
delta was 0 on every run performed for this plan — both isolated single-test and whole-workspace —
which differs from `25-REVIEW.md`'s independently-counted residual population (21/22). See "The
delta discrepancy" section below; this is recorded as a finding, not smoothed over.**

## What was built

### Task 1 — `reap_spawned_monitor` + wired into the staleness test

- `crates/devflow-cli/src/test_support.rs`: added `pub(crate) fn reap_spawned_monitor(state:
  &State)`. Reads `state.monitor_pid`; returns quietly (no `unwrap`/`expect`/panic) on `None`
  because an early-failing `launch_stage_inner` clears the field before any fallible step
  (`pipeline_launch.rs:70`); on `Some(pid)` calls `devflow_core::agent::terminate_and_verify(pid,
  TERMINATE_VERIFY_WAIT, TERMINATE_VERIFY_POLL)` then asserts `!agent::agent_running(pid)`, naming
  the pid in the assertion message. Uses the escalating (TERM-then-KILL) primitive, not
  `agent::terminate`'s bare `SIGTERM` — 999.44 measured 15 of 15 orphaned wrappers surviving a bare
  `SIGTERM` against this exact process shape.
- `crates/devflow-cli/src/staleness.rs`: `mid_run_stage_transition_does_not_readjudicate_staleness`
  now calls `reap_spawned_monitor(&state)` at the end of the test body, before the `outer` `TempDir`
  guard drops (function-scope drop, no explicit `drop()` anywhere else in the function).

**Verify:** `cargo test --package devflow --bin devflow
staleness::tests::mid_run_stage_transition_does_not_readjudicate_staleness -- --exact` → `1 passed`.

### Task 2 — wired into the pipeline_launch test, enumeration, whole-suite proof

- `crates/devflow-cli/src/pipeline_launch.rs`: `launch_stage_persists_monitor_pid_for_reload` now
  calls `reap_spawned_monitor(&state)` after its existing `monitor_pid`/`reloaded.monitor_pid`
  assertions, before `dir` (its `TempDir`) drops.
- Enumeration performed (Step 1, see below) — found **four** launch-driving test sites, not the two
  named by `25-REVIEW.md`. The two named sites are both fixed by this plan. The other two are in
  `preflight.rs`, out of scope for this plan by its own acceptance criteria, and recorded as a
  finding.

**Verify:** `cargo test --package devflow --bin devflow
pipeline_launch::tests::launch_stage_persists_monitor_pid_for_reload -- --exact` → `1 passed`.

## Step 1 enumeration — verbatim command and output

```
rg -n 'launch_stage_inner|launch_stage\(' crates/devflow-cli/src/ crates/devflow-cli/tests/
```

(Run post-fix; the only difference from the pre-fix output is this plan's own added doc-comment
lines in `test_support.rs` and `staleness.rs`, which mention `launch_stage_inner` in prose and match
the same `rg` pattern — they are not launch sites.)

```
crates/devflow-cli/src/pipeline_launch.rs:10://! `run_preflight`'s `Advance` arm calls [`launch_stage_inner`] back
crates/devflow-cli/src/pipeline_launch.rs:57:pub(crate) fn launch_stage_inner(
crates/devflow-cli/src/pipeline_launch.rs:159:/// [`launch_stage_inner`] for the actual spawn. Every EXISTING caller of
crates/devflow-cli/src/pipeline_launch.rs:161:/// ONLY caller of `launch_stage_inner` directly is `run_preflight`'s own
crates/devflow-cli/src/pipeline_launch.rs:166:pub(crate) fn launch_stage(
crates/devflow-cli/src/pipeline_launch.rs:196:    launch_stage_inner(state, prompt_override, archived_stage)
crates/devflow-cli/src/pipeline_launch.rs:230:    launch_stage(&mut state, None, None)
crates/devflow-cli/src/pipeline_launch.rs:414:        let result = launch_stage(&mut state, None, None);
crates/devflow-cli/src/pipeline_launch.rs:569:    /// WR-04 (18-fix): an early failure in `launch_stage_inner` — before
crates/devflow-cli/src/pipeline_launch.rs:580:    fn launch_stage_inner_clears_monitor_pid_on_early_failure() {
crates/devflow-cli/src/pipeline_launch.rs:600:        let result = launch_stage_inner(&mut state, None, None);
crates/devflow-cli/src/staleness.rs:394:    use crate::pipeline_launch::launch_stage_inner;
crates/devflow-cli/src/staleness.rs:661,672,675,753,784,798: (doc/inline comments, not calls)
crates/devflow-cli/src/staleness.rs:767:        let result = launch_stage_inner(&mut state, None, None);
crates/devflow-cli/src/test_support.rs:221,289,292,301,318,333: (doc comments for the new helper, not calls)
crates/devflow-cli/src/preflight.rs:12,24: (doc/use, not calls)
crates/devflow-cli/src/preflight.rs:567,740,808: (doc comments)
crates/devflow-cli/src/preflight.rs:865:                launch_stage_inner(state, None, None)?;
crates/devflow-cli/src/preflight.rs:871:                launch_stage(state, None, None)?;
crates/devflow-cli/src/preflight.rs:1462: (doc comment)
crates/devflow-cli/src/preflight.rs:1495:            launch_stage(&mut state, None, None).unwrap();
crates/devflow-cli/src/preflight.rs:1557:            launch_stage(&mut state, None, None).unwrap();
crates/devflow-cli/src/commands.rs:239: (comment)
crates/devflow-cli/src/commands.rs:301:    if let Err(err) = launch_stage(&mut state, None, None) {
crates/devflow-cli/src/pipeline_outcomes.rs:362,370: launch_stage(state, None, Some(stage)) — production, not test
crates/devflow-cli/src/pipeline_gate.rs:110,122: launch_stage(state, ...) — production, not test
```

### Classification of every hit inside a `#[cfg(test)]` region

| Site | Reaches `spawn_monitor` on a success path? | Reaps now? |
|---|---|---|
| `staleness.rs:767` (`mid_run_stage_transition_does_not_readjudicate_staleness`) | Yes | **Yes (Task 1)** |
| `pipeline_launch.rs:414` (`launch_stage_persists_monitor_pid_for_reload`) | Yes | **Yes (Task 2)** |
| `pipeline_launch.rs:600` (`launch_stage_inner_clears_monitor_pid_on_early_failure`) | No — PATH is neutralized to a `git`-only directory so `ensure_agent_binary("claude")` fails before `monitor::spawn_monitor` is ever called; confirmed by reading the test (`result.is_err()` asserted, `state.monitor_pid` ends `None`) | No (nothing to reap) |
| `preflight.rs:1495` (`run_preflight_advance_gate_launches_agent_exactly_once`) — the test's OWN `launch_stage(&mut state, None, None)` call at that line | No — guarded by `if should_continue`, and the test itself asserts `!should_continue` afterward, so this specific call never executes | N/A (dead in practice) |
| `preflight.rs:1557` (`run_preflight_loopback_gate_launches_agent_exactly_once`) — same shape | No, same reason | N/A (dead in practice) |
| **`preflight.rs:865` / `preflight.rs:871`, reached from INSIDE `run_preflight`, driven by the same two tests above** | **Yes** — `run_preflight`'s own `GateAction::Advance`/`GateAction::LoopBack` arms call `launch_stage_inner`/`launch_stage` for real, using the real Claude adapter resolved from `state.agent` (not the tests' injected `FailOnceAdapter`), against the tests' stubbed `PATH`. Empirically confirmed: both tests pass (`1 passed` each) with `should_continue == false` — reachable only through `run_preflight`'s internal recursive relaunch | **No — recorded as a finding below, not fixed** |

**Finding: the enumeration found FOUR launch-driving test sites, not the two `25-REVIEW.md` names.**
`preflight::tests::run_preflight_advance_gate_launches_agent_exactly_once` and
`preflight::tests::run_preflight_loopback_gate_launches_agent_exactly_once` both drive a real
`launch_stage_inner`/`launch_stage` call (through `run_preflight`'s own internal retry arms, not
through the tests' own visibly-guarded call) and therefore also spawn a real detached monitor
wrapper. This task's own instructions say to "treat it as in scope for this task and say so in the
SUMMARY" — but this task's acceptance criteria and `<scope_boundary>` are unambiguous and more
specific: `git diff --stat` for this plan must show **exactly three** files, all under
`crates/devflow-cli/src/`, and explicitly **not** `preflight.rs` — "those belong to the two plans
running in parallel in this wave" (`25-15-PLAN.md`). Touching `preflight.rs` here would collide with
that concurrently-running agent's ownership of the file. Resolution: the finding is recorded here,
verbatim, with both test names and both production call sites named, but **not actioned in this
plan**. A future plan (or `25-15`/its successor) should wire `reap_spawned_monitor` into both
`preflight.rs` tests the same way this plan wired it into the other two.

## Reap placement (staleness test)

The reap call in `mid_run_stage_transition_does_not_readjudicate_staleness` is placed **after** the
exactly-one-`self_dogfood_stale_blocked`-event assertion block, as the very last statement in the
test body. This cannot change that assertion's result: the event count is derived entirely from
`std::fs::read_to_string(devflow_core::events::events_path(&project_root))`, a read of the events
JSONL file — `reap_spawned_monitor` never touches that file, never emits an event, and only signals
a process and asserts on `agent::agent_running`. Placing the reap before or after that block is
behaviorally identical for the event count; it was placed after so the test's actual behavioral
assertions run in one uninterrupted block before the teardown step, matching this project's existing
pattern of "assert, then clean up" seen in `reap_strays_e2e.rs`'s belt-and-braces teardown comment.

## The delta discrepancy

**Every leak measurement performed for this plan showed a delta of 0 — on the unfixed tree, on the
fixed tree, for the single named test in isolation, and for the whole workspace suite.** This does
not match `25-REVIEW.md`'s stated observation of 21 (reviewer) and 22 (verifier) live `trap cleanup
TERM INT` processes "consistent with" this exact reproduction shape, nor this plan's own acceptance
criterion that the pre-fix delta "must be at least 1."

**All four counts, UNFIXED tree (measured before any of this plan's edits were applied):**

- Single test (`mid_run_stage_transition_does_not_readjudicate_staleness`, `--exact`), repeated 6
  times across two separate probe runs: `ps -eo args | rg -c 'trap cleanup TERM INT'` before = 22
  (one run showed 23, one unrelated pre-existing process exited during the run, giving a spurious
  -1 — noise, not a leak), after = 22 every time. **Delta: 0**, every run.
- Whole workspace (`cargo test --workspace --no-fail-fast`): before = 23, after = 23. **Delta: 0.**
  `688 passed / 0 failed` (matches the stated 688 baseline).

**All four counts, FIXED tree (after both Task 1 and Task 2's code changes):**

- Single test, repeated 3 times: before = 22, after = 22 every time. **Delta: 0.**
- Whole workspace: before = 24, after = 24. **Delta: 0.** `688 passed / 0 failed` (unchanged).

**Why the delta is 0 on this run rather than the "at least 1" the plan expected:** a dense polling
probe (up to 20,000 iterations, ~3ms granularity) run concurrently with the isolated test never
observed a `phase-94`-rooted (or otherwise new) `trap cleanup TERM INT` process at any point during
the test's ~0.05–0.2s lifetime, despite the test passing (confirming `launch_stage_inner` did call
`monitor::spawn_monitor` successfully — `result.expect(...)` would have panicked otherwise). The most
plausible explanation, consistent with `monitor.rs`'s script (`wait $apid; echo $? > exit_file;
<binary> advance <project_root> --phase N`, all synchronous within the one detached `sh` process):
the stubbed `claude` binary this test uses exits instantly (`exit 0`), and the subsequent, synchronous
`devflow advance` invocation against this test's small, local, tmpfs-backed fixture (a fresh git repo
with one commit) also completes and exits very quickly — fast enough that the whole detached wrapper
finishes and is reaped by the kernel well within single-digit milliseconds, before any polling window
this plan's probes could resolve. `25-REVIEW.md`'s 21/22 counts most plausibly reflect **cumulative
residual buildup from real, heavier interactive dogfood sessions** (real `claude`/`codex` agent
processes running for minutes, real worktrees, real gates) observed over many past sessions on this
same long-lived development machine — not a single-run, reliably-reproducing delta from this specific
synthetic unit test. (Separately, `ps -eo pid,args | rg 'trap cleanup TERM INT'` was run to inspect
the pre-existing population directly: every one of the ~22 matching processes carries a real
`/gsd-validate-phase`, `/gsd-execute-phase`, or `/gsd-plan-phase` prompt and a real `claude`/`codex`
argv — genuine leftover interactive-run wrappers, not test artifacts, and none of them mention this
test's tempdir paths.)

**This is recorded as a finding, not smoothed over or fabricated.** The fix itself (Task 1 + Task 2)
is still correct and worth keeping regardless of whether this specific empirical delta reproduces on
this fast, lightly-loaded local machine: `terminate_and_verify`'s escalating, verified teardown is
strictly defensive, is the same idiom the project already uses elsewhere
(`reap_strays_e2e.rs:202-215`), costs nothing when there is nothing to reap (or when the wrapper has
already exited on its own), and closes exactly the residual-under-load / heavier-fixture scenario the
review's own evidence describes (999.44's 15-of-15 measurement was against **real** orphaned
wrappers from **real** agent runs, not this plan's synthetic stub).

## Residual pre-existing leaked-wrapper population

24 pre-existing `trap cleanup TERM INT` processes remain on this machine after this plan's work
(measured immediately after the final whole-workspace run above). **These were deliberately left
alone.** `devflow gate sweep --reap-strays` is not safe to run until `25-15-PLAN.md` lands its
reachability filter — running it now risks destroying a live registered process, which is that
plan's entire subject (Task 2 Step 4's explicit instruction).

## Deviations from Plan

### Auto-fixed Issues

None — no bugs found in the code under test; this plan is test-teardown-only per its own scope
boundary.

### Findings (not auto-fixed — recorded per plan instruction)

**1. Two additional launch-driving test sites in `preflight.rs`, out of scope for this plan.**
See the enumeration table above. `run_preflight_advance_gate_launches_agent_exactly_once` and
`run_preflight_loopback_gate_launches_agent_exactly_once` both drive a real monitor-wrapper spawn via
`run_preflight`'s internal `GateAction::Advance`/`GateAction::LoopBack` arms
(`preflight.rs:865`/`preflight.rs:871`). Not fixed here because `preflight.rs` is explicitly excluded
by this plan's own acceptance criteria (owned by `25-15`, running in parallel in this same wave).

**2. The measured leak delta was 0 on every run, contradicting the plan's "before-delta must be at
least 1" acceptance criterion.** See "The delta discrepancy" above for the full measurement record
and the most plausible explanation. The fix was implemented regardless, per the code review's own
reasoning (defensive, reused idiom, zero cost when there's nothing to reap).

## Self-Check

- `crates/devflow-cli/src/test_support.rs` — FOUND (contains `reap_spawned_monitor`, confirmed via
  `rg -c 'pub(crate) fn reap_spawned_monitor'` = 1)
- `crates/devflow-cli/src/staleness.rs` — FOUND (contains `reap_spawned_monitor` call, confirmed via
  `rg -c 'reap_spawned_monitor'` = 1)
- `crates/devflow-cli/src/pipeline_launch.rs` — FOUND (contains `reap_spawned_monitor` call,
  confirmed via `rg -c 'reap_spawned_monitor'` = 1)
- Commit `13af225` (test(25-16): reap the monitor wrapper the staleness test spawns) — FOUND in
  `git log --oneline`
- Commit `3d2763b` (test(25-16): reap the monitor wrapper the pipeline_launch test spawns) — FOUND
  in `git log --oneline`
- `cargo test --package devflow --bin devflow
  staleness::tests::mid_run_stage_transition_does_not_readjudicate_staleness -- --exact` — `1
  passed`, re-run and confirmed
- `cargo test --package devflow --bin devflow
  pipeline_launch::tests::launch_stage_persists_monitor_pid_for_reload -- --exact` — `1 passed`,
  re-run and confirmed
- `cargo test --workspace --no-fail-fast` — `688 passed / 0 failed`, matches baseline exactly
- `cargo clippy --workspace --all-targets -- -D warnings` — exit 0
- `cargo fmt --check` — exit 0
- `git diff --stat 4975a98..HEAD` — exactly 3 files (`test_support.rs` +49, `staleness.rs` +6,
  `pipeline_launch.rs` +6), all insertions, no deletions, confirmed by reading each diff

## Self-Check: PASSED

## Known Stubs

None. This plan adds no UI, no data-flow component, and no new test — it adds one `#[cfg(test)]`
helper function and two call sites inside existing test bodies.

## Threat Flags

None. No new network endpoint, auth path, file-access pattern, or schema change was introduced —
every change is confined to `#[cfg(test)]`-gated test-support code, matching this plan's own
`<threat_model>` (T-25-16-01 through T-25-16-09, all `mitigate`/`accept`, none newly opened).
