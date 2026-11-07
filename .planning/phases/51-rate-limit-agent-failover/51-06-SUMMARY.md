---
phase: 51-rate-limit-agent-failover
plan: 06
subsystem: pipeline
tags: [rust, supervisor, rate-limit, failover, canary, ownership-fence]

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "51-01: hand_off_agent, failover_target, fail_over_on_rate_limit, the RateLimited arm and the 50-13 handoff harness with a second stub; 51-04: State.fallback_agent set by --fallback-agent"
provides:
  - "hand_off_agent's single commit clears state.canary and state.session_id (D-04)"
  - "agent_handoff event keys tree_dirty (bool or null) and commit_count (u32 or null) (D-10, D-08)"
  - "agent_handoff_relaunch_failed event (stage, from_agent, to_agent, reason)"
  - "fail_over_on_rate_limit routes a non-NotOwner relaunch error to handle_infra_outcome (R2-D1) and returns NotOwner unchanged"
  - "private FailoverRelaunchGuard and in_failover_relaunch(), read by refuse_launch and record_failed_launch"
  - "#[cfg(test)] hold point after_failover_commit"
  - "eight tests and one scan clause"
affects: [51-07, 51-09, 51-10, 51-11, 51-12, 51-13]

actuals:
  tokens: 10634
  tasks: 3
  commits: 5
plan_head_before: 84687ae4da55b7e6569004e2eb843ac24982c667
plan_head_after: 7c9eba5567943ddfbdd834f4fe3b57920a86ff44

tech-stack:
  added: []
  patterns:
    - "Per-thread RAII flag (FailoverRelaunchGuard) beside ENGAGED_ADVANCE/HELD_RESERVATION that narrows WR-04's record clear to non-failover launches"
    - "Source-scan clause that matches only a line's code part (cut at the first // outside a string literal)"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs

key-decisions:
  - "A failover relaunch that fails after the handoff commit emits agent_handoff_relaunch_failed and calls handle_infra_outcome (one infra bump, then the never-silent gate or the abort at the ceiling); FailoverRelaunchGuard keeps the supervisor record through that relaunch only, so those writes stay owned under the Phase 50 D-03 fence"
  - "agent_handoff_relaunch_failed carries the full error text; the gate context starts 'the handoff to <fallback> could not relaunch the <stage> stage:' so truncation keeps the fallback's name"
  - "Tree fields are measured before the handoff commit at state.worktree_path or project_root; null means git could not answer, never a made-up value"

patterns-established:
  - "A relaunch that may keep the supervisor record is marked by an RAII guard scoped to one launch_stage call and pinned to one call site by a source scan"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "The handoff commit clears the recorded canary and session id, so a fallback whose own canary reads Absent is refused after one canary run; control: without the handoff the recorded Confirmed admits it with zero runs"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_handoff_reruns_the_delivery_canary_for_the_new_agent"
        status: pass
    human_judgment: false
  - id: D2
    description: "agent_handoff records tree_dirty and commit_count; an untracked file survives the handoff"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_handoff_event_records_the_tree_it_hands_over"
        status: pass
    human_judgment: false
  - id: D3
    description: "A zero-commit Code rate limit still hands off through advance, with commit_count 0 and tree_dirty false"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rate_limit_before_any_commit_still_hands_off"
        status: pass
    human_judgment: false
  - id: D4
    description: "Boundaries at Code: one below the infra ceiling a hop charges nothing (no-fallback control aborts); a consumed or equal fallback takes the cron arm; an Ambiguous outcome re-drives without handing off"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::an_unused_fallback_hands_off_one_below_the_infra_ceiling"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rate_limit_after_the_one_hop_takes_the_cron_arm"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::an_ambiguous_outcome_with_an_unused_fallback_re_drives_without_handing_off"
        status: pass
    human_judgment: false
  - id: D5
    description: "Inside a real supervisor, a relaunch refused after the handoff commit emits agent_handoff_relaunch_failed and ends at the Code gate opened by the recorded supervisor; no cron arm, no supersession"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_handoff_whose_relaunch_fails_ends_at_a_gate"
        status: pass
    human_judgment: false
  - id: D6
    description: "A supervisor replaced before its failover relaunch's first owned write writes nothing; the guard is engaged only in fail_over_on_rate_limit"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_failover_superseded_mid_relaunch_writes_nothing"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_supervisor_record_is_cleared_only_on_launch_failure_paths"
        status: pass
    human_judgment: false

duration: 14min
completed: 2026-11-07
status: complete
---

# Phase 51 Plan 06: Failover Hop Hardening Summary

**Per-agent evidence no longer crosses the hop, because the handoff commit clears the canary and the session id. The `agent_handoff` event now records the tree it hands over. A relaunch that fails after the handoff commit now emits `agent_handoff_relaunch_failed` and stops at the never-silent Code gate, inside a real supervisor. Before this plan, that run ended with no gate, nothing running and nothing scheduled. The Phase 50 ownership fence is unchanged.**

## Performance

- **Duration:** 14 min
- **Started:** 2026-11-07T21:50:10Z
- **Completed:** 2026-11-07T22:04:11Z
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments
- `hand_off_agent`'s one commit now also sets `canary = None` and `session_id = None`. Before the commit, it measures `tree_dirty` (from `git status --porcelain` at the launch root) and `commit_count` (from `agent_result::phase_commit_count`), and adds both to `agent_handoff`. Each is `null` when it cannot be measured. Nothing is reset, stashed or cleaned, and there is no commit-count precondition.
- `FailoverRelaunchGuard` is a per-thread RAII flag. `refuse_launch` and `record_failed_launch` skip `supervisor = None` while it is engaged. The `pending_supervisor` clear stays unconditional. It is engaged only around `fail_over_on_rate_limit`'s `launch_stage` call. Doc comments on both clear sites and the WR-04 comment in `spawn_agent_and_record` name this exception.
- `fail_over_on_rate_limit` routes the relaunch result. `Ok` returns `Ok`. A `NotOwner` error is returned unchanged. Any other error emits `agent_handoff_relaunch_failed`, then calls `handle_infra_outcome`, whose reason starts with `the handoff to <fallback> could not relaunch the <stage> stage:`. A refusal before the commit (D-07) still takes the cron arm.
- `launch_record_clear_violations` gains a clause: every non-test line whose code part holds `FailoverRelaunchGuard::engage(` must lie inside `fail_over_on_rate_limit`.

## Task Commits

1. **Task 1 RED** - `fbe17cd` test(51-06): add failing tests for per-agent evidence and tree fields at handoff
2. **Task 1 GREEN (tracer)** - `970e70f` feat(51-06): clear per-agent evidence and record the tree at handoff
3. **Task 2** - `4db1f10` test(51-06): pin the hop's boundaries at Code (tests only; 51-01's code already held them)
4. **Task 3 RED** - `1b0e081` test(51-06): add failing tests for a failed failover relaunch (includes the `after_failover_commit` hold)
5. **Task 3 GREEN** - `7c9eba5` feat(51-06): end a failed failover relaunch at the infra gate

## Files Created/Modified
- `crates/devflow-cli/src/pipeline_launch.rs` - the handoff clear and tree fields, `FailoverRelaunchGuard`, the two clear-site exemptions, the relaunch-failure routing, the hold point, eight tests and one scan clause

## Verification

| Check | Command | Result |
|-------|---------|--------|
| Task 1 gate | the plan's 4 chained `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=…)'` | exit 0 (also re-run as the tracer gate before Task 2) |
| Task 2 gate | the plan's 3 chained nextest commands | exit 0 |
| Task 3 gate | the plan's 6 chained nextest commands + `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Plan-level | `cargo test -p devflow --bin devflow` | 666 passed, 0 failed, 2 ignored, exit 0 |
| doc_check | `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::source_devflow_env_vars_and_subcommands_are_documented)'` | pass (no env read or Command variant added) |
| Scope | `git diff --name-only 84687ae..HEAD` | only `crates/devflow-cli/src/pipeline_launch.rs`; `pipeline_outcomes.rs` and core `workflow.rs` untouched; `update_engaged_state` byte-identical (10 lines both sides) |
| Test env | `git diff 84687ae..HEAD` added lines with `set_var`/`remove_var` | none |

**What these checks do not establish.**
- The relaunch-failure test uses one real failure point: the fallback's canary refusing, which reaches `refuse_launch`. The other clear site, `record_failed_launch` after a spawn failure, is not exercised in a supervisor here. Plans 51-10 Task 1 and 51-13 Task 2 own that.
- The supersession test models a newer supervisor by writing its record into the state. It does not run a second launcher.
- No test here covers a failed relaunch that aborts at the infra ceiling (`infra_failures` already at `MAX_INFRA_FAILURES - 1`). That case is `gate_or_abort_infra`'s existing branch.
- Untested, and not new to failover:
  - (a) If `save_successor_stop` fails after the record already names the successor, the run ends superseded with no gate.
  - (b) After an RV-08 unconfirmed shutdown (`shut_down_unrecorded`, `pipeline_launch.rs` L2450, message ~L2482), a failover now parks at a gate (R2-D1) while an unrecorded successor may still run. Its error text names the `devflow stop --phase N --launch <id>` repair.
  - (c) The cron arm's commit-then-unfenced-write window (R2-2, T-51-40). See the backlog finding below.
- Nesting edge, untested: if `run_preflight` opens its own preflight gate inside the guarded relaunch and that gate's answer relaunches from within it, the nested launch also runs guarded. A failure there would then keep the record and still reach the infra gate, so the run stays never-silent. No test covers it.
- Each e2e passed on every run it got: the gate test 3 times and the supersession test 3 times, counting the post-GREEN runs and the full suite. That bound is weak and does not size flakiness.

## TDD Gate Compliance

RED → GREEN: `fbe17cd` → `970e70f` and `1b0e081` → `7c9eba5`. Each GREEN descends from its RED. Task 2 has no GREEN commit because no production change was needed. No REFACTOR commit was needed.

RED evidence came from nextest JUnit reports. They were generated with a scratch `red` profile and passed through `gsd-tools check tdd-red-evidence`. Every record returned `RED_EVIDENCE_OK`. The records are in `target/red-evidence/51-06-*.json` (gitignored).

| Test | Failed on | Semantic assessment |
|------|-----------|---------------------|
| `a_handoff_reruns_the_delivery_canary_for_the_new_agent` | `the primary's canary evidence does not cross the hop` (left `Some(Confirmed)`, right `None`) | Intended. The control (zero canary runs without a handoff) passed first |
| `the_handoff_event_records_the_tree_it_hands_over` | `tree_dirty` left `Null`, right `true` | Intended: the key was absent from the event |
| `a_rate_limit_before_any_commit_still_hands_off` | `commit_count` left `Null`, right `0` | Intended. The hop itself happened, so only the field was missing |
| `the_supervisor_record_is_cleared_only_on_launch_failure_paths` | `an engage call in another fn` (left `[]`) | Intended: the clause did not exist |
| `a_handoff_whose_relaunch_fails_ends_at_a_gate` | `no Code gate fired` after 45 s. The log ends at `agent_handoff` then `antigravity_delivery_canary_absent`, with nothing after | Intended. This is the silent end the plan describes. The `after_failover_commit` hold was reached, so the run was fenced |

**Task 2 RED-equivalents.** All three tests passed on their first run, because 51-01 already placed this behaviour. Each mutation below was applied, run, then restored from a saved copy and confirmed with `cmp`. Each mutation run's record is `RED_EVIDENCE_OK`.
- `an_unused_fallback_hands_off_one_below_the_infra_ceiling`: I moved the failover below the bump (commit `infra_failures + 1`, then `gate_or_abort_infra` at the ceiling, then failover). The test failed with `a handed-off run keeps its state: MissingState(…/state-5164.json)`: the bump reached the ceiling and aborted.
- `a_rate_limit_after_the_one_hop_takes_the_cron_arm`: I dropped `!state.fallback_consumed` from `failover_target`. The test failed with `phase 5166: no hop; events: [… "agent_handoff" … "to_agent": "codex" …]`.
- `an_ambiguous_outcome_with_an_unused_fallback_re_drives_without_handing_off`: I routed `Ambiguous` through `fail_over_on_rate_limit` when `failover_target` selects. The test failed with `the Ambiguous route ran` (left 0, right 1).

**Task 3 negative controls.**
- **Old ordering.** I removed `let _keep_record = FailoverRelaunchGuard::engage();` and reran `a_handoff_whose_relaunch_fails_ends_at_a_gate`. It failed after 45.2 s with `no Code gate fired`. Its event log showed `agent_handoff_relaunch_failed` followed by `{"event":"supervisor_superseded","ours":"18e5e242816a4110","recorded":null,"stage":"code","reason":"superseded",…}`. That is the self-inflicted `NotOwner`: the failed launch cleared the record before the infra path's first write. So the test does see the fence.
- **Replacement vs self-inflicted.** I removed the `Err(err) if is_not_owner(&err) => Err(err)` arm and reran `a_failover_superseded_mid_relaunch_writes_nothing`. It failed with `a replaced supervisor emits no agent_handoff_relaunch_failed`. On the restored code it passes, with `recorded == "newer-supervisor"`.

## In-supervisor re-drive check

**Verdict: the second shape. The `Ambiguous` re-drive ends silently, and nothing writes after the clear.** Read at `7c9eba5`:
1. `advance_with` (`pipeline_launch.rs` L3734) reaches the `Ambiguous` arm at L4075-4077. That arm returns `handle_ambiguous_outcome(..)` as the match value.
2. `handle_ambiguous_outcome` (L2970) does three things: an owned `commit` of `infra_failures + 1`, the `ambiguous_transport_retry` event, then `launch_stage(state, None, Some(stage))` as its tail expression (L2998).
3. On failure inside the relaunch, the record is cleared by `refuse_launch` (L630, canary refusal) or `record_failed_launch` (L2054, reached from `spawn_agent_and_record` L2231). The guard is not engaged on this path, so the clear happens. The `Err` then goes back up through `launch_stage`, `handle_ambiguous_outcome` and `advance_with` with no further state write. No self-inflicted `NotOwner` occurs.
4. `run_monitor`'s `other =>` arm (L1651-1654) returns the error. The `__monitor` process exits non-zero, and its stderr is `/dev/null` (`devflow-core/src/monitor.rs` L647-649).

**What the run is left with:** `supervisor` cleared and `infra_failures` bumped. The event log ends at `ambiguous_transport_retry`, plus `*_delivery_canary_absent` when the canary refused. There is no gate, no cron instruction, no event naming the relaunch error, and nothing running. This plan did not change it, as instructed.

## Backlog finding for the orchestrator

These are for `999.x` entries. Neither is fixed here, as the plan directs.

1. **The cron arm writes the cron file unfenced after its owned commit (R2-2, T-51-40; pre-existing).**
   - **Path:** `pipeline_outcomes.rs` `handle_rate_limited_outcome` (L89) commits `infra_failures + 1` as an owned write (L97-99). It then calls `devflow_core::ship::write_cron_instructions` (L106; `ship.rs` L94), a plain filesystem write.
   - **Window:** a replacement supervisor that lands between the two writes lets the superseded supervisor still write `cron-instructions-NN.json`.
   - **Observable:** a cron file scheduled by a supervisor the state no longer records.
   - **Fix:** run the write inside `fenced` (`pipeline_launch.rs` L1118). It holds the ownership check across the write. `ensure_still_recorded` (L1133) is only a check, so it would narrow the window without closing it.
   - **Reach:** no failover path enters this window after a handoff (R2-D1). The window is still live for the no-fallback cron arm and for the pre-commit refusal (D-07).
2. **An in-supervisor `Ambiguous` re-drive whose relaunch fails ends silently (defect).**
   - **Path:** an `Ambiguous` Layer 1 outcome (an Antigravity transport cancel with a success marker), then `handle_ambiguous_outcome`, then `launch_stage`. That relaunch is refused, for example when the recorded canary is `Absent`/`Unverified` or the binary is missing, or its spawn fails.
   - **Observable:** see the re-drive check above. The record is cleared, the supervisor exits non-zero with stderr to `/dev/null`, and there is no gate, no cron instruction and no event naming the error. The run reads "not launched", and nothing tells the operator why.
   - **Candidate fix:** route a non-`NotOwner` re-drive failure to `gate_or_abort_infra`. The counter was already bumped, so it must not be bumped twice. The fix also needs the same record-keeping exemption this plan added for failover, or the gate write meets the self-inflicted `NotOwner`.

## Operator remedy

This is the way on after `agent_handoff_relaunch_failed` because the fallback's canary refused. I confirmed it by reading the code at `7c9eba5`, not by a live run.
- `canary_gate` commits `canary = Some(outcome)` before refusing (L574). So the `Absent` stays recorded and refuses every later launch in the run.
- **Retry.** Answering the gate with retry first commits `gate_pending = false` (`pipeline_gate.rs` L672). `handle_stage_failure`'s `Advance` arm (`pipeline_outcomes.rs` L933-937) then closes the gate and calls `launch_stage` with the guard not engaged. The recorded `Absent` refuses again, and `refuse_launch` clears the record. The supervisor exits and the run reads not live. No gate is reopened.
- **Stop.** Plain `devflow stop --phase N` answers the open gate with `abort: stopped by devflow stop` (`commands.rs` `stop_via_gate` L3915 → `stop_reap_observed_gate`). An abort ends the run and removes its state. `devflow stop --phase N --launch <id>` for the recorded launch delegates to `stop_in` (`stop_launch_in` L3194, L3205-3211), so it does the same.
- **To continue on another agent:**
  1. Answer the gate with retry.
  2. Wait until `devflow status` shows the run not live.
  3. Run `devflow resume --phase N --agent <agent>`, naming an agent other than the run's current one. `resume --agent` goes through `hand_off_agent`, whose commit now clears the recorded canary, so the named agent runs its own canary.

## Decisions Made
- **Canary test mode.** The canary test (5161) runs under `Mode::Supervise` and builds its own state, not `canary_state`, whose mode is Auto. The handoff's full preflight then runs on a pair it admits. The test is about the evidence clear, not the launch shape.
- **`.devflow/` ignored in the tree tests.** The two tree-field tests commit a `.gitignore` holding `.devflow/` first. Without it, the run's own state file would make every tree read dirty, `tree_dirty: true` would assert nothing, and `tree_dirty: false` could never be shown.
- **Both stubs wait.** The stub for Tasks 1-2 makes both `claude` and `codex` wait (bounded, about 60 s). The Ambiguous re-drive relaunches Claude, so a no-op `claude` would let its supervisor advance and write state while the test reads it.
- **Scan clause.** It checks location only ("no engage outside the failover fn") and does not require the call to exist. Requiring it would change the existing planted cases' results, which the plan forbids. Its positive planted case shows the sanctioned site is not flagged.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] No existing test feeds an `Ambiguous` capture to `advance`**
- **Found during:** Task 2
- **Issue:** the plan said to reuse such a fixture. The only `Ambiguous` test calls `handle_ambiguous_outcome` directly.
- **Fix:** added `AMBIGUOUS_CANCEL_CAPTURE`, built from the same Antigravity transport-cancel envelope that `agent_result`'s own tests use (`ANTG_INIT` + `ANTG_RESULT_CANCEL_WITH_MARKER`). Those constants are private to core's test module, so the bytes are copied. Layer 1 reads it as `Ambiguous` whatever the run's agent, which the test's `ambiguous_transport_retry` assertion confirms.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Commit:** `4db1f10`

**2. [Rule 3 - Blocking] The Code preflight did admit Antigravity in the 50-13 rig**
- The plan asked me to name the failing check if it did not. It did, and no fixture file was needed: the RED log shows the Antigravity canary running, which happens only after `run_preflight`.

---

**Total deviations:** 1 auto-fixed (a missing fixture), plus one plan contingency that did not trigger. **Impact:** none on scope.

## Issues Encountered
- `cargo fmt` in the Task 3 GREEN commit also reflowed two lines of my Task 1 test code. That is formatting only. The first commit's subject is 80 characters, over the 72-character guideline; the commit-msg hook warned and did not block.

## Known Stubs
None.

## DevFlow Defects Found
- The `Ambiguous` re-drive's silent end (backlog finding 2) is a real defect, found by reading the code. No run hit it here.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- 51-07 and later plans that write `pipeline_launch.rs` must keep `FailoverRelaunchGuard::engage(` inside `fail_over_on_rate_limit`. The scan clause enforces this.
- 51-11 can document the `Operator remedy` section above as written.

## Self-Check: PASSED
- FOUND: `crates/devflow-cli/src/pipeline_launch.rs`
- FOUND (ancestors of HEAD): `fbe17cd`, `970e70f`, `4db1f10`, `1b0e081`, `7c9eba5`
- Commits measured from the ledger: `git rev-list --count 84687ae..7c9eba5` = 5

---
*Phase: 51-rate-limit-agent-failover*
*Completed: 2026-11-07*
