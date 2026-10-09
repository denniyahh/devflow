---
phase: 49-live-unattended-run-the-milestone-s-instrument
reviewed: 2026-10-09T18:25:54Z
depth: deep
files_reviewed: 8
files_reviewed_list:
  - crates/devflow-core/src/gates.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
  - OPERATIONS.md
  - docs/guides/configuration.md
findings:
  critical: 0
  warning: 3
  info: 2
  total: 5
status: issues_found
---

# Phase 49: Code Review Report (final incremental re-review of b3f7fdf)

**Reviewed:** 2026-10-09T18:25:54Z
**Depth:** deep
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Scope: `git diff 23c0449 HEAD` over the eight listed files. The only code commit in that range is b3f7fdf. I traced the call chains into the following:

- `run_monitor` → `advance_for_stage` → `advance_with`, the chain's in-process advance.
- `refuse_resume_over_a_live_run` and the phase lock that `resume` takes.
- `finish_workflow_with_gate_timeout`'s retry loop.
- `staleness.rs::enforce_build_staleness`, the second gate-fire site.
- `workflow::load_state` and `save_state`.
- The rate-limit cron command in `ship.rs`.

The design is taken as decided. The state file is the one source of the command. `start`, `resume` and `advance` apply the variable to it. The chain never carries the variable. Each gate re-reads the field before its resolution save.

New IDs continue from the prior highest: WR-11 to WR-13, IN-10 and IN-11. No ID is reused. There are no new Critical findings.

**Do the fixes close their findings?**

| Prior finding | Verdict | Basis |
|---|---|---|
| CR-03: the variable set at `resume`/`advance` never reaches the chain | **Closed in code. One arm has no test (WR-13).** | `resume` (pipeline_launch.rs:1431) and `advance_with` (:1658) apply `apply_notify_env` straight after `load_state`, under the lock. `resume` saves at :1511, or at :1465 on a handoff, before `launch_stage` spawns anything. `start` (commands.rs:346) still starts from a fresh state, so its behaviour is unchanged. Each chain `advance` runs in a `__monitor` or `sh` process, and 9e78c83 strips the variable from both, so the chain keeps the persisted value. Gaps: the unset-keeps arm of `resume` is pinned by no test (WR-13), and the docs' route for a *running* phase does not work (WR-11). |
| WR-10: an edit made while a gate is parked is reverted when the gate is answered | **Closed for the wait window. Not closed for the other windows (WR-12).** | pipeline_gate.rs:435-437 re-reads the field before the resolution save. Mutation control: deleting that assignment fails `pipeline_gate::tests::gate_resolution_keeps_a_notify_cmd_edited_during_the_wait` at :1852. The test edits the file only after the gate file exists. The entry save (:370) comes before `write_gate` (:371), so the test cannot race the entry save. |
| IN-09: the e2e leaks the detached chain when it fails | **Closed** | `MarkedReaper` is created right after the `approve` assertion, not before it as the prior fix said. That is equivalent, because no detached chain exists until `approve` succeeds; before that, the foreground `start` is reaped by `ReapOnDrop`. `resume_with` creates its reaper before it spawns. After all the runs below, including two failing mutant runs, a scan of every readable `/proc/*/environ` found no process carrying `DEVFLOW_NOTIFY_E2E_MARKER` and no `sleep` processes. |

**Test runs.** All runs used a `git archive HEAD` snapshot in scratch (`diff -rq crates` against the worktree came back empty) with `DEVFLOW_GATE_NOTIFY_CMD` unset.

- Baseline:
  - `cargo test -p devflow --test gate_notify_persist_e2e`: 9 passed.
  - The WR-10 unit test: 1 passed.
  - `persisted_notify_cmd_follows_the_environment_rule`: 1 passed.
- Mutants:
  - Resume mutant (pipeline_launch.rs:1431 → `apply_notify_env(None)`): **all 9 e2e and 19 resume-filtered `--bin devflow` tests still pass**.
  - Control, the same mutation at :1658 (advance): fails `advance_gate_runs_the_persisted_command_when_the_variable_is_unset` at e2e:303.
  - Control, the WR-10 assignment removed: fails the WR-10 unit test.

**What these checks do not establish:**

- No live chain gate was fired. WR-11 and WR-12 come from reading the code, at the lines cited.
- The leak scan was done right after the runs and is a single sample. Not finding a leaked process is weak evidence that the reaper works. It does not show that the reaper's own race (it lists `/proc` and then kills) cannot leak.

## Warnings

### WR-11: OPERATIONS.md says `devflow resume` changes a running phase's hook, but `resume` refuses every live run

**File:** `OPERATIONS.md:108-110`; `crates/devflow-cli/src/pipeline_launch.rs:1418-1427`; `crates/devflow-cli/src/commands.rs:2470-2491`

**Issue:** The new text reads: "To change the hook of a running phase, export the new value and run `devflow resume`". For a running phase, `resume` always fails:

- **Parked at a gate.** The parked process holds the per-phase lock for the whole wait (`advance_with` keeps `_lock` across `run_gate_with_timeout`). `resume` uses the non-blocking `lock::acquire`, so it gets `Contended` and fails with "another devflow process (pid N) is already running".
- **Agent running.** `refuse_resume_over_a_live_run` refuses whenever the recorded monitor or agent pid is alive, and prints "refusing to resume; nothing was written".

The `resume` route works only once nothing is alive: after a gate timeout, a stop, or a rate-limit pause. The refusal is loud, so nothing is lost silently. But the documented primary route fails every time it is used as described. The only advice the refusal offers for getting past it, `devflow stop`, ends the run. For a live run, the only working route is the state-file edit, and WR-12 limits that too.

**Fix:** Scope the sentence to what the code does:

```markdown
To change the hook of a phase with no live process (after a gate timeout,
`devflow stop`, or a rate-limit pause), export the new value (or set it
empty) and run `devflow resume`. While a run is live, `resume` refuses;
edit `gate_notify_cmd` in the state file instead — an edit made while a gate
is waiting is kept.
```

### WR-12: The gate's entry save still reverts an on-disk edit, and the gate fires the copy loaded when `advance` started

**File:** `crates/devflow-cli/src/pipeline_gate.rs:368-370` (entry save), `:400-406` (fire), `:233-266` (the retry loop in `finish_workflow_with_gate_timeout`); `crates/devflow-cli/src/pipeline_launch.rs:1654` (load), `:1725` (intermediate save); claims in `crates/devflow-core/src/state.rs:434-435` and `crates/devflow-core/src/monitor.rs:285-288`

**Issue:** b3f7fdf re-reads the field only at the resolution save. Every other `save_state(state)` in a chain process writes the copy that `advance_with` loaded at :1654. That includes the gate's own entry save at :370 (`state.gate_pending = true; save_state(state)`). The notify then fires from that same copy (:404).

An edit made between the process's `load_state` and the entry save is therefore overwritten, and the gate fires the old command. The resolution re-read then finds the overwritten value and keeps it. Nothing reports that the edit was lost.

In the common Code-gate path, the window is short: `evaluate_agent_result`, the session-id save at :1725, then the gate. The longest window is the Ship finalization retry:

1. The operator answers the "[finalization failed]" gate.
2. `run_checkout_hooks` re-runs Merge, VersionBump, ChangelogAppend and BranchCleanup. This includes waiting up to `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` (120s) for the checkout lock.
3. The next retry gate's entry save clobbers any edit made during step 2.

Two docs overstate the behaviour:

- monitor.rs:285-288 says each chain gate "resolves the command from the run's state file at the moment it fires".
- state.rs:434-435 says the chain "reads this field at each gate".

Both are false for edits made in this window. OPERATIONS.md:110 promises only "an edit made while a gate is waiting is kept", so the user-facing doc is accurate. The fix covers one instance and leaves the class open.

**Fix:** Make the file authoritative at gate entry too. A plain entry re-read would discard the variable that a foreground `advance` applied in memory but has not saved yet. So persist the applied value first, then re-read the raw field at entry exactly as at resolution:

```rust
// pipeline_launch.rs, advance_with — after the stage-binding match (so a
// mismatch still writes nothing), persist what apply_notify_env produced:
workflow::save_state(&state)?;

// pipeline_gate.rs, run_gate_with_timeout — before the entry save and the fire:
if let Ok(on_disk) = workflow::load_state(project_root, state.phase) {
    state.gate_notify_cmd = on_disk.gate_notify_cmd;
}
state.gate_pending = true;
workflow::save_state(state)?;
```

Extend the WR-10 test with a second arm. Seed the file with `A` and keep `A` in memory, write `B` to disk before calling `run_gate_with_timeout`, and assert that the fired command (sentinel) and the saved field are both `B`. Also correct the monitor.rs and state.rs doc comments to say "the value on disk when the gate is entered and when it is resolved".

### WR-13: Nothing fails if `resume` clears the saved command when the variable is unset, and that is the rate-limit cron path

**File:** `crates/devflow-cli/src/pipeline_launch.rs:1431`; `crates/devflow-cli/tests/gate_notify_persist_e2e.rs:367-423`

**Issue:** The two new `resume_*` e2e tests cover set-replaces and empty-clears. The `resume_with` helper cannot express unset-keeps. It polls until the saved value differs from `"old"`, and panics if `resume` exits first. So the arm most often taken in production has no e2e test. That arm is the Hermes cron `cd {project} && devflow resume --phase N` (ship.rs:299), run after a rate limit from a scheduler environment that usually lacks the variable.

Mutation-verified: replacing :1431 with `apply_notify_env(None)` (drop the persisted value) leaves all 9 e2e tests and all 19 `resume`-filtered `--bin devflow` unit tests passing. Under that mutant, every rate-limit resume silently disables the chain's notifications. That is CR-03's original failure, reached through a different door. The same mutation at the `advance` site (:1658) is caught by `advance_gate_runs_the_persisted_command_when_the_variable_is_unset`. The gap is therefore specific to `resume`. The unit test of `persisted_notify_cmd_after_env` does not cover it, because the mutation is at the call site. The prior CR-03 fix text asked for this exact control ("the same run with the variable unset must leave the field").

**Fix:** Add an unset arm that waits for `resume` to finish its relaunch rather than for a change:

```rust
#[test]
fn resume_without_the_variable_keeps_the_persisted_command() {
    // same fixture as resume_with, NotifyEnv::Unset; wait until the
    // resume child exits successfully (legacy launch returns after
    // spawning the monitor), then:
    assert_eq!(persisted_cmd(root, phase).as_deref(), Some("old"));
}
```

## Info

### IN-10: The WR-10 re-read silently discards an edit that left the state file unparseable

**File:** `crates/devflow-cli/src/pipeline_gate.rs:435-438`

**Issue:** `if let Ok(on_disk) = workflow::load_state(...)` falls back to the in-memory copy without saying so, and the next line writes that copy over the file. The only documented route is a hand edit of JSON. A typo is therefore the most likely way for the re-read to fail, and when it does, the operator's change and the file are both silently replaced with the pre-wait value. `load_state` returns `Err` on any serde failure (workflow.rs:266).

**Fix:** On `Err`, log a warning and emit an event that names the reason, so the operator knows the edit was not applied:

```rust
match workflow::load_state(project_root, state.phase) {
    Ok(on_disk) => state.gate_notify_cmd = on_disk.gate_notify_cmd,
    Err(err) => warn!(phase = %state.phase, "state re-read failed ({err}); \
        an on-disk gate_notify_cmd edit was not applied and the file is being rewritten"),
}
```

### IN-11: The docs say a rate-limit resume ignores the shell environment, but that resume applies its scheduler's environment

**File:** `OPERATIONS.md:104-106`; `crates/devflow-core/src/ship.rs:299`

**Issue:** "gates fired by the detached monitor, including after a rate-limit resume, use it without your shell's environment." The rate-limit resume is a `devflow resume` run by the Hermes cron. Under the decided rule, that resume applies whatever `DEVFLOW_GATE_NOTIFY_CMD` the scheduler's environment carries. A Hermes daemon started from a shell that exports a different hook replaces the run's saved command, and one that has the variable set to empty clears it. The design is correct; the sentence implies that no environment takes part.

**Fix:** Append to that paragraph: "The rate-limit auto-resume is itself a `devflow resume`, so the variable in the scheduler's environment, if set, applies the same way."

---

_Reviewed: 2026-10-09T18:25:54Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
