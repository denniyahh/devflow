---
phase: 48-survivable-state-writes-and-honest-gate-recovery
verified: 2026-09-22T21:44:51Z
status: gaps_found
score: 6/7 must-haves verified
covered_files: [".planning/REQUIREMENTS.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-01-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-01-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-02-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-02-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-03-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-03-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-04-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-04-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-05-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-05-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-06-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-06-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-07-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-07-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-08-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-08-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-09-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-09-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-10-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-10-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-11-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-11-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-12-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-12-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-13-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-13-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-14-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-14-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-15-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-15-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-16-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-16-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-17-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-17-SUMMARY.md","clippy.toml","crates/devflow-cli/src/commands.rs","crates/devflow-cli/src/main.rs","crates/devflow-cli/src/pipeline_gate.rs","crates/devflow-cli/src/pipeline_launch.rs","crates/devflow-cli/src/pipeline_outcomes.rs","crates/devflow-cli/src/preflight.rs","crates/devflow-cli/src/staleness.rs","crates/devflow-cli/src/test_support.rs","crates/devflow-cli/tests/gate_sweep_e2e.rs","crates/devflow-cli/tests/gate_wedge_e2e.rs","crates/devflow-cli/tests/gitignore_coverage.rs","crates/devflow-cli/tests/plan_bashism_scanner.rs","crates/devflow-cli/tests/recover_clean_e2e.rs","crates/devflow-cli/tests/start_lock_e2e.rs","crates/devflow-cli/tests/stop_e2e.rs","crates/devflow-core/src/agent.rs","crates/devflow-core/src/agent_result.rs","crates/devflow-core/src/agents/opencode.rs","crates/devflow-core/src/agents/pi.rs","crates/devflow-core/src/config.rs","crates/devflow-core/src/doc_check.rs","crates/devflow-core/src/gates.rs","crates/devflow-core/src/lock.rs","crates/devflow-core/src/monitor.rs","crates/devflow-core/src/recover.rs","crates/devflow-core/src/ship.rs","crates/devflow-core/src/state.rs","crates/devflow-core/src/test_support.rs","crates/devflow-core/src/verify.rs","crates/devflow-core/src/workflow.rs","scripts/lint-plan-bashisms.sh"]
covered_digest: "v1:sha256:e946f6d121ae640ca5e31b709f54df68e3e9dffcbd48c4f6e038a5aa29be3965"
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "Criterion 4: the #200 wedge is reproducible on demand and its shape is pinned in both directions — the wedge requires the foreground `start` to be interrupted, and a run left alone self-resolves"
    status: partial
    reason: >-
      Both arms exist and pass, but the committed pin (gate_wedge_e2e.rs) drives `devflow advance`
      parked at a Code gate, not `devflow start` parked at its Define preflight gate (#200's shape).
      No committed test interrupts a `start`. The 48-16 SUMMARY justifies the substitution by saying
      "the live gate owner is the foreground `advance` invocation"; that is false for the #200 shape —
      a verifier scratch probe at HEAD shows the parked foreground `start` itself is the lock holder
      (lock pid == start pid) and the Define gate's poller. The start-driven behaviour does hold at
      HEAD (probe: interrupted start → reject refused, no response file, recover --clean clears
      state/lock/gate; live start → reject consumed, start exits 0, state cleared, pickup 990 ms), but
      nothing in the repo pins it, so a regression that stops `start` holding its lock while parked
      (which would wedge the self-resolving arm) is not caught by any committed test.
    artifacts:
      - path: "crates/devflow-cli/tests/gate_wedge_e2e.rs"
        issue: "Both arms spawn `devflow advance` at a Code gate; the test named wedge_arm_killed_start_... kills an advance, not a start"
      - path: ".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-16-SUMMARY.md"
        issue: "Line 85's stated reason for substituting advance for start is contradicted by the HEAD probe"
    missing:
      - "Two start-driven #200 arms: `devflow start --mode auto` with no .planning/config.json parked at the Define preflight gate; (a) kill start, `gate reject N define` → non-zero, 'no confirmed waiter', no response file, then `recover --clean --phase N` removes state, gate files, temps and lock-NN; (b) leave start running, `gate reject` → start exits 0, state cleared, pickup interval printed"
human_verification:
  - test: "Decide whether criterion 2's universal wording ('a second writer for the same phase is excluded') covers windows in which no process holds the per-phase lock during a live run"
    expected: "Either an explicit statement that criterion 2 is scoped to the D-01 writer inventory (start, stop, advance, resume, ship_override, recover), with the unlocked-window hazard tracked by backlog 999.136, or a new gap"
    why_human: "Scope decision. 999.136 (operator-deferred, reasoned not reproduced) documents that in the Legacy flow no lock is held between agent exit and the monitor's advance; `start` also releases its lock when it returns after launching, and `start` has no existing-state check, so exclusion rests on the lock alone. No violation was reproduced by the verifier; this is reasoned from source."
---

# Phase 48: Survivable State Writes and Honest Gate Recovery — Verification Report

**Phase Goal:** A second writer cannot silently erase another's state update, a gate whose consumer is gone tells the operator the repair that actually works instead of asserting a waiter that does not exist, preflight and resume agree on where blocking-human gates exist, checkpoints added after preflight are re-scanned, and the test-suite PATH race is isolated.
**Verified:** 2026-09-22 at HEAD `a692468` (worktree `.worktrees/phase-48`, branch `feature/phase-48`)
**Status:** gaps_found (1 partial gap, criterion 4; 1 scope decision for a human)
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (ROADMAP Phase 48 success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A second-writer test fails on the pre-fix implementation; a single-writer control round-trips | ✓ VERIFIED | Independently reproduced by the verifier against a `git archive` of the pre-fix tree `167363f` (see Behavioral Spot-Checks). `stop_writes_no_state_while_the_lock_holder_survives_the_signal` FAILED pre-fix with its behavioural assertion; a companion probe showed pre-fix `stop` exits 0, **rewrites the state bytes** (`stopped=true`) and leaves the live holder running — a silent overwrite. `start_refuses_while_another_process_holds_the_phase_lock` also FAILED pre-fix ("start must refuse a held lock"). Controls passed on the same pre-fix build: `stop_marks_state_stopped_and_records_reason`, `stop_marks_stopped_after_the_signalled_lock_holder_exits`, `start_proceeds_when_no_process_holds_the_phase_lock`. |
| 2 | After the fix a second writer is excluded or refused loudly; the single-writer control still round-trips | ✓ VERIFIED | `commands.rs:360` `start` takes `lock::acquire` right after the dry-run return, before gate cleanup and worktree/branch effects, and refuses contention naming the phase and pid. `commands.rs:2120` `persist_stopped_state` acquires the lock (a bounded `acquire_blocking` for `TERMINATE_VERIFY_WAIT` after a signal) before load → mutate → save, and fails non-zero with "was not marked stopped" while the holder lives. `pipeline_launch.rs:1621` `advance` waits with `acquire_blocking(ADVANCE_LOCK_WAIT)`. At HEAD all 22 `stop_e2e` tests and all 3 `start_lock_e2e` tests pass. Residual scope question → Human Verification 1. |
| 3 | approve/reject/stop/sweep check for a live waiter; with none they say so, write no non-Ship answer and name the repair; no `rm -f`; resume never reads a pending answer | ✓ VERIFIED | `gate_respond` (`commands.rs:1414-1424`) checks `holder_status` first and returns "no confirmed waiter … no response was written. run `devflow resume --phase N` or `devflow recover --clean --phase N`". Ship with `NoHolder` writes the answer and names `devflow ship --phase N` (`no_waiter_repair`). `stop_via_gate` (`:1926`) and sweep (`gate_sweep_may_reap`, `:1736`, Live only) write nothing without a Live holder and print the same repair. No `rm -f` appears in non-comment production source. `resume` (`pipeline_launch.rs:1408`) goes straight to `launch_stage` and reads no response. Tests pass at HEAD: `gate_approve_without_a_live_holder_does_not_claim_advance`, `stop_at_a_ship_gate_with_a_recycled_holder_refuses_without_writing_a_response`, `stop_at_a_gate_with_an_unconfirmable_holder_refuses_unevidenced_success`, `dry_run_sweep_reports_no_waiter_gates_as_left_alone`, `commands::tests::gate_response_message_does_not_claim_pickup_after_live_becomes_no_holder`. |
| 4 | #200 wedge reproducible on demand; shape pinned both ways (interrupted `start` wedges, left-alone self-resolves) | ✗ FAILED (partial) | Both arms exist and pass (`gate_wedge_e2e.rs`: interrupted → reject refused, no response file, `recover --clean` clears everything; live → consumed, `pickup_ms=990` this run). **The committed pin drives `devflow advance`, not `devflow start`.** A verifier probe shows the `start` shape also holds at HEAD, but nothing committed pins it. See Gaps. |
| 5 | CHKPT-01: preflight and resume predicates share one parser; a marker off a task-opening line neither blocks preflight nor arms resume | ✓ VERIFIED | `verify.rs:131` `phase_has_blocking_human_checkpoint` and `:379` `phase_has_human_only_checkpoint` both filter `phase_checkpoint_declarations` (`:320`), anchored by `task_opening_line`. Callers: `preflight.rs:1076` and `pipeline_launch.rs:1773`; no other production substring check remains. Pre-fix the resume predicate was a whole-file `contains` (`git show c0fb193:crates/devflow-core/src/verify.rs`), so the prose tests discriminate. Passing at HEAD: `blocking_human_checkpoint_ignores_a_marker_mentioned_only_in_prose`, `human_only_checkpoint_ignores_a_marker_mentioned_only_in_prose`, `task_level_blocking_human_gate_is_human_only_on_both_predicates`, `checkpoint_parser_matches_real_plan_fixtures`. |
| 6 | CHKPT-02: checkpoints added or changed after Code's preflight are re-scanned from a fresh snapshot before the resume decision | ✓ VERIFIED | `pipeline_launch.rs:1794-1795` re-reads `phase_checkpoint_declarations(execution_root)` at decision time and compares it with `state.checkpoint_approval.unapproved`. Anything unapproved opens a gate with `None` auto-response before `relaunch_checkpoint_session`. Recorded RED in `48-07-task1-pipeline-red.json` shows "The existing route emitted checkpoint_auto_decided". Passing at HEAD: `checkpoint_added_after_code_preflight_parks_at_the_rescan_gate`, `rewritten_body_of_an_approved_checkpoint_parks_at_the_rescan_gate`, `unrecorded_…`, `pending_checkpoint_set_parks_at_the_rescan_gate`, `approving_the_rescan_gate_records_the_set_and_relaunches`, the control `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records`, and `preflight::tests::code_preflight_records_a_pending_set_from_the_execution_root`. |
| 7 | TEST-01: process-global PATH mutations isolated / converted to per-Command scoping | ✓ VERIFIED (structural) | `git grep 'set_var("PATH"\|remove_var("PATH"'` finds 1 line at HEAD, a doc comment (`test_support.rs:302`). The same search, extended to the deleted helper names, finds 219 lines at `c0fb193`. No generic `set_var(key)` receives `PATH` (every key passed is a `DEVFLOW_*` literal). `run_test_in_child` / `assert_child_ran_exactly_one_passing_test` exist in `devflow_core::test_support` and are used across pipeline_*, preflight, staleness, pi and opencode. `clippy.toml` disallows `std::env::set_var` and `std::env::remove_var`. The lint discriminates in a verifier control: an injected `set_var("PATH", …)` → clippy rc=101 "use of a disallowed method"; restored file → rc=0. |

**Score:** 6/7 truths verified (0 present-but-behavior-unverified).

### What each piece of evidence does NOT establish

- **Criterion 1/2:** the "first writer" in the discriminating test is a `sh -c "trap '' TERM; exec sleep 60"` process named in a lock file written by the fixture. That file uses the same `pid\nstart_time` format as `lock::acquire`, but the process never writes state. The test therefore proves "stop wrote state while a live lock holder existed", not a lost update between two real DevFlow writers. The criterion's own caveat stands: no field interleaving has been observed; Phase 49 criterion 5 owns that. The 48-10 unique-temp tests had only compile-failure RED, not behavioural RED. They are a backstop, not the criterion-1 evidence.
- **Criterion 3:** `gate approve`/`reject` still **write** an answer when the holder is `Unconfirmable` (alive but identity uncheckable: non-Linux or a legacy one-line lock), while `stop` and `sweep` do not. This is the operator's D-05 decision ("claims neither a waiter nor its absence"). It diverges from a literal reading of criterion 3 ("identity-confirmed … holder"). It is not counted as a gap. A stale answer that outlives its waiter can still decide a relaunched same-stage gate after `resume` (accepted as 48-SECURITY AR-48-04, backlog 999.130). `resume` itself reads no answer.
- **Criterion 4:** `pickup_ms` (1010 in the 48-16 SUMMARY, 990 in this run, 990 in the start probe) is pinned by the gate poll's first 1 s backoff step. It measures the poll interval, not anything about the wedge.
- **Criterion 6:** the re-scan runs only on the auto-decide arm (`(Some(session_id), true)`), by design (C-8). The other arms already fall through to a human per-stage gate.
- **Criterion 7:** this is structural removal plus a lint. It does not prove the NotFound flakes cannot recur. REQUIREMENTS itself calls the 2-CPU run a sanity check only.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/commands.rs` | start lock, stop lock-before-load, holder-aware respond/stop/sweep, `no_waiter_repair` | ✓ VERIFIED | Read at the cited lines; exercised by e2e suites |
| `crates/devflow-core/src/lock.rs` | `holder_status`, `acquire_blocking` | ✓ VERIFIED | `lock::tests::holder_status_distinguishes_recycled_and_unconfirmable` passes |
| `crates/devflow-core/src/workflow.rs` / `gates.rs` | unique temps, exclusive response publish | ✓ VERIFIED | `unique_temp_creation_retries_after_a_same_pid_orphan`, `second_publisher_past_the_existence_check_gets_already_responded` pass |
| `crates/devflow-core/src/verify.rs` | shared checkpoint parser | ✓ VERIFIED | see truth 5 |
| `crates/devflow-cli/src/pipeline_launch.rs` | re-scan at resume decision, bounded advance wait | ✓ VERIFIED | see truths 2, 6 |
| `clippy.toml` | disallowed-methods lint | ✓ VERIFIED | negative/positive control run |
| `crates/devflow-cli/tests/start_lock_e2e.rs`, `stop_e2e.rs` | SURV-01 red/green + controls | ✓ VERIFIED | red pre-fix, green at HEAD |
| `crates/devflow-cli/tests/gate_wedge_e2e.rs` | #200 both arms | ⚠️ PARTIAL | arms drive `advance`, not `start` (gap) |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `start` | per-phase lock | `lock::acquire` before effects (`commands.rs:360`) | WIRED |
| `stop` | per-phase lock | `persist_stopped_state` acquire → load → save (`:2120`) | WIRED |
| monitor `advance` | per-phase lock | `acquire_blocking(ADVANCE_LOCK_WAIT)` (`pipeline_launch.rs:1621`) | WIRED |
| `gate approve/reject`, `stop`, `sweep` | `lock::holder_status` | checked before any write | WIRED |
| preflight / resume | `phase_checkpoint_declarations` | both predicates filter one parser | WIRED |
| resume decision | fresh declarations vs `checkpoint_approval` | `unapproved(&current)` before `relaunch_checkpoint_session` | WIRED |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Criterion 1 RED on the pre-fix tree | `git archive 167363f` into scratch; a probe file holding the current stop lock tests + control; `cargo test -p devflow --test red_probe_e2e` | `stop_writes_no_state_while_the_lock_holder_survives_the_signal` FAILED "stop must refuse to mark state while the holder survives" (stdout "stop: signalled pid …"); both controls ok | ✓ PASS (red as required) |
| Pre-fix stop silently overwrites | scratch probe on the same pre-fix build | `exit_success=true bytes_changed=true holder_alive=true stopped=true` | ✓ PASS (confirms overwrite) |
| Pre-fix start ignores a held lock | current `start_lock_e2e.rs` against the pre-fix build | `start_refuses_…` FAILED "start must refuse a held lock"; `start_clears_leftover_…` FAILED "planted response must not abort a fresh start"; `start_proceeds_…` ok | ✓ PASS (red as required) |
| Criteria 1-4 green at HEAD | `cargo test -p devflow --test stop_e2e --test start_lock_e2e --test gate_wedge_e2e --no-fail-fast` | 22 + 3 + 3 passed, 0 failed; `pickup_ms=990` | ✓ PASS |
| Criterion 4, `start` shape at HEAD | scratch copy of HEAD plus a start-driven probe (not committed) | wedge: start pid held the lock while parked; reject rc≠0 "no confirmed waiter … no response was written", response file absent; `recover --clean` → state, lock and gate all absent. Live: reject ok, start exit 0, state cleared, 990 ms | ✓ PASS (behaviour); not pinned in repo |
| CHKPT-02 / gate message tests | `cargo test -p devflow --bin devflow -- --exact <8 module-qualified names>` | 8 passed, 400 filtered | ✓ PASS |
| CHKPT-01 / SURV core tests | `cargo test -p devflow-core --lib -- --exact <7 names>` | 7 passed, 842 filtered | ✓ PASS |
| TEST-01 lint discriminates | clippy on a scratch copy with an injected `set_var`, then restored | rc=101 disallowed method / rc=0 | ✓ PASS |

The full workspace suite was not re-run by the verifier. The orchestrator reports the pinned container gate `scripts/check-in-container.sh all` at `a692468` exited 0 with 1437 passed; the verifier did not independently reproduce that run.

### Probe Execution

No `scripts/*/tests/probe-*.sh` exist, and no PLAN declares one. Step 7c is not applicable.

### Requirements Coverage

| Requirement | Source Plans | Status | Evidence |
|-------------|-------------|--------|----------|
| SURV-01 | 48-01, 48-10, 48-11, 48-12 | ✓ SATISFIED (test arm) | truths 1-2; field arm owned by Phase 49 criterion 5 |
| SURV-02 | 48-01, 48-10, 48-12, 48-13, 48-14, 48-16 | ⚠️ PARTIAL | truth 3 verified; truth 4's `start` pin missing |
| CHKPT-01 | 48-01, 48-02 | ✓ SATISFIED | truth 5 |
| CHKPT-02 | 48-01, 48-07, 48-15 | ✓ SATISFIED | truth 6 |
| TEST-01 | 48-01, 48-03..48-06, 48-08, 48-09, 48-17 | ✓ SATISFIED (structural) | truth 7 |

All five IDs from ROADMAP and REQUIREMENTS.md are claimed by at least one plan. None is orphaned. REQUIREMENTS.md still shows SURV-01, SURV-02, CHKPT-02 and TEST-01 unchecked ("Pending"). That is traceability bookkeeping for phase completion, not an implementation gap.

### Anti-Patterns Found

The scan covered the 10,648 added lines in `crates/` and `clippy.toml` since `c0fb193`. It found no TBD, FIXME, XXX, TODO, HACK, `todo!` or `unimplemented!`. The pattern was confirmed to match on a known-positive sample.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/devflow-cli/tests/gate_wedge_e2e.rs` | 94 | Test named `wedge_arm_killed_start_…` kills `advance` | ⚠️ Warning | The name misstates what is pinned; part of the criterion-4 gap |

### Known limits accepted elsewhere (not gaps against the stated criteria)

- 999.136 (WR-05): the recover sweep can delete a live run in the window between agent exit and advance. It is reasoned, not reproduced, and pre-dates Phase 48. It bears on criterion 2's scope, so it is routed to Human Verification 1 rather than silently absorbed.
- 999.137 (R-2/R-7/R-8): sweep reach. It leaves only inert leftovers and does not contradict any of criteria 1-7.
- 999.130 / AR-48-04: a stale answer consumed by a relaunched same-stage gate.

### Human Verification Required

#### 1. Scope of criterion 2 across unlocked windows

**Test:** Decide whether "a second writer for the same phase is excluded" is meant to cover periods of a live run when no process holds `lock-NN`. Examples: the Legacy agent-execution window after `start` returns, and the agent-exit → `advance` window that 999.136 documents.
**Expected:** Either state explicitly that criterion 2 is scoped to the D-01 writer inventory, with 999.136 tracking the window, or open a gap.
**Why human:** This is a scope decision, and it is reasoned from source only. `start` has no existing-state check, so exclusion rests entirely on the lock. The verifier did not reproduce a violation.

### Gaps Summary

One partial gap, one root cause. Criterion 4 names the foreground **`start`** as what must be interrupted, but the committed #200 reproduction interrupts a foreground **`advance`** at a Code gate. The 48-16 SUMMARY's reason for that substitution is contradicted at HEAD: while parked at its Define preflight gate, `start` itself holds the per-phase lock and polls the gate. The behaviour is correct today (verifier probe, both arms), so this is a missing regression pin, not a broken feature. It is small to close: two e2e arms driving `devflow start --mode auto` with no `.planning/config.json`. The probe used here is at `scratchpad/head/crates/devflow-cli/tests/start_wedge_probe_e2e.rs` in this verification session.

**If the substitution is acceptable,** accept it explicitly by adding this to the frontmatter:

```yaml
overrides:
  - must_have: "the #200 wedge is reproducible on demand and its shape is pinned in both directions the wedge requires the foreground start to be interrupted"
    reason: "advance-driven arms pin the same lock-holder/response boundary; start-driven behaviour confirmed by verifier probe at a692468"
    accepted_by: "{operator}"
    accepted_at: "{ISO timestamp}"
```

---

_Verified: 2026-09-22_
_Verifier: Claude (gsd-verifier)_
