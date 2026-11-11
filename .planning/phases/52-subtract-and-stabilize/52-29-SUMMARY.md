---
phase: 52-subtract-and-stabilize
plan: 29
subsystem: validate-routing
tags: [rust, devflow-cli, devflow-core, gsd, validate, gates, finish, proptest, tdd, semver-break]

requires:
  - phase: 52-08
    provides: "finish_run, the --until halt's workflow_finished, the safe-point-6 stop before a finish"
  - phase: 52-22
    provides: "a GateKind on every gate, validate_route's labels, State.validate_pass_head, the GSD stub seams"
  - phase: 52-26
    provides: "State.relaunch_note and its write before a stage-failure relaunch; code_fix_note"
  - phase: 52-27
    provides: "the gap loop and the gaps Plan gates (no_gap_closure_plan, reader_failure at Plan)"
  - phase: 52-28
    provides: "FixType::StaleReverify, record_validate_failure as the one streak update, the counted Plan-pass gate"
provides:
  - "can_finish and FinishPermit (private field, made only by can_finish); finish_run takes the permit by value"
  - "next_step(Trigger) -> Step, the D-45 table and the only routing function"
  - "Evidence, read_evidence and its pure part evidence_from_reads, with the D-46 dirty-tree read read_tree"
  - "answer_family, status_less_step, approve_route, reject_route, screen_answer and the one dispatch_answer"
  - "FixType::Quick: rejecting a verification gate runs /gsd-quick with the note, then Validate"
  - "GateFile.gsd_status and GateLabel; GsdStatus gains Serialize/Deserialize"
  - "gate_resolved records gate_kind, route, gsd_status and agent_pass_on_head; a new gate_answer_refused event"
  - "the workflow_finished emission scan, and the rule-6 proptest with its mutant control"
  - "operator decision: a permitted finish wins over a late stop (ShutdownOutcome::AlreadyFinished)"
affects: [52-31, 52-32, 52-33]

actuals:
  tokens: 77257
  tasks: 3
  commits: 9
plan_head_before: 87c8adcab5c33c201e722ba0e052c338ab28a6ae
plan_head_after: e36666a810fa20b2413857f495aa3aa13f317bdf

tech-stack:
  added: ["proptest 1.12.0 (dev-dependency of devflow)"]
  patterns:
    - "A capability token (FinishPermit) in its own module, so only one function in the crate can make it"
    - "Answers are routed after the claim and before gate_resolved, so the event names the route that runs"
    - "A property over a routing function, run against the real dispatch and a mutant copy"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-29-SUMMARY.md
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs
    - crates/devflow-cli/Cargo.toml
    - Cargo.lock
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/gsd_reader.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/supervisor.rs
    - crates/devflow-core/src/supervisor/runtime.rs
    - crates/devflow-core/src/supervisor/protocol.rs
    - OPERATIONS.md
    - ARCHITECTURE.md

key-decisions:
  - "Operator decision 2026-11-11: a permitted finish wins over a late stop. finish_run no longer stops at safe point 6; the supervisor answers the pending stop already_finished; a stop without a finish permit keeps its handling"
  - "After a Validate, every route comes from next_step's table, except that a failure_cap label (the streak update's own gate, D-12) opens ahead of it. So an ambiguous verdict in auto mode over gaps_found, stale or missing now takes the gap loop or the re-verify pass instead of an ambiguous gate"
  - "Evidence keeps the whole read (Result<StatusReport, EvidenceFailure>), not just a status, so the re-verify pass can still quote GSD's route and next action"
  - "Names: the pure part is evidence_from_reads, the answer screen is screen_answer, the quick arm is FixType::Quick, the one dispatch is dispatch_answer"
  - "relaunch_failed_stage is gone. The stage-failure routers choose the relaunch-refused step from canary_refusal; a verification step that relaunches a stage (Validate again, the gaps Plan pass again) keeps the canary guard through relaunch_or_refuse"

patterns-established:
  - "A refused answer is never claimed: the gate reopens with the label it was opened with, and the wait starts again"

requirements-completed: [GATE-02, GATE-01, GATE-03]

coverage:
  - id: D1
    description: "A finish needs GSD's fresh passed and a Validate PASS on the current HEAD over a clean tree. A commit, or an uncommitted file, after the Validate makes the approve run Validate again. A Validate over a dirty tree records no PASS, so cleaning the tree does not bring one back"
    requirement: GATE-03
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#approving_a_clean_pass_after_a_new_commit_runs_validate_again"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#approving_a_clean_pass_over_an_uncommitted_change_runs_validate_again"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::can_finish_needs_passed_and_a_pass_on_the_current_head"
        status: pass
    human_judgment: false
  - id: D2
    description: "One status read per Validate outcome picks the label, the count and the route; an approve reads fresh; a supervise stale gate's approve follows the fresh read"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_validate_outcome_reads_gsd_status_once"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_supervise_stale_validate_gates_and_approve_reverifies"
        status: pass
    human_judgment: false
  - id: D3
    description: "The chain-flag guard of an auto Code stage leaves .planning/config.json byte-identical (no key, false in 4-space indent) and keeps an edit made while engaged"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::an_auto_code_stage_leaves_the_gsd_config_as_it_found_it"
        status: pass
    human_judgment: false
  - id: D4
    description: "Only finish_run and the --until halt emit workflow_finished (source scan, with planted controls)"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::workflow_finished_is_emitted_only_by_finish_run_and_the_until_halt"
        status: pass
    human_judgment: false
  - id: D5
    description: "Rejecting any verification gate runs /gsd-quick with the note, then Validate; a stopped quick pass resumes with its note"
    requirement: GATE-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#rejecting_a_clean_validate_pass_runs_gsd_quick_with_the_note"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::a_stop_keeps_the_recorded_fix_pass"
        status: pass
    human_judgment: false
  - id: D6
    description: "Every answer on every gate kind (and on a kindless file) takes the route the routers name. Refused answers are never claimed. A stage-failure reject keeps its note in relaunch_note and leaves the fix pass alone"
    requirement: GATE-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::the_reject_path_takes_the_route_reject_route_names"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::the_approve_path_takes_the_route_approve_route_names"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rejected_stage_failure_relaunches_the_stage_with_the_note"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#a_rejection_note_reaches_the_stream_prompt_verbatim"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#a_rejection_note_reaches_the_capture_prompt_verbatim"
        status: pass
    human_judgment: false
  - id: D7
    description: "Each verification gate records the GSD status it was opened over, as a label"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::a_gate_round_trips_its_gsd_status"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::each_verification_gate_records_the_status_that_chose_it"
        status: pass
    human_judgment: false
  - id: D8
    description: "D-45 rule 6: the routing finishes only when can_finish holds and follows the table; the forced-finish mutant fails the property. A failure-cap approve resets once and then follows the table. A Code stage failure retries without reading GSD"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::workflow_finished_is_reached_only_when_can_finish_holds"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::approving_a_failure_cap_gate_resets_the_cap_and_follows_the_table"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::approving_a_code_stage_failure_over_an_old_passed_report_retries_code"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_counted_gate_at_the_phase_ceiling_resets_the_count_when_answered"
        status: pass
    human_judgment: false
  - id: D9
    description: "Operator decision: a late stop loses to a permitted finish and is told already_finished; without a permit the stop still stops"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_stop_after_a_gate_answer_loses_to_a_permitted_finish"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/supervisor.rs#supervisor::tests::request_shutdown_reads_accepted_then_stopped"
        status: pass
    human_judgment: false
  - id: D10
    description: "A real agent run through /gsd-quick, and a real GSD install answering these reads"
    verification: []
    human_judgment: true
    rationale: "Every test uses a stub gsd_run and a fake agent. No real /gsd-quick pass and no real GSD status read has gone through this routing"

duration: 86min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 29: One finish check, one routing table, one answer dispatch Summary

**A run now finishes only through `can_finish`. It needs a fresh GSD `passed` and the run's Validate agent PASS recorded on the evidence root's current HEAD, over a tree that `git status --porcelain --untracked-files=all` finds clean (D-45 rule 1, D-46). `finish_run` takes the `FinishPermit` that only `can_finish` makes. Every route after a Validate, and after an approve of any verification gate, comes from one table, `next_step`, and that table reads the fresh status, never the gate's kind. Rejecting a verification gate runs `/gsd-quick` with the operator's note. The status-less gates keep their own answers. An answer the routers refuse is never claimed: the gate reopens never-silently. Per the operator's decision, a permitted finish now wins over a late `devflow stop`.**

## Performance

- **Duration:** about 86 min
- **Started:** 2026-11-11T11:20:52Z
- **Completed:** 2026-11-11T12:47:00Z
- **Tasks:** 3, plus the operator-decision TDD pair
- **Files modified:** 16

## Accomplishments

- **One finish check (Task 1).**
  - `can_finish(&Evidence) -> Option<FinishPermit>` sits in its own `finish_permit` module. `FinishPermit` has one private field, so no other code in the crate can build one.
  - The three finish sites (an auto pass, a gate approve, an ambiguous approve) now go through `Step::Finish(permit)`. A source scan pins `finish_run` and the `--until` halt as the only two emitters of `workflow_finished`.
- **The D-45 table (Task 1).** `next_step(Trigger) -> Step` holds the whole table:
  - after a Validate, auto mode: finish, gate, gap loop or re-verify;
  - after a Validate, supervise mode: a gate on every row;
  - an approve: finish, Validate again, gap loop, re-verify, or reopen;
  - a reject: quick;
  - an abort: end.

  It has no wildcard arm and no `GateKind` parameter. 52-22's labels and the failure-cap gate stay ahead of it, so the label, the count and the route all come from the outcome's one read.
- **D-46 (Task 1).** At a Validate outcome the one dirty-tree read runs after the HEAD read and before the PASS is recorded.
  - A PASS over a dirty tree, or one whose tree read failed, writes `validate_pass_head = None`.
  - The gate names the dirty paths in a `[never-silent]` line.
  - At an approve, `read_evidence` reads the status, then the tree, then HEAD.
- **AutoChainGuard (Task 1, V3).**
  - The guard keeps the bytes it found and the bytes it wrote. On drop it writes the earlier bytes back (temp file, then rename), but only while the file still holds the guard's own write.
  - It never keeps bytes whose flag reads `true`, so a leaked `true` that an ineligible launch clears stays cleared (F-3).
  - An edit made to the file during the stage is kept.
- **One dispatch by family (Task 2).**
  - `answer_family` maps the ten verification kinds to one family.
  - `approve_route`, `reject_route` and `status_less_step` match the family with no wildcard arm.
  - `dispatch_answer` runs every routed answer for Validate gates, gaps Plan gates and stage-failure gates. It is also the only caller of `reset_phase_failures_at_ceiling`.
  - The preflight and checkpoint gates run their own approve step: the D-18f override, and recording the scanned checkpoint set.
- **The answer screen (Task 2, R10).** `screen_answer` runs in `open_and_await` before the claim, the `gate_pending` write and `gate_resolved`. It reads only the gate file on disk and the answer.
  - A refused answer is never claimed. It is set aside by reopening the gate with the label the supervisor opened it with.
  - The refusal emits `gate_answer_refused`, and the wait starts again.
- **The quick route (Task 2, GATE-02).** `FixType::Quick` runs `/gsd-quick` (claude style) or `quick.md` (workflow style), with the note block as the task. The loop-back clears the gap marker.
- **The rule-6 property (Task 3).** It runs over status, recorded and current HEAD, the tree, the trigger (with an arbitrary note), the kind and the canary, through a `TestRunner` with `failure_persistence: None`.
  - The real routing holds.
  - The forced-finish mutant fails, on a counterexample with `passed` and no PASS on HEAD.

## Operator decision: finish wins over a late stop

Operator's answer, 2026-11-11: when the finish is permitted, the run is recorded finished even if a stop request is pending.

- `finish_run` no longer checks for a pending stop at safe point 6.
- After an advance that recorded a finish, `run_monitor` marks the stop token `Finished`. `Supervisor::finish` then answers the parked stop `{"ok":false,"error":"already_finished"}`.
- `request_shutdown` maps that answer to the new `ShutdownOutcome::AlreadyFinished`. `devflow stop` prints "already finished (Validate passed) before the stop could act; nothing was stopped" and writes nothing over the finished state.
- A stop that lands where no finish is permitted keeps its handling: it is honoured at the next advance safe point.

It was built as its own TDD pair:

- `9721a5f` test: 52-08/52-22's safe-point-6 test was rewritten as `pipeline_launch::tests::a_stop_after_a_gate_answer_loses_to_a_permitted_finish`. Its no-permit control (a fresh `gaps_found`) still stops. A row was added to `supervisor::tests::request_shutdown_reads_accepted_then_stopped`.
- `e5fc8bc` feat.

OPERATIONS.md ("How a run ends") and ARCHITECTURE.md describe the new rule.

## Task Commits

1. **Task 1 (tracer):** `466ee13` test (RED, with the `can_finish` skeleton), `512069a` feat (GREEN)
2. **Task 2:** `433da7e` test (RED, with skeletons: GateLabel, FixType::Quick, the pure routers), `2662d55` feat (GREEN)
3. **Task 3:** `7983ee1` test (proptest dependency and three tests, all passing on arrival), `45b33ea` test (boxes the runner's error so clippy passes)
4. **Operator decision:** `9721a5f` test (RED), `e5fc8bc` feat (GREEN)
5. **Docs:** `e36666a` (two stale doc links to `relaunch_failed_stage`)

The commit scope is `52-29`, because the TDD gate validator greps `feat(52-29)`. Every subject is 72 characters or fewer.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch `--profile red` config. Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `supervisor_e2e::approving_a_clean_pass_after_a_new_commit_runs_validate_again` | `RED_EVIDENCE_OK` | The first approve finished the run, after the new commit. |
| `supervisor_e2e::approving_a_clean_pass_over_an_uncommitted_change_runs_validate_again` | `RED_EVIDENCE_OK` | The first approve finished the run, over the untracked file. |
| `pipeline_outcomes::tests::a_supervise_stale_validate_gates_and_approve_reverifies` | `RED_EVIDENCE_OK` | The control's approve over a fresh `passed` looped back with `StaleReverify` (the label's route), instead of finishing. |
| `pipeline_outcomes::tests::can_finish_needs_passed_and_a_pass_on_the_current_head` | `RED_EVIDENCE_OK` | The skeleton read no tree, so the untracked row got a permit. |
| `pipeline_outcomes::tests::a_validate_outcome_reads_gsd_status_once` | `RED_EVIDENCE_OK` | The approve control made no fresh read: 1 read, expected 2. |
| `pipeline_launch::tests::an_auto_code_stage_leaves_the_gsd_config_as_it_found_it` | `RED_EVIDENCE_OK` | No-key row: the drop wrote `_auto_chain_active: false`. A one-off run with the row order swapped showed the 4-space `false` row also RED (it was rewritten in 2-space form). The third row passed alone before and after the fix. Run once against an unconditional-restore variant, the third row failed ("the edit made while engaged is kept"); the variant was discarded. |
| `pipeline_gate::tests::workflow_finished_is_emitted_only_by_finish_run_and_the_until_halt` | `INVALID_RED` / `no_target_test_failure` | Passed on arrival, as the plan expected after 52-08 and 52-13. Its planted controls (a third emission, a constant, an empty walk) are what make it discriminate, and they are asserted in the test. |
| `supervisor_e2e::rejecting_a_clean_validate_pass_runs_gsd_quick_with_the_note` | `RED_EVIDENCE_OK` | No Code prompt held `/gsd-quick`: the reject looped back with the full execute. |
| `gates::tests::a_gate_round_trips_its_gsd_status` | `RED_EVIDENCE_OK` | No `gsd_status` key was written. |
| `pipeline_outcomes::tests::each_verification_gate_records_the_status_that_chose_it` | `RED_EVIDENCE_OK` | The `clean_pass` gate had no `gsd_status`. |
| `pipeline_outcomes::tests::the_reject_path_takes_the_route_reject_route_names` | `RED_EVIDENCE_OK` | The `clean_pass` reject set `code_fix` to `FullExecute`, not `Quick`. |
| `pipeline_outcomes::tests::the_approve_path_takes_the_route_approve_route_names` | `RED_EVIDENCE_OK` | The verification rows already held after Task 1. The canary-refused stage-failure approve took `retry_stage`, not `relaunch_refused`. |
| `commands::tests::a_stop_keeps_the_recorded_fix_pass` (`Quick` row) | `RED_EVIDENCE_OK` | The skeleton `Quick` rendered the full execute. |
| `pipeline_launch::tests::a_rejected_stage_failure_relaunches_the_stage_with_the_note` (`Quick` row) | `RED_EVIDENCE_OK` | The relaunch had no `/gsd-quick`. |
| `pipeline_outcomes::tests::workflow_finished_is_reached_only_when_can_finish_holds`, `approving_a_failure_cap_gate_resets_the_cap_and_follows_the_table`, `approving_a_code_stage_failure_over_an_old_passed_report_retries_code` | not RED | All three passed on arrival, as the plan said they might: Tasks 1 and 2 built the routing they pin. The property's mutant control fails it in the same test. |
| `pipeline_launch::tests::a_stop_after_a_gate_answer_loses_to_a_permitted_finish` | `RED_EVIDENCE_OK` | Safe point 6 stopped the run (`stopped: true`, exit 143) instead of the finish. |
| `supervisor::tests::request_shutdown_reads_accepted_then_stopped` | `RED_EVIDENCE_OK` | `already_finished` read as `Refused("already_finished")`. |

Gate sequence:

- `test` 466ee13 → `feat` 512069a
- `test` 433da7e → `feat` 2662d55
- `test` 7983ee1, then `test` 45b33ea; no feat, since nothing was RED
- `test` 9721a5f → `feat` e5fc8bc

There is no REFACTOR commit. The RED commits carry skeletons that compile (52-28's pattern). The Task 2 skeleton includes the pure routers, unwired, so their tests compiled. They were wired at GREEN.

Tracer gate: the run was interactive, in `end-of-phase` mode, and Task 1's verify is automated only. Its seven-command chain was re-run after GREEN and exited 0, so expansion continued.

## Gate writers: the kind and the status each records

| Writer | Kind | `gsd_status` |
|---|---|---|
| Validate outcome, agent pass over `passed` (supervise, or auto when the PASS does not count) | `clean_pass` | `passed` |
| Validate outcome over `gaps_found` (supervise) | `gaps_found` | `gaps_found` |
| Validate outcome over `stale` / `missing` (supervise) | `reverify_needed` | `stale` / `missing` |
| agent pass over `human_needed` | `human_needed` | `human_needed` |
| agent fail over `passed` / `human_needed` | `disagreement_over_passed` / `disagreement_over_human_needed` | `passed` / `human_needed` |
| ambiguous verdict that gates | `ambiguous_verdict` | the status read |
| a counter at its cap (over `passed`, `gaps_found`, `stale`, `missing`) | `failure_cap` | the status read |
| a reader failure, or an unreadable HEAD | `reader_failure` | none |
| 52-27 gaps Plan, no new gap plan | `no_gap_closure_plan` | `gaps_found` |
| 52-27 gaps Plan post-check failed | `reader_failure` | none |
| approve over `human_needed` / a reader failure (reopen) | `human_needed` / `reader_failure` | `human_needed` / none |
| a refused answer (reopen) | the label the supervisor opened the gate with | likewise |
| stage failure, preflight, checkpoint declaration | `stage_failure`, `preflight`, `checkpoint_declaration` | none |

## Each answer's route, per family

| Family | Approve | Reject | Abort |
|---|---|---|---|
| Verification (10 kinds) | fresh read through `next_step`: `passed` with the PASS on HEAD → `finish`; `passed` without it → `validate_again`; `gaps_found` → `gap_loop`; `stale` / `missing` → `reverify`; `human_needed` / reader failure → `reopen` | a note → `quick` (`/gsd-quick`); an empty note → refused | `end` |
| StageFailure | `retry_stage`; `relaunch_refused` when the recorded canary refuses (no note written) | a note → `relaunch_with_note` (writes `relaunch_note`); `relaunch_refused` when the canary refuses (keeps the note); an empty note → refused | `end` |
| Preflight | `preflight_override` (D-18f) | refused | `end` |
| CheckpointDeclaration | `record_checkpoint_set` | refused | `end` |
| Unclassified (no kind) | refused | refused | `end` |

A status-less approve reads no GSD status. A refused answer reopens the gate, naming the answers that apply.

## Existing tests the action named

- `preflight::tests::run_preflight_loopback_gate_launches_agent_exactly_once`: deleted.
- `preflight::tests::run_preflight_loopback_bounds_recursion`: rewritten as `preflight::tests::run_preflight_at_the_retry_ceiling_aborts_before_opening_a_gate`. It now has no gate, asserts the ceiling event and then the abort separately, and no longer sets `DEVFLOW_GATE_TIMEOUT_SECS`. `run_preflight`'s doc comment was updated.
- `preflight::tests::approving_the_preflight_refusal_gate_records_the_set`: the approve half is kept. The reject half is now a refused reject: not claimed, `checkpoint_approval` still `Pending`, and the reopened gate aborted by a bounded writer thread.
- `pipeline_launch::tests::auto_rescan_rejection_parks_for_supervised_repair`: deleted, along with `park_auto_rescan_repair` and `unapproved_plan_files` (both orphaned). The `checkpoint_repair_parked` event is no longer emitted. No doc named it.
- `pipeline_launch::tests::rejecting_the_rescan_gate_leaves_the_follow_on_gate_waiting_for_its_own_answer`: rewritten as `pipeline_launch::tests::rejecting_the_rescan_gate_is_refused_and_widens_nothing`. The plan file is still named in the reopened gate, and a bounded writer aborts it.
- `ambiguous_gate_loop_back_respects_the_mid_arc_check`, `failure_gate_loop_back_respects_the_mid_arc_check`: already gone (52-22 deleted them).
- The eight `LOOP_BACK_RESPONSE` tests: unchanged. None asserted the loop-back's fix, and all pass on the quick route.
- 52-26's `gate_wedge_e2e` pair and `a_rejected_stage_failure_relaunches_the_stage_with_the_note`: unchanged and passing. The `Quick` row was added.
- 52-27's `a_supervise_gaps_round_counts_once_and_its_approve_counts_nothing`: its stub gained a third `gaps_found` reply. The plan said the stub "answers `gaps_found` at every read", but it scripted only two replies, and the approve's fresh read took the second. Its assertions are unchanged.
- 52-27's `approving_a_gaps_gate_runs_the_gap_loop`: rewritten to fresh-read stubs, name kept. Its control now approves a `gaps_found` gate over a fresh `human_needed` and gets a reopened `human_needed` gate, with no Plan move.
- 52-28's `a_counted_gate_at_the_phase_ceiling_resets_the_count_when_answered`: the fixture was changed as planned. A `human_needed` fresh read reopens the gate, and a bounded writer aborts it. The counts are read from the in-memory state and the reset from the events.

No test this plan wrote or rewrote waits on a gate timeout to end. One check found no `timed out` in the reject parity run's output.

## Other tests changed

- `pipeline_outcomes::tests::approving_an_ambiguous_validate_gate_finishes_the_run` was renamed `approving_an_ambiguous_validate_gate_runs_validate_again`. An ambiguous verdict records no PASS, so D-45 rule 1 refuses the finish 52-08 allowed; 52-22 noted that `can_finish` would decide this case.
- Fixtures that now hold a clean tree (D-46):
  - `verifier_tail_report` commits the report;
  - `HandoffRun::launch_with` commits its ROADMAP and excludes the stub agent's signal files (`go`, `go2`, `first-done`, `descendant`) through `.git/info/exclude`.
- Fixtures that gained a stub reply for the approve's fresh read:
  - `approving_a_passed_validate_gate_finishes_the_run`;
  - the `a_stale` / `a_missing_report_routes_to_execute_phase` helper;
  - `supervise_gaps_state` (four replies);
  - the safe-point-6 handoff test.
- `a_verifier_written_report_from_code_routes_by_gsd_status`: its supervise reject row now asserts `Quick`.
- Callers that went from `GateAction` to the routed `Answer`, with no behaviour change: `spawn_code_gate_waiter` and four other `pipeline_gate`/`commands` tests. `finish_run`'s test gets its permit through `can_finish`. `pre_fix_prompt_builder` gained an `unreachable!` `Quick` arm.

## Decisions Made

See `key-decisions`. Three need a note here:

- **An ambiguous verdict in auto mode follows the table.** The D-45 table routes on status and the PASS on HEAD only. So an ambiguous verdict over `gaps_found`, `stale` or `missing` in auto mode now takes the gap loop or the re-verify pass. Over `passed` it gates, because no PASS was recorded. Before, it always gated. The plan names only the label and the failure-cap gate as running ahead of the table, so nothing else does.
- **`finish_run` closes the gate of `state.stage`.** It used to close Validate's only. A `no_gap_closure_plan` approve over a fresh `passed` with the PASS on HEAD finishes at Plan.
- **Canary guard on verification relaunches.** `relaunch_or_refuse` keeps today's guard for Validate again and the gaps Plan pass again. Without it, a refused canary would end the supervisor silently (999.187).

## Open operator decision: untouched

- `phase_validate_failures` still carries over a finished state into the next `start`; `start` was not changed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] A guard restore that would have revived a leaked `true`**
- **Found during:** Task 1
- **Issue:** The plan's "restore the bytes it read" would put back a leaked `true` after an ineligible launch's `engage(false)`, which defeats F-3.
- **Fix:** Bytes whose flag reads `true` are never kept. The doc comment says so.
- **Commit:** `512069a`

**2. [Rule 3 - Blocking] D-46 dirty fixtures and missing stub replies**
- **Issue:** Several fixtures left untracked files in the evidence root, or scripted one GSD reply where the approve now reads a second time.
- **Fix:** Listed under "Other tests changed". No check was weakened.
- **Commits:** `512069a`, `2662d55`

**3. [Process] A commit made without gating on clippy**
- **Issue:** `7983ee1` was committed in a command chain where clippy's failure (`result_large_err`) did not stop the commit.
- **Fix:** `45b33ea`. History was not rewritten.

**4. [Scope] Evidence's shape and the claim probe**
- `Evidence.read` is the whole read (`ValidateRead`), not `Result<GsdStatus, ReaderFailure>`, so the re-verify pass can quote GSD.
- The parity tests prove "not claimed" by the missing `.ack.json` claim file, not by `CLAIM_PROBE`, whose 30 s hold waits for a second consumer.
- The preflight and checkpoint approve rows test the routers. Their executors stay pinned by the existing preflight and re-scan tests.

**5. Files beyond `files_modified`:** `crates/devflow-core/src/supervisor.rs`, `supervisor/runtime.rs`, `supervisor/protocol.rs`, `OPERATIONS.md` and `ARCHITECTURE.md` (all for the operator decision).

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking), 1 process note, 1 scope note, 1 file-list note.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<verify>` chain (7 named runs); re-run as the tracer gate | 0 |
| Task 2 `<verify>` chain (11 named runs) | 0 |
| Task 3 `<verify>` (3 named runs) and `scripts/check.sh deps` | 0, 0 |
| `cargo nextest run --workspace` (bounded profile) after Task 1 GREEN | 0 (2019 passed) |
| `cargo clippy --workspace --all-targets -- -D warnings`, after each GREEN | 0 |
| `scripts/check.sh all` after the operator GREEN | 0 (2027 passed, 0 failed across its `test result` lines) |
| `scripts/check.sh all` after the last change (`e36666a`, doc comments) | 0 (2027 passed, 0 failed) |
| `cargo semver-checks check-release -p devflow-core` | 100, expected |

semver-checks: this plan adds `constructible_struct_adds_field` for `GateFile.gsd_status`. The `GateLabel` parameter change on `open_gate`, `open_registered` and `write_gate` falls under those functions' existing 52-22 finding. `FixType::Quick` and `ShutdownOutcome::AlreadyFinished` are not flagged, because both enums are `#[non_exhaustive]`. 4.0.0 covers all of these, and the version was not bumped.

`Cargo.lock` gained proptest 1.12.0 and 22 crates: autocfg, bit-set, bit-vec, borsh, borsh-derive, bytes, cfg_aliases, chacha20, core_detect, cpufeatures, fnv, num-traits, proc-macro-crate, quick-error, rand, rand_core, rand_xorshift, rusty-fork, syn 3.0.10, toml_edit 0.25.13, unarray, wait-timeout. `cargo deny` (advisories, bans, licenses, sources) and `cargo machete` passed. After the property runs, `git status --porcelain --untracked-files=all` listed no `proptest-regressions` path.

The known flakes 999.221, 999.222 and 999.223 did not occur in any full run. `test_support::plant_gsd_run_stub` was not touched.

**What these checks do not establish:**
- No real agent has run the `/gsd-quick` prompt, and no real GSD install has answered these reads (D10). Every status comes from the stub.
- The property covers the routing functions and the answer routers over every input class. It does not cover the I/O inside `read_evidence`, which the `can_finish` unit test and the two e2e tests pin.
- The dirty-tree read is one call when the evidence is built. An edit made after it, and before the finish is written, is not seen.
- `FinishPermit` is a guarantee only inside the `devflow` crate, by module privacy.
- The emission scan pins where the event literal is written, not every runtime path to `finish_run`.
- The finish-wins control is a single row: a fresh `gaps_found` at the approve. No-permit cases with a dirty tree or a moved HEAD were not run against a late stop.
- `devflow stop`'s `already_finished` line is tested as a function. No end-to-end `devflow stop` over a finishing run was run.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: protocol-reply | crates/devflow-core/src/supervisor/runtime.rs | A new final shutdown reply, `{"ok":false,"error":"already_finished"}`, sent when the CLI marked the run Finished. An older `devflow stop` reads it as a refusal and records nothing, which fails closed |

## Known Stubs

None.

## Issues Encountered

- Task 1's GREEN and Task 2's RED had been written in the same tree. To commit them separately, the Task 2 parts were saved in the scratch directory, taken out of the Task 1 commit, and restored afterwards. Each commit was built and tested on its own.
- The tests left live in the tree after the first Task 2 wiring (`run_preflight_loopback_gate_launches_agent_exactly_once`) waited out the 180 s bounded-profile slow timeout. That test was then deleted as the plan says.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-30/52-31. For 52-31: add `RateLimit` to `GateKind`, `AnswerFamily` and `status_less_step`. The exhaustive matches in `answer_family`, `parity_case` and `every_gate_kind` fail the build until it is placed. For 52-33: the routers return named routes (`Step::name`, `StatusLessStep::name`), and `Answer::resolved_fields` is what `gate_resolved` records.

## Self-Check: PASSED

- Files exist: `crates/devflow-cli/src/pipeline_outcomes.rs` (`fn can_finish`, `fn next_step`, `fn answer_family`), `crates/devflow-core/src/prompt.rs` (`/gsd-quick`), `crates/devflow-core/src/gates.rs` (`gsd_status`), `crates/devflow-cli/Cargo.toml` (`proptest`), `crates/devflow-cli/src/pipeline_gate.rs` (`fn workflow_finished_is_emitted_only_by_finish_run_and_the_until_halt`).
- Commits `466ee13`, `512069a`, `433da7e`, `2662d55`, `7983ee1`, `45b33ea`, `9721a5f`, `e5fc8bc` and `e36666a` are ancestors of HEAD, and `git rev-list --count 87c8adc..HEAD` is 9.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
