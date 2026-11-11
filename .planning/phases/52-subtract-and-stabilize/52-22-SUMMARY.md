---
phase: 52-subtract-and-stabilize
plan: 22
subsystem: validate-routing
tags: [rust, devflow-cli, devflow-core, gsd, gates, validate, tdd, semver-break]

requires:
  - phase: 52-19
    provides: GateResponse.abort and `devflow gate abort`, so every test gate here ends with an explicit abort
  - phase: 52-20
    provides: gsd_reader (verification_status, phase_dir, runtime_version, ReaderFailure::label)
provides:
  - "Validate routes on (agent verdict, GSD status or reader failure) through pipeline_outcomes::validate_route"
  - "ValidateRoute { Finish, LoopBack(FixType), Gate(GateKind) }, AgentVerdict { Pass, Fail, Ambiguous }, EvidenceFailure, ValidateRead"
  - "pub enum GateKind (13 variants, snake_case, not non_exhaustive) and GateFile.gate_kind: Option<GateKind>"
  - "a required GateKind on Gates::open_gate, open_registered, write_gate and on run_gate, run_gate_with_timeout, run_gate_with_event_context"
  - "State.validate_pass_head: Option<String>, written only by handle_validate_outcome"
  - "events gsd_verification ({verdict, status | failure}) and gsd_runtime ({version} or {version: null, failure}); gate_fired carries gate_kind"
  - "devflow_core::test_support::plant_gsd_run_stub / gsd_run_calls / gsd_status_json; tests/common::stub_gsd_run / gsd_run_calls"
  - "CLI test seam: test_support::GsdStub (registered per evidence root) and DEVFLOW_TEST_GSD_HOME for child processes"
affects: [52-24, 52-27, 52-28, 52-29, 52-31, 52-32, 52-33]

actuals:
  tokens: 67000
  tasks: 4
  commits: 7
plan_head_before: ad71cf08e5bd2ed1647729c7631a677d49654cd0
plan_head_after: 27fe5d4a8327433e53cb05398a144d7f80fbbfa6

tech-stack:
  added: []
  patterns:
    - "One GSD read per Validate outcome, for every verdict; the pair feeds one pure route function"
    - "A gate whose context carries GSD's or git's message opens through run_gate_with_event_context, so gate_fired carries a path-free context"
    - "A unit test reaches GSD only through a stub registered for its evidence root; a test build with none fails closed as a missing launcher"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-22-SUMMARY.md
  modified:
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/test_support.rs
    - crates/devflow-core/src/recover.rs
    - crates/devflow-cli/tests/common/mod.rs
    - crates/devflow-cli/tests/supervisor_e2e.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - OPERATIONS.md
    - doc-check-allowlist.toml

key-decisions:
  - "gsd_runtime is emitted by `devflow start`, right after workflow_started: once per run by construction, with the failure label when no version can be read"
  - "An agent pass over any GSD status but passed is a counted Validate failure, so the consecutive and per-phase caps bound an agent that always says pass over a report with gaps"
  - "failure_cap means a counter is at its cap in either mode (Mode::Auto.should_gate); a reader failure, an ambiguous verdict, human_needed and both disagreements keep their own kind at the cap"
  - "Interim approve route until 52-29: approval finishes only over GSD's passed (after an agent pass, or an ambiguous verdict as 52-08 left it); gaps_found loops back gaps-only, stale/missing loop back full-execute, an agent fail over passed, human_needed and a reader failure rerun Validate. Reject loops back gaps-only on gaps_found, full-execute otherwise"
  - "validate_pass_head is written in its own state commit right after the read, before the route acts, not inside the route's commit"
  - "The ambiguous-verdict detail says the external verification 'succeeded', so no gate context says passed unless GSD did"

patterns-established:
  - "Every gate DevFlow opens records a GateKind; a 3.x gate file without one still loads as None"

requirements-completed: [GATE-02, GATE-03]

coverage:
  - id: D1
    description: "A supervise run whose agent passes over a GSD gaps_found report does not finish; its gate names gaps_found and never says passed; GSD is asked from the worktree about the worktree's phase directory; a planted lie fails the never-lies check"
    requirement: GATE-03
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_gaps_found_report_never_reads_as_passed"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/supervisor_e2e.rs#a_run_that_passes_validate_finishes_and_keeps_its_worktree"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every GSD status and reader failure routes, after an agent pass and an agent fail, and the reader runs in the worktree evidence root"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_stale_report_routes_to_execute_phase"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_missing_report_routes_to_execute_phase"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_human_needed_report_gates_in_auto_mode"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::an_agent_fail_over_a_passed_report_gates_as_a_disagreement"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::the_reader_reads_the_worktree_evidence_root"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_reader_failure_at_validate_fires_a_never_silent_gate"
        status: pass
    human_judgment: false
  - id: D3
    description: "An agent PASS is recorded with the HEAD it ran on and survives a stop, a transition and a takeover's publish write; FAIL, ambiguous and a failed HEAD read record none"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_validate_pass_is_recorded_with_the_head_it_ran_on"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every gate records its kind: the nine Validate kinds and the three status-less kinds, a 3.x gate file loads as None"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/gates.rs#gates::tests::a_gate_round_trips_its_kind"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::each_validate_gate_records_its_kind"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::each_status_less_gate_records_its_kind"
        status: pass
    human_judgment: false
  - id: D5
    description: "The suites that reach Validate answer through a GSD stub, and OPERATIONS.md documents the paired verdict"
    verification:
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
    human_judgment: false
  - id: D6
    description: "A real validate-phase agent's verdict paired with a real GSD status"
    verification: []
    human_judgment: true
    rationale: "The plan's own stated limit (D-08): every test uses a stub agent and a stub gsd_run; no live run has paired them"

duration: 59min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 22: Validate passes only when GSD reports passed Summary

**A Validate now ends a run as finished only when the agent says pass and GSD's own `gsd query verification status`, run in the worktree, says `passed`. Every other status and every reader failure has its own route, mostly a gate that names GSD's status, and a reader failure is a `[never-silent]` gate. Every gate DevFlow opens records its kind in the gate file.**

## Performance

- **Duration:** about 59 min
- **Started:** 2026-11-11T05:57:17Z
- **Completed:** 2026-11-11T06:56:40Z
- **Tasks:** 4 (Task 1 tracer, Tasks 2 and 4 TDD, Task 3 auto)
- **Files modified:** 24 (1 created)

## Accomplishments

- `handle_validate_outcome` reads GSD's status once per outcome, for every verdict, in the evidence root (`state.worktree_path`, else the project root), and passes the pair to `validate_route`. `validate_route` has one arm per (verdict, status-or-failure) pair, 18 in all, and no wildcard arm that finishes.
- The routes: an agent pass over `passed` finishes (auto) or opens the `clean_pass` gate (supervise). `gaps_found` loops back gaps-only (auto) or opens a `gaps_found` gate. `stale` and `missing` loop back with the full execute command (auto) or open a `reverify_needed` gate that launches nothing until it is answered (supervise, U4). `human_needed` gates in both modes. An agent fail over `passed` or `human_needed` is a disagreement gate. A failure counter at its cap is a `failure_cap` gate. An ambiguous verdict is an `ambiguous_verdict` gate. A reader failure or an unreadable HEAD is a `reader_failure` gate, and takes precedence over every verdict.
- Gate contexts quote GSD's status verbatim. They say `passed` only when GSD did. A reader failure's context names the label and GSD's message, and its `gate_fired` event carries the label only (`run_gate_with_event_context`).
- An agent PASS records `State.validate_pass_head`, the evidence root's HEAD. Every other verdict records `None`. A HEAD that cannot be read is a reader-failure gate.
- `GateKind` has 13 variants and is not `#[non_exhaustive]`. `Gates::open_gate`, `open_registered`, `write_gate` and the CLI's `run_gate*` require one. The production gate sites and the kinds they pass:
  - `pipeline_outcomes::handle_validate_outcome`: the nine Validate kinds above, chosen by `validate_route`.
  - `pipeline_outcomes::answer_stage_failure_gate`, reached from `handle_stage_failure` (launch failure, crash, reported failure, timeout), `handle_infra_outcome` / `gate_or_abort_infra` and `handle_rate_limited_outcome`: `stage_failure`.
  - `preflight::run_preflight`: `preflight`.
  - the checkpoint re-scan gate in `pipeline_launch::advance` (the `run_gate_with_timeout` call in the `(Some(session_id), true)` arm): `checkpoint_declaration`.
  - `no_gap_closure_plan` is defined but nothing writes it yet. 52-27 writes it.
- `gsd_runtime` records GSD's version once per run, when `devflow start` begins it.
- OPERATIONS.md has a new "How Validate decides" section, and the Validate bullet names the paired verdict.

## Task Commits

1. **Task 1 (tracer):** `8d0404c` test (RED), `0a487d8` feat! (GREEN)
2. **Task 2:** `06c50ae` test (RED, with compile skeletons), `1cf0248` feat! (GREEN)
3. **Task 3:** `c367694` test (phase7_cli stub, OPERATIONS.md)
4. **Task 4:** `b8804a4` test (RED), `27fe5d4` feat! (GREEN)

The plan's context suggested the subject `feat(cli)!: Validate passes only when GSD reports passed`. The commits use the `52-22` scope instead, because the TDD gate validator greps `feat(52-22)`.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch nextest config (`--profile red`). Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `supervisor_e2e::a_gaps_found_report_never_reads_as_passed` | `RED_EVIDENCE_OK` | The run did not finish (supervise always gated), but the gate read `Validation passed — approve to finish the run?` with no GSD status. That is the planned failure: the gate said passed over `gaps_found`. |
| `a_stale_report_routes_to_execute_phase`, `a_missing_report_routes_to_execute_phase` | `RED_EVIDENCE_OK` | Task 1's catch-all opened a gate in auto mode over `stale` / `missing`, where no gate is the planned route. |
| `a_human_needed_report_gates_in_auto_mode` | `RED_EVIDENCE_OK` | The gate opened and named `human_needed`, but its file had no `gate_kind`. |
| `an_agent_fail_over_a_passed_report_gates_as_a_disagreement` | `RED_EVIDENCE_OK` | No gate opened: the agent fail looped back to Code without reading GSD. |
| `the_reader_reads_the_worktree_evidence_root` | `RED_EVIDENCE_OK` | The stub logged no call: an agent fail did not read GSD. |
| `a_reader_failure_at_validate_fires_a_never_silent_gate` | `RED_EVIDENCE_OK` | The context did not start with `[never-silent]`. |
| `each_validate_gate_records_its_kind` | `RED_EVIDENCE_OK` | Row 0 (`clean_pass`) had no `gate_kind`. |
| `a_validate_pass_is_recorded_with_the_head_it_ran_on` | `RED_EVIDENCE_OK` | `validate_pass_head` was `Null` after a PASS. |
| `gates::tests::a_gate_round_trips_its_kind` | `RED_EVIDENCE_OK` | The skeleton `open_gate_with_kind` ignored the kind. |
| `pipeline_launch::tests::each_status_less_gate_records_its_kind` | `RED_EVIDENCE_OK` | The Plan stage-failure gate had no `gate_kind`. It failed on its first row, so the RED run did not reach the later rows; GREEN shows each of them opens its gate. |

Gate sequence: `test` 8d0404c → `feat` 0a487d8; `test` 06c50ae → `feat` 1cf0248; `test` b8804a4 → `feat` 27fe5d4. Task 3 has no TDD gate. There is no REFACTOR commit.

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode was `end-of-phase`, and Task 1's verify is automated only. Its two-test chain was re-run after GREEN and exited 0, so expansion continued.

## Existing tests changed

These tests now provision a GSD stub, because a Validate outcome with no stub is a reader-failure gate:

- **Given a git repository as well (their agent PASS records a HEAD):** `validate_pass_advances`, `external_verify_agreement_finishes_the_run`. `a_passing_validate_at_the_ceiling_explains_why_it_gated` and `approving_a_passed_validate_gate_finishes_the_run` were already repositories.
- **A stub answering `gaps_found`, so the agent-fail auto loop is unchanged (`test_support::gsd_gaps_found`):** `consecutive_failures_reaches_ceiling_across_cycles`, `external_verify_cycles_reach_ceiling_without_unbounded_loop` (arm B), `phase_validate_failures_increment_saturates`, `ceiling_clause_appears_only_at_the_ceiling_even_in_supervise_mode`, `phase_validate_failure_ceiling_gates_despite_trivial_commit_progress`, `repeated_failure_without_new_commits_still_reaches_the_ceiling`, `validate_gate_message_leads_with_the_per_phase_total`, `loop_back_reason_is_distinct_when_no_commit_baseline_exists`, `validate_gaps_does_not_advance_to_ship`, `validate_missing_verdict_does_not_advance`, `a_ceiling_loop_back_persists_the_reset_and_the_verification_observation`.
- **A stub answering `passed`:** `approving_an_ambiguous_validate_gate_finishes_the_run`.
- **`pipeline_launch::tests::a_stop_after_a_gate_answer_stops_before_the_finish_and_reports_it`:** its Validate agent now reports `verdict: pass` and a stub answers `passed`. The stub reaches the spawned supervisors through `DEVFLOW_TEST_GSD_HOME`. Without that, its approval would rerun Validate instead of reaching `finish_run`.
- **`phase7_cli::cleanup_worktree_cli_flow`** provisions `stub_gsd_run` answering `gaps_found` and now asserts its gate is `failure_cap`. No other e2e suite reaches a Validate outcome. The rest only write Validate gate files, and those calls now pass `GateKind::StageFailure`. `supervisor_e2e`'s four 52-08 finish tests get a `passed` stub from `FinishRig`.

**`select_loop_back_fix` tests.** Validate no longer calls the heuristic. Its nine tests now call it directly through `heuristic_fix`, so 52-24 can delete them with it: `mid_arc_loop_back_issues_plain_execute_command`, `genuine_gaps_loop_back_still_issues_gaps_only`, `worktree_mode_genuine_gaps_loop_back_issues_gaps_only`, `worktree_mode_mid_arc_loop_back_issues_plain_execute`, `worktree_mode_main_checkout_only_artifact_is_the_or_both_roots_discriminator`, `stale_verification_artifact_dispatches_full_execute`, `verification_written_this_run_dispatches_gaps_only`, `an_uncaptured_baseline_does_not_claim_an_inherited_artifact` and `an_idempotent_rewrite_is_authored_not_inherited`. In `verification_written_this_run_dispatches_gaps_only`, the on-disk half of F-11 is gone (nothing persists the observation now), so it asserts the in-memory record instead. Two tests were deleted, because the behaviour they pinned (the heuristic at the ambiguous and failure-gated `LoopBack` arms) no longer exists: `ambiguous_gate_loop_back_respects_the_mid_arc_check` and `failure_gate_loop_back_respects_the_mid_arc_check`. 52-29 already planned to delete them if they survived.

## Decisions Made

See `key-decisions` in the frontmatter. Two of them need a note here:

- **The interim approve route.** The plan fixes an approve only for `reverify_needed` (relaunch Code, never finish). For the other gates I followed the D-45 table's status column, using the round's own read instead of a fresh one: an approval finishes only over `passed`. **52-08 behaviour this keeps:** approving an ambiguous gate over `passed` still finishes, although D-45 rule 1 would refuse it (an ambiguous verdict records no PASS). 52-29's `can_finish` decides that case.
- **The stub contract.** `runtime-identity` is answered without consuming a scripted reply. `query verification status <dir>` answers `missing` for a directory that does not exist (the plan's control). A run that reads more often than it scripted gets exit 97, which reaches DevFlow as a reader failure, never as a repeated answer.

## Open operator decision: untouched

When `devflow stop` lands just as a passing Validate finishes the run, the stop still wins (`finish_run`'s safe point 6, from 52-08). This plan did not change `finish_run`. Its one test, `a_stop_after_a_gate_answer_stops_before_the_finish_and_reports_it`, was re-provisioned (agent pass plus GSD `passed`) so it still drives that path. It passes and still asserts safe point 6, no `workflow_finished` and a stopped, unfinished run.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] An agent pass over a non-`passed` status counts as a Validate failure**
- **Found during:** Task 1
- **Issue:** Before this plan, only an agent fail incremented the counters. An agent that always says pass over a report with gaps would have looped in auto mode with no ceiling.
- **Fix:** `counted_failure` includes (pass, any status but `passed`). An ambiguous verdict and a pass whose evidence could not be read still touch no counter, because they gate at once.
- **Commits:** `0a487d8`, `1cf0248`

**2. [Rule 2 - Security] GSD's message stays out of events**
- **Issue:** `gate_fired` carried the full gate context, and a reader failure's context holds GSD's message, which can name absolute paths (Pitfall 11).
- **Fix:** `run_gate_with_event_context`. The reader-failure gate's event context has the label and no detail, and the `gsd_verification` event carries the label only. Asserted in `a_reader_failure_at_validate_fires_a_never_silent_gate`: no event contains the stub's message.
- **Commit:** `1cf0248`

**3. [Rule 1 - Bug] The ambiguous-verdict detail said "passed"**
- **Issue:** `external verification passed but …` would put `passed` in a gate context while GSD reports something else (GATE-03).
- **Fix:** the wording is now `external verification succeeded but …`, in the classifier and in its two test copies.
- **Commit:** `1cf0248`

**4. [Rule 3 - Blocking] The test seam for GSD's home**
- **Issue:** unit tests share the real `HOME`, and some run `advance` on a scoped thread or in spawned supervisor processes.
- **Fix:** the test build's `gsd_home` reads a stub registered for the evidence root (`GsdStub`, process-wide, keyed by root), then `DEVFLOW_TEST_GSD_HOME` (allowlisted in `doc-check-allowlist.toml`), and otherwise `None`, which is a missing launcher. A test build never reads the real `HOME` for GSD.
- **Commits:** `0a487d8`

**5. [Rule 3 - Blocking] The takeover row drives `save_publish_fields`, not `publish_record`**
- **Issue:** `monitor::Published` is `#[non_exhaustive]` with no constructor, so a CLI test cannot build one to pass to `publish_record`.
- **Fix:** the row calls `save_publish_fields`, the save `publish_record` makes in production (now `pub(crate)`), over a state naming a prior record.
- **Commit:** `06c50ae`

**6. [Order] Task 3 was committed before Task 4's RED**
- Task 4's RED test was first committed before Task 3. I undid that commit with `git reset --soft HEAD~1` (local, unpushed), set the test aside as a patch, ran Task 3's `scripts/check.sh all` on the Task 3 tree, committed Task 3, then re-applied the patch and committed it as `b8804a4`. Its RED evidence was recorded on the identical test.

**7. Files beyond `files_modified`:** `crates/devflow-core/src/test_support.rs` (the stub writer, shared with the e2e helper), `crates/devflow-core/src/recover.rs` and the e2e suites `doctor_json_e2e`, `gate_sweep_e2e`, `gate_wedge_e2e`, `log_format_env`, `recover_clean_e2e`, `start_lock_e2e`, `stop_e2e` and `crates/devflow-core/tests/devflow_dir_gitignore.rs` (their `Gates::*` fixture calls now pass a kind), and `doc-check-allowlist.toml`.

---

**Total deviations:** 7 (2 missing-critical, 1 bug, 2 blocking, 1 commit-order note, 1 file-list note)
**Impact on plan:** Each one is on the fail-closed path this plan exists for, or keeps the suite hermetic. No feature beyond the plan was added.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<verify>` (2 named supervisor_e2e runs; re-run as the tracer gate) | 0 |
| Task 2 `<verify>` chain (9 named runs, each `1 passed`) | 0 |
| Task 3 `<verify>`: `doc_check::doc_referenced_identifiers_exist_in_source` && `scripts/check.sh all` (on the Task 3 tree) | 0 |
| Task 4 `<verify>`: `each_status_less_gate_records_its_kind` && `a_gate_round_trips_its_kind` && `scripts/check.sh all` (on the final tree) | 0 |
| `cargo nextest run -p devflow --bin devflow` (bounded profile, after Task 2) | 0 (612 passed) |
| `cargo nextest run --workspace -E 'kind(test)'` (bounded, after Task 2) | 0 (295 passed) |
| `cargo nextest run -p devflow-core --lib` (after Task 2) | 0 (1048 passed) |
| `cargo semver-checks check-release -p devflow-core` | 100, expected |

semver-checks reports this plan's breaks: `constructible_struct_adds_field` for `GateFile.gate_kind`, and `method_parameter_count_changed` for `Gates::open_gate`, `open_registered` and `write_gate`. `State.validate_pass_head` is not flagged, since `State` is `#[non_exhaustive]`. The other findings are from earlier phase-52 plans. 4.0.0 covers all of them, and the version was not bumped. Known flake 999.221 did not occur. `commands::tests::stop_marks_stopped_after_ending_a_pending_supervisor_beside_a_dead_recorded_one` runs for about 30 s; it was already that slow in the Task 1 run and passes.

**What these checks do not establish:**
- No real validate-phase agent has been paired with a real GSD status (the plan's own stated limit; D6 above). Every status and failure shape comes from the stub, which uses 52-20's recorded gsd-core 1.20.1 shapes.
- `gsd_runtime` at `devflow start` is covered only through the e2e stub's version and the unit build's missing-launcher path. Nothing checks its event payload against a real GSD install.
- The interim approve and reject routes are tested for `reverify_needed` (approve), the disagreement (approve) and `failure_cap` (reject, via `phase7_cli` and the ceiling tests). There is no dedicated test for approving `gaps_found`, `human_needed` or a reader-failure gate. 52-29 replaces those arms.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: process-exec | crates/devflow-cli/src/pipeline_outcomes.rs (`record_gsd_runtime`, called from `commands::start`) | `devflow start` now runs GSD's `gsd_run runtime-identity --raw` under `$HOME`, through 52-20's bounded, hermetic runner (argv only, GIT_* removed, 60 s kill). It is the same launcher the Validate read runs, at one more point in time; the version is the only thing recorded |

## Known Stubs

- `GateKind::NoGapClosurePlan` is defined and nothing writes it. That is intended: 52-27 writes it (the plan's truth says so).

## Issues Encountered

- The first full bin run, before the existing tests had stubs, left a test waiting at a reader-failure gate with the 7-day default timeout. I killed that run by its recorded PIDs and bounded every later run with a nextest `slow-timeout` (terminate after 120 s) and a 20 s gate timeout in the new tests' children.
- A `cat >> /dev/null` in one command blocked on stdin (`cat` is `bat` here). I killed the `bat` process by its PID, and the edit after it ran.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-23. For 52-24: `select_loop_back_fix`, `verification_authored_this_run` and the `heuristic_fix` test helper are `#[cfg_attr(not(test), allow(dead_code))]`, and their nine tests are listed above. For 52-29: the ambiguous-approve finish noted under Decisions Made is still in place.

## Self-Check: PASSED

- Files exist: `crates/devflow-core/src/gates.rs` (`pub enum GateKind`, `gate_kind`), `crates/devflow-cli/tests/common/mod.rs` (`pub fn stub_gsd_run`), `crates/devflow-cli/src/pipeline_outcomes.rs` (`gsd_reader::verification_status` through `read_verification_status`, `fn validate_route`).
- Commits `8d0404c`, `0a487d8`, `06c50ae`, `1cf0248`, `c367694`, `b8804a4` and `27fe5d4` are ancestors of HEAD, and `git rev-list --count ad71cf0..HEAD` is 7.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
