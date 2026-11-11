---
phase: 52-subtract-and-stabilize
plan: 28
subsystem: validate-routing
tags: [rust, devflow-cli, devflow-core, gsd, validate, re-verify, failure-ceiling, tdd, semver-break]

requires:
  - phase: 52-22
    provides: "validate_route, the reverify_needed gate, the GSD stub seams, an agent PASS over a non-passed status counted as a Validate failure"
  - phase: 52-26
    provides: "State.code_fix_note beside code_fix, the note blocks, default_stage_prompt as the one no-override renderer"
  - phase: 52-27
    provides: "State.gap_closure_pass, the gap loop, handle_gaps_plan_outcome and its no_gap_closure_plan gate, D-34's .planning-only progress rule"
provides:
  - "FixType::StaleReverify: the re-verify pass for a stale or missing Validate (claude and workflow renderers)"
  - "prompt::ReverifyReport { status, route, next_action } and StageIntent::Code.reverify_report"
  - "State.code_fix_report: GSD's words for the re-verify pass, set with code_fix in the loop-back commit and cleared with it"
  - "gsd_reader::StatusReport.next_action"
  - "pipeline_outcomes::record_validate_failure as the one streak update, returning mode::phase_failure_ceiling_reached after it"
  - "a counted no_gap_closure_plan gate that names the per-phase ceiling and resets the count on a continuing answer"
affects: [52-29, 52-31, 52-32]

actuals:
  tokens: 17472
  tasks: 3
  commits: 5
plan_head_before: 50b8cba78ffafe70e011e870cbb4941bc2fe46ee
plan_head_after: 0a2e6057dd19632ab9aab7fd6f6dc5e74bb31e75

tech-stack:
  added: []
  patterns:
    - "GSD's own words ride a fix pass in state (code_fix_report beside code_fix), so a resumed pass quotes what the Validate read, not a re-read"
    - "One counting function per counted round, returning the ceiling read after its own update, so every gate that counts can name the ceiling the same way"

key-files:
  created:
    - .planning/phases/52-subtract-and-stabilize/52-28-SUMMARY.md
  modified:
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/gsd_reader.rs
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/commands.rs
    - OPERATIONS.md
    - .planning/phases/52-subtract-and-stabilize/deferred-items.md

key-decisions:
  - "Names: FixType::StaleReverify, prompt::ReverifyReport, StageIntent::Code.reverify_report, State.code_fix_report, StatusReport.next_action; the one streak update keeps its name record_validate_failure"
  - "GSD's status, route and next action are persisted in State.code_fix_report in the loop-back commit that sets code_fix, so a resumed re-verify pass quotes the same words without reading GSD again"
  - "Approving or rejecting a reverify_needed gate also takes the re-verify pass (was the full execute), so a stale or missing Validate reaches the same pass in both modes"
  - "A gaps Plan pass with no new gap-closure plan now counts one failure through the streak update (52-27 did not count it); its reader-failure gate still counts none"
  - "The ceiling reset on that gate runs on approve and on reject (both continue the run), never on abort"

patterns-established:
  - "The re-verify prompt: quote GSD's status, route and next_action, tell the agent to resume at the phase gates and re-run the verifier (#4682), and never name --gaps-only"

requirements-completed: [GATE-03, GATE-02]

coverage:
  - id: D1
    description: "The re-verify prompt quotes GSD's status, route and next action verbatim and asks to resume at the phase gates and re-run the verifier, in both renderers, with no --gaps-only; the full-execute prompt is the control"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::the_stale_reverify_prompt_quotes_gsd_status_and_route"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::fix_prompts_carry_the_chain_flag_token_only_where_it_reaches_execute_phase"
        status: pass
    human_judgment: false
  - id: D2
    description: "An auto Validate over stale relaunches Code with the re-verify pass; the Code pass's end reads no GSD status before Validate; the next fresh read decides (passed finishes, stale takes the pass again and never finishes)"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_stale_validate_runs_the_reverify_pass_then_validate"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_stale_report_routes_to_execute_phase"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_missing_report_routes_to_execute_phase"
        status: pass
    human_judgment: false
  - id: D3
    description: "A Validate over stale or gaps_found counts exactly one failure on each counter whatever the agent said (four rows from zero); three .planning-only re-verify rounds gate at the cap with no third Code pass; a passed finish counts nothing"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_validate_that_takes_the_reverify_route_counts_one_failure"
        status: pass
    human_judgment: false
  - id: D4
    description: "The re-verify route taken inside a gap-closure pass clears the gap marker and sets code_fix to the re-verify pass, so a stopped and resumed pass renders the re-verify prompt; a marker-set gaps Code pass stopped before Validate still renders --gaps-only"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_reverify_route_clears_the_gap_marker"
        status: pass
    human_judgment: false
  - id: D5
    description: "A counted no_gap_closure_plan gate reaching the per-phase ceiling names it in its own words (not 'Validation has failed'), an approve resets the count with one phase_failure_budget_reset event, an abort resets nothing; below the ceiling it counts one and resets nothing"
    requirement: GATE-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::a_counted_gate_at_the_phase_ceiling_resets_the_count_when_answered"
        status: pass
    human_judgment: false
  - id: D6
    description: "A stop keeps code_fix and code_fix_report (re-verify and gaps-only rows); a transition out of Code clears them (control)"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::a_stop_keeps_the_recorded_fix_pass"
        status: pass
    human_judgment: false
  - id: D7
    description: "OPERATIONS.md describes the re-verify pass, its counted round, the per-phase ceiling on a Plan-pass gate and the #6078 limit; doc_check and scripts/check.sh all pass"
    requirement: GATE-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false
  - id: D8
    description: "A real execute-phase agent given the re-verify prompt resumes at the phase gates and re-runs the verifier"
    verification: []
    human_judgment: true
    rationale: "The plan's stated limit: the prompt cannot make an agent re-verify (#6096). Every test uses a stub gsd_run and no agent; no real agent has received this prompt"

duration: 38min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 28: The Re-verify Pass for Stale and Missing Reports Summary

**A Validate that reads GSD's `stale` or `missing` now relaunches Code with an explicit re-verify pass (`FixType::StaleReverify`). The pass runs `/gsd-execute-phase N`, tells the agent to resume at the phase gates and re-run the verifier, and quotes GSD's status, route and next action. DevFlow reads nothing when that pass ends, and the next Validate's own read decides: only `passed` finishes. Every round over `stale`, `missing` or `gaps_found` counts once, so a re-verify agent that keeps skipping the verifier reaches the consecutive-failure gate. A gaps Plan pass that wrote no gap plan now counts through the same update, and at the per-phase ceiling its gate says so and resets on a continuing answer.**

## Performance

- **Duration:** about 38 min
- **Started:** 2026-11-11T10:32:13Z
- **Completed:** 2026-11-11T11:10:30Z
- **Tasks:** 3 (Task 1 tracer TDD, Task 2 TDD, Task 3 docs)
- **Files modified:** 10

## Accomplishments

- **The re-verify pass (Task 1).** `validate_route` sends `stale` and `missing` to `LoopBack(FixType::StaleReverify)` in auto mode. Approving or rejecting the supervise `reverify_needed` gate takes the same pass. Its prompt, in both renderers, has three parts:
  - GSD's status and route in backticks, and GSD's `next_action` quoted on its own line (control characters other than newline and tab removed, as an operator note is);
  - "resume at the phase gates ... and re-run the verifier" (#4682);
  - `/gsd-execute-phase N --auto` (claude style) or the execute-phase workflow file (workflow style), with the unattended decision policy. It never names `--gaps-only`.
- **GSD's words are persisted.** `prepare_loop_back_to_code` writes `State.code_fix_report` in the commit that sets `code_fix`. The same commit also clears the gap marker: 52-27 already clears it for every fix but `GapsOnly`. `default_stage_prompt` passes the report only while `code_fix` is `StaleReverify`, so a resume, failover or re-drive quotes what that Validate read. `transition` (both commits) and `finish_run` clear it with `code_fix`.
- **No read when the pass ends (D-45 rule 3).** The Code pass ends through `transition(Code → Validate)`, like any Code pass. The test counts the stub's status reads before and after that call, and the count does not change.
- **One streak update (Task 2).** `record_validate_failure` is the only place either counter is incremented. It now returns `mode::phase_failure_ceiling_reached` read after its own update:
  - `handle_validate_outcome` calls it at most once per outcome and takes `ceiling_gate` from its return;
  - `handle_gaps_plan_outcome` calls it before opening the `no_gap_closure_plan` gate. At the ceiling that gate keeps its kind and adds "This phase has counted N failure(s), reaching its per-phase failure ceiling of 10: an answer that continues the run restarts the count, and `devflow gate abort` does not." An approve or reject runs `reset_phase_failures_at_ceiling` (event `phase_failure_budget_reset`), and an abort does not. The Plan reader-failure gate counts nothing.
- **A stop keeps the fix pass (T4).** `commands::tests::a_stop_keeps_the_recorded_fix_pass` pins it. `persist_stopped_state` is unchanged.
- **OPERATIONS.md (Task 3).** The `stale`/`missing` row now points at the re-verify pass. New paragraphs cover the pass and its resume, the counted round that bounds a skipped re-verify (#6096), the counted Plan-pass gate at the ceiling, and D-45 rule 4's limit (open-gsd/gsd-core#6078, with re-running `/gsd-execute-phase N` by hand as the refresh).

## Task Commits

1. **Task 1 (tracer):** `8596070` test (RED, with skeletons), `dc0c96e` feat! (GREEN)
2. **Task 2:** `de08c7b` test (RED), `73183e8` feat (GREEN)
3. **Task 3:** `0a2e605` docs

The plan's context suggested the subject `feat(cli): re-verify stale reports through an explicit pass`. The commits use the `52-28` scope, because the TDD gate validator greps `feat(52-28)`. Every subject is 72 characters or fewer.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports, written by a scratch `--profile red` config. Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `prompt::tests::the_stale_reverify_prompt_quotes_gsd_status_and_route` | `RED_EVIDENCE_OK` | The skeleton `StaleReverify` rendered the full-execute prompt. It failed on the first planned assertion, that the status is quoted (`claude stale: GSD's status is quoted`). |
| `pipeline_outcomes::tests::a_stale_validate_runs_the_reverify_pass_then_validate` | `RED_EVIDENCE_OK` | `code_fix` was `FullExecute`, not `StaleReverify`: the 52-22 route. |
| `pipeline_outcomes::tests::a_validate_that_takes_the_reverify_route_counts_one_failure` | `RED_EVIDENCE_OK` | Row 1 (agent pass over `stale`) already counted `(1, 1)`, since 52-22 counts it. It then failed on the route assertion (`FullExecute`). The later rows were not reached at RED; GREEN runs all of them. |
| `pipeline_outcomes::tests::a_reverify_route_clears_the_gap_marker` | `RED_EVIDENCE_OK` | `code_fix` was `FullExecute`. The marker clear itself already held (52-27 clears it for any fix but `GapsOnly`), as 52-27's SUMMARY predicted. |
| `pipeline_outcomes::tests::a_counted_gate_at_the_phase_ceiling_resets_the_count_when_answered` | `RED_EVIDENCE_OK` | From 9, the `no_gap_closure_plan` gate opened with the count still at 9: the Plan-pass round was not counted. |
| `commands::tests::a_stop_keeps_the_recorded_fix_pass` | `INVALID_RED` / `unexpected_green` | Expected: the plan says `persist_stopped_state` already keeps `code_fix` (T4), and this test pins it. Mutation control, run by hand and reverted: adding `state.code_fix = None;` to `persist_stopped_state` failed it (`StaleReverify: a stop keeps the fix pass`). |

Gate sequence: `test` 8596070 → `feat` dc0c96e; `test` de08c7b → `feat` 73183e8. There is no REFACTOR commit.

Tracer gate: the run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode `end-of-phase`, and Task 1's verify automated only. Its four-command chain was re-run after the GREEN commit and exited 0, so expansion continued.

## Existing tests changed

- `pipeline_outcomes::tests::assert_a_reverify_report_routes_to_execute_phase`, the helper behind `a_stale_report_routes_to_execute_phase` and `a_missing_report_routes_to_execute_phase`: its auto rows and its supervise approve row now assert the loop-back fix `StaleReverify`, not `FullExecute`.
- `prompt::tests::fix_prompts_carry_the_chain_flag_token_only_where_it_reaches_execute_phase` gained a `StaleReverify` row.
- `prompt::tests::pre_fix_prompt_builder`, the historical copy of the builder, gained an `unreachable!` arm for the new variant.
- `gsd_reader::tests::a_stale_report_reads_as_stale_with_its_moved_files` now also asserts that `next_action` is read verbatim.
- Every `StageIntent::Code` literal, 17 in all across prompt.rs, agent_result.rs, pipeline_gate.rs and pipeline_launch.rs, gained `reverify_report: None`.

## Decisions Made

See `key-decisions`. Two need a note here:

- **The quote is persisted, not re-read.** D-45 rule 3 forbids a read when the pass ends. A read at relaunch would also mean another scripted stub reply in every resume test. So the Validate read is stored beside `code_fix`, and its lifetime is `code_fix`'s.
- **`record_validate_failure` keeps its name.** It was already the only place either counter is incremented, so the "extracted" update is this function with a return value and a second caller. Its doc now says it is the one streak update.

## Open operator decisions: untouched

- **Stop against finish at Validate's end.** A stop still wins. `finish_run` gained only the `code_fix_report` clear.
- **`phase_validate_failures` carried across a finished state into the next `start`.** `start` was not changed. The per-phase ceiling inside a run is this plan's.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Correctness] 52-27's `no_gap_closure_plan` gate did not count**
- **Found during:** Task 2 (reading `handle_gaps_plan_outcome`)
- **Issue:** The plan calls it "52-27's counted gate", but 52-27's code opened it without calling the streak update. Its doc said "it counts as a failure", but the code did not do it.
- **Fix:** It now calls `record_validate_failure` before opening the gate, as the plan's T10 test requires (from 9 → 10). In auto mode, a gaps Plan pass with no new gap plan now moves both counters by one.
- **Commit:** `73183e8`

**2. [Rule 2 - Consistency] The `reverify_needed` gate's answers take the re-verify pass**
- **Issue:** The plan routes `stale` and `missing` to the new arm. The supervise gate over the same statuses would still have relaunched the full execute, and its context said so.
- **Fix:** `approve_route` and `reject_fix` map `stale`/`missing` to `StaleReverify` and pass the round's report. The gate context now says the answers relaunch the re-verify pass. 52-29's routers replace these interim arms.
- **Commit:** `dc0c96e`

**3. [Rule 3 - Blocking] GSD's `next_action` was not read, and the quote needed a carrier**
- **Fix:** `gsd_reader::StatusReport.next_action: Option<String>`. It is `None` when absent, and a reader failure when present and not a string. The quote is carried by `ReverifyReport`, `StageIntent::Code.reverify_report` and `State.code_fix_report`.
- **Commit:** `dc0c96e`

**4. Files beyond `files_modified`:** `crates/devflow-core/src/state.rs`, `gsd_reader.rs` and `agent_result.rs` (one test literal), `crates/devflow-cli/src/pipeline_gate.rs`, and `.planning/.../deferred-items.md`.

---

**Total deviations:** 3 auto-fixed (2 correctness, 1 blocking) and 1 file-list note.
**Impact on plan:** Deviation 1 is the plan's own T10 premise made true. Deviations 2 and 3 are needed for the pass to quote GSD and to be reached the same way in both modes. No feature beyond the plan was added.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<verify>` chain (4 named runs), after GREEN and again as the tracer gate | 0, 0 |
| Task 2 `<verify>` chain (2 named runs) | 0 |
| Task 3 `doc_check::doc_referenced_identifiers_exist_in_source` | 0 |
| `cargo nextest run --workspace` after Task 1 GREEN | 100: 1 failure, `pending_supervisor_e2e::stop_ends_a_launch_its_running_launcher_promotes_during_the_wait`; 3/3 reruns alone passed (see Issues) |
| `cargo nextest run --workspace` after Task 2 GREEN | 0 (2012 passed) |
| `cargo clippy --workspace --all-targets -- -D warnings` (after each code change) | 0 |
| `scripts/check.sh all` on the final tree (after the last code change, with the OPERATIONS.md edit) | 0 (`==> check.sh: all OK`; 2014 passed, 0 failed across its `cargo test` result lines) |
| `cargo semver-checks check-release -p devflow-core` | 100, expected |

semver-checks reports one break from this plan: `enum_struct_variant_field_added` for `StageIntent::Code.reverify_report`. The other changes are not flagged:
- `FixType::StaleReverify`: `FixType` is `#[non_exhaustive]`.
- `State.code_fix_report`: `State` is `#[non_exhaustive]`.
- `StatusReport.next_action`: `gsd_reader` is new since the 3.0.0 baseline.

The other 14 findings come from earlier phase-52 plans. 4.0.0 covers all of them, and the version was not bumped. Known flakes 999.221 and 999.222 did not occur. `test_support::plant_gsd_run_stub` was not touched.

**What these checks do not establish:**
- No real agent has received the re-verify prompt (D8). The prompt cannot make an agent run the verifier (#6096). The counted round bounds a skipped re-verify but does not prevent one.
- The "Code launch carries the re-verify prompt" and "the resumed pass is the re-verify pass" assertions render `default_stage_prompt` from the state on disk. That function is the only renderer of a no-override launch, and a resume launches without an override. The tests spawn no agent, so the loop-back's own override prompt is not captured. It is built by `prepare_loop_back_to_code` from the same intent fields.
- "Clears the marker in the same commit that sets `code_fix`" comes from reading the code: one `commit` closure in `prepare_loop_back_to_code` makes both writes. The test checks the state after the route, not the number of commits.
- The no-read-after-Code assertion drives `transition(Code → Validate)` directly, which is the call advance's Code success arm makes. No full `advance` loop runs an agent here.
- A stale `gaps_found` report is not detected (D-45 rule 4, #6078). This is documented, not tested.

## Threat Flags

None. No new process, file or network surface. GSD's `next_action` text now reaches the re-verify prompt with control characters removed. It already came from the same hermetic reader. It is not written to any event: the `gsd_verification` event still carries only the status name.

## Known Stubs

None.

## Issues Encountered

- One full workspace run failed `pending_supervisor_e2e::stop_ends_a_launch_its_running_launcher_promotes_during_the_wait` at its liveness-file check (`Live`, not `Missing`). Three isolated reruns passed, and the next two full runs passed. This plan touches no stop, supervisor or liveness code. It is not one of the two known flakes, so it is logged in the phase's `deferred-items.md` (cause not established; suspected to be in 999.221's fork-inherits-fd family).

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-29. For 52-29:
- the interim `reverify_needed` answers in `approve_route` and `reject_fix` now take `StaleReverify` and pass `reverify_report(read)`;
- the `no_gap_closure_plan` gate's answers in `handle_gaps_plan_outcome` now reset the count at the ceiling before rerunning Plan or looping back;
- `loop_back_to_code` and `prepare_loop_back_to_code` take a `reverify_report` argument (ignored unless the fix is `StaleReverify`).

## Self-Check: PASSED

- Files exist: `crates/devflow-core/src/prompt.rs` (contains `re-run the verifier`), `crates/devflow-cli/src/pipeline_outcomes.rs` (contains `fn a_validate_that_takes_the_reverify_route_counts_one_failure`), `crates/devflow-cli/src/commands.rs` (contains `fn a_stop_keeps_the_recorded_fix_pass`).
- Commits `8596070`, `dc0c96e`, `de08c7b`, `73183e8` and `0a2e605` are ancestors of HEAD, and `git rev-list --count 50b8cba..HEAD` is 5.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
