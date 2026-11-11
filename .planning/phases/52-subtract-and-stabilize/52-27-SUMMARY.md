---
phase: 52-subtract-and-stabilize
plan: 27
subsystem: validate-routing
tags: [rust, devflow-cli, devflow-core, gsd, gates, validate, gap-closure, tdd, semver-break]

requires:
  - phase: 52-19
    provides: "an explicit abort, so a gaps gate's abort ends the run and every test gate ends with one"
  - phase: 52-20
    provides: "gsd_reader::frontmatter_field and phase_dir, the post-check's reads"
  - phase: 52-22
    provides: "validate_route, GateKind (NoGapClosurePlan defined, unwritten), the GSD stub seams, an agent PASS over a non-passed status counted as a Validate failure"
  - phase: 52-26
    provides: "the relaunch note on every stage intent and its clear on leaving the stage"
provides:
  - "StageIntent::PlanGaps { phase, relaunch_note }: /gsd-plan-phase N --gaps (claude style) or plan-phase.md for phase N --gaps (workflow style), with no idempotency short-circuit"
  - "State.gap_closure_pass: bool (serde default, skipped when false)"
  - "ValidateRoute::GapLoop: an auto gaps_found Validate moves to Plan"
  - "pipeline_outcomes::handle_gaps_plan_outcome: the post-check after a gaps Plan pass; writes GateKind::NoGapClosurePlan and ReaderFailure at Plan"
  - "agent_result::phase_source_commit_count (pub, additive)"
  - "mode::transition_resets_consecutive_failures(from, to, gap_closure_pass) (breaking)"
  - "devflow start --until validate prints a note and emits one until_noop event {\"until\":\"validate\"}"
affects: [52-28, 52-29, 52-31, 52-32, 52-33]

actuals:
  tokens: 28369
  tasks: 3
  commits: 6
plan_head_before: 26ac8b25af071373653e4fae3b4b5fd24f3ed0ea
plan_head_after: c10b7158afd0c682c94640cee3472ed2750aa199

tech-stack:
  added: []
  patterns:
    - "A persisted marker, not a prompt, carries a multi-stage pass across a stop: the default prompt builder derives the gaps Plan intent and the gaps-only fix from State.gap_closure_pass"
    - "A post-check after an agent stage reads GSD through the stub-able reader and gates never-silently when it cannot tell"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-27-SUMMARY.md
  modified:
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/mode.rs
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - OPERATIONS.md

key-decisions:
  - "Names: StageIntent::PlanGaps, State.gap_closure_pass, ValidateRoute::GapLoop, pipeline_outcomes::handle_gaps_plan_outcome"
  - "The gap-closure marker is set inside transition's own commit on the Validate->Plan move (the only Validate->Plan transition), so the move and the marker are one write"
  - "The post-check compares file modified times (each *-PLAN.md against NN-VERIFICATION.md), not commit order; only plans newer than the report are read through GSD, and GSD's 'Field not found' means not a gap plan"
  - "gap_closure is true when GSD returns the string \"true\" (what GSD's own plan reader compares) or a JSON true"
  - "A loop-back to Code with any fix but GapsOnly clears the marker in the same commit (another fix route replaces the pass); a GapsOnly loop-back keeps it"
  - "The gaps-gate approve arm keys on the GateKind the gate was opened with (the value written to GateFile.gate_kind), never on the status in the context"
  - "D-25's streak rule (transition_resets_consecutive_failures with the marker) landed in Task 1, because Task 1's route change otherwise broke the existing streak tests"

patterns-established:
  - "A gap-closure pass: Validate gaps_found -> Plan --gaps -> post-check -> Code --gaps-only -> Validate, the marker cleared only by a passed read, the finish, an abort or a non-gaps fix route"

requirements-completed: [GATE-02, GATE-03]

coverage:
  - id: D1
    description: "An auto run whose first Validate reads gaps_found runs /gsd-plan-phase N --gaps, then /gsd-execute-phase N --gaps-only (no full execute), then Validate, which reads passed, and finishes in the same run"
    requirement: GATE-03
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_gaps_found_validate_runs_plan_gaps_then_execute_gaps_only_then_passes"
        status: pass
    human_judgment: false
  - id: D2
    description: "--until validate halts nothing: a gaps verdict runs the gap loop and finishes, with one printed note and one until_noop event; --until plan still halts at Plan with neither"
    requirement: GATE-03
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#an_until_validate_run_with_gaps_runs_the_gap_loop"
        status: pass
    human_judgment: false
  - id: D3
    description: "The gaps Plan prompt runs --gaps with no short-circuit in both styles and carries the relaunch note; the ordinary Plan prompt keeps the short-circuit"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::the_gaps_plan_prompt_runs_plan_phase_gaps_even_when_plans_exist"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::a_relaunch_note_reaches_every_stage_prompt_in_both_styles"
        status: pass
    human_judgment: false
  - id: D4
    description: "A stopped gaps Plan or gaps Code pass resumes as one and keeps the marker (review C6); the Validate->Plan exit clears the relaunch note; the default-render pin is unchanged"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_stopped_gaps_plan_pass_resumes_with_gaps"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_stopped_gaps_code_pass_resumes_with_gaps_only"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::leaving_the_relaunched_stage_clears_the_relaunch_note"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::every_no_override_launch_renders_through_default_stage_prompt"
        status: pass
    human_judgment: false
  - id: D5
    description: "Approving a supervise gaps_found gate runs the gap loop and records no workflow_finished (also under --until validate); a human_needed gate quoting gaps_found does not; abort ends the run"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::approving_a_gaps_gate_runs_the_gap_loop"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::aborting_a_gaps_gate_ends_the_run"
        status: pass
    human_judgment: false
  - id: D6
    description: "A gaps Plan pass with no gap_closure plan newer than the report gates no_gap_closure_plan (an earlier round's gap plan does not count); a failed read gates reader_failure; a newer gap plan proceeds to the gaps-only Code pass"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_gaps_plan_pass_without_a_new_gap_closure_plan_gates"
        status: pass
    human_judgment: false
  - id: D7
    description: "Gap rounds keep the streak and only commits outside .planning/ are progress: three planning-only rounds gate at the cap with the agent failing or passing, each round moving each counter by one; a source-changing round resets the streak; in supervise each round counts once and its approve counts nothing"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#agent_result::tests::phase_source_commit_count_ignores_planning_only_commits"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/mode.rs#mode::tests::a_gap_closure_pass_keeps_the_failure_streak"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::three_gap_rounds_without_progress_force_the_gate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_gap_round_that_changes_a_source_file_resets_the_streak"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_supervise_gaps_round_counts_once_and_its_approve_counts_nothing"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false
  - id: D8
    description: "A real gsd-plan-phase --gaps agent writes a gap plan that the post-check accepts, and a real gaps-only pass closes the gaps"
    verification: []
    human_judgment: true
    rationale: "The plan's stated limit: every test uses a fake agent and a stub gsd_run; the live GSD route was shown once (52-GAP-LOOP-VERIFICATION, n=1), never through DevFlow"

duration: 41min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 27: Close verification gaps inside the run Summary

**A Validate that reads GSD's `gaps_found` now closes the gaps inside the same run. Plan runs `/gsd-plan-phase N --gaps`, a post-check confirms a new `gap_closure: true` plan, Code runs `/gsd-execute-phase N --gaps-only`, and Validate runs again. A persisted marker carries the pass across a stop or resume. In supervise mode, approving the gaps gate runs the same loop and abort ends the run. Gap rounds keep the failure streak, and only commits outside `.planning/` count as progress, so three idle rounds gate.**

## Performance

- **Duration:** about 41 min
- **Started:** 2026-11-11T09:41:37Z
- **Completed:** 2026-11-11T10:22:25Z
- **Tasks:** 3 (Task 1 tracer, Tasks 2 and 3 TDD)
- **Files modified:** 12

## Accomplishments

- **The gap-closure route (Task 1).** `validate_route` has a new `GapLoop` route for `gaps_found` in auto mode. It calls `transition(Validate → Plan)`, and that transition's commit sets `State.gap_closure_pass`. While the marker is set, `default_stage_prompt` builds `StageIntent::PlanGaps` at Plan. At Code it derives `FixType::GapsOnly` whenever `code_fix` is `None`. So an `--until code` halt, a resume or a takeover still relaunches the gaps pass.
- **Where the marker is cleared.** Only four places clear it: a Validate read of `passed` (in the same commit as `validate_pass_head`), `finish_run`, an abort (which clears the whole state), and `prepare_loop_back_to_code` with a fix other than `GapsOnly`. A source scan of the writes confirms these four. `transition`'s `--until` halt, the stop path and resume do not touch it.
- **`--until validate` halts nothing.** `transition`'s `--until` halt skips `from == Validate`. `devflow start --until validate` prints `note: --until validate halts nothing: …` and emits one `until_noop` event. `--until plan` still halts at Plan, which the e2e control checks.
- **The gaps gate's answers (Task 2).** A supervise gate of kind `gaps_found` now behaves as follows:
  - Approve closes the gate and moves to the gaps Plan. It does not reset or count anything except the existing ceiling reset.
  - Abort ends the run.
  - Reject is unchanged (gaps-only loop back to Code, 52-22/52-26).
  - The context names approve and abort and says "Finishing over open gaps is not offered."
- **The post-check after a gaps Plan pass.** It looks for a plan file newer than `NN-VERIFICATION.md` (compared by modified time) whose `gap_closure` GSD reads as true.
  - If none exists, the run stops at a Plan gate of kind `no_gap_closure_plan`.
  - If the check cannot tell, the run stops at a `[never-silent]` `reader_failure` gate. Its event context carries the label only.
  - Both gates answer the same way: approve reruns the gaps Plan pass, reject loops back gaps-only with the note, abort ends the run.
- **Streak progress (Task 3).** `phase_source_commit_count` is `phase_commit_count` with the pathspec `-- . ':(exclude).planning'`. `record_validate_failure` now reads it for every Validate failure. `transition_resets_consecutive_failures` takes the marker: Validate→Plan and Plan→Code keep the streak inside a gap pass.
- **OPERATIONS.md** describes the gap-closure pass and its gates, and the `--until validate` note and event.

## Task Commits

1. **Task 1 (tracer):** `f2de746` test (RED, with skeletons), `e9b5161` feat! (GREEN)
2. **Task 2:** `a6c6762` test (RED, with skeletons), `2f43e41` feat (GREEN)
3. **Task 3:** `4f82e4e` test (RED, with a skeleton), `c10b715` feat (GREEN)

The plan's context suggested the subject `feat(cli): close verification gaps inside the run`. The commits use the `52-27` scope, because the TDD gate validator greps `feat(52-27)`.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch `--profile red` config. Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `supervisor_e2e::a_gaps_found_validate_runs_plan_gaps_then_execute_gaps_only_then_passes` (Task 1) | `RED_EVIDENCE_OK` | The run looped back to Code, never ran Plan `--gaps`, and ended at a reader-failure gate. The test failed in `wait_for_end`'s "ended at a gate" assertion, a consequence of the missing route. |
| `supervisor_e2e::an_until_validate_run_with_gaps_runs_the_gap_loop` | `RED_EVIDENCE_OK` | Same: no Plan `--gaps`, ended at a gate. |
| `prompt::tests::the_gaps_plan_prompt_runs_plan_phase_gaps_even_when_plans_exist` | `RED_EVIDENCE_OK` | The skeleton `PlanGaps` rendered the ordinary Plan prompt, which has no `--gaps`. |
| `pipeline_launch::tests::a_stopped_gaps_plan_pass_resumes_with_gaps` | `RED_EVIDENCE_OK` | The gaps prompt had no `--gaps`. It failed on the test's own premise assertion about the rendered gaps intent, which is the planned behaviour. |
| `pipeline_launch::tests::a_stopped_gaps_code_pass_resumes_with_gaps_only` | `RED_EVIDENCE_OK` | The resumed agent received the full execute. |
| `prompt::tests::a_relaunch_note_reaches_every_stage_prompt_in_both_styles` (gaps Plan row) | green at RED | The skeleton rendered `PlanGaps` through the Plan path, which already carried the note. GREEN keeps the row passing on the real renderer. |
| `pipeline_gate::tests::leaving_the_relaunched_stage_clears_the_relaunch_note` (Validate→Plan row) | green at RED | Expected by the plan: `transition`'s commit already cleared the note. |
| `pipeline_outcomes::tests::approving_a_gaps_gate_runs_the_gap_loop` (Task 2) | `RED_EVIDENCE_OK` | Approve looped back to Code (stage `Code`, not `Plan`). |
| `pipeline_outcomes::tests::a_gaps_plan_pass_without_a_new_gap_closure_plan_gates` | `RED_EVIDENCE_OK` | The skeleton moved to Code without checking, and no Plan gate opened. |
| `supervisor_e2e::a_gaps_found_validate_runs_plan_gaps_then_execute_gaps_only_then_passes` (Task 2, with the frontmatter reply) | `RED_EVIDENCE_OK` | Nothing read `gap_closure`, so the second Validate consumed that reply as a reader failure. |
| `pipeline_outcomes::tests::aborting_a_gaps_gate_ends_the_run` | green at RED | 52-19/52-22's abort already ended the run. The test pins that this still holds after Task 2. |
| `agent_result::tests::phase_source_commit_count_ignores_planning_only_commits` (Task 3) | `RED_EVIDENCE_OK` | The skeleton counted 3, not 1. |
| `pipeline_outcomes::tests::three_gap_rounds_without_progress_force_the_gate` | `RED_EVIDENCE_OK` | Round 2 gave `(1, 2)`: the `.planning/` commit read as progress and reset the streak. The run with the agent passing was not reached at RED, because the failing run came first. GREEN runs both. |
| `pipeline_outcomes::tests::a_gap_round_that_changes_a_source_file_resets_the_streak` | `RED_EVIDENCE_OK` | It failed on the premise that two `.planning/`-only rounds build a streak (1, not 2), which is the D-34 behaviour. |
| `pipeline_outcomes::tests::a_supervise_gaps_round_counts_once_and_its_approve_counts_nothing` | `RED_EVIDENCE_OK` | Round 2's gate showed `(1, 2)`. |
| `mode::tests::a_gap_closure_pass_keeps_the_failure_streak` | green at RED | D-25 landed in Task 1 (deviation 1). |

Gate sequence: `test` f2de746 → `feat` e9b5161; `test` a6c6762 → `feat` 2f43e41; `test` 4f82e4e → `feat` c10b715. There is no REFACTOR commit.

Mutation control, run by hand and reverted: with the post-check's newer-than-report filter disabled (`|| true`), `a_gaps_plan_pass_without_a_new_gap_closure_plan_gates` failed (exit 100).

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode `end-of-phase`, and Task 1's verify automated only. Its eight-command chain was re-run after the GREEN commit and exited 0, so expansion continued.

## Existing tests changed

- `pipeline_outcomes::tests::a_verifier_written_report_from_code_routes_by_gsd_status`: its auto row now asserts the move to Plan (a `transition` validate→plan and no Validate gate), not a `GapsOnly` loop-back.
- `pipeline_outcomes::tests::loop_back_reason_is_distinct_when_no_commit_baseline_exists`: it answers `stale`, so the outcome still loops back to Code and records a `loop_back` reason. `gaps_found` now goes through Plan and records no `loop_back`.
- `consecutive_failures_reaches_ceiling_across_cycles`, `repeated_failure_without_new_commits_still_reaches_the_ceiling`, `external_verify_cycles_reach_ceiling_without_unbounded_loop` (arm B) and `phase_validate_failure_ceiling_gates_despite_trivial_commit_progress`: each saves the state at Validate before the outcome. Before, they forced `state.stage` only in memory, and `commit` reloads the stage from disk, so the new `transition` ran from the stale stage on disk.
- `phase7_cli::cleanup_worktree_cli_flow`: its stub answers `stale`, so the auto loop back to Code reaches its `failure_cap` gate at Validate. With `gaps_found` its run stopped at the gaps Plan post-check (the fixture writes no report).
- The 999.66 guard tests whose progress meaning changed are listed here. Each had only its doc updated, and each still passes:
  - `healthy_multi_wave_progress_does_not_reach_the_ceiling` and `phase_validate_failure_ceiling_gates_despite_trivial_commit_progress`: their commits are files outside `.planning/`, so they still count as progress.
  - `repeated_failure_without_new_commits_still_reaches_the_ceiling`: it makes no commits, so nothing changed.
  - `test_support::commit_on_feature_branch`'s doc now says its file counts as progress.
- `mode::tests::consecutive_reset_*` pass `false` for the new parameter.

## Decisions Made

See `key-decisions`. Two of them need a note here:

- **The comparison the post-check uses: file modified time.** It compares each `*-PLAN.md` in the phase directory, strictly newer, against `NN-VERIFICATION.md`. Commit order would need git, and a plan the agent wrote but did not commit would have no commit to order by. The test fixture spaces commit dates (`GIT_COMMITTER_DATE`) and file times (`File::set_modified`) 120 s apart, so either comparison orders it the same way.
- **The approve arm keys on the gate's kind, held in memory.** It uses the `GateKind` value `handle_validate_outcome` opened the gate with, which is the value written to `GateFile.gate_kind`. It does not read the file back. The kind-keyed control calls `answer_validate_gate` with kind `human_needed` over a `gaps_found` read, and the marker stays unset.

## Open operator decisions: untouched

- **Stop against finish at Validate's end.** A stop still wins. `finish_run` gained only the marker clear.
- **`phase_validate_failures` carried across a finished state into the next `start`.** `start` was not changed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] D-25's streak rule landed in Task 1**
- **Found during:** Task 1 GREEN
- **Issue:** With the gaps route going through `transition(Validate → Plan)`, the old reset rule zeroed `consecutive_failures` on every gap round. Six existing streak tests failed.
- **Fix:** `transition_resets_consecutive_failures(from, to, gap_closure_pass)` landed in Task 1's GREEN commit. Task 3's mode test was then green at RED (see the TDD table).
- **Commit:** `e9b5161`

**2. [Rule 2 - Correctness] A non-gaps loop-back clears the marker**
- **Issue:** Code derives `--gaps-only` from the marker. Without a clear, a stale or missing route (full execute) taken while the marker was set would have rendered gaps-only.
- **Fix:** `prepare_loop_back_to_code` clears the marker when `fix != GapsOnly`, in the same commit. This is the plan's "another fix route replaces it". The default builder also prefers a recorded `code_fix` over the marker.
- **Commit:** `e9b5161`

**3. The e2e's GSD replies between Task 1 and Task 2**
- Task 1's GREEN commit scripted two replies (gaps_found, passed), because the post-check did not exist yet. Task 2's RED commit restored the frontmatter reply the plan names, and that is what made the e2e RED again.

**4. The resume tests assert the Codex rendering**
- `a_stopped_gaps_plan_pass_resumes_with_gaps` and `…_code_pass_resumes_with_gaps_only` use the Codex argv recorder, as 52-26's resume tests do. They assert `plan-phase.md for phase N --gaps` and the workflow `--gaps-only` text, not the claude `/gsd-plan-phase N --gaps` slash command. The claude text is asserted in the prompt test, in the e2e and in `approving_a_gaps_gate_runs_the_gap_loop`.

**5. Commit-subject length**
- `a6c6762`'s subject is 73 characters, one over the 72 limit. The commit-msg hook warned about it. I did not amend it, because the orchestrator said not to rewrite history.

**6. Files beyond `files_modified`:** `crates/devflow-cli/src/test_support.rs` (doc only) and `crates/devflow-cli/tests/phase7_cli.rs`.

---

**Total deviations:** 6 (1 blocking, 1 correctness, 4 notes)
**Impact on plan:** Each one is either needed to keep the existing suite green, or a guard on the marker the plan introduces. Nothing was added beyond the plan.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<verify>` chain (8 named runs), after GREEN and again as the tracer gate | 0, 0 |
| Task 2 `<verify>` chain (3 named runs) | 0 |
| Task 3 `<verify>` named runs (5) | 0 |
| `cargo nextest run --workspace` (bounded profile) after Task 1 | 0 (1998 passed) |
| `cargo nextest run --workspace` (bounded profile) after Task 2 | 100: `phase7_cli::cleanup_worktree_cli_flow` failed (fixed, see above; re-run alone: 0). The other 2000 passed |
| `scripts/check.sh all` on the final tree (after the last code change, before commit `c10b715`) | 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| `cargo semver-checks check-release -p devflow-core` | 100, expected |

semver-checks reports this plan's breaks:
- `enum_variant_added` for `StageIntent::PlanGaps`
- `function_parameter_count_changed` for `mode::transition_resets_consecutive_failures`

`State.gap_closure_pass` is not flagged, since `State` is `#[non_exhaustive]`, and `phase_source_commit_count` is additive. The other findings are from earlier phase-52 plans. 4.0.0 covers all of them, and the version was not bumped. Known flakes 999.221 and 999.222 did not occur. `test_support::plant_gsd_run_stub` was not touched.

**What these checks do not establish:**
- No real agent has run the gaps Plan or the gaps-only pass through DevFlow (D8 above). Every test uses a fake agent and a stub `gsd_run`.
- GSD's real `frontmatter get` output for `gap_closure` is taken from the 1.20.1 source (`cmdFrontmatterGet`; `plan-document.cjs` compares the string `"true"`), not from a live run. The check accepts the string and a JSON boolean.
- Several answers have no dedicated test:
  - approving or rejecting the `no_gap_closure_plan` and Plan `reader_failure` gates (rerun the gaps Plan; loop back gaps-only);
  - rejecting the gaps gate, which is unchanged.
- Modified-time ordering assumes the report is written before the gaps Plan pass in the same checkout. A checkout that rewrites both files' times at once (a fresh clone of the worktree) would make the new plan not newer, and the run would stop at the `no_gap_closure_plan` gate. That is the fail-closed direction.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: process-exec | crates/devflow-cli/src/pipeline_outcomes.rs (`has_new_gap_closure_plan`) | After a gaps Plan pass DevFlow runs GSD's `gsd_run query frontmatter get <plan> --field gap_closure` for each plan newer than the report. It goes through 52-20's bounded, hermetic reader: argv only, GIT_* removed, 60 s kill. GSD's message goes only to the gate context and stderr, never to an event |

## Known Stubs

None. `GateKind::NoGapClosurePlan`, which 52-22 left unwritten, is now written.

## Issues Encountered

- After Task 2's GREEN, the workspace run failed `cleanup_worktree_cli_flow`: it waited 5 s for a Validate gate while the run sat at the new Plan post-check gate. Fixed as above.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-28. For 52-28: the stale and missing (re-verify) route loops back with `FullExecute`, and that already clears the marker in `prepare_loop_back_to_code`, so `a_reverify_route_clears_the_gap_marker` should pass against it. For 52-29: the gaps-gate approve arm in `answer_validate_gate` (`GateAction::Advance if kind == GateKind::GapsFound`) and the post-check gates' answers in `handle_gaps_plan_outcome` are the interim arms `approve_route` and `reject_route` replace.

## Self-Check: PASSED

- Files exist: `crates/devflow-core/src/prompt.rs` (`PlanGaps`, `--gaps`), `crates/devflow-core/src/agent_result.rs` (`fn phase_source_commit_count`), `crates/devflow-cli/tests/supervisor_e2e.rs` (`fn a_gaps_found_validate_runs_plan_gaps_then_execute_gaps_only_then_passes`).
- Commits `f2de746`, `e9b5161`, `a6c6762`, `2f43e41`, `4f82e4e` and `c10b715` are ancestors of HEAD, and `git rev-list --count 26ac8b2..HEAD` is 6.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
