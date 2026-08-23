# Phase 11-Remediation Summary: Critical Bug Sprint

> Completed: 2026-06-20 | Agent: Claude | Version: v1.2.0 (same branch as Phase 11, pre-merge)
> **Retroactively documented 2026-07-11** — this SUMMARY.md was reconstructed
> from `11r-PLAN.md`, `11r-VALIDATION.md`, `11r-CONTEXT.md`, and
> `11r-DISCUSSION-LOG.md`; no SUMMARY.md was written when the phase shipped.

## Accomplished

Delivered on branch `feature/phase-11` — a continuation of Phase 11, not a
separate branch (`11r-PLAN.md`: "Branch: `feature/phase-11` (continue on
same branch)"). Four commits, one per critical finding from
`11-REVIEW.md`, each verified in `11r-VALIDATION.md`'s "Commits Verified"
table and "Per-Task Verification" table:

### 11r-A — Fix CR-02 (+ CR-05, subsumed): persist `consecutive_failures` (`c90e2fc`)
- [x] Removed `#[serde(skip)]` from `State::consecutive_failures` in
      `crates/devflow-core/src/state.rs`; field now persists across
      `devflow advance` invocations, so the Auto-mode 3-consecutive-failure
      forced-gate threshold actually functions instead of always reading 0
- [x] Test renamed from `consecutive_failures_is_runtime_only_not_persisted`
      to `consecutive_failures_persists_across_advance_calls`, now asserting
      the field round-trips through serde rather than being absent from the
      serialized JSON
- [x] CR-05 (unconditional reset in `transition()`) required no separate
      fix — `11r-DISCUSSION-LOG.md` Decision 2 confirms the reset is correct
      behavior once CR-02 is fixed, and is fully subsumed by it (one commit
      covers both)

### 11r-B — Fix CR-04: persist state before gate ack (`fa8c8fe`)
- [x] In `run_gate()` (`crates/devflow-cli/src/main.rs`), swapped the
      ordering so `state.gate_pending = false` + `workflow::save_state()`
      run *before* `Gates::ack()`. Closes the window where a process kill
      between the old ack-then-save steps left `gate_pending: true` on disk
      with no response file present — which would otherwise block the next
      `devflow advance` for the full `GATE_TIMEOUT_SECS` (7 days)

### 11r-C — Fix CR-03: divergence check before branch mutation (`5094d4c`)
- [x] Moved the develop-divergence check in `start()` to run before
      worktree/branch creation. A `behind > 50` bail-out no longer leaves a
      stale feature branch on disk, and the check now measures divergence
      against develop's actual tip rather than the just-created feature
      branch's self-relative (always-zero) divergence

### 11r-D — Fix CR-01: capture agent stderr to file (`93bc3d0`)
- [x] Added `stderr_path()` to `crates/devflow-core/src/agent_result.rs`,
      parallel to the existing `stdout_path()`
- [x] Changed the monitor's generated shell script in
      `crates/devflow-core/src/monitor.rs` from `2>/dev/null` to
      `2>{stderr_file}`, so agent failures are diagnosable via
      `.devflow/phase-NN-stderr.log` instead of silently discarded

## Scope Discipline

Per `11r-CONTEXT.md` and `11r-DISCUSSION-LOG.md` Decision 1, this sprint
deliberately fixed only the 5 criticals (CR-01–CR-05, with CR-05 subsumed
into the CR-02 commit). All 11 warnings and 5 info items from
`11-REVIEW.md` were explicitly out of scope, including WR-07 (non-atomic
`save_state`) despite being flagged in `11r-DISCUSSION-LOG.md` as "the most
serious warning" — deferred to Phase 12 rather than folded in here, to keep
the remediation diff small and reviewable.

## Deviations from CONTEXT.md

- `11r-CONTEXT.md`/`11r-PLAN.md` specified removing `#[serde(skip)]` from
  `consecutive_failures` with no replacement attribute. The shipped code
  (confirmed in the current `state.rs`) instead uses `#[serde(default)]`.
  This is a more defensive persisted-field pattern (older on-disk
  `state.json` files written before this fix, which lack the field, would
  deserialize with a default of `0` rather than failing to parse) — not a
  functional deviation from the "must persist" requirement, but not
  literally what the plan's code snippet showed either.
- No other deviations found — all 4 shipped commits map 1:1 to the plan's 4
  tasks, in the plan's specified execution order (CR-02 → CR-04 → CR-03 →
  CR-01).

## Verification (retroactive, 2026-07-11)

- Confirmed `crates/devflow-core/src/state.rs`: `consecutive_failures`
  carries `#[serde(default)]` (no `#[serde(skip)]`), its doc comment states
  it is "Persisted across `devflow advance` invocations," and the test
  `consecutive_failures_persists_across_advance_calls` exists and asserts
  round-trip behavior exactly as `11r-PLAN.md` specified.
- Confirmed `crates/devflow-cli/src/main.rs`'s `start()`: the divergence
  check (`GitFlow::new(project_root).divergence_from_develop()`) runs
  immediately after `State::new()`/dry-run handling and before both the
  worktree-creation branch and the `git.feature_start(...)`/
  `feature_start_force(...)` branch.
- Confirmed `crates/devflow-cli/src/main.rs`'s `run_gate()`: the success arm
  of `Gates::poll_response(...)` sets `state.gate_pending = false`, calls
  `workflow::save_state(state)?`, and only then calls
  `Gates::ack(project_root, state.phase, stage)?` — matching the CR-04 fix
  ordering exactly.
- Confirmed `crates/devflow-core/src/monitor.rs`: the generated script
  redirects the agent's stderr to a `stderr_file` path (via
  `crate::agent_result::stderr_path`) rather than `/dev/null`, and
  `crates/devflow-core/src/agent_result.rs` defines `stderr_path()`
  alongside `stdout_path()`.
- Did not independently re-run `cargo test`/`cargo clippy` in this session;
  relied on `11r-VALIDATION.md`'s recorded results (142 lib + 4 CLI = 146
  passing, 1 pre-existing flake unrelated to remediation, clippy clean)
  rather than re-executing the suite.
- Could not run `git log`/`git show` in this session (no shell tool
  available) to independently confirm the four commit hashes' authorship,
  dates, or diffs beyond what `11r-VALIDATION.md`'s "Commits Verified" table
  already states; treated that table as the source of truth for
  hash-to-fix attribution, cross-checked against each task's description in
  `11r-PLAN.md` and against the current source tree's behavior.
