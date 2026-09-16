---
phase: 48-survivable-state-writes-and-honest-gate-recovery
plan: 07
subsystem: infra
tags: [rust, serde, checkpoint, preflight, gate, state-machine, unattended]
requires:
  - phase: 48-02
    provides: Fence-aware checkpoint parser and normalized CheckpointDeclaration elements
  - phase: 48-05
    provides: Child-process pipeline-launch test shape and child-output guard
  - phase: 48-06
    provides: PATH-isolated preflight fixtures
provides:
  - devflow_core::state::CheckpointApproval (Unrecorded / Pending / Recorded)
  - devflow_core::state::ApprovedCheckpoint { plan_file, element }
  - CheckpointApproval::unapproved — order-insensitive multiset difference
  - State.checkpoint_approval, serde-defaulted to Unrecorded
  - Checkpoint-set recording at a passing Code preflight, from the execution root
  - Re-scan gate at the resume decision, before relaunch_checkpoint_session
affects: [48-15, 48-08, 48-09, CHKPT-02]
tech-stack:
  added: []
  patterns:
    - Three-variant provenance enum where an Option would conflate "never looked" with "looked and found none"
    - One shared fixture builder behind a control and its gating siblings
key-files:
  created: []
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/pipeline_launch.rs
key-decisions:
  - "Recorded(vec![]) round-trips distinct from Unrecorded; collapsing them would hide every later addition."
  - "An Unrecorded approval is never upgraded by a Code evaluation, so an upgraded run gates at its loop-back too."
  - "The compare lives only in the (Some(session_id), true) arm — the only route that arms auto-decide."
  - "Task 1 parks at the gate and returns Ok(()); response handling is plan 48-15's, not silently pre-empted here."
  - "Element text is stored, not hashed — a hash is unreadable in state.json and unexplainable in a gate message."
patterns-established:
  - "Every new gating test pre-writes an abort gate response, so a regression fails on an assertion instead of blocking on the three-day gate poll."
  - "Recorded sets in fixtures are derived by running 48-02's own parser over the fixture body, never hand-written."
requirements-completed: [CHKPT-02]
plan_head_before: c3fc82236ac3c7e0f2908575c1c14e6024e2bbfa
commits: 2
actuals:
  tokens: 11924
  tasks: 2
  commits: 2
coverage:
  - id: D1
    description: A human-only checkpoint added after Code preflight parks at a Code gate instead of reaching the agent's auto-decide route.
    requirement: CHKPT-02
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::checkpoint_added_after_code_preflight_parks_at_the_rescan_gate -- --exact"
        status: pass
  - id: D2
    description: A rewritten body, an Unrecorded set and a Pending set each park at the re-scan gate; an unchanged recorded set still auto-decides.
    requirement: CHKPT-02
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::{rewritten_body_of_an_approved_checkpoint,unrecorded_checkpoint_set,pending_checkpoint_set}_parks_at_the_rescan_gate and unchanged_recorded_checkpoint_still_auto_decides -- --exact"
        status: pass
  - id: D3
    description: A passing Code preflight records the human-only set from the execution root, and never records from Unrecorded.
    requirement: CHKPT-02
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow preflight::tests::code_preflight_records_a_pending_set_from_the_execution_root and code_preflight_never_records_from_an_unrecorded_approval -- --exact"
        status: pass
  - id: D4
    description: CheckpointApproval::unapproved is an order-insensitive multiset difference over human-only declarations.
    requirement: CHKPT-02
    verification:
      - kind: unit
        ref: "cargo test -p devflow-core --lib state::tests::unapproved_checkpoints_is_an_order_insensitive_multiset_difference -- --exact"
        status: pass
---

# Plan 48-07 — Checkpoint Re-Scan at the Resume Decision

## What This Closes

An agent writes plan files during the Code stage. Before this change, a
`blocking-human` checkpoint **added after** Code's preflight reached the
resume decision's auto-decide route, where the agent was instructed to resolve
its own gate. Nothing a human had ever seen stood between the two. That is the
elevation-of-privilege path CHKPT-02 (backlog 999.126) names.

DevFlow now records the human-only checkpoint set it saw at the Code preflight
and re-scans at the resume decision. New or rewritten declarations park at a
Code gate.

## Accomplishments

1. **`CheckpointApproval` provenance on `State`.** Three variants because
   there are three facts, and an `Option` conflates two of them:
   `Unrecorded` (the serde default — a state file from a binary predating the
   field), `Pending` (set by `State::new`), `Recorded(Vec<ApprovedCheckpoint>)`.
   `Recorded(vec![])` means "we looked and this phase declared none", and it
   round-trips distinct from `Unrecorded` — that distinction is the whole
   mechanism by which a later addition is detectable.
2. **Recording at a passing Code preflight**, read from the execution root
   (`worktree_path` before `project_root`), persisted through `run_preflight`'s
   existing save path. Recorded only from `Pending`; an `Unrecorded` approval
   is never upgraded, so a run whose first Code evaluation predates this binary
   gates at its loop-back rather than blessing whatever the agent wrote first.
3. **The re-scan gate** at the resume decision, immediately before
   `relaunch_checkpoint_session`, fed by a fresh `phase_checkpoint_declarations`
   snapshot re-read at decision time and compared through
   `CheckpointApproval::unapproved`.
4. **`unapproved` as a multiset difference** keyed by `(plan_file, element)`:
   order-insensitive, and one recorded entry is consumed by at most one current
   declaration, so byte-identical tasks in two plans count separately.

## Verification

Both of the plan's `<automated>` gates were run verbatim and printed `gate_rc=0`.

**RED was reproduced before implementation**, satisfying Task 1's acceptance
criterion. `checkpoint_added_after_code_preflight_parks_at_the_rescan_gate`
exited 101 on its intended assertion —
`a checkpoint added after Code preflight must not auto-decide` — with the
`checkpoint_auto_decided` event quoted in the failure, `0 passed; 1 failed`
and 364 filtered out. `checkpoint_approval_defaults_to_unrecorded_for_old_state_files`
exited 101 on its own provenance assertion, `0 passed; 1 failed`, 790 filtered.
Records: `48-07-task1-pipeline-red.json`, `48-07-task1-state-red.json`,
committed in `6e746b8`.

### What the green result does NOT establish

The four mutation runs below are the load-bearing evidence, not the green run.
Every test here passed on its first execution after implementation, which on
its own is equally consistent with a test that cannot fail.

| Mutation | Expected | Observed |
|---|---|---|
| Recording scans `project_root` instead of the execution root | recording test fails | exit 101 on `must record the execution root's human-only set` |
| Recording removed entirely | recording test fails | exit 101, same assertion |
| Re-scan compare disabled | 4 gating tests fail, control still passes | exactly that: 4 × exit 101, control exit 0 |
| Compare over-fires (gates on any declaration) | control fails | exit 101 on `must still auto-decide exactly once` |

Each mutation was reverted and the file confirmed byte-identical afterwards.

The compare-disabled row is the important one: the control passing while the
gating tests fail is what rules out a fixture difference, and all four build
through one `build_rescan_fixture`.

**Not established by any of this:** no live agent run exercised the path. The
gate's *response* behaviour is untested here because it does not exist yet —
48-15 owns it. Whether a real agent's mid-Code plan rewrite produces a
declaration this parser normalizes identically is pinned only by the 19-05
fixture body, not by field evidence.

## Task Commits

1. **Task 1: Tracer — record at Code preflight, compare at the resume decision** — `9714e79`
2. **Task 2: Changed bodies, missing records and the unchanged-plan control** — `9714e79` (same operator-approved orchestrator normal-hook commit)

RED evidence committed separately as `6e746b8`.

## Files Modified

- `crates/devflow-core/src/state.rs` — `ApprovedCheckpoint`, `CheckpointApproval`,
  `unapproved`, the `State` field, `State::new` set to `Pending`, two tests.
- `crates/devflow-cli/src/preflight.rs` — recording at the passing Code
  evaluation, plus two tests with a built-in root-discrimination control.
- `crates/devflow-cli/src/pipeline_launch.rs` — the re-scan gate,
  `rescan_gate_context`, the shared re-scan fixture builder, four new tests,
  and the two C-10 relaunch fixtures.

## Decisions Made

- Element text is stored rather than hashed (48-RESEARCH's rejected
  alternative): a hash is unreadable in `.devflow/state-NN.json` and cannot be
  explained in a gate message.
- The gate fires through `run_gate_with_timeout` with an explicit `None`
  auto-response. That is load-bearing, not a placeholder — this gate exists
  because a human has not seen these declarations, so nothing may pre-authorize it.
- The control test's plan body differs from the recorded body by trailing
  whitespace and a CR, making the control also the normalization test. A
  byte-exact compare would gate there, and gating on whitespace is how this
  feature would become noise an operator learns to click through.

## Deviations from Plan

1. **The plan references a "Task 3" that does not exist.** Its `estimate` says
   `tasks: 2` and it defines two tasks, but contract C-12 ("both run unedited in
   Task 3's gate") and threat T-48-07-02 ("Task 3 test") both cite one. The
   must-have truths that Task 3 would have carried were implemented inside
   Task 1: `preflight::tests::code_preflight_records_a_pending_set_from_the_execution_root`
   and, for T-48-07-02, `code_preflight_never_records_from_an_unrecorded_approval`.
   Without them, step 3 of Task 1 would have shipped with no test at all.
2. **A seventh contract test the plan's C-10 did not name.**
   `pipeline_launch::tests::resume_with_agent_preserves_every_state_field_except_agent_and_monitor_pid`
   is a whole-state tripwire whose own doc comment says it exists to break when
   `State` gains a field. It broke. `checkpoint_approval` is now excluded from
   its whole-state diff **and asserted explicitly** (`pending` → `{"recorded": []}`)
   rather than silently dropped — removing a field from a tripwire without
   replacing its coverage is how such a check quietly stops checking. The write
   is intended: an agent handoff re-enters `launch_stage` for Code, which is a
   Code evaluation, and it is a handoff to a *different* agent.
3. **A Task 2 test was written during Task 1.**
   `state::tests::unapproved_checkpoints_is_an_order_insensitive_multiset_difference`
   covers `unapproved`, which Task 1's own action step 2 implements; leaving it
   to Task 2 would have shipped that function untested through Task 1's gate.
   It runs in Task 2's gate as specified.

## Known Stubs

The re-scan gate's `GateAction` is deliberately discarded: Task 1 parks and
returns `Ok(())`. Approve/reject/abort handling is plan 48-15's scope
(`depends_on: ["48-07"]`), and pre-empting it here would have written the
behaviour its RED tests are supposed to drive.

## Threat Flags

- **T-48-07-01 (mitigated):** fresh-scan compare; new or changed parks.
- **T-48-07-02 (mitigated):** recorded only from `Pending`;
  `code_preflight_never_records_from_an_unrecorded_approval` pins it.
- **T-48-07-03 (mitigated):** serde default is `Unrecorded`, which gates;
  `unrecorded_checkpoint_set_parks_at_the_rescan_gate` pins it.
- **T-48-07-04 (mitigated in part):** recording exists, but the approval path
  that consumes it is 48-15's. Until 48-15 lands, a parked re-scan gate is
  re-evaluated on every resume — by construction, since nothing yet records a
  human's answer. Expected, and the reason 48-15 is a hard dependent.

## Operational Hazard Observed

A parked re-scan gate polls `gate_timeout_secs()` — the three-day production
default. When the two C-10 relaunch fixtures first began parking, they did not
fail, they **hung**, and an unbounded hang cannot be told apart from a wedged
harness. Every gating test added here pre-writes an abort gate response for
that reason, including the control, where it is inert.

## TDD Gate Compliance

`type: tdd`. RED was observed on both tests' intended assertions before any
implementation and is recorded above and in two committed JSON files. GSD's
`tdd-red-evidence` checker was deliberately not re-run: it parses Node TAP only
and reports truthful Cargo output as `INVALID_RED/zero_tests_discovered`. The
operator accepted the Cargo evidence manually; no TAP was fabricated and no
test was weakened to satisfy the checker.

## Pre-Existing Failures (not introduced here)

Both were reproduced on a clean detached worktree at `c3fc822` with a separate
target dir, and both fail there identically:

- `devflow-core`: `doc_check::source_devflow_env_vars_and_subcommands_are_documented`
  — 789 passed / 1 failed at HEAD. `DEVFLOW_TEST_AGENT_FREE_ROOT` is read as a
  string literal in `pipeline_outcomes.rs` (introduced by `47425fa`, plan 48-04)
  and is neither documented nor allowlisted. Every env var added here is read
  through a `const`, which `source_read_env_vars` cannot see, so none of them
  can have caused it.
- `devflow`: `commands::tests::stray_process_finding::doctor_finds_a_real_stray_and_never_signals_it_across_two_runs`
  — 363 passed / 1 failed at HEAD, same single failure. Passes 2/2 in isolation;
  its failure output shows real host tool versions, the signature of the known
  PATH race (999.38). This plan changes zero lines in `commands.rs`.

With this plan's work: `devflow-core` 791 passed / 1 failed (+2 tests),
`devflow` 370 passed / 1 failed (+7 tests) — the same single pre-existing
failure in each. `cargo clippy --workspace --all-targets -- -D warnings` exits 0.

**Update (same session, after operator decision).** The `doc_check` failure was
fixed on this branch — see the `fix(48-04)` commit; the env var is now read
through a `const`, which is the convention every other child-test root var in
that crate already follows. Both suites then go green: `devflow-core` 792/0
(twice) and `devflow` 371/0, clippy 0.

The stray-process failure was NOT fixed and is not claimed to be. It is flaky,
not constant: it failed three consecutive full-suite runs and then passed one.
A sibling in `devflow-core`,
`agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape`,
failed once the same way (`exec visibility timed out`) and passes 3/3 in
isolation. Both are the known `/proc`-visibility and PATH races; a green suite
run is not evidence either is resolved.

## Next Phase Readiness

48-15 can proceed: it needs `CheckpointApproval`, the normalized element text,
the Pending-recording path, and a re-scan gate sited before
`relaunch_checkpoint_session`. All four exist, and its own precondition — that
the fixture can load `CheckpointApproval::Recorded` — is demonstrated by
`recorded_from_body` and `recorded_approval_for`.

TEST-01 remains pending; plans 48-08 and 48-09 still own its remaining work.

## Self-Check: PASSED

Verified in this session: both plan gates print `gate_rc=0`; both RED runs
reproduced at exit 101 on their intended assertions before implementation; four
mutations each failed on the intended assertion and reverted byte-identical;
workspace clippy exits 0; both pre-existing failures reproduce at clean HEAD.
