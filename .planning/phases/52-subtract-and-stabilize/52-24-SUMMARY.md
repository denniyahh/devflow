---
phase: 52-subtract-and-stabilize
plan: 24
subsystem: validate-routing
tags: [rust, devflow-cli, devflow-core, gsd, validate, state, tdd, semver-break]

requires:
  - phase: 52-22
    provides: Validate routes on (agent verdict, GSD status) through validate_route; the heuristic's nine tests already called it directly
  - phase: 52-20
    provides: gsd_reader::verification_status, the routing input this plan leaves alone
provides:
  - "Validate routing has one input, GSD's verification status: select_loop_back_fix, verification_authored_this_run and stamp_validate_dispatch_window are deleted"
  - "State has no last_verification_fingerprint, last_verification_mtime_nanos or verification_run_nonce; a state file that still carries them loads and drops them on save"
  - "State.verification_baseline_captured is kept, documented as the prepared-run flag commands::run_never_prepared reads"
  - "999.192's intent as a test: pipeline_outcomes::tests::a_verifier_written_report_from_code_routes_by_gsd_status"
affects: [52-27, 52-28, 52-29]

actuals:
  tokens: 25600
  tasks: 2
  commits: 4
plan_head_before: 46d8e33c7a269c78ef80927613008f512b26b01d
plan_head_after: ee3a15813ab15a1138a9d67fd25097746b61e798

tech-stack:
  added: []
  patterns:
    - "A removed State field is tolerated, not migrated: serde ignores the unknown key on load and the next save drops it, pinned by one load-and-save test"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-24-SUMMARY.md
  modified:
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/commands.rs
    - .planning/phases/52-subtract-and-stabilize/deferred-items.md

key-decisions:
  - "verification_baseline_captured is kept: commands::run_never_prepared reads it, so devflow resume and a hand-run advance refuse a run start never prepared, and status's recovery hints name start for it. Its name is now historical; renaming it would be one more semver break for no behaviour"
  - "agent_result::phase_verification_fingerprint and phase_verification_mtime_nanos are kept although nothing outside their tests calls them now: they are pub in the published crate and the plan did not list them (logged in deferred-items.md)"
  - "The verification_baseline_absent event is gone with select_loop_back_fix, its only emitter; no doc named it"

patterns-established:
  - "A deleted pub State field gets one test that loads a JSON state carrying it and asserts the key is gone after a save"

requirements-completed: [GATE-02]

coverage:
  - id: D1
    description: "A VERIFICATION.md written by the verifier during Code routes by GSD's status: gaps_found loops back gaps-only in auto mode and on a supervise reject, passed finishes the run, and no row relaunches a full-phase execute"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_verifier_written_report_from_code_routes_by_gsd_status"
        status: pass
    human_judgment: false
  - id: D2
    description: "select_loop_back_fix, verification_authored_this_run and stamp_validate_dispatch_window and their tests are deleted"
    requirement: GATE-02
    verification:
      - kind: other
        ref: "rg -n 'fn select_loop_back_fix|fn stamp_validate_dispatch_window' crates/devflow-cli/src (exit 1, no match)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Three verification-window State fields are deleted with their writes; a state file carrying all four keys still loads and drops the three on save"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/state.rs#state::tests::a_state_with_the_removed_verification_fields_still_loads"
        status: pass
      - kind: integration
        ref: "cargo nextest run -p devflow --test gate_notify_persist_e2e (11 passed)"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false

duration: 26min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 24: Route Validate by GSD status only Summary

**The report-authorship heuristic behind 999.192 is gone. Validate's only routing input is GSD's own verification status. Three of the four verification-window State fields are deleted with every write to them. The fourth, `verification_baseline_captured`, is kept because `devflow resume` reads it for a different reason.**

## Performance

- **Duration:** about 26 min
- **Started:** 2026-11-11T07:51:01Z
- **Completed:** 2026-11-11T08:16:54Z
- **Tasks:** 2 (Task 1 tracer, Task 2 TDD)
- **Files modified:** 6 (5 source, plus deferred-items.md; 185 insertions, 1569 deletions in source)

## Accomplishments

- **999.192's case is pinned as a test.** `a_verifier_written_report_from_code_routes_by_gsd_status` writes the phase's `{N}-VERIFICATION.md` before Validate runs, the way GSD's verifier does at the end of Code. It then checks three rows:
  - auto mode with `gaps_found`: the run loops back gaps-only with no gate;
  - supervise mode, operator rejects the `gaps_found` gate (the Phase 51 case): the run loops back gaps-only;
  - auto mode with `passed`: the run finishes and nothing loops back.
  No row emits a `loop_back` with `FullExecute`.
- **Deleted from `pipeline_outcomes.rs`:** `select_loop_back_fix`, `verification_authored_this_run`, the `heuristic_fix` test helper, and the 12 tests that pinned them:
  - `mid_arc_loop_back_issues_plain_execute_command`
  - `genuine_gaps_loop_back_still_issues_gaps_only`
  - `worktree_mode_genuine_gaps_loop_back_issues_gaps_only`
  - `worktree_mode_mid_arc_loop_back_issues_plain_execute`
  - `worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator`
  - `stale_verification_artifact_dispatches_full_execute`
  - `verification_written_this_run_dispatches_gaps_only`
  - `verification_freshness_truth_table_is_exhaustive`
  - `an_uncaptured_baseline_does_not_claim_an_inherited_artifact`
  - `an_idempotent_rewrite_is_authored_not_inherited`
  - `a_checkout_between_dispatches_does_not_read_as_authored_this_run`
  - `both_dispatch_directions_are_demonstrated_in_one_run`
- **Deleted from `pipeline_launch.rs`:** `stamp_validate_dispatch_window`, its call in `launch_stage_inner`, and its 2 tests, `launch_stage_inner_stamps_the_validate_dispatch_nonce_with_its_baseline` and `a_validate_launch_commits_its_dispatch_stamp`.
- **The four State fields:**

| Field | Outcome | Why |
|---|---|---|
| `last_verification_fingerprint` | deleted | Only the heuristic read it. Writes removed from `start`, `spawn_agent_and_record` and `prepare_loop_back_to_code` |
| `last_verification_mtime_nanos` | deleted | Same as the fingerprint, whose pair it was |
| `verification_run_nonce` | deleted | Only the heuristic read it. Writes removed from `record_validate_failure` and `spawn_agent_and_record` |
| `verification_baseline_captured` | **kept** | Read by `commands::run_never_prepared`, which `pipeline_launch::resume_loaded` and `advance_command` use (through `refuse_unprepared_run`) to refuse a run that `start` never prepared, and which `commands::recovery_hints` uses for `status`. `start` still sets it, once. Its doc now describes this role and says the name is historical |

- **Old state files:** a state file that still carries the removed keys loads, and the next save drops them. `a_state_with_the_removed_verification_fields_still_loads` loads a state with all four keys, then checks that the three removed keys are absent after a save and that `verification_baseline_captured` keeps `true`. No migration code was added.

## Task Commits

1. **Task 1 (tracer):** `974287e` test (the 999.192 test, green at baseline), `3377af2` refactor (delete the heuristic and its tests)
2. **Task 2:** `523ab3e` test (RED), `ee3a158` feat! (GREEN: delete the fields and their writes)

The plan suggested the subject `refactor(cli)!: route Validate by GSD status only`. The commits use the `52-24` scope instead, because the TDD gate validator greps `test(52-24)`/`feat(52-24)`. The plan's subject is on the `feat(52-24)!` commit, which carries the `BREAKING CHANGE:` footer for the three `pub` fields. The Task 1 commit deletes only private CLI functions, so it has no `!`.

## TDD Gate Compliance

| Test | Verdict | Semantic assessment |
|---|---|---|
| `pipeline_outcomes::tests::a_verifier_written_report_from_code_routes_by_gsd_status` | **No RED, by design.** It passed on the 52-24 base tree | The plan says 52-22 should already make it pass and asks for that as the baseline. To show the test can fail, a negative control: with the reject arm temporarily routed back through `select_loop_back_fix`, the old 999.192 behaviour, the test failed with exit 100 on its planned assertion ("a rejected gaps_found gate takes the gaps route"). The control edit was reverted from a saved copy before the commit |
| `state::tests::a_state_with_the_removed_verification_fields_still_loads` | `RED_EVIDENCE_OK` (`target_test_failed`, exit 100, nextest JUnit report) | The JSON loaded, then the planned drop-on-save assertion failed (`state.rs:1714`): `last_verification_fingerprint` was still serialized because the field still existed. It was not a fixture or parse fault |

- **Gate sequence:** `test` 974287e, then `refactor` 3377af2; `test` 523ab3e, then `feat` ee3a158.
- **Task 1 has no `feat(52-24)` GREEN commit.** Its behaviour was already GREEN from 52-22, and the task itself is a deletion, so it is committed as `refactor`. An end-of-phase TDD review that expects a `feat` after every `test` will flag Task 1. That is the reason.
- **Tracer gate:** the run is interactive (`auto_advance` false, `_auto_chain_active` false), the mode is `end-of-phase`, and Task 1's verify is automated only. The named test was re-run after the deletion and exited 0, and the full `devflow` bin suite passed (604 passed), so expansion continued.

## Decisions Made

See `key-decisions` in the frontmatter.

## Open operator decision: untouched

When `devflow stop` lands just as a passing Validate finishes the run, the stop still wins (`finish_run`'s safe point 6, from 52-08). This plan did not change `finish_run`. The only `pipeline_gate.rs` production change is in `prepare_loop_back_to_code`, which no longer copies the two removed fields. The finish row of the new test calls `handle_validate_outcome` without a stop, so it does not exercise safe point 6.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Doc] `launch_stage_inner`'s doc paragraph was attached to the stamp**
- **Found during:** Task 1
- **Issue:** when 35.2 extracted `stamp_validate_dispatch_window`, the paragraph that begins "The post-preflight body of [`launch_stage`]" stayed on top of the extracted function. Deleting the function would have deleted that paragraph too.
- **Fix:** the paragraph is moved back to the top of `launch_stage_inner`'s doc. The "Validate dispatch stamp" mention in `spawn_agent_and_record`'s comment is removed.
- **Commit:** `3377af2`

**2. [Scope note] Tests renamed or narrowed, not deleted**
- `pipeline_gate::a_loop_back_commits_its_fields_and_the_verification_observation` is renamed `a_loop_back_commits_its_fields`. It now also asserts that `code_fix` is `Some(GapsOnly)` on disk, in place of the removed observation.
- `pipeline_outcomes::a_ceiling_loop_back_persists_the_reset_and_the_verification_observation` is renamed `a_ceiling_loop_back_persists_the_reset`. Its fingerprint fixture is gone.
- `a_validate_failure_commits_its_counters_and_keeps_a_concurrent_field` loses its nonce assertion.
- In `state.rs`, the four round-trip and absent-default tests of the removed fields are deleted. The absent-default test of the kept flag is now `verification_baseline_captured_absent_from_json_defaults_to_false`.
- **Commit:** `ee3a158`

**3. [Scope note] `gate_notify_persist_e2e` was not rewritten**
- The plan asked for a rewrite of that file's use of the fields. Its two uses are `state.verification_baseline_captured = true`, which marks the fixture run as prepared so `resume` accepts it. That field is kept, so the file needed no change. The same applies to the `= true` fixture lines in `commands.rs`, `pipeline_launch.rs` and the other e2e suites. The suite passes unchanged (11 passed).

**4. [Scope note] The `verification_baseline_absent` event is removed**
- `select_loop_back_fix` was its only emitter. No doc and no test outside the deleted ones named it.

---

**Total deviations:** 4 (1 doc fix, 3 scope notes)
**Impact on plan:** none of them adds behaviour; each keeps the deletion clean or records why the plan's suggested edit was unnecessary.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<verify>`, `a_verifier_written_report_from_code_routes_by_gsd_status`: base tree, after the deletion, final tree | 0 (1 passed) each time |
| Same test with the reject arm routed back through the heuristic (negative control, reverted) | 100 |
| `cargo nextest run -p devflow --bin devflow --no-fail-fast` after Task 1 | 0 (604 passed) |
| `cargo clippy --workspace --all-targets -- -D warnings` after each task | 0 |
| Task 2 `<verify>`: `a_state_with_the_removed_verification_fields_still_loads` | 0 (1 passed); RED run on the pre-deletion tree exited 100 |
| Task 2 `<verify>`: `cargo nextest run -p devflow --test gate_notify_persist_e2e --no-tests=fail` | 0 (11 passed) |
| Task 2 `<verify>`: `scripts/check.sh all`, first run | **101**: one failure, see below |
| `scripts/check.sh all`, second run, on the tree committed as `ee3a158` | 0 ("all OK") |
| `rg -n 'fn select_loop_back_fix\|fn stamp_validate_dispatch_window' crates/devflow-cli/src` | 1 (no match) |
| `cargo semver-checks check-release -p devflow-core` | 100, expected |

**The first `scripts/check.sh all` run failed one test, which this plan did not touch.**
- `gsd_reader::tests::a_gaps_found_report_reads_as_gaps_found` failed with `Spawn { message: "Text file busy (os error 26)" }`. That run's devflow-core lib result was 1062 passed, 1 failed.
- The test then passed 3 times in isolation, and the full second run passed.
- 52-24 changed neither `gsd_reader.rs` nor `test_support.rs`.
- This is not the known flake 999.221. The likely cause is the write-then-exec race in `test_support::plant_gsd_run_stub`, but that is inferred from the error message and was not reproduced on demand. It is logged in `deferred-items.md` with a fix shape.

**semver-checks:** this plan's break is `struct_pub_field_missing` for `State.last_verification_fingerprint`, `last_verification_mtime_nanos` and `verification_run_nonce`. The other findings come from earlier phase-52 plans. 4.0.0 covers all of them, and the version was not bumped.

**What these checks do not establish:**
- The new test drives `handle_validate_outcome` with a stub agent verdict and a stub `gsd_run`. No live Code→Validate run with a real verifier tail has been routed by this build.
- The negative control proves the test catches the heuristic on the reject arm. It does not prove the test catches a heuristic re-added on the auto arm. The auto `gaps_found` row would catch one that returned `FullExecute` there, but that was not driven.
- One passing second run of `check.sh all` bounds the `ETXTBSY` failure only weakly. It shows the failure is intermittent, not how often it happens.

## Known Stubs

None.

## Issues Encountered

- The first background test command assigned the scratch path inside the backgrounded list, so the PID file write failed. The run itself was fine. I found its PIDs (1720078, 1720081, 1720082) by their exact command lines and recorded them. All of them exited on their own.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-25. For 52-27..52-29: Validate routing has the one input `validate_route` reads, so a new route needs no authorship proof. `State.verification_baseline_captured` remains, and only `run_never_prepared` reads it.

## Self-Check: PASSED

- Files exist: `crates/devflow-cli/src/pipeline_outcomes.rs` contains `a_verifier_written_report_from_code_routes_by_gsd_status`, and `crates/devflow-core/src/state.rs` contains `a_state_with_the_removed_verification_fields_still_loads`.
- Commits `974287e`, `3377af2`, `523ab3e` and `ee3a158` are ancestors of HEAD. `git rev-list --count 46d8e33..HEAD` was 4 when this file was written.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
