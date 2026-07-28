---
status: complete
phase: 25-end-to-end-dogfood-blockers
source: [25-VERIFICATION.md]
started: 2026-07-28T18:35:00Z
updated: 2026-07-28T18:52:00Z
---

> **Diagnosis note.** The `diagnose_issues` step's parallel debug agents were not spawned. Both
> gaps arrived pre-diagnosed to file:line precision by two independent agents this round
> (`gsd-code-reviewer` → `25-REVIEW.md` WR-05/WR-06, then `gsd-verifier` → `25-VERIFICATION.md`,
> which re-derived both from source rather than accepting the reviewer's word). Re-running
> discovery would re-derive known facts. The `artifacts`/`missing` fields below are populated
> from those findings.

## Current Test

number: 2
name: WR-06 — the two "fixed" reap call sites are not unwind-safe
expected: |
  Either the reap survives a future assertion panic, or the residual is explicitly accepted.
awaiting: none — session complete

## Tests

### 1. WR-05 — two `preflight.rs` tests still leak a real monitor wrapper
expected: Either both tests reap what they spawn, or the residual is explicitly accepted. Decide: fix `run_preflight_advance_gate_launches_agent_exactly_once` and `run_preflight_loopback_gate_launches_agent_exactly_once` in a follow-up plan (wire `reap_spawned_monitor(&state)` in after their existing `assert_eq!(launches, 1, ...)`), or waive `WINDOWS.md` item 1 with a stated reason.
result: issue
reported: "might as well spend another gap-closure round to clean up the tests"
severity: major

### 2. WR-06 — the two "fixed" reap call sites are not unwind-safe
expected: Either the reap survives a future assertion panic, or the residual (bounded to these two tests' own success-path invariants) is explicitly accepted. Decide: harden `pipeline_launch.rs`'s and `staleness.rs`'s reap calls into an RAII `Drop` guard bound before the panicking assertions that currently precede them (per `25-REVIEW.md`'s own sketch), or accept the current plain-trailing-statement form.
result: issue
reported: "might as well spend another gap-closure round to clean up the tests"
severity: major

## Summary

total: 2
passed: 0
issues: 2
pending: 0
skipped: 0
blocked: 0

## Gaps

- gap_id: G-25-1
  truth: "Every test that drives a real `launch_stage_inner` reaps the monitor wrapper that launch spawned — including the two `preflight.rs` tests that reach `monitor::spawn_monitor` via `run_preflight`'s internal `Advance`/`LoopBack` recursion."
  status: failed
  reason: "User reported: might as well spend another gap-closure round to clean up the tests. WR-05 — `run_preflight_advance_gate_launches_agent_exactly_once` (preflight.rs:1543) and `run_preflight_loopback_gate_launches_agent_exactly_once` (preflight.rs:1604) both reach a real `monitor::spawn_monitor` (via preflight.rs:941 Advance arm and :946 LoopBack arm) and neither calls `reap_spawned_monitor`. Independently confirmed by gsd-code-reviewer and gsd-verifier. Tracked as WINDOWS.md item 1."
  severity: major
  test: 1
  artifacts:
    - "crates/devflow-cli/src/preflight.rs::tests::run_preflight_advance_gate_launches_agent_exactly_once (:1543)"
    - "crates/devflow-cli/src/preflight.rs::tests::run_preflight_loopback_gate_launches_agent_exactly_once (:1604)"
    - "crates/devflow-cli/src/test_support.rs::reap_spawned_monitor (existing helper — reuse, do not re-derive)"
  missing:
    - "A `reap_spawned_monitor(&state)` call in each of the two tests, placed before their `TempDir` guard drops."
  root_cause: >
    25-16 enumerated launch-driving tests by searching for direct `launch_stage`/`launch_stage_inner`
    calls. These two reach `monitor::spawn_monitor` indirectly, through `run_preflight`'s internal
    resolution arms — `Advance` calls `launch_stage_inner` at `preflight.rs:941`, `LoopBack` calls
    `launch_stage` at `:946` — so a call-site search never surfaced them. The launch is NOT stubbed:
    there is no test-mode branch in `launch_stage_inner` or `spawn_monitor`, and both tests use the
    same `stub_agent_binary("claude")` + `prepend_path` construction as the sites 25-16 did fix.

- gap_id: G-25-2
  truth: "At every site that reaps a spawned monitor, the reap runs on EVERY exit path including paths on which a later assertion panics — 25-16's own stated must-have."
  status: failed
  reason: "User reported: might as well spend another gap-closure round to clean up the tests. WR-06 — at both sites 25-16 fixed (`pipeline_launch.rs::launch_stage_persists_monitor_pid_for_reload` reap at :440 behind 4 panicking checkpoints at :423/:425/:429/:430; `staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness` reap at :802 behind panicking checkpoints at :777/:792) the reap is a plain trailing statement, so a panic unwinds past it and drops the TempDir anyway. Fails precisely when a real regression fires. Independently confirmed by gsd-code-reviewer and gsd-verifier. Fix sketch (RAII Drop guard) in 25-REVIEW.md WR-06. Tracked as WINDOWS.md item 3."
  severity: major
  test: 2
  artifacts:
    - "crates/devflow-cli/src/pipeline_launch.rs::tests::launch_stage_persists_monitor_pid_for_reload (reap at :440; panicking checkpoints at :423 unwrap, :425 assert!, :429 unwrap, :430 assert_eq!)"
    - "crates/devflow-cli/src/staleness.rs::tests::mid_run_stage_transition_does_not_readjudicate_staleness (reap at :802; panicking checkpoints at :777 expect, :792 assert_eq!)"
    - "crates/devflow-cli/src/test_support.rs::reap_spawned_monitor (:322-336)"
  missing:
    - "An RAII `Drop` guard bound BEFORE the panicking assertions, so the reap runs during an unwind. Sketch in 25-REVIEW.md WR-06."
  root_cause: >
    Rust does not execute subsequent statements once a panic begins unwinding. A plain trailing
    `reap_spawned_monitor(&state)` therefore runs only on the success path, which is the one path
    where the leak is least consequential. `reap_spawned_monitor`'s own doc comment
    (test_support.rs:313-315) states it "must be called BEFORE the caller's TempDir guard drops" —
    a requirement a trailing statement structurally cannot honour on a panic path. The defect is in
    how the helper is CALLED, not in the helper, which is correct as written.
