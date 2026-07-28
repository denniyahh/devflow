---
status: testing
phase: 25-end-to-end-dogfood-blockers
source: [25-VERIFICATION.md]
started: 2026-07-28T18:35:00Z
updated: 2026-07-28T18:35:00Z
---

## Current Test

number: 1
name: WR-05 — two `preflight.rs` tests still leak a real monitor wrapper
expected: |
  Either both tests reap what they spawn, or the residual is explicitly accepted.
  Decide: fix `run_preflight_advance_gate_launches_agent_exactly_once` and
  `run_preflight_loopback_gate_launches_agent_exactly_once` in a follow-up plan (wire
  `reap_spawned_monitor(&state)` in after their existing `assert_eq!(launches, 1, ...)`),
  or waive `WINDOWS.md` item 1 with a stated reason.
awaiting: user response

## Tests

### 1. WR-05 — two `preflight.rs` tests still leak a real monitor wrapper
expected: Either both tests reap what they spawn, or the residual is explicitly accepted. Decide: fix `run_preflight_advance_gate_launches_agent_exactly_once` and `run_preflight_loopback_gate_launches_agent_exactly_once` in a follow-up plan (wire `reap_spawned_monitor(&state)` in after their existing `assert_eq!(launches, 1, ...)`), or waive `WINDOWS.md` item 1 with a stated reason.
result: [pending]

### 2. WR-06 — the two "fixed" reap call sites are not unwind-safe
expected: Either the reap survives a future assertion panic, or the residual (bounded to these two tests' own success-path invariants) is explicitly accepted. Decide: harden `pipeline_launch.rs`'s and `staleness.rs`'s reap calls into an RAII `Drop` guard bound before the panicking assertions that currently precede them (per `25-REVIEW.md`'s own sketch), or accept the current plain-trailing-statement form.
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
