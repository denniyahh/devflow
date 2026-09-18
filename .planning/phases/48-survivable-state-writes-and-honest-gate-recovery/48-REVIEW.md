---
phase: 48-survivable-state-writes-and-honest-gate-recovery
reviewed: 2026-09-18T13:34:06Z
depth: deep
files_reviewed: 25
files_reviewed_list:
  - clippy.toml
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-cli/src/test_support.rs
  - crates/devflow-cli/tests/gate_wedge_e2e.rs
  - crates/devflow-cli/tests/plan_bashism_scanner.rs
  - crates/devflow-cli/tests/start_lock_e2e.rs
  - crates/devflow-cli/tests/stop_e2e.rs
  - crates/devflow-core/src/agents/opencode.rs
  - crates/devflow-core/src/agents/pi.rs
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/lock.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/recover.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/test_support.rs
  - crates/devflow-core/src/verify.rs
  - crates/devflow-core/src/workflow.rs
  - scripts/lint-plan-bashisms.sh
findings:
  critical: 1
  warning: 2
  info: 0
  total: 3
status: issues_found
---

# Phase 48: Code Review Report

**Reviewed:** 2026-09-18T13:34:06Z
**Depth:** deep
**Files Reviewed:** 25
**Status:** issues_found

## Summary

The phase's new lock-held entry points, response publication, recovery paths, monitor-to-advance stage binding, and child-test isolation were traced across their callers. The per-phase lock can lose exclusivity during publication, which defeats the single-writer guarantee. The sweep dry-run is not behaviorally faithful for no-waiter non-Ship gates, and the OpenCode timeout fixtures leak their `sleep` descendants while claiming the hung child was killed.

`git diff --check` was clean for the reviewed diff. That whitespace check does not establish runtime correctness, concurrency safety, or test reliability.

## Narrative Findings (AI reviewer)

The findings below are independently substantiated against the current source and the Phase 48 plans/summaries. No structural-fallow findings were provided.

## Critical Issues

### CR-01: Lock acquisition can evict a live holder before it records its PID

**File:** `crates/devflow-core/src/lock.rs:142-166`

**Issue:** `File::create_new` makes the lock path visible before the successful acquirer writes its PID/start-time record. A concurrent `acquire` that reaches the `AlreadyExists` arm in that interval reads an empty file as `"unknown"`; `pid_is_alive("unknown")` is false, so it unlinks the just-created lock and successfully creates a second one. The first caller then writes to its now-unlinked file and both callers proceed with `LockGuard`s for the same phase. This directly breaks the Phase 48 single-writer invariant for `start`, `resume`, `advance`, `ship`, and destructive recovery; either guard can also unlink the other holder's pathname on drop.

**Fix:** Publish a fully populated lock record atomically. For example, create a unique sibling temp, write (and sync if crash durability is required), then hard-link that completed inode to the lock path; `AlreadyExists` must be treated as contended until a complete record is safely classified stale. Do not delete unreadable/empty records in the acquisition path. Add a synchronization-controlled regression test that holds the first writer between temp creation and publication and proves a second acquirer cannot obtain the same phase lock.

## Warnings

### WR-01: Gate-sweep dry run reports a reap that the real command refuses

**File:** `crates/devflow-cli/src/commands.rs:1548-1569`

**Issue:** The `dry_run` branch increments `reaped` and prints `would reap` before inspecting the phase holder. The real path then detects a no-waiter non-Ship gate and leaves it alone with the recovery command. Consequently, `devflow gate sweep --dry-run` can tell an operator it would publish an abort response, while the corresponding non-dry-run invocation will not write anything. This is a misleading safety preview exactly for the new no-waiter recovery rule.

**Fix:** Compute `holder_status` and apply the no-waiter/non-Ship decision before branching on `dry_run`; have dry-run print the same `left alone` decision and count. Add a control for an aged non-Ship gate with no lock that asserts dry-run neither says `would reap` nor increments the would-reap count.

### WR-02: OpenCode timeout tests leave the stub's `sleep` process running

**File:** `crates/devflow-core/src/agents/opencode.rs:147-153, 401-415, 612-647`

**Issue:** The hanging stub is `/bin/sh` running `sleep` followed by `echo`, so the shell remains the spawned child while `sleep` is its separate descendant. On timeout, `spawn_with_timeout` calls `kill` only on the shell PID and waits for that PID; it neither terminates nor reaps the `sleep` process. The new isolated-path fixtures make this path run for both the direct timeout test and the child-isolated health test, leaking 10- and 60-second descendants into concurrent test runs. The test therefore demonstrates only that the parent shell exits within the deadline, not that the hung work is killed.

**Fix:** For the fixture, make the script `exec sleep ...` so the killed PID is the sleeping process, and assert the recorded PID is no longer alive after the timeout. If the production contract is intended to bound all probe work, spawn probes in a separate process group and terminate/reap that group rather than only the immediate child.

---

_Reviewed: 2026-09-18T13:34:06Z_
_Reviewer: Codex (gsd-code-reviewer)_
_Depth: deep_
