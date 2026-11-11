---
phase: 52-subtract-and-stabilize
plan: 08
subsystem: cli
tags: [cut, ship, validate, finish, notify, resume, rust]

requires:
  - phase: 52-07
    provides: devflow parallel removed (previous plan in the cut order)
provides:
  - "pipeline_gate::finish_run: a passing Validate ends the run, keeps state/worktree/branch, emits workflow_finished {reason: finished, shipped: false}, notifies once"
  - "State.finished (#[serde(default)] pub bool, additive on the non_exhaustive State)"
  - "the --until halt notifies once with reason stopped_at"
  - "resume refuses a finished run, naming devflow start"
  - "Stage::Ship is unreachable from handle_validate_outcome: no transition in it targets Ship"
affects: [52-09 and later Ship deletions (now dead-code removal), 52-25 OPS-02 cleanup wording, 52-27 gap loop, 52-29/52-33 gate routes and ceiling text]

actuals:
  tokens: 12417
  tasks: 3
  commits: 9
plan_head_before: d1193d6b4dbfdfbd7b0059096799e71cbfb35be6
plan_head_after: 38d51e0e3537bd7072f2665a3636015ecf52e681

tech-stack:
  added: []
  patterns:
    - "A run end emits workflow_finished, then fires the notify command once, then records notify_fired carrying the same reason (notify_run_end)"
    - "An e2e counts a run's events only after LaunchedRunReaper::supervisors_exited_within says no supervisor can add more"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs
    - crates/devflow-cli/tests/common/mod.rs
    - OPERATIONS.md

key-decisions:
  - "finish_run does not read stop_until: under --until validate a passing Validate ends as finished, not stopped_at (plan decision C9, implemented as written)"
  - "finish_run honours a stop taken during its advance at safe point 6 before recording the finish: the run ends stopped at Validate (finished: false) and resume reruns Validate. Without it the finish ran and devflow stop was answered handed_off"
  - "resume refuses a finished state inside reserve_launch's locked read, beside the live-run guard, so the refusal writes nothing"
  - "deregistration runs before the workflow_finished event, so an observer that sees the event also sees the registry entry gone"

patterns-established:
  - "Each run end site calls notify_run_end after workflow_finished; the two sites (finish_run, the --until halt) are exclusive per run end"

requirements-completed: [CUT-02]

coverage:
  - id: D1
    description: "An auto run whose Validate passes finishes: one workflow_finished {reason: finished, shipped: false}, one notify then notify_fired, state kept stopped+finished, worktree and branch kept, no workflow_shipped/hook_run, phase deregistered"
    requirement: CUT-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_run_that_passes_validate_finishes_and_keeps_its_worktree"
        status: pass
    human_judgment: false
  - id: D2
    description: "Approving a passed or ambiguous supervise Validate gate finishes the run; the gate asks to approve finishing, not shipping"
    requirement: CUT-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::approving_a_passed_validate_gate_finishes_the_run"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::approving_an_ambiguous_validate_gate_finishes_the_run"
        status: pass
    human_judgment: false
  - id: D3
    description: "The --until plan halt emits stopped_at, notifies exactly once after workflow_finished and stays finished: false; --until validate on a passing run ends as finished"
    requirement: CUT-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#an_until_halt_finishes_with_stopped_at_and_notifies_once"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#an_until_validate_run_that_passes_finishes_as_finished"
        status: pass
    human_judgment: false
  - id: D4
    description: "After a finished run, cleanup without --force keeps the worktree, and resume exits non-zero naming devflow start and launches nothing"
    requirement: CUT-02
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_finished_run_is_kept_by_cleanup_and_refused_by_resume"
        status: pass
    human_judgment: false
  - id: D5
    description: "A stop taken after a Validate approval is honoured at safe point 6 before the finish"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_stop_after_a_gate_answer_stops_before_the_finish_and_reports_it"
        status: pass
    human_judgment: false
  - id: D6
    description: "A run started with --no-worktree finishes the same way (branch kept, state recorded finished, nothing deleted)"
    requirement: CUT-02
    verification: []
    human_judgment: true
    rationale: "Plan marks this truth verification: backstop. finish_run makes no worktree- or git-specific call, so it holds by construction, but no test runs a --no-worktree finish; the verifier should confirm by reading finish_run"

duration: 32min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 08: End a run as finished after Validate Summary

**Validate is now the last stage: a passing Validate ends the run through `finish_run`, which keeps the state (stopped, finished), worktree and branch, records `workflow_finished {"reason":"finished","shipped":false}` and notifies once; no transition reaches `Stage::Ship`, and `resume` refuses a finished run.**

## Performance

- **Duration:** 32 min
- **Started:** 2026-11-10T21:21:10Z
- **Completed:** 2026-11-10T21:53:42Z
- **Tasks:** 3
- **Files modified:** 8

## Accomplishments
- `finish_run` in `pipeline_gate.rs` replaces all three `transition(…, Stage::Ship)` sites in `handle_validate_outcome` (auto pass, gate approve, ambiguous approve). It makes no git call and runs no hook, and it does not call `clear_run_state`.
- `State.finished` (`#[serde(default)]`) records the finish. The stop reason is `finished: Validate passed; the worktree and branch are kept`.
- The `--until` halt and the finish each fire `DEVFLOW_GATE_NOTIFY_CMD` once, after `workflow_finished`, and record `notify_fired` with the reason (`stopped_at` / `finished`).
- The supervise Validate gate now reads `Validation passed — approve to finish the run? The worktree and branch are kept.` The failure-cap ceiling sentence still says `approve to ship`; 52-33 owns it.
- `devflow resume` refuses a finished run before anything is written or launched, and names `devflow start`.
- OPERATIONS.md has a new "How a run ends" section covering both payloads, `--until validate`, the stop-before-finish case and the cleanup/resume behaviour. The notify, env-var, pipeline and `resume` rows were updated.

## Task Commits

1. **Task 1 (tracer): an auto run whose Validate passes finishes end to end**: `1c7289c` (test, RED), `b34d504` (feat!, GREEN)
2. **Task 2: supervise approvals finish, the `--until` halt notifies once, gate asks to finish**: `0a0cf69` (test, RED), `a8f71a0` (feat, GREEN), `ee8af94` (fix: unused import from a8f71a0)
3. **Task 3: a finished run is kept by `cleanup` and refused by `resume`**: `1179064` (test, RED), `973e012` (feat, GREEN), `07801f2` (test, RED for the stop-before-finish fix), `38d51e0` (fix, GREEN)

## TDD Gate Compliance

Each `feat`/`fix` commit follows a `test(52-08)` commit for its behaviour. RED evidence came from nextest JUnit reports (a scratch nextest config, `--profile red`). Every record was passed to `gsd_run check tdd-red-evidence`:

| Target | Verdict | Semantic assessment |
|---|---|---|
| `supervisor_e2e::a_run_that_passes_validate_finishes_and_keeps_its_worktree` | RED_EVIDENCE_OK | Failed on "the run ended at a gate, not finished": the log showed `transition validate→ship`, a `DocsUpdate` `hook_run` and a Ship preflight gate. This is the planned reason. |
| `pipeline_outcomes::tests::approving_a_passed_validate_gate_finishes_the_run` | RED_EVIDENCE_OK | `left: Ship, right: Validate`: the approval transitioned to Ship. |
| `pipeline_outcomes::tests::approving_an_ambiguous_validate_gate_finishes_the_run` | RED_EVIDENCE_OK | `left: Ship, right: Validate`. |
| `supervisor_e2e::an_until_halt_finishes_with_stopped_at_and_notifies_once` | RED_EVIDENCE_OK | The `stopped_at` payload assertion passed, then the test failed on "the halt notified exactly once: []" (0 vs 1). |
| `supervisor_e2e::an_until_validate_run_that_passes_finishes_as_finished` | INVALID_RED (`unexpected_green`), as the plan expected | Authored after Task 1, which already routed the auto pass to `finish_run`, so it was green from the start. It is the regression pin that `finish_run` ignores `stop_until`, and the `--until plan` test is its opposite case. |
| `supervisor_e2e::a_finished_run_is_kept_by_cleanup_and_refused_by_resume` | RED_EVIDENCE_OK | The cleanup half passed: cleanup already kept a stopped worktree. The test then failed on "resume refuses a finished run": resume relaunched Validate (`stage validate → launched Claude Code`). |
| `pipeline_launch::tests::a_stop_after_a_gate_answer_stops_before_the_finish_and_reports_it` | RED_EVIDENCE_OK | The shutdown reply was `{"ok":false,"error":"handed_off"}`, so `last["stopped"]` read Null. |

## Existing tests changed because Validate no longer transitions to Ship

| Test | Disposition |
|---|---|
| `pipeline_outcomes::tests::external_verify_agreement_advances_to_ship` | Rewritten as `external_verify_agreement_finishes_the_run`. It asserts the stage stays Validate, the run is `finished && stopped`, the counter is untouched and the call returns Ok. A doc reference to it in `grafted_failure_shape_gates_instead_of_shipping` was updated. |
| `pipeline_launch::tests::a_stop_after_a_gate_answer_stops_after_its_transition_and_reports_it` | Rewritten as `a_stop_after_a_gate_answer_stops_before_the_finish_and_reports_it`. It used the Validate gate, whose approval used to transition to Ship and stop before the launch. It now expects the stop honoured at safe point 6: no transition, no `workflow_finished`, nothing launched, exit 143, stopped and not finished. |
| `pipeline_outcomes::tests::…` (the test at ~L1810 driving `Some("pass")`) | Assertion message only: "must advance to Ship" became "must reach the passed gate". The check (`context.contains("Validation passed")`) is unchanged. |

No other test asserted a Validate→Ship transition. `scripts/check.sh all` is green.

## Files Created/Modified
- `crates/devflow-cli/src/pipeline_gate.rs`: `finish_run`, `FINISHED_STOP_REASON`, `notify_run_end`, and the notify call at the `--until` halt
- `crates/devflow-cli/src/pipeline_outcomes.rs`: the three Ship sites now call `finish_run`, plus the gate text, the new unit tests and the rewritten test
- `crates/devflow-cli/src/pipeline_launch.rs`: the rewritten stop-after-gate-answer test
- `crates/devflow-cli/src/commands.rs`: `refuse_resume_of_a_finished_run`, called from `reserve_launch`
- `crates/devflow-core/src/state.rs`: `State.finished`
- `crates/devflow-cli/tests/supervisor_e2e.rs`: `FinishRig` and four e2e tests
- `crates/devflow-cli/tests/common/mod.rs`: `LaunchedRunReaper::supervisors_exited_within`
- `OPERATIONS.md`: How a run ends, the notify hook, and the pipeline/env/resume rows

## Decisions Made
- `finish_run` ignores `stop_until`, as the plan decided (C9).
- Deregistration runs right after the state commit and before the events, so the run's last observable event comes after all its bookkeeping.
- Resume's refusal sits inside `reserve_launch`'s locked read, so it writes nothing (not even a `pending_supervisor`). The message deliberately does not reuse `restart_verb`, which would suggest `--force` and a worktree wipe for a finished run.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] A stop taken during the finish was answered `handed_off`**
- **Found during:** Task 3 (`scripts/check.sh all`)
- **Issue:** `a_stop_after_a_gate_answer_stops_after_its_transition_and_reports_it` failed after Task 2. A stop accepted while the run advanced after a Validate approval used to be honoured at the safe point before Ship's launch. `finish_run` had no safe point, so the finish ran and the supervisor answered the stop `handed_off`. `devflow stop` reports that answer as a run that already moved on to a successor.
- **Fix:** `finish_run` checks `stop_requested_at_safe_point()` first. If a stop was requested, it closes the Validate gate, records the stop at safe point 6 and returns an error, so the run ends stopped at Validate, not finished. The test was rewritten to that behaviour (RED `07801f2`, GREEN `38d51e0`), and OPERATIONS.md documents it.
- **Note for the orchestrator:** this is a behavioural choice I made inside the plan's remit: honour the operator's stop over the finish, consistent with the existing safe points. The alternative was to let the finish win and accept the `handed_off` reply.
- **Commits:** 07801f2, 38d51e0

**2. [Rule 1 - Bug] Unused `transition` import broke clippy**
- **Found during:** Task 3 (`scripts/check.sh all`, first run)
- **Issue:** after Task 2, `handle_validate_outcome` no longer called `transition`, so the non-test build warned and `clippy -D warnings` failed. Commit `a8f71a0` does not pass `scripts/check.sh all` on its own.
- **Fix:** removed the import from the production `use` and imported it in the test module (`ee8af94`).

**3. [Rule 3 - Blocking] Test-harness helper outside the plan's file list**
- **Found during:** Task 1
- **Issue:** to count events and notify lines reliably, the e2e must wait until the run's supervisors have exited.
- **Fix:** added `LaunchedRunReaper::supervisors_exited_within` to `tests/common/mod.rs`. It is read-only and ends nothing.

**4. [Rule 2 - Missing coverage] Ambiguous-approve test added**
- The plan named a unit test only for the passed-gate approval. The ambiguous-approve site changed too, so `approving_an_ambiguous_validate_gate_finishes_the_run` covers it.

**5. Test fixture bug fixed between RED and GREEN (Task 1)**
- The payload comparison first left the event envelope's `v` field in. RED had already failed earlier, at the gate assertion (the planned reason). The fix went into the GREEN commit `b34d504`.

---

**Total deviations:** 5 (2 bugs, 1 blocking, 1 coverage, 1 fixture fix)
**Impact on plan:** No scope creep. Deviation 1 adds one safe-point check to `finish_run`.

## Issues Encountered
- The Task 1 RED commit subject is 78 characters, and commit-msg warned (>72). It was left as committed.
- `cargo semver-checks check-release -p devflow-core` exits 100, but only on pre-existing removals from earlier phase-52 plans (`claude_legacy_launch`, `State.legacy_claude_launch`). The new `finished` field adds no failure.

## Verification

| Command | Exit |
|---|---|
| `cargo nextest run -p devflow --test supervisor_e2e --no-tests=fail -E 'test(=a_run_that_passes_validate_finishes_and_keeps_its_worktree)'` (also re-run as the tracer gate after b34d504) | 0 |
| Task 2 chain: `approving_a_passed_validate_gate_finishes_the_run` && `an_until_halt_finishes_with_stopped_at_and_notifies_once` && `an_until_validate_run_that_passes_finishes_as_finished` && `doc_check::doc_referenced_identifiers_exist_in_source` | 0 |
| `cargo nextest run -p devflow --test supervisor_e2e --no-tests=fail -E 'test(=a_finished_run_is_kept_by_cleanup_and_refused_by_resume)'` | 0 |
| `scripts/check.sh all` (fmt, clippy -D warnings, cargo test --workspace) on the tree committed as 38d51e0 | 0 |
| The four new e2e tests, repeated 5 times | 5/5 pass |
| Mutation: `registry::deregister` removed from `finish_run` | the Task 1 e2e failed on "the finished phase is deregistered" (control works) |

Acceptance checks: `finish_run` has no `clear_run_state` and no `stop_until`. `handle_validate_outcome` has no `Stage::Ship` and has 3 `finish_run(` calls. The passed-branch message contains `finish` and no `approve to ship`.

Limits:
- The e2e tests use a fake agent. That a real Validate pass reaches `finish_run` is shown by the routing tests, not by a live run.
- Five clean e2e repeats is a weak bound on flakiness.
- The `--no-worktree` finish is not exercised (D6, backstop).
- `status` output for a finished run, and the wording of cleanup's keep message (it still says "halted via --until; run `devflow resume`", which resume now refuses), belong to OPS-02 (52-25) and are unchanged here.

## Known Stubs
None.

## User Setup Required
None.

## Next Phase Readiness
- `Stage::Ship` still compiles, but no transition reaches it. The later Ship deletions are now dead-code removal.
- For 52-25 (OPS-02): cleanup's keep message for a finished run should stop pointing at `devflow resume`.

## Self-Check: PASSED
- Files exist: pipeline_gate.rs (`fn finish_run`), state.rs (`pub finished: bool`), supervisor_e2e.rs (`fn a_run_that_passes_validate_finishes_and_keeps_its_worktree`).
- Commits 1c7289c, b34d504, 0a0cf69, a8f71a0, ee8af94, 1179064, 973e012, 07801f2 and 38d51e0 are ancestors of HEAD (9 commits from d1193d6).

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*
