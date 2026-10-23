---
phase: 48-survivable-state-writes-and-honest-gate-recovery
verified: 2026-10-23T10:17:44Z
status: passed
score: 7/7 must-haves verified
covered_files: [".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-01-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-01-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-02-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-02-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-03-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-03-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-04-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-04-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-05-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-05-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-06-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-06-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-07-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-07-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-08-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-08-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-09-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-09-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-10-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-10-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-11-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-11-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-12-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-12-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-13-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-13-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-14-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-14-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-15-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-15-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-16-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-16-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-17-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-17-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-18-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-18-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-19-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-19-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-20-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-20-SUMMARY.md","clippy.toml","crates/devflow-cli/src/commands.rs","crates/devflow-cli/src/main.rs","crates/devflow-cli/src/pipeline_gate.rs","crates/devflow-cli/src/pipeline_launch.rs","crates/devflow-cli/src/pipeline_outcomes.rs","crates/devflow-cli/src/preflight.rs","crates/devflow-cli/src/staleness.rs","crates/devflow-cli/src/test_support.rs","crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs","crates/devflow-cli/tests/gate_sweep_e2e.rs","crates/devflow-cli/tests/gate_wedge_e2e.rs","crates/devflow-cli/tests/gitignore_coverage.rs","crates/devflow-cli/tests/plan_bashism_scanner.rs","crates/devflow-cli/tests/recover_clean_e2e.rs","crates/devflow-cli/tests/start_lock_e2e.rs","crates/devflow-cli/tests/stop_e2e.rs","crates/devflow-core/src/agent.rs","crates/devflow-core/src/agent_result.rs","crates/devflow-core/src/agents/opencode.rs","crates/devflow-core/src/agents/pi.rs","crates/devflow-core/src/config.rs","crates/devflow-core/src/doc_check.rs","crates/devflow-core/src/gates.rs","crates/devflow-core/src/lock.rs","crates/devflow-core/src/monitor.rs","crates/devflow-core/src/recover.rs","crates/devflow-core/src/ship.rs","crates/devflow-core/src/state.rs","crates/devflow-core/src/test_support.rs","crates/devflow-core/src/verify.rs","crates/devflow-core/src/workflow.rs","scripts/lint-plan-bashisms.sh"]
covered_digest: "v3:sha256:c4b3da73d0480f642dda0447065bf38c8cc4d83622c73a45da9c6c5534721184"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: passed
  previous_score: 7/7
  reason: "stale: covered files changed through Phases 49-50 (supervisor launch model, socket stop, hermetic e2e helper)"
  gaps_closed:
    - "Previous report's residue, 999.139 on `start` (unloadable state hides a live monitor): now refused. Phase 50-15 (R-12) sweeps the root+phase supervisor sockets when the state will not load; `start_refuses_over_a_live_supervisor_when_the_state_will_not_load` and `resume_refuses_over_a_live_supervisor_when_the_state_will_not_load` pass at HEAD, and the resume one turns red when the guard call is removed"
    - "Previous advisory 999.141 (e2e tests write the real ~/.cache/devflow/roots): not reproduced. Entry count was 1093 before and 1093 after this verifier's start_lock/gate_wedge/gate_sweep/recover_clean/stop_e2e runs"
  gaps_remaining: []
  regressions: []
gaps: []
deferred:
  - truth: "Criterion 2, residual window: a live supervisor whose record was never saved (launcher dies in its publish wait, T-50-90; stage N dies after spawning N+1 before saving its record, T-50-91) is not seen by the start/resume guard when the state file loads"
    addressed_in: "Phase 50.1"
    evidence: "Phase 50.1 success criterion 1: 'A supervisor that is live but whose record was never saved is never joined by a second one: `devflow start` and `devflow resume` refuse over it even when the state file loads.' The code's own doc comment on refuse_launch_over_a_live_run says the case 'is accepted until Phase 50.1'"
---

# Phase 48: Survivable State Writes and Honest Gate Recovery: Verification Report

**Phase Goal:** A second writer cannot silently erase another's state update. A gate whose consumer is gone tells the operator the repair that actually works, instead of asserting a waiter that does not exist. Preflight and resume agree on where blocking-human gates exist. Checkpoints added after preflight are re-scanned. The test-suite PATH race is isolated.
**Verified:** 2026-10-23T10:17:44Z on `workspace/denniyahh`. Code is identical to `78178f6`; HEAD moved to `775f6b2` during this run through two docs-only commits (`.planning/ROADMAP.md`, `DEV-SETUP-CHECKLIST.md`).
**Status:** passed
**Re-verification:** Yes. The previous report (passed, 7/7, at `5eb3a61`) was stale: 218 commits from Phases 49-50 changed the covered code, including a full launch-model change (Rust supervisor socket replaces `monitor_pid`).

## What argues against this result (read first)

- **The liveness mechanism that criterion 2 rests on is no longer Phase 48's.** Phase 50 replaced `State::monitor_pid` with `State::supervisor` and the guard now probes a supervisor socket. The Phase 48 outcome still holds and the tests still discriminate (two mutants, below), but the evidence is for Phase 50's mechanism carrying Phase 48's guarantee, not for Phase 48's code.
- **One second-writer window is open and accepted until Phase 50.1.** A supervisor that is alive but whose record never reached the state file is invisible to the guard when the state loads. Phase 50.1's criterion 1 owns it, so it is recorded as deferred, not as a gap.
- **999.136 is still open and still only operator-deferred.** `recover::is_stale_state` still decides on `started_at` age plus the agent pid file alone, so the sweep can delete a run in the agent-exit-to-advance window. It was reasoned from code and never reproduced. As in the previous report, it is excluded only by the operator's backlog deferral, not by an explicit scope ruling.
- **The full suite was not run by this verifier.** The orchestrator reports `scripts/check.sh all` green in the pinned container at `78178f6`. I confirmed only that `crates/`, `scripts/` and `clippy.toml` are unchanged between `78178f6` and HEAD.

## Goal Achievement

### Observable Truths (ROADMAP Phase 48 success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A second-writer test fails against the pre-fix implementation; a single-writer control round-trips | ✓ VERIFIED | **Mutant, this run:** in a `git archive` copy of HEAD, `start`'s `lock::acquire` was replaced with `let _phase_lock = ();`. `start_refuses_while_another_process_holds_the_phase_lock` failed at `start_lock_e2e.rs:103` with "start must refuse a held lock". Its control `start_proceeds_when_no_process_holds_the_phase_lock` passed in the same run (2 run: 1 passed, 1 failed). At HEAD both pass. The original RED against pre-fix `167363f` was reproduced by an earlier verifier, not by this one. |
| 2 | After the fix, a second writer is excluded or refused loudly and never silently overwrites; the single-writer control still round-trips | ✓ VERIFIED (one window deferred to Phase 50.1) | `start` takes the per-phase lock after the dry-run return and calls `refuse_start_over_a_live_run` before `Gates::cleanup` (commands.rs:367-377). `resume` takes the lock, then calls `refuse_resume_over_a_live_run`, then `load_state` (pipeline_launch.rs:2241-2250). Both delegate to `refuse_launch_over_a_live_run`, which now checks the supervisor socket (recorded, or swept by root and phase when the state will not load) plus the agent pid file. `advance` waits on the lock with a bound (`advance_waits_for_a_held_lock_then_proceeds`, `advance_lock_wait_expiry_emits_advance_failed` pass). `Gates::respond` refuses a second publisher (`second_publisher_past_the_existence_check_gets_already_responded` passes). `start_lock_e2e` at HEAD: 18/18. **Resume-guard mutant, this run:** with the guard call deleted, exactly three tests failed, each with its named reason: `resume_refuses_a_phase_whose_recorded_monitor_is_alive`, `resume_refuses_a_stopped_phase_whose_recorded_agent_is_alive` and `resume_refuses_over_a_live_supervisor_when_the_state_will_not_load`. The other 15, including the dead-process relaunch control and the no-state carve-out, passed. |
| 3 | approve/reject/stop/sweep check for a live waiter; with none they say so, write no non-Ship answer, and name the repair; no `rm -f`; resume never reads a pending answer | ✓ VERIFIED | `gate_respond` (approve and reject) checks `lock::holder_status` before writing and refuses with "no confirmed waiter … no response was written" plus `no_waiter_repair`. At Ship it writes only for `NoHolder`. `no_waiter_repair` names `devflow ship --phase N` at Ship and `devflow resume --phase N` or `devflow recover --clean --phase N` elsewhere. `stop_via_gate` and `gate_sweep` apply the same rule before `Gates::reap`. The gate waiter is still the lock holder: `advance_for_stage` takes `lock::acquire_blocking` and `run_gate_with_timeout` runs inside it. `rg 'rm -f' crates/*/src` finds nothing; the same search finds 3 hits under `scripts/` (positive control). `resume` makes no `Gates::poll*` call; the only polls are in the gate-wait helper. Tests passing at HEAD: 8 named `commands::tests` unit tests (no-waiter at non-Ship writes nothing, live waiter writes, recycled pid writes nothing, Ship no-waiter writes and names ship, sweep leaves a no-waiter gate alone, three doctor reconciliation findings), plus `stop_e2e` 37/37, `gate_sweep_e2e` 5/5 and `recover_clean_e2e` 12/12. |
| 4 | The #200 wedge is reproducible on demand and pinned both ways (interrupted start wedges; left alone it self-resolves, with the pickup interval recorded) | ✓ VERIFIED | `gate_wedge_e2e` 5/5 at HEAD. The wedge arm kills the foreground `start`, then asserts that `gate reject` says "no confirmed waiter", names `devflow recover --clean --phase N` and writes no response file. Run with `--no-capture`, the self-resolving arm printed `reject_wording=live` and `pickup_ms=992`. |
| 5 | CHKPT-01: preflight and resume use one shared checkpoint parser; a marker outside a task-opening line neither blocks nor arms | ✓ VERIFIED | `verify.rs` is unchanged since `5eb3a61`. `phase_has_human_only_checkpoint` and `phase_has_blocking_human_checkpoint` are both filters over `phase_checkpoint_declarations`. The callers are preflight.rs:1078 and pipeline_launch.rs:2641. Passing at HEAD: `verify::tests::human_only_checkpoint_ignores_a_marker_mentioned_only_in_prose`, `blocking_human_checkpoint_ignores_a_marker_mentioned_only_in_prose` and `task_opening_must_start_the_line`. |
| 6 | CHKPT-02: checkpoints added after Code's preflight are re-scanned from a fresh snapshot before the resume decision | ✓ VERIFIED | pipeline_launch.rs:2663-2664 re-reads `phase_checkpoint_declarations(execution_root, …)` inside the `(Some(session_id), true)` arm and compares it with `checkpoint_approval.unapproved`. Anything unapproved parks at `run_gate_with_timeout` with no auto-response. Passing at HEAD: `checkpoint_added_after_code_preflight_parks_at_the_rescan_gate`, `unrecorded_checkpoint_set_parks_at_the_rescan_gate` and `pending_checkpoint_set_parks_at_the_rescan_gate`, plus the control `unchanged_recorded_checkpoint_still_auto_decides` and `preflight::tests::code_preflight_records_a_pending_set_from_the_execution_root`. |
| 7 | TEST-01: process-global PATH mutations in tests are isolated | ✓ VERIFIED | A `git grep` for `set_var("PATH"` and `remove_var("PATH"` across `crates/` finds one hit, a doc comment (`test_support.rs:302`). The pattern matched a planted sample (positive control). The three non-literal setters (`config.rs` `EnvOverride`) are called only with five `DEVFLOW_*` keys, never PATH. `PathGuard`, `NeutralPath` and `prepend_path` are gone from code. `clippy.toml` still disallows `std::env::set_var` and `remove_var`. Passing at HEAD: `test_support::tests::child_with_an_empty_path_dir_cannot_spawn_git_while_the_parent_can`, `child_guard_rejects_a_name_that_matches_no_test` (the negative control) and `staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking` (the original 999.38 flake victim). |

**Score:** 7/7 truths verified (0 present-but-behavior-unverified).

### Prohibitions declared by the plans (answered from the evidence)

Nothing was found broken. The items below are recorded because 48-20 declared them with a `test` or `judgment` tier; every other plan's prohibitions are tier `explicit`.

| Prohibition (48-20) | Tier | Ruling | Basis |
|---|---|---|---|
| The resume refusal never signals or acts on a recorded pid | test | Holds | `refuse_launch_over_a_live_run` only probes sockets (`supervisor::probe`, read-only) and runs `agent::agent_running`. The refusal arms assert the live pid survives, and they pass at HEAD. |
| The refusal never says `devflow stop` ends a lock-free run | test | Superseded by Phase 50, honesty intent holds | The premise (999.138: `stop` could not end a lock-free run) was fixed in Phase 50: `stop_ends_a_lock_free_live_run_through_the_socket` passes. The refusal now says "`devflow stop` ends a supervisor through its socket", which matches the behaviour. |
| For an unloadable state, 48-20 changes no launch and no write, and no hint text | judgment | Historical; held at 48-20 (previous report's probe), then deliberately changed by Phase 50-15 | Phase 50 now refuses over a live supervisor when the state will not load, which is the 999.139 closure the operator folded into Phase 50. This is not a Phase 48 regression. |

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | A live supervisor whose record was never saved is not seen by the start/resume guard when the state loads (T-50-90, T-50-91) | Phase 50.1 | Criterion 1: "`devflow start` and `devflow resume` refuse over it even when the state file loads" |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/commands.rs` | lock-before-guard `start`; `gate_respond`/`stop_via_gate`/`gate_sweep` waiter checks; `no_waiter_repair`; shared live-run guard | ✓ VERIFIED | Present and called; the mutant proves the lock line is load-bearing |
| `crates/devflow-cli/src/pipeline_launch.rs` | resume guard under the lock; bounded `advance` lock wait; CHKPT-02 re-scan | ✓ VERIFIED | Lines 2241-2250 and 2663-2664; the mutant proves the guard is load-bearing |
| `crates/devflow-core/src/lock.rs` | `holder_status` and `may_be_waiting` (Live and Unconfirmable only); `acquire_blocking` | ✓ VERIFIED | `may_be_waiting` is at lines 64-69; `holder_status_distinguishes_recycled_and_unconfirmable` passes |
| `crates/devflow-core/src/verify.rs` | one checkpoint parser shared by both predicates | ✓ VERIFIED | Unchanged since `5eb3a61` |
| `crates/devflow-core/src/gates.rs` | hard-link publish; `AlreadyResponded` for a second responder | ✓ VERIFIED | Named test passes |
| `crates/devflow-core/src/test_support.rs` | `run_test_in_child` plus a guard with a negative control | ✓ VERIFIED | Two named tests pass; 149 call sites across `crates/` |
| `clippy.toml` | disallows `set_var` and `remove_var` | ✓ VERIFIED | Present with its reasons |
| `crates/devflow-cli/tests/{start_lock,stop,gate_wedge,gate_sweep,recover_clean}_e2e.rs` | criterion 1-4 e2e evidence | ✓ VERIFIED | 18, 37, 5, 5 and 12 tests, all passing |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `commands::start` | `lock::acquire`, then `refuse_start_over_a_live_run` | before `Gates::cleanup` and any git mutation | WIRED (mutant breaks the RED test) |
| `pipeline_launch::resume` | `commands::refuse_resume_over_a_live_run` | after `lock::acquire`, before `load_state` | WIRED (mutant turns exactly three resume arms red) |
| `gate_respond` / `stop_via_gate` / `gate_sweep` | `lock::holder_status` | checked before `Gates::respond` / `Gates::reap` | WIRED |
| gate waiter (`advance_for_stage`) | per-phase lock | `lock::acquire_blocking` at pipeline_launch.rs:2452; `run_gate_with_timeout` called inside it (2678) | WIRED |
| resume decision | `verify::phase_checkpoint_declarations` | fresh re-read at decision time | WIRED |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| 16 named CLI unit tests (criteria 2, 3, 6, 7) | `cargo nextest run -p devflow --bin devflow --no-tests=fail -E '<16 exact names>'` | All 16 names were resolved first with `nextest list`; 16 run, 16 passed | ✓ PASS |
| 7 named core unit tests (criteria 2, 3, 5, 7) | `cargo nextest run -p devflow-core --lib --no-tests=fail -E '<7 exact names>'` | All 7 names were resolved first; 7 run, 7 passed | ✓ PASS |
| e2e files | `cargo nextest run -p devflow --test <file> --no-tests=fail` | start_lock 18/18, gate_wedge 5/5, gate_sweep 5/5, recover_clean 12/12, stop 37/37 | ✓ PASS |
| Wedge pickup recorded | `gate_wedge_e2e` self-resolving start arm, `--no-capture` | `reject_wording=live`, `pickup_ms=992` | ✓ PASS |
| Lock-removal mutant (criterion 1 discrimination) | archived HEAD, `start` lock replaced with `()` | RED test fails with its named reason; single-writer control passes | ✓ discriminates |
| Resume-guard mutant (criterion 2 discrimination) | archived HEAD, guard call deleted | 15 passed, 3 failed (exactly the resume refusal arms) | ✓ discriminates |

Both mutants ran in a `git archive` copy with their own target dir (`~/.cache/claude-verify48-mutant-target`, 570 MB). Both were deleted afterwards and `git status` is clean. No repository file was mutated.

### What this evidence does NOT establish

- **It does not establish a field interleaving.** As criterion 1 states, the SURV-01 evidence is the test plus its mutant, not an observed incident.
- **Every e2e run is a single run.** One pass bounds nothing about races or flake rates, including the TEST-01 `NotFound` flake that criterion 7 targets. For that criterion the proof is structural (no global PATH mutation remains), not statistical.
- **The original pre-fix RED was not replayed.** Discrimination is shown by this verifier's own mutants of the current code.
- **The 999.141 count is a count, not an attribution.** The cache root count was unchanged (1093 → 1093), but the contents of `~/.cache/devflow/roots` were not diffed.
- **The full suite was not run here** (see the top section).

### Requirements Coverage

| Requirement | Source Plans | Status | Evidence |
|-------------|--------------|--------|----------|
| SURV-01 | 48-01, 48-10, 48-11, 48-12, 48-19, 48-20 | ✓ SATISFIED (test arm; one window deferred to 50.1) | Criteria 1-2 |
| SURV-02 | 48-01, 48-10, 48-12, 48-13, 48-14, 48-16, 48-18, 48-19 | ✓ SATISFIED | Criteria 3-4 |
| CHKPT-01 | 48-01, 48-02 | ✓ SATISFIED | Criterion 5 |
| CHKPT-02 | 48-01, 48-07, 48-15 | ✓ SATISFIED | Criterion 6 |
| TEST-01 | 48-01, 48-03..48-06, 48-08, 48-09, 48-17 | ✓ SATISFIED | Criterion 7 |

REQUIREMENTS.md maps exactly these five IDs to Phase 48 (Traceability rows 199-203, all Complete). No ID is orphaned and every ID is claimed by at least one plan.

### Probe Execution

No `scripts/*/tests/probe-*.sh` exists and no Phase 48 PLAN declares one. Step 7c does not apply.

### Anti-Patterns Found

The added lines in the covered files since `5eb3a61` contain no `TBD`, `FIXME`, `XXX`, `todo!` or `unimplemented!`. The pattern matched a planted `FIXME` sample. No `rm -f` prescription exists in `crates/*/src`.

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| `crates/devflow-core/src/recover.rs` `is_stale_state` | no supervisor/lock check before declaring a run stale (999.136) | ℹ️ Info | Operator-deferred backlog item, unchanged since the previous report |

### Gaps Summary

There are no open gaps. All seven criteria hold against the current code, which now runs under Phase 50's supervisor model. The two load-bearing guards were proven by mutation. The previous report's 999.139 residue on `start` is now closed by Phase 50. One second-writer window remains: a live supervisor whose record was never saved. It is deferred to Phase 50.1, whose first criterion targets it exactly.

---

_Verified: 2026-10-23T10:17:44Z_
_Verifier: Claude (gsd-verifier)_
