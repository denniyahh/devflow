# Phase 11-Remediation: Critical Bug Sprint

**Status:** Ready for Execution | **Priority:** BLOCKER — must merge before Phase 12
**Date:** 2026-06-20 | **Branch:** feature/phase-11 (same branch as Phase 11)

---

## Goal

Fix the 5 CRITICAL issues identified in the Phase 11 code review before merging
`feature/phase-11` into `develop`. This sprint does not add Phase 12 features,
change architecture, or touch warnings/info items.

---

## Scope

### In Scope — 5 Criticals Only

| ID    | File                                        | Exact Location          | Issue                                              |
|-------|---------------------------------------------|-------------------------|----------------------------------------------------|
| CR-02 | `crates/devflow-core/src/state.rs`          | Line 33 (`#[serde(skip)]`) | `consecutive_failures` not persisted → auto-gate dead |
| CR-05 | `crates/devflow-cli/src/main.rs`            | Lines 440–458 (`transition()`) | Secondary confirmation of CR-02; subsumed by CR-02 fix |
| CR-04 | `crates/devflow-cli/src/main.rs`            | Lines 513–517 (`run_gate()`) | `ack` before `save_state` → kill leaves `gate_pending:true` + no response file → 7-day poll block |
| CR-03 | `crates/devflow-cli/src/main.rs`            | Lines 276–306 (`start()`) | Branch created before divergence check; error leaves stale branch on disk |
| CR-01 | `crates/devflow-core/src/monitor.rs`        | Line 105 (shell script) | `2>/dev/null` discards agent stderr → failures become opaque |

### Out of Scope

- Warnings (WR-01 through WR-11) — deferred to Phase 12 or separate sprint
- Info items (IN-01 through IN-05) — low priority, deferred
- New features, Phase 12 bootstrap commands
- Architectural changes beyond the targeted fixes
- `save_state` atomicity (WR-07) — valuable but not blocking merge

---

## Implementation Decisions

### CR-02: Remove `#[serde(skip)]` from `consecutive_failures`

**File:** `crates/devflow-core/src/state.rs:33`

The field is currently:
```rust
#[serde(skip)]
pub consecutive_failures: u32,
```

**Decision:** Remove the `#[serde(skip)]` attribute. The field will be persisted
to `state.json` and loaded on every `devflow advance` invocation. The existing
`transition()` reset (`state.consecutive_failures = 0`) is the correct and only
reset path — no additional reset needed at load time.

**Impact on tests:** The test at `state.rs:181` (`consecutive_failures_is_runtime_only_not_persisted`)
asserts that the field is NOT in the serialized JSON and resets to 0. This test
must be inverted — it should now verify the field IS persisted and round-trips
correctly. The test name should also be renamed to reflect the new behavior.

**CR-05 is subsumed:** Once CR-02 is fixed, `transition()` zeroing
`consecutive_failures` on any stage advance is the correct behavior (reset the
failure counter when the pipeline successfully moves forward). No separate fix needed.

---

### CR-04: Swap `save_state` and `ack` ordering in `run_gate()`

**File:** `crates/devflow-cli/src/main.rs:513–517`

Current ordering (unsafe):
```rust
Gates::ack(project_root, state.phase, stage)?;   // line 515 — ack first
state.gate_pending = false;                        // line 516
workflow::save_state(state)?;                     // line 517 — save last
```

If killed between `ack` (line 515) and `save_state` (line 517), the persisted
state still has `gate_pending: true` but no response file exists (ack implies the
response was consumed). On restart, `poll_response` blocks for `GATE_TIMEOUT_SECS`
(7 days) with no response file ever appearing.

**Decision:** Persist `gate_pending = false` to disk BEFORE writing the ack:
```rust
state.gate_pending = false;
workflow::save_state(state)?;   // persist first
Gates::ack(project_root, state.phase, stage)?;   // then signal Hermes
```

This is safe because: if killed after `save_state` but before `ack`, Hermes
will not get the ack and may retry delivery — benign, since DevFlow's state
already reflects `gate_pending: false` and the pipeline can advance correctly
on restart.

---

### CR-03: Move divergence check before branch creation in `start()`

**File:** `crates/devflow-cli/src/main.rs:276–306`

Current ordering (unsafe):
1. Lines 264–274: worktree OR branch created
2. Lines 295–306: divergence check runs (on wrong branch — now on feature branch, not develop)

**Decision:** Move the divergence check to the top of `start()`, before any git
mutations. The check should run via `GitFlow::new(project_root).divergence_from_develop()`
while `HEAD` is still on `develop` (or wherever the user is before branching).

The check at line 296 calls `divergence_from_develop()` which computes
`git rev-list --count HEAD..develop`. After `feature_start`, HEAD is the new
feature branch which shares the same history as develop's tip, so `behind` will
be 0 and the check silently passes even when develop is ahead.

**Fix ordering:**
1. Check divergence (bail out if behind > 50, warn if > 10)
2. Create worktree or feature branch
3. Save state and launch monitor

No other logic changes needed. The error message is already correct
("Rebase onto develop first") — it just needs to fire before the branch exists.

---

### CR-01: Capture agent stderr to a separate file in `monitor.rs`

**File:** `crates/devflow-core/src/monitor.rs:105`

Current shell script fragment:
```sh
{agent_cmd} > {stdout_file} 2>/dev/null &
```

**Decision:** Redirect stderr to a separate file. Add `stderr_path()` to
`agent_result.rs` mirroring the existing `stdout_path()`. Change the script:
```sh
{agent_cmd} > {stdout_file} 2>{stderr_file} &
```

The stderr file is available for `devflow recover` and for `devflow advance` to
include in error messages when the agent exits non-zero. No behavior changes
beyond capturing the file — there is no automatic stderr-to-stdout fallback
(that would break the JSON stdout parsing contract).

**agent_result.rs addition:** Add `pub fn stderr_path(project_root: &Path, phase: u32) -> PathBuf`
alongside the existing `stdout_path()`. The path: `.devflow/phase-NN-stderr.log`.

---

## Constraints

- `cargo test` must pass with zero regressions after each fix
- `cargo clippy -- -D warnings` must be clean
- Each fix is one atomic commit on `feature/phase-11`
- No changes to non-targeted files unless required by the fix
- Test assertions that relied on the broken behavior (e.g., `consecutive_failures_is_runtime_only_not_persisted`) must be updated to match the fixed behavior

---

## Source File Reference

| File | Purpose |
|---|---|
| `crates/devflow-core/src/state.rs` | `State` struct — CR-02 target |
| `crates/devflow-core/src/monitor.rs` | Shell script generation — CR-01 target |
| `crates/devflow-core/src/agent_result.rs` | Stdout/stderr path helpers — CR-01 dependency |
| `crates/devflow-cli/src/main.rs` | `start()`, `run_gate()`, `transition()` — CR-03, CR-04 targets |
| `crates/devflow-core/src/gates.rs` | Gate file protocol — read-only for CR-04 (fix is in main.rs) |
