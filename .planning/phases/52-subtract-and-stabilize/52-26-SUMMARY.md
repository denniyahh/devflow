---
phase: 52-subtract-and-stabilize
plan: 26
subsystem: gates
tags: [gates, prompt, state, gate-01, semver-break]

requires:
  - phase: 52-19
    provides: "an explicit abort field, so a rejection whose note says `do not abort, fix the test` loops back and its note must reach the relaunched agent"
  - phase: 52-25
    provides: "the tree this plan starts from (2a15a10)"
provides:
  - "GateAction::LoopBack { stage, note }: from_response keeps the rejection's note"
  - "State.code_fix_note (the fix pass's note, set and cleared with code_fix) and State.relaunch_note (a rejected stage-failure, preflight or re-scan gate's note for the stage's relaunch)"
  - "StageIntent: relaunch_note on Plan, Code and Validate, fix_note on Code; StageIntent::with_relaunch_note"
  - "one note-block renderer, prompt::note_block(introduction, note), used by every stage builder of both styles; pub FIX_NOTE_INTRODUCTION and RELAUNCH_NOTE_INTRODUCTION"
  - "fix_prompt(fix_type, phase, note)"
  - "pipeline_launch::record_relaunch_note; default_stage_prompt is pub(crate)"
  - "relaunch_note cleared by transition (both commits), prepare_loop_back_to_code and finish_run"
  - "the stage-write source scan pipeline_gate::tests::state_stage_is_written_only_by_transition_and_the_code_loop_back"
affects: [52-27, 52-29, 52-31, 52-32, gate answers, prompts]

actuals:
  tokens: 17756
  tasks: 3
  commits: 6
plan_head_before: 2a15a10513e7424f1391a8dadb1215c5b2e5017c
plan_head_after: 5e1fae17f31a7eddbded3c2c6064cdf6824dbf55

tech-stack:
  added: []
  patterns:
    - "Operator text reaches a prompt only through note_block: control characters except newline and tab removed, an END marker the note does not contain, otherwise byte-identical"
    - "A note lives as long as what it was given for: code_fix_note with code_fix, relaunch_note with the stage"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs

key-decisions:
  - "Names: StageIntent::Code { fix_note, relaunch_note }, StageIntent::with_relaunch_note, the renderer prompt::note_block(introduction, note) with note_blocks putting the fix request first"
  - "A rejection with a note replaces the relaunch note; a rejection with no note keeps an earlier one (record_relaunch_note), so a second failure answered without a note does not drop what the operator asked for at that stage"
  - "A note that is empty once control characters are removed renders no block"
  - "default_stage_prompt passes code_fix_note only while code_fix is set, so a stale fix note can never reach a plain Code pass"
  - "The preflight gate's and the checkpoint re-scan gate's rejections also write relaunch_note: both relaunch the stage they stopped at, and the plan requires that no LoopBack consumer drops the note"
  - "A reject on a canary-refused stage-failure gate keeps its note on disk for the `devflow resume --agent <other>` the stop names (the plan's decision, now tested)"

patterns-established:
  - "Note markers: `----- BEGIN OPERATOR NOTE -----` / `----- END OPERATOR NOTE -----`, numbered (` 2`, ` 3`, ...) until the END line does not occur in the note"

requirements-completed: [GATE-01]

coverage:
  - id: D1
    description: "A rejection note reaches the relaunched agent verbatim on both launch shapes: the Claude stream turn on stdin and the Codex argv"
    requirement: GATE-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#a_rejection_note_reaches_the_stream_prompt_verbatim"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/gate_wedge_e2e.rs#a_rejection_note_reaches_the_capture_prompt_verbatim"
        status: pass
    human_judgment: false
  - id: D2
    description: "The note block's byte contract in both renderers, with the pre-fix builder as the control; the relaunch note in every stage prompt; both notes in a Code pass, each under its own introduction"
    requirement: GATE-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::fix_prompt_carries_the_note_byte_identical"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::workflow_code_prompt_carries_the_note_byte_identical"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::the_note_block_strips_control_characters_except_newline_and_tab"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::a_note_holding_the_end_marker_cannot_close_the_block"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::the_pre_fix_prompt_builder_fails_the_verbatim_assertion"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::a_relaunch_note_reaches_every_stage_prompt_in_both_styles"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::a_code_pass_with_both_notes_introduces_each_by_its_source"
        status: pass
    human_judgment: false
  - id: D3
    description: "A replayed fix pass keeps its note; a rejected stage-failure gate relaunches Plan, Code and Validate with the note through relaunch_note (never code_fix_note); leaving the stage clears it; the stage-write scan pins the clear sites"
    requirement: GATE-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_replayed_fix_pass_keeps_the_note"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rejected_stage_failure_relaunches_the_stage_with_the_note"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::leaving_the_relaunched_stage_clears_the_relaunch_note"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#pipeline_gate::tests::state_stage_is_written_only_by_transition_and_the_code_loop_back"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::every_no_override_launch_renders_through_default_stage_prompt"
        status: pass
      - kind: other
        ref: "scripts/check.sh all"
        status: pass
    human_judgment: false

duration: 33min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 26: The Rejection Note Reaches the Agent Summary

**A rejected gate's note now reaches the relaunched agent byte for byte, on the Claude stream shape and the Codex capture shape. A verification gate's note rides the fix pass (`State.code_fix_note`, kept across a resume or failover replay). A stage-failure gate's note rides the relaunch of that stage (`State.relaunch_note`, at Plan, Code and Validate), and is cleared on every way out of the stage. One renderer, `note_block`, puts operator text into every prompt of both styles.**

## Performance

- **Duration:** about 33 min
- **Started:** 2026-11-11T09:00:00Z
- **Completed:** 2026-11-11T09:33:17Z
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments

- `GateAction::LoopBack { stage, note }`. `from_response` copies the response's note into it.
- `State.code_fix_note` is set in the commit that sets `code_fix`. It is cleared with `code_fix` in `transition` (both commits) and `finish_run`. `State.relaunch_note` is committed by a rejected stage-failure gate before `relaunch_failed_stage` runs, and also by a rejected preflight gate and a rejected checkpoint re-scan gate. Both fields are `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- `default_stage_prompt` fills `fix_note` (only while `code_fix` is set) and `relaunch_note` on the Code literal. For Plan and Validate it calls `.with_relaunch_note(..)` on the intent that the single `for_stage_in_project(` call returns. `every_no_override_launch_renders_through_default_stage_prompt` is unchanged and passes.
- `note_block(introduction, note)` renders every note block in both styles, just before the completion contract. When both notes are set, the fix request comes first. A prompt with no note is byte-identical to the old one, and no `.snap` file changed.
- `relaunch_note` is cleared in `transition`'s `--until` halt and its ordinary commit, in `prepare_loop_back_to_code`'s stage-changing commit, and in `finish_run`.

## Task Commits

1. **Task 1 RED:** `7ab930d` test(52-26): a rejection note must reach the stream prompt verbatim
2. **Task 1 GREEN:** `fe09efd` feat(52-26)!: carry the rejection note to the relaunched agent
3. **Task 2 RED:** `a2200c0` test(52-26): pin the note block's byte contract in both renderers
4. **Task 2 GREEN:** `d341a51` feat(52-26): render the operator's notes in the workflow-style prompts
5. **Task 3 RED:** `24bd704` test(52-26): the relaunch note must leave with its stage
6. **Task 3 GREEN:** `5e1fae1` feat(52-26): clear the relaunch note when the run leaves its stage

**Plan metadata:** the docs commit that adds this file.

## The note block's contract

```
<introduction>

----- BEGIN OPERATOR NOTE -----
<note>
----- END OPERATOR NOTE -----

```

- **What it protects.** The note's bytes are kept, and so is the block's boundary. Every printable character, every non-ASCII letter, newline and tab come through unchanged and in order. Every other `char::is_control()` character is removed: ESC, CR, BEL, DEL and the C1 controls. The END line is the first of `----- END OPERATOR NOTE -----`, `... 2 -----`, and so on that does not occur in the note, so a note can never close its own block. The note travels by argv or stdin (a JSON string in the stream turn) and never through a shell.
- **What it does not protect.**
  - The note is operator text, and the agent is told to act on it. Nothing weighs what it says.
  - Unicode format characters are not control characters, so they pass through. This includes bidi overrides such as U+202E and zero-width characters.
  - A note can contain a fake BEGIN line, or a line that starts with `DEVFLOW_RESULT:`. A note line that declares a `blocking-human` checkpoint would break the invariant `no_line_start_marker_in_a_code_prompt_declares_a_checkpoint` pins for the prompt's own text, but only if the agent's capture echoes the prompt. This was not tested.
  - The e2e fakes prove what the agent process received. They do not show how a real model weighs the note against the rest of the prompt.
- **Introductions.** `FIX_NOTE_INTRODUCTION` says the operator rejected the previous result and left the note for this fix pass, to act on. `RELAUNCH_NOTE_INTRODUCTION` says this is a relaunch of the stage and that the operator left the note when rejecting the gate the stage stopped at. It names no gate kind.

## Every `GateAction::LoopBack` consumer

| Site | What it does with the note |
|---|---|
| `pipeline_outcomes::answer_validate_gate` (rejected verification gate) | Passes it to `loop_back_to_code` → `prepare_loop_back_to_code`, which commits it as `code_fix_note` with `code_fix` and renders it in the returned fix prompt |
| `pipeline_outcomes::answer_stage_failure_gate` | `record_relaunch_note` before `relaunch_failed_stage`; `code_fix`/`code_fix_note` untouched |
| `preflight::run_preflight` (preflight gate) | `record_relaunch_note` before `launch_stage` |
| `pipeline_launch` checkpoint re-scan gate | `record_relaunch_note`, then the Auto park or the Supervise fall-through to the stage-failure gate |
| `pipeline_gate` `gate_resolved` event | `LoopBack { .. }` → the label `loop_back`. This is an event label, not a relaunch |
| `commands::stop_with_existing_response` | `LoopBack { .. }` → reports a non-abort answer and writes nothing. This is not a loop-back path |
| `mode.rs` | Doc comment only. No code consumer, so no change |

`ApproveRoute::LoopBack` and `ValidateRoute::LoopBack` are not `GateAction` values. They come from an approval or from an unattended loop-back, so they carry no rejection note and pass `None`.

**Construction sites passing no note:** `pipeline_gate::prepare_loop_back_to_code` (relaunch_note `None`), `agent_result.rs`'s test module, the `prompt.rs` tests, and three `pipeline_launch.rs` test sites: the loop-back render near the stream-turn test and the two Codex `code_prompt` closures. `StageIntent::for_stage_in_project` builds none.

## Stage-write sweep at this wave

The scan rule:
- Take the non-test text, before `"\n#[cfg(test)]\nmod tests {"`.
- Drop each line from its first `//`.
- Count each match of `\b([A-Za-z_]\w*)\.stage\s*=(?![=>])` whose receiver is not `spec`.

It finds exactly two writes:
- `crates/devflow-cli/src/pipeline_gate.rs` L121 `s.stage = to;` in `fn transition`. Its clear sites are L132 for the ordinary commit and L92 for the `--until` halt, which writes no stage.
- `crates/devflow-cli/src/pipeline_gate.rs` L306 `s.stage = Stage::Code;` in `fn prepare_loop_back_to_code`. Its clear site is L315.

`finish_run` ends the run without a stage write, and clears at L191. A bare `.stage = ` count over the same text finds 4. The plan counted 6 at 5a01b30, and the two comment lines in that count have since moved or gone. The 4 are those two writes, one comment line (pipeline_gate.rs L71) and `spec.stage = invocation.stage` (pipeline_launch.rs L1740).

Planted controls:
- `persisted.stage = Stage::Plan;` counts.
- `// state.stage = to`, `spec.stage = a;`, `a.stage == b`, `!= state.stage =>` and `let stage` count none.
- A write after the test-module marker counts none.

Limits, also in the test's doc comment: a write through a struct literal, through `std::mem::replace` or through a `&mut s.stage` borrow is not seen, and a `//` inside a string literal ends that line's scanned text early.

## Snapshots

No `.snap` changed: `git diff --stat 2a15a10..HEAD -- '*.snap'` is empty. Every snapshot intent has no note, and a prompt without a note is byte-identical by construction.

## Decisions Made

See `key-decisions` in the frontmatter. One decision is new and not in the plan: a reject with no note keeps an earlier relaunch note.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Correctness] The preflight and checkpoint re-scan rejections also record the note**
- **Found during:** Task 1
- **Issue:** Both are `GateAction::LoopBack` consumers that relaunch the stage they stopped at. The plan lists only the stage-failure gate, but it also requires that no consumer drops the note.
- **Fix:** Each calls `record_relaunch_note` before relaunching.
- **Commit:** `fe09efd`

**2. [Rule 1 - Bug] `code_fix_note` is cleared with `code_fix` in Task 1, not Task 3**
- **Issue:** Between Task 1 and Task 3, a stale fix note would otherwise outlive its fix pass. `default_stage_prompt` also passes the fix note only while `code_fix` is set.
- **Commit:** `fe09efd`

**3. [Rule 3 - Blocking] e2e fixtures**
- The stream e2e needed a confirmed delivery canary seeded in state, plus a `.planning/config.json` and a plan file (`init_launchable_repo`). Without them, the relaunch stopped at the canary or at the Code preflight gate and no agent started.
- The capture e2e runs in supervise mode, because auto mode refuses an unattended Code stage on the capture shape.
- `spawn_gated_advance_as` was added to seed the agent and the state.
- **Commits:** `7ab930d`, `24bd704`

**4. [Visibility] `pipeline_launch::default_stage_prompt` widened to `pub(crate)`**
- The plan allowed this. The clear test builds the replayed intent through it.
- **Commit:** `24bd704`

---

**Total deviations:** 4 (1 Rule 2, 1 Rule 1, 1 Rule 3, 1 planned visibility widening)
**Impact on plan:** No scope added beyond the consumers the plan already required to keep the note.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports (a scratch `--profile red` config). Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `gate_wedge_e2e::a_rejection_note_reaches_the_stream_prompt_verbatim` | `RED_EVIDENCE_OK` (exit 100) | The relaunched Code agent read a plain Code prompt with no note. It failed on the verbatim `contains` assertion. A first attempt timed out at the preflight gate and was not counted as RED; the fixture was fixed and the test re-run. |
| `prompt::tests::workflow_code_prompt_carries_the_note_byte_identical` | `RED_EVIDENCE_OK` | The workflow renderer ignored the note. |
| `prompt::tests::a_relaunch_note_reaches_every_stage_prompt_in_both_styles` | `RED_EVIDENCE_OK` | Failed at `workflow Plan`. The claude-style rows passed. |
| `prompt::tests::a_code_pass_with_both_notes_introduces_each_by_its_source` | `RED_EVIDENCE_OK` | Failed at `workflow GapsOnly`. |
| `prompt::tests::{fix_prompt_carries_the_note_byte_identical, the_note_block_strips_control_characters_except_newline_and_tab, a_note_holding_the_end_marker_cannot_close_the_block, the_pre_fix_prompt_builder_fails_the_verbatim_assertion}` | `INVALID_RED` / `no_target_test_failure` (green) | Task 1 already shipped the claude-style block and the sanitizer, so the plan's "RED for those Task 1 does not already satisfy" applies. The control test is green by design. |
| `pipeline_gate::tests::leaving_the_relaunched_stage_clears_the_relaunch_note` | `RED_EVIDENCE_OK` | `plan -> code: the transition clears the relaunch note`: left was `Some(note)`, right `None`. |
| `pipeline_gate::tests::state_stage_is_written_only_by_transition_and_the_code_loop_back`, `pipeline_launch::tests::{a_replayed_fix_pass_keeps_the_note, a_rejected_stage_failure_relaunches_the_stage_with_the_note}`, `gate_wedge_e2e::a_rejection_note_reaches_the_capture_prompt_verbatim` | green at RED | The scan pins a property that already held. Task 1 had already wired the replay and the stage-failure write, and Task 2 the workflow render. |

Mutation controls for the tests that were green at RED were run by hand and then reverted:
- Removing the `record_relaunch_note` call failed `a_rejected_stage_failure_relaunches_the_stage_with_the_note` (left `None`).
- Setting `fix_note: None` in `default_stage_prompt` failed `a_replayed_fix_pass_keeps_the_note`.
- Dropping the relaunch note from the workflow Code render failed the capture e2e.
- Removing each of the four `relaunch_note = None` clears, one at a time, failed `leaving_the_relaunched_stage_clears_the_relaunch_note` on that clear's own row: the halt, the transition, `finish_run` and the loop-back.

- **Gate sequence:** `test` 7ab930d → `feat` fe09efd; `test` a2200c0 → `feat` d341a51; `test` 24bd704 → `feat` 5e1fae1. There is no REFACTOR commit.
- **Tracer gate:** the run is interactive (`auto_advance` and `_auto_chain_active` are false), the mode is `end-of-phase`, and Task 1's verify is automated only. Both commands were re-run after GREEN (exit 0) and the full `gate_wedge_e2e` suite passed (9/9), so expansion continued.

## Verification

| Command | Exit |
|---|---|
| Task 1 `<automated>` (2 named runs) | 0 |
| Task 2 `<automated>` (7 named runs, each `1 passed`) | 0 |
| Task 3's 5 named tests | 0 |
| `cargo nextest run -p devflow-core --lib` (1071 tests, after Task 2) | 0 |
| `cargo nextest run -p devflow --bin devflow` (606 tests, after Task 1) | 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` (after the last change) | 0 |
| `scripts/check.sh all` on the final tree, after the last code change | 0 (`==> check.sh: all OK`) |
| `cargo semver-checks check-release -p devflow-core` | 100, expected |

semver-checks reports 15 major checks failed. This plan adds three of them: `enum_struct_variant_field_added` (the four new `StageIntent` fields), `enum_tuple_variant_changed_kind` (`GateAction::LoopBack`) and `function_parameter_count_changed` (`fix_prompt`). The rest predate it. `State` is `#[non_exhaustive]`, so its new fields are not a break. 4.0.0 covers these breaks. The version was not bumped.

Known flakes 999.221 and 999.222 did not occur.

**What these checks do not establish:**
- The fix-request carrier (`code_fix_note`) is pinned by the renderer tests and the replay test. No e2e drives a rejected verification gate end to end; that is 52-29's `rejecting_a_clean_validate_pass_runs_gsd_quick_with_the_note`.
- The clear test drives `transition`, `prepare_loop_back_to_code` and `finish_run` directly, not through the Validate routes that call them. That those routes reach Code only through `prepare_loop_back_to_code` rests on the stage-write scan.
- The stream e2e compares the decoded `message.content` of the JSON stdin turn. The agent's raw stdin is JSON-escaped.
- No live DevFlow run (the installed binary) has carried a note.

## Issues Encountered

- The first stream RED run timed out at the Code preflight gate (no `.planning/config.json`). It was treated as not-RED and the fixture was fixed.

## User Setup Required

None.

## Next Phase Readiness

Ready for 52-27. 52-27 adds `relaunch_note` to its gaps Plan intent and a `transition`-out-of-Validate row. 52-29 keeps the `relaunch_note` write in its routers. 52-32 reuses `RELAUNCH_NOTE_INTRODUCTION` for the rate-limit reject. The two open operator decisions are unchanged: stop wins over finish at Validate's end, and `phase_validate_failures` carries over a finished state.

## Self-Check: PASSED

- Commits `7ab930d`, `fe09efd`, `a2200c0`, `d341a51`, `24bd704` and `5e1fae1` are ancestors of HEAD, and `git rev-list --count 2a15a10..HEAD` is 6.
- The artifact strings `fn fix_prompt_carries_the_note_byte_identical`, `pub code_fix_note` and `pub relaunch_note` exist in their files.
- `scripts/check.sh all` exited 0.

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*
