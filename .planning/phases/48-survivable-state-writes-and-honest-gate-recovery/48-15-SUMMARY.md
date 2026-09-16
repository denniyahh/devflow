---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 15
subsystem: infra
tags: [rust, checkpoint, gate, preflight, state-machine, unattended]
requires:
  - phase: 48-07
    provides: CheckpointApproval, normalized elements, Pending recording, the re-scan gate
provides:
  - Explicit GateAction handling at the re-scan gate (Advance / LoopBack / Abort)
  - Checkpoint-set recording on a human-approved Code preflight refusal
  - record_checkpoint_set_for_code_evaluation — one recorder shared by both sites
  - pipeline_launch::recorded_approval — one definition of "the set"
affects: [48-09, 48-10, 48-11, CHKPT-02]
tech-stack:
  added: []
  patterns:
    - One shared recorder behind every site that writes an approval
    - Opposite-result halves inside one test, so approve and reject share a fixture
key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/preflight.rs
key-decisions:
  - "Advance records the freshly scanned set, never the stale set the gate was opened about."
  - "Advance cleans up the gate; LoopBack deliberately does not, because the fall-through gate asks the same question of the same stage."
  - "The Unrecorded guard lives in the shared recorder, so the refusal-gate path cannot bypass it."
patterns-established:
  - "A test whose first draft passes against the previous implementation is recorded as vacuous and strengthened, not quietly kept."
requirements-completed: [CHKPT-02]
plan_head_before: 3ce5b28
commits: 2
actuals:
  tokens: 9563
  tasks: 2
  commits: 2
coverage:
  - id: D1
    description: Approving the re-scan gate records the freshly scanned set and relaunches; the next resume's compare finds nothing to gate.
    requirement: CHKPT-02
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::approving_the_rescan_gate_records_the_set_and_relaunches -- --exact"
        status: pass
  - id: D2
    description: A non-abort rejection records nothing, does not relaunch, and reaches the never-silent dispatch naming the unapproved plan file; an abort note terminates through pipeline_gate::abort.
    requirement: CHKPT-02
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::rejecting_the_rescan_gate_records_nothing_and_falls_through and rejecting_the_rescan_gate_with_abort_aborts -- --exact"
        status: pass
  - id: D3
    description: A human-approved Code preflight refusal records the execution-root set; LoopBack records nothing; an Unrecorded set is never upgraded, even by an approval.
    requirement: CHKPT-02
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow preflight::tests::approving_the_preflight_refusal_gate_records_the_set and code_preflight_does_not_record_an_unrecorded_set -- --exact"
        status: pass
---

# Plan 48-15 — CHKPT-02's Human Response Paths

## What This Closes

48-07 made a new or changed checkpoint park at a gate, then threw the answer
away. The gate reopened on every resume, which is how a safety gate becomes
noise an operator clicks through. All three answers now mean something, and
the approved set widens only when a human says so.

## Accomplishments

1. **Re-scan gate responses.** `Advance` records the freshly SCANNED set (not
   the stale one the gate was opened about), cleans up the gate so a later Code
   gate cannot consume the approval as its own answer, saves, and relaunches.
   `LoopBack` records nothing and falls through to the unchanged
   non-auto-decide dispatch with the unapproved plan files named in its reason.
   `Abort` routes through `pipeline_gate::abort`.
2. **Preflight refusal recording.** In Auto, a Code launch declaring a
   human-only checkpoint is refused, so 48-07's pass path can only ever record
   an EMPTY set. This gate is where a non-empty set becomes legitimate.
3. **One recorder.** `record_checkpoint_set_for_code_evaluation` and
   `pipeline_launch::recorded_approval` are now the single definition of "the
   set", shared by the pass path, the refusal-gate approval and the re-scan
   approval. A divergence between them would gate forever or approve too much,
   and neither is visible from a passing test of any one site.

## Verification

Both `<automated>` gates run verbatim: `gate_rc=0`. Workspace clippy exits 0.
`devflow-core` 792 passed / 0 failed; `devflow` 375 passed / 0 failed.

RED was observed on all three Task 1 tests and on Task 2's before implementing
(exit 101, `0 passed; 1 failed`), recorded in `48-15-task1-red.json`.

### The measurements that actually carry the result

Green runs are not the evidence; these are.

| Mutation | Expected | Observed |
|---|---|---|
| LoopBack also records at the re-scan gate | rejection test fails | exit 101, `a rejection must leave the approved set exactly as it was` |
| Advance records the STALE set | approving test fails | exit 101, `must record the freshly scanned set` |
| Preflight records from `project_root` | refusal test fails | exit 101, `must record the EXECUTION-root set` |
| LoopBack also records at the refusal gate | refusal test fails | exit 101, `a LoopBack answer is not approval` |
| Drop the `Unrecorded` guard | Unrecorded test fails | **passed first — see below** |

**A negative control caught a hole in my own test.** Dropping the `Unrecorded`
guard did NOT fail `code_preflight_does_not_record_an_unrecorded_set`. The pass
path is wrapped in its own `== Pending` condition, so the guard inside the
recorder was dead code there and the test could not see it. The refusal-gate
Advance arm has no such outer condition — the guard is the only thing standing
between an older run and a blessed set — and that path was untested. A second
half was added covering it; the mutation now fails with `not even an APPROVED
refusal gate may upgrade an Unrecorded set`. Without the control this would
have shipped as a passing test over an unguarded path.

**One test was vacuous on its first draft and is recorded as such.**
`rejecting_the_rescan_gate_records_nothing_and_falls_through` initially passed
against 48-07's code, because asserting "some gate context names the plan file"
is satisfied by 48-07's own re-scan gate context. It was strengthened to
require a `[never-silent]` context carrying an augmented unresolved-checkpoint
reason, which only the fall-through dispatch produces.

### What this does NOT establish

- Still no live agent run. Unit-tested only.
- **The durability claim is a composition, not one end-to-end test.** This plan
  asserts that approval produces a set for which the next resume's compare is
  empty; 48-07's `unchanged_recorded_checkpoint_still_auto_decides` asserts that
  such a set auto-decides with no gate. A second real `advance()` cannot join
  them in this fixture: the relaunch spawns the stub agent, whose stdout
  REPLACES the confirmed-checkpoint capture, so the next advance classifies a
  different result and never reaches the re-scan route. Measured, not assumed.
- The rejection test asserts `>= 2` Code gates, not an exact count: the
  LoopBack relaunch's background monitor fires gates on its own schedule.

## Task Commits

1. **Task 1: re-scan gate responses** — `5d55e57`
2. **Task 2: preflight refusal recording** — `5d55e57` (same operator-approved orchestrator normal-hook commit)

RED evidence: `1b56e5f`.

## Deviations from Plan

1. **Two test names reconciled with 48-07.** The plan's Task 2 gate names
   `code_preflight_does_not_record_an_unrecorded_set` and
   `code_preflight_records_a_pending_set_from_the_execution_root`. 48-07 had
   already written the first under a different name
   (`..._never_records_from_an_unrecorded_approval`); it was renamed to the
   plan's contract name so the gate is runnable as written.
2. **The rejection test runs in Supervise, not Auto.** The rejection path ends
   in `handle_stage_failure`'s LoopBack arm, which calls `launch_stage` ->
   `run_preflight`. In Auto, `preflight_unattended_launch_check` refuses that
   launch precisely BECAUSE the plan declares a human-only checkpoint, so a
   third gate opens with no response left and the test blocks on the three-day
   poll instead of failing. Measured: the Auto run printed `[DOES NOT HOLD] no
   plan declares a human-only checkpoint` and spawned zero monitors. The route
   under test has no mode check (D-07) and nothing in the rejection path reads
   `mode`.
3. **The shared fixture builder gained a `mode` parameter and a recorded
   canary.** Without the canary, any path reaching `launch_stage` refuses
   because the stub agent cannot return a notification token — observed as a
   real failure, not anticipated. A run that has launched Code once has a
   recorded canary, so this makes the fixture more faithful, not less.

## Known Stubs

None. CHKPT-02's recording, comparison and response paths are complete.

## Threat Flags

- **T-48-15-01 (mitigated):** only `Advance` records, from a fresh scan, before
  relaunch. Two mutations pin it.
- **T-48-15-02 (mitigated):** recording reads the execution root; the fixture's
  project root holds a decoy so the wrong root yields an empty set, and that
  mutation fails.
- **T-48-15-03 (mitigated):** both `LoopBack` arms record nothing; both
  mutations that make them record fail.

## TDD Gate Compliance

`type: tdd`. RED observed on all four new tests before implementation and
recorded in `48-15-task1-red.json`, including the note that one of them was
vacuous on its first draft. GSD's `tdd-red-evidence` checker was not re-run —
it parses Node TAP only, per the operator-accepted workaround. No TAP was
fabricated and no test was weakened.

## Next Phase Readiness

CHKPT-02 is complete across 48-07 and 48-15. Remaining Phase 48 work is
unblocked and independent: 48-08 (`pipeline_outcomes.rs`, TEST-01) shares no
files with either plan.

Two known-flaky tests remain, unrelated to this work and not fixed here:
`agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape`
(`/proc` visibility) and
`commands::tests::stray_process_finding::doctor_finds_a_real_stray_and_never_signals_it_across_two_runs`
(PATH race, 999.38). Both passed in this session's final runs; that is luck on a
race, not a resolution.

## Self-Check: PASSED

Verified in this session: both gates print `gate_rc=0`; RED observed on every
new test before implementation; five mutations run, four failing on their
intended assertion and the fifth exposing a real gap that was then closed and
re-verified; clippy 0; both suites fully green.
