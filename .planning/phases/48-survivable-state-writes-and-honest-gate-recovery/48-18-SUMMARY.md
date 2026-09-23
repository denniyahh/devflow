---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 18
subsystem: testing
tags: [rust, e2e, gates, lock, recover, "#200"]

requires:
  - phase: 48-14
    provides: "`recover --clean --phase N` removes state, gate files, write temps and lock-NN without a live holder"
provides:
  - "#200 pinned on the real `start` shape: a foreground `devflow start` parked at its Define preflight gate holds lock-NN itself"
  - "Interrupted-start arm: reject refuses honestly, writes no response, names `devflow recover --clean --phase N`; recover clears everything"
  - "Live-start arm (negative control): the same reject is consumed and `start` exits 0 with state and gate cleared"
  - "Mutation control proving the start lock-holder pin discriminates"
  - "Advance-driven arms renamed so no test name says `start` while killing `advance`"
affects: [phase-48 verification criterion 4, gate_wedge_e2e]

actuals:
  tokens: 2900      # chars/4 over the realized diff of gate_wedge_e2e.rs (11522 changed-line chars, ea8ac12..working tree)
  tasks: 3
  commits: 3        # task commits 0ec6028, f932cd2, a213acb (orchestrator, #4799); this SUMMARY is committed separately
plan_head_before: ea8ac12

tech-stack:
  added: []
  patterns:
    - "ReapOnDrop(Child) owns every spawned `devflow start`, so a panicking assertion never leaks a parked process"
    - "Transient source mutation inside a verify gate, with a cmp-checked backup and an EXIT-trap restore"

key-files:
  created: []
  modified:
    - crates/devflow-cli/tests/gate_wedge_e2e.rs

key-decisions:
  - "Accept either reject wording in the live-start arm (`live` or `no_holder_after_exit`), but only alongside an observed successful `start` exit, because gate_respond re-checks the holder after publishing"
  - "Recorded reject_wording from a separate standalone run, because the Task 2 gate deletes its logs"

requirements-completed: [SURV-02]

coverage:
  - id: D1
    description: "Interrupted foreground `start` at the Define gate: the lock holder is the start pid; after kill, reject refuses with 'no confirmed waiter' and names `devflow recover --clean --phase 93`; no response file; recover --clean removes state, gate files, temps and lock-NN"
    requirement: SURV-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#start_wedge_arm_interrupted_start_leaves_a_define_gate_nothing_answers"
        status: pass
    human_judgment: false
  - id: D2
    description: "Live foreground `start` left alone consumes the same Define rejection and exits 0, with its state and gate request cleared"
    requirement: SURV-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#start_self_resolving_arm_live_start_consumes_the_define_rejection"
        status: pass
    human_judgment: false
  - id: D3
    description: "Mutation control: dropping `start`'s lock guard (`let _phase_lock` -> `let _`) turns the live-start arm red on 'the parked start must hold its own phase lock'; commands.rs restored byte-identical"
    requirement: SURV-02
    verification:
      - kind: other
        ref: "48-18-PLAN.md Task 2 <verify> gate (mutant_exit=101 mutant_failed_line=1 mutant_reason=1 commands_restored_diff_exit=0)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Advance-driven arms renamed truthfully; file doc names both drivers; whole file 5 passed; workspace clippy and fmt clean"
    verification:
      - kind: other
        ref: "48-18-PLAN.md Task 3 <verify> gate (five_passed_line=1 misnamed_arms=0 clippy_exit=0 fmt_exit=0)"
        status: pass
    human_judgment: false

duration: not recorded (see Performance)
completed: 2026-09-23
status: complete
---

# Phase 48 Plan 18: #200 pinned on the foreground `start` shape Summary

**Two e2e arms drive a real `devflow start --mode auto` parked at its Define preflight gate. An interrupted `start` leaves a gate that `gate reject` refuses and `recover --clean` clears. A live `start` consumes the same rejection and exits 0. A one-token lock-drop mutation turns the live arm red.**

## Performance

- **Duration:** not recorded. The executor did not capture a start timestamp. The Task 1 commit landed at 2026-09-23T11:36:52Z, the Task 2 commit at 11:38:43Z, and the Task 3 gate passed at about 11:40Z.
- **Tasks:** 3
- **Files modified:** 1 (`crates/devflow-cli/tests/gate_wedge_e2e.rs`). No production source changed.

## Accomplishments

- Criterion 4 is now pinned on the `start` shape in both directions. Both start arms assert that `lock::holder_identity`'s pid equals the start child's pid. That makes the #200 shape (the foreground `start` is the lock holder) an asserted fact rather than an assumption.
- The pin is shown to discriminate. See the mutation control below.
- No test name misstates the process it interrupts, and the `//!` doc says which driver each pair exercises.

## Task Commits

1. **Task 1: interrupted `start` at the Define gate** - `0ec6028` (test)
2. **Task 2: self-resolving start arm + mutation control** - `f932cd2` (test)
3. **Task 3: rename the advance arms, document both drivers** - `a213acb` (test). All three task commits were made by the orchestrator after re-running each gate; the executor made no commits (Phase 48 #4799 workaround).

## Gate Outputs

All three gates were run byte-for-byte from PLAN.md, from the worktree root, after a warm build.

**Task 1**
```
start_wedge_arm_interrupted_start_leaves_a_define_gate_nothing_answers exit=0 one_passed=1 filtered=3 filtered out
gate_rc=0
```

**Task 2**
```
commands_clean_before=0
backup_matches_exit=0
start_wedge_arm_interrupted_start_leaves_a_define_gate_nothing_answers exit=0 one_passed=1 filtered=4 filtered out
start_self_resolving_arm_live_start_consumes_the_define_rejection exit=0 one_passed=1 filtered=4 filtered out
observed_pickup_ms=992
mutation_sites=1
mutant_applied=1
mutant_exit=101 mutant_failed_line=1 mutant_reason=1
commands_restored_diff_exit=0
gate_rc=0
```
Afterwards, `git status --short crates/devflow-cli/src/commands.rs` printed nothing. The orchestrator re-ran this gate independently and got an identical result.

**Task 3**
```
file_exit=0 five_passed_line=1
defined advance_wedge_arm_killed_advance_leaves_a_code_gate_that_reject_reports_honestly=1
defined advance_self_resolving_arm_live_advance_consumes_the_code_rejection=1
defined start_wedge_arm_interrupted_start_leaves_a_define_gate_nothing_answers=1
defined start_self_resolving_arm_live_start_consumes_the_define_rejection=1
defined dry_run_sweep_reports_no_waiter_gates_as_left_alone=1
misnamed_arms=0
clippy_exit=0
fmt_exit=0
gate_rc=0
```

## Observations

- **reject_wording=live.** This was observed in a separate standalone run of `start_self_resolving_arm_live_start_consumes_the_define_rejection` with `--nocapture`, after the Task 2 gate had restored commands.rs. The run exited 0 with `1 passed; 0 failed; 4 filtered out`. The Task 2 gate deletes its own logs, so the wording printed during the gate's run cannot be read. The `no_holder_after_exit` path, the sub-millisecond race where `start` consumes the answer and releases its lock before `gate_respond` re-checks the holder, was never observed. It is accepted by the test but unexercised.
- **pickup_ms=992** in both the gate run and the standalone run. The value is set by the gate poll's first backoff step: `Gates` polls with exponential backoff starting at 1 s and doubling (gates.rs, `backoff = Duration::from_secs(1)`, then `backoff * 2`). It measures when the parked `start` next looked for a response. It says nothing about the wedge itself.
- **Mutation control.** The Task 2 gate rewrote the single binding `let _phase_lock = match lock::acquire(project_root, phase) {` in `commands::start` to `let _ = match …`. The guard then drops immediately, so the parked `start` no longer holds lock-NN. The self-resolving start arm failed with exit 101, `FAILED. 0 passed; 1 failed`, and the message "the parked start must hold its own phase lock". The gate restored commands.rs from a `cmp`-verified copy, and `git diff --quiet` exited 0.
- **An executor-side control in Task 1, beyond the plan.** The kill and reap were transiently removed from the interrupted arm, so `start` stayed alive during the same reject. The test failed with exit 101 on "reject against an interrupted start unexpectedly succeeded". The test file was restored byte-identical (`cmp` exit 0) before the gate ran. This shows the reject-refusal assertion tells a dead `start` from a live one.
- **Process hygiene.** After every run, `pgrep` found no leaked `devflow start --phase 93/94` process and no orphaned `sleep 60` fake agent. The only `start` spawn site, in `spawn_parked_start`, is wrapped in `ReapOnDrop` when it is created. The pre-existing `advance` spawn in `spawn_gated_advance` is not wrapped; it is outside this plan's scope and the orchestrator agreed.

## Test-name Mapping (Task 3)

| Old name | New name |
|----------|----------|
| `wedge_arm_killed_start_leaves_a_gate_that_reject_reports_honestly` | `advance_wedge_arm_killed_advance_leaves_a_code_gate_that_reject_reports_honestly` |
| `self_resolving_arm_live_start_consumes_the_rejection` | `advance_self_resolving_arm_live_advance_consumes_the_code_rejection` |

The bodies are unchanged except that the advance self-resolving arm now prints `advance_pickup_ms=` instead of `pickup_ms=`. That keeps a whole-file `--nocapture` run unambiguous.

## Historical Gates

- **48-16-PLAN.md** names both old tests in its verify gate with `--exact`. It also reads `pickup_ms=` from `self_resolving_arm_live_start_consumes_the_rejection.log`. After the rename, each old name matches nothing (checked: `0 passed; 0 failed; … 5 filtered out`, exit 0). A literal re-run of that historical gate therefore fails loudly with one_passed=0 (and `observed_pickup=` empty), rather than passing falsely.
- **48-14-PLAN.md** does not name the old tests in its verify gate. The name appears only in its `<behavior>` text. Its gate runs the whole `gate_wedge_e2e` file and requires exactly `2 passed; 0 failed`. The file has held more than two tests since 48-16 added the dry-run sweep test, and now holds five. A literal re-run of 48-14's gate therefore already fails, with two_passed=0, and fails for the same reason after this rename. This corrects the plan's statement that both historical gates "name the old tests".
- Neither historical PLAN file was edited.

## Correction to 48-16-SUMMARY

48-16-SUMMARY's "Execution constraint" bullet justifies driving `advance` on the grounds that "the live gate owner is the foreground `advance` invocation". That reason does not hold for the #200 shape. While parked at its Define preflight gate, the foreground `start` itself holds lock-NN and polls the gate. Both start arms now assert this: the lock holder pid equals the `start` child pid. The mutation control shows the assertion fails when it stops being true. 48-16-SUMMARY was not edited. This SUMMARY is the correction of record.

## Files Created/Modified

- `crates/devflow-cli/tests/gate_wedge_e2e.rs`:
  - start-fixture helpers: `init_start_repo`, `FakeBin`, `fake_bin_dir`, `start_child`, `ReapOnDrop`, `spawn_parked_start`, `reject_define`, `dir_entries`, `phase_gate_entries`
  - the two start arms
  - the two renamed advance arms
  - the rewritten `//!` doc

## Decisions Made

- The live-start arm accepts the `no_holder_after_exit` wording only in combination with the later assertion that `start` exits successfully within 30 s. That is the race the plan and commands.rs's `response_pickup_message` comment describe. If stdout contains neither wording, the test panics.
- The recovery checks treat a missing directory as empty for both `Gates::dir` and `.devflow`. Before `recover`, the arm asserts that the gate scan sees a gate for the phase. That keeps the "no gate files remain" assertion from passing vacuously.

## Deviations from Plan

- **The plan's historical-gate claim was corrected, not acted on.** The plan says the 48-14 and 48-16 verify gates both name the old tests. Only 48-16's does; 48-14's fails on a whole-file count instead. See Historical Gates above. No file was changed for this.
- **Extra control assertions** (Rule 2, test correctness). In Task 1, the arm asserts that the Define gate still exists after the kill, and that the gate scan is non-empty before recovery. These are additions within the task's own file. They are not scope changes.

**Total deviations:** 1 factual correction and 1 set of additive assertions. **Impact:** none on scope. No production source was changed.

## What This Does NOT Establish

- **One parked shape only.** The start arms pin the Define preflight gate in auto mode with no `.planning/config.json`. They do not cover other gates `start` can park at, or `start` after it hands off to a detached pipeline.
- **No waiter-lifetime or lost-update guarantee.** A `live` reject wording means a live holder *may* pick up the response. The test proves that pickup happened in this run, not that it always will.
- **Not the `no_holder_after_exit` path.** That path was never observed.
- **One kind of lock mutation.** The mutation control shows the lock-holder assertion catches a `start` that drops its guard at acquisition. It does not cover a `start` that holds the lock but stops polling the gate, or one that releases the lock later.
- **A weak reliability bound.** Each start arm passed 3 times in executor runs (gates plus one standalone run), plus the orchestrator's independent gate re-runs, on an idle host. That says nothing about flakiness under load; the 20 s park and 30 s pickup bounds are the only margin.
- **No container parity.** `scripts/check-in-container.sh` was not run by the executor. That is the orchestrator's step.
- **`pickup_ms` is not a wedge measurement.** See Observations.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Verification gap A (criterion 4) is closed at the test level, pending the orchestrator's re-verification and container parity run.

## Self-Check: PASSED

- FOUND: `crates/devflow-cli/tests/gate_wedge_e2e.rs`
- FOUND: commit `0ec6028` (Task 1), commit `f932cd2` (Task 2)
- FOUND: commit `a213acb` (Task 3, orchestrator; its gate re-run independently: gate_rc=0)
- `git status --short crates/devflow-cli/src/commands.rs` prints nothing

---
*Phase: 48-survivable-state-writes-and-honest-gate-recovery*
*Completed: 2026-09-23*

## Post-review fix (2026-09-23)

The gap-closure code review (48-REVIEW.md WR-06) found that the wedge arm's temp-removal assertion could not fail. No temp existed in the fixture, and the scan covered only `.devflow/`. The claim above that the arm proves temp removal was false until `a33060f`. That commit plants a state temp and a Define gate temp, asserts that they, the state file and lock-93 all exist before recovery, and scans `.devflow/` and `.devflow/gates/` afterwards. Negative control: a temp name the sweep does not remove turns the arm red in either directory.
