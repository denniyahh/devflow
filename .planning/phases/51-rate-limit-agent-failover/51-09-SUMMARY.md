---
phase: 51-rate-limit-agent-failover
plan: 09
subsystem: gate-notify
tags: [rust, gate-notify, resume, doc-check, d-17, backlog-999.154]
status: complete

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "51-05 and 51-07 (wave-order dependencies; no API consumed)"
provides:
  - "e2e resume_with_the_variable_unset_keeps_the_saved_command (WR-13), read after the first stage_launched event, red on the apply_notify_env(None) mutant"
  - "e2e resume_with_an_empty_variable_is_seen_cleared_by_the_post_commit_waiter (waiter control)"
  - "unit pipeline_gate::tests::gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded (WR-12 verdict)"
  - "unit pipeline_gate::tests::gate_entry_over_an_unparseable_state_does_not_rewrite_it (IN-10 verdict)"
  - "doc_check::strip_notify_env_doc_names_exactly_its_call_sites and doc_check::gate_notify_cmd_doc_cites_the_tests_that_pin_it"
  - "rewritten doc comments on monitor::strip_notify_env and State::gate_notify_cmd"
affects: [51-11]

actuals:
  tokens: 7500
  tasks: 3
  commits: 4
plan_head_before: 12fb276d4e9442243c8f14b189ec2ee33a6e8d37
plan_head_after: bd178fecfb1bc185d8eeb1276f9d0681dcc22dd0

tech-stack:
  added: []
  patterns:
    - "A post-commit e2e waiter keys on an event the code emits strictly after the write under test (stage_launched after the publish), with a fixture assertion that no such event predates the spawn"
    - "Doc comment pinned to call sites by a source scan whose needle is assembled with concat! so the scanning file cannot report itself"

key-files:
  created: []
  modified:
    - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-core/src/doc_check.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/state.rs

key-decisions:
  - "WR-12 closed: no longer reproduces at 12fb276; evidence gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded (passes on current code, fails when the gate-entry commit is replaced with a blind save_state)"
  - "IN-10 closed: no longer reproduces at 12fb276; evidence gate_entry_over_an_unparseable_state_does_not_rewrite_it (passes on current code, fails under the same blind-save mutant)"
  - "The new WR-13 waiter arms run the shared resume fixture in Mode::Supervise: in Mode::Auto the fixture parks at the D-07 Define preflight gate before any stage launches, so stage_launched never fires; the two existing resume arms keep Mode::Auto"

patterns-established:
  - "Gate-entry reproductions use their own test child with DEVFLOW_GATE_NOTIFY_CMD removed (the runner's variable would replace both commands)"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "devflow resume with DEVFLOW_GATE_NOTIFY_CMD unset keeps the saved notify command (WR-13), read after resume's save"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/gate_notify_persist_e2e.rs#resume_with_the_variable_unset_keeps_the_saved_command"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/gate_notify_persist_e2e.rs#resume_with_an_empty_variable_is_seen_cleared_by_the_post_commit_waiter"
        status: pass
    human_judgment: false
  - id: D2
    description: "WR-12 and IN-10 settled at gate entry by tests run against current code"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::gate_entry_over_an_unparseable_state_does_not_rewrite_it"
        status: pass
    human_judgment: false
  - id: D3
    description: "strip_notify_env and gate_notify_cmd doc comments state only what is established, pinned by source scans"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::strip_notify_env_doc_names_exactly_its_call_sites"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::gate_notify_cmd_doc_cites_the_tests_that_pin_it"
        status: pass
    human_judgment: false

duration: 12 min
completed: 2026-11-07
---

# Phase 51 Plan 09: Notify-command resume arm, gate-entry reproductions and doc pinning Summary

**`devflow resume` with the notify variable unset is now pinned to keep the saved command, shown red on the WR-13 defect itself; WR-12 and IN-10 no longer reproduce at gate entry; both notify doc comments are pinned to the code by source scans.**

## Performance

- **Duration:** 12 min (2026-11-07T22:40:16Z to 22:52:14Z)
- **Tasks:** 3
- **Files modified:** 5
- **Plan base (`<plan-base>`):** `12fb276d4e9442243c8f14b189ec2ee33a6e8d37`

## Verdicts for 51-11 (OPERATIONS.md stated limit)

- **WR-12: closed, no longer reproduces at `12fb276`.** Evidence: `pipeline_gate::tests::gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded`. An on-disk `gate_notify_cmd` edit made after the advance loaded its copy is the command the gate fires, and it stays on disk. The gate-entry `commit` reloads the file.
- **IN-10: closed, no longer reproduces at `12fb276`.** Evidence: `pipeline_gate::tests::gate_entry_over_an_unparseable_state_does_not_rewrite_it`. With an unparseable state file at gate entry, `run_gate_with_timeout` returns `Err` (`state JSON failed: ...`), the file's bytes are unchanged, and no gate file is opened.
- Limit: the error text says the state JSON failed to parse. It does **not** include the state file's path, because `WorkflowError::Json` carries none. Adding the path would change a public core error, which is outside this plan.

## Accomplishments

### Task 1: WR-13 resume arm (tracer)
- `with_spawned_resume(mode, notify, observe)` holds the shared fixture. `resume_with` (the two existing arms) runs it unchanged in `Mode::Auto`.
- `wait_for_first_stage_launched` waits (bounded at 20 s) for the first `stage_launched` event in `.devflow/events.jsonl`. It panics with the event log if the marker never appears, or if `resume` exits without it (it re-reads once after an exit to cover the race). The fixture asserts there is no `stage_launched` event before the spawn.
- **Mutant run (RED-equivalent on the defect).** `commit_notify_env` was changed to `devflow_core::gates::apply_notify_env(None)`, and `resume_with_the_variable_unset_keeps_the_saved_command` failed (nextest rc=100):
  ```
  panicked at crates/devflow-cli/tests/gate_notify_persist_e2e.rs:506:5:
  assertion `left == right` failed: resume with DEVFLOW_GATE_NOTIFY_CMD unset must keep the saved notify command
    left: None
   right: Some("old")
  ```
  It was reverted with `git checkout -- crates/devflow-cli/src/pipeline_launch.rs`, and `git diff --exit-code HEAD -- crates/devflow-cli/src/pipeline_launch.rs` then exited 0. The rerun passed (1 passed). The mutant was never committed: `git log --format=%H 12fb276..HEAD -- crates/devflow-cli/src/pipeline_launch.rs` prints nothing. As a control, the same command for `pipeline_gate.rs` prints `61f3b8c`.
- Waiter control: `resume_with_an_empty_variable_is_seen_cleared_by_the_post_commit_waiter` reads `None` through the same waiter.

### Task 2: WR-12 / IN-10 reproductions
- Both tests passed on current code on their first run. To show they can fail, the gate-entry `commit(state, |s| s.gate_pending = true)?;` was replaced with `state.gate_pending = true; workflow::save_state(state)?;`:
  - WR-12 test: rc=100, `the gate must fire the command edited on disk after the advance loaded`.
  - IN-10 test: rc=100. The gate opened over the rewritten file and timed out after 30 s, then failed with `the error must say the state did not load: gate for stage code timed out awaiting a response`.
- The file was restored from a scratch copy. `git diff` after the restore showed only the 125 added test lines.

### Task 3: doc comments pinned by scans
- RED, before the comments were changed (both rc=100, and the planted controls passed before the real-tree assertion):
  - `strip_notify_env_doc_names_exactly_its_call_sites`: `no `Called from` line in the doc comment above the definition`, with the three call sites listed.
  - `gate_notify_cmd_doc_cites_the_tests_that_pin_it`: `the comment does not cite` both test names.
- GREEN after the rewrite. Only comments changed in `monitor.rs` and `state.rs`: every +/- line of that diff starts with `///` (36 lines, 0 non-comment lines).
- Call sites named on `strip_notify_env`'s `Called from` line (each confirmed by reading the call):
  - `spawn_monitor`: `crates/devflow-core/src/monitor.rs:640`. Detached supervisor `child_cmd`.
  - `spawn_stage_agent`: `crates/devflow-core/src/monitor.rs:2170`. The agent spawned by the pipe-owning supervisor.
  - `run_stream_canary`: `crates/devflow-core/src/canary.rs:374`. The delivery canary.
  - The call at `monitor.rs:3139` is inside `mod tests {` and is excluded.
- `gate_notify_cmd` now cites `resume_with_the_variable_unset_keeps_the_saved_command` and `advance_gate_runs_the_persisted_command_when_the_variable_is_unset`. It also states the gate-entry read, and notes that the stale-build block's notify (`staleness.rs:467`) opens no gate and fires from its caller's copy. That last point comes from reading the source; no test covers it.

## Task Commits

1. **Task 1:** `5474c8b`: test(51-09): pin resume keeping the saved notify command when unset
2. **Task 2:** `61f3b8c`: test(51-09): settle WR-12 and IN-10 at gate entry with reproductions
3. **Task 3 RED:** `946917b`: test(51-09): add failing scans pinning the two notify doc comments
4. **Task 3 GREEN:** `bd178fe`: docs(51-09): match the notify doc comments to what is established

## Verification run

- Task 1 gate: `git diff --exit-code HEAD -- crates/devflow-cli/src/pipeline_launch.rs`, then `cargo nextest run -p devflow --test gate_notify_persist_e2e --no-tests=fail -E 'test(=…)'` for each of the 4 resume tests. All 4 passed, and the whole file passed (11/11).
- Task 2 gate: the 3 `--bin devflow` named tests passed (chain rc=0).
- Task 3 gate: the 3 `doc_check` named tests passed. `cargo semver-checks check-release -p devflow-core` rc=0 ("no semver update required"). `cargo clippy --workspace --all-targets -- -D warnings` rc=0.
- Plan-level: `cargo test -p devflow --bin devflow` rc=0 (672 passed, 2 ignored). `cargo test -p devflow-core --lib` rc=0 (1137 passed, 2 ignored).
- `RUSTDOCFLAGS="-D warnings" cargo doc -p devflow-core --no-deps` fails with 61 errors, all already present (private intra-doc links elsewhere in the crate). None are in the edited ranges of `monitor.rs` or `state.rs`. The new comments use plain backticks.
- No marked e2e process was left running: a `/proc` scan for `DEVFLOW_NOTIFY_E2E_MARKER=` found 0. As a control, the same scan found 204 processes with `HOME=`.
- `Cargo.lock` is unchanged (T-51-SC).

## What this evidence does not establish

- One WR-13 mutant was run (`apply_notify_env(None)`). Other ways of losing the command are caught only if they leave the saved value different from `"old"` when `stage_launched` is emitted.
- The WR-13 arms run in supervise mode. The unset-keeps arm in auto mode is covered only because `commit_notify_env` runs before the mode-dependent preflight (`resume_loaded` commits first). This was read from the source, not tested in auto mode.
- The WR-12 test covers `run_gate_with_timeout`'s entry. An edit landing in the few milliseconds between the entry commit and the notify is not covered.

## Deviations from Plan

**1. [Rule 3 - Blocking] The fixture's auto mode never reaches `stage_launched`**
- **Found during:** Task 1
- **Issue:** In `Mode::Auto` the shared fixture (no `.planning/config.json`, legacy launch) parks at the D-07 Define preflight gate. The first run of both new tests timed out after 20 s with only `gate_fired`/`notify_fired` in the event log.
- **Fix:** `with_spawned_resume` now takes the mode. The new waiter arms use `Mode::Supervise`, where the same preflight reports and proceeds (D-08). The two existing arms keep `Mode::Auto` and behave as before.
- **Files modified:** `crates/devflow-cli/tests/gate_notify_persist_e2e.rs`
- **Commit:** `5474c8b`

**2. [Commit type] Task 3's GREEN commit is `docs(51-09)`, not `feat(51-09)`**
- The plan allows only doc-comment changes in `monitor.rs` and `state.rs`, so there is no feature commit. RED (`946917b`) comes before GREEN (`bd178fe`).

**Total deviations:** 1 auto-fixed (blocking), 1 commit-type note. **Impact:** none on scope. No production code changed.

## TDD Gate Compliance

- Task 1: the target passes on current code, as the plan expects. The RED-equivalent is the mutant failure recorded above. No `feat` commit, because no production change was needed.
- Task 2: both reproductions passed on current code (WR-12 and IN-10 are closed). Their ability to fail was shown with the blind-save mutant. No `feat` commit.
- Task 3: `test(51-09)` RED `946917b` comes before GREEN `bd178fe` (`docs`, since the change is to comments only).
- The `gsd_run check tdd-red-evidence` classifier was not run. It has no adapter for nextest/libtest output. RED was assessed by reading the failing assertion text quoted above.

## Issues Encountered

None beyond the deviation above.

## Next Phase Readiness

Ready for 51-11: the WR-12 and IN-10 verdicts above let it correct OPERATIONS.md's stated limit.

## Self-Check: PASSED
