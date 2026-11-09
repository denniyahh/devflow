---
phase: 47-unattended-decision-policy-consistency
plan: 04
subsystem: testing
tags: [insta, snapshot, prompt, decision-policy, checkpoint-resume, d-15]

requires:
  - phase: 47-01
    provides: "insta dev-dependency and the hardened run_test (`env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`) that makes a baseline a failing guard"
  - phase: 47-03
    provides: "the final gate-rule text (GATE_RESOLUTION_RULE, split CODE_STAGE_POLICY sentence) and `resume_launch_shape`, the turn-2 constructor"
provides:
  - "Six named core baselines: the FullExecute fix prompt as each adapter delivers it (4 claude-style, 2 workflow-style)"
  - "Two named CLI baselines: turn 1 (PipeOwning stdin) and turn 2 (resume argv[1]), captured separately"
  - "Observed: a missing baseline fails the pinned run for both new tests (exit 101, nothing written)"
affects: [47-05, "D-15 snapshot suite", "phase 47 verification/review"]

actuals:
  tokens: 5845        # chars/4 over the realized diff 6f748e1..75c2a4c (23382 chars, 10 files, 306 insertions)
  tasks: 2
  commits: 2          # MEASURED: git rev-list --count 6f748e1..HEAD at SUMMARY write (excludes this docs commit)
plan_head_before: 6f748e1376649812d3eea76b4ca603929545246b

tech-stack:
  added: []
  patterns:
    - "Per-adapter snapshots are walked through an exhaustive AgentKind match chain, so a new variant is a compile error until it has a baseline"
    - "Delivered-turn snapshots are taken from the same captures the delivery test asserts on, as two named files, never joined"

key-files:
  created:
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap
    - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_one_code_prompt.snap
    - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap
  modified:
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-cli/src/pipeline_launch.rs

key-decisions:
  - "The six core baselines are emitted by one test that walks every AgentKind through an exhaustive match chain, as the D-07 test does, plus a visited.len() == 6 check"
  - "The two turn snapshots live inside the_gate_rule_holds_in_both_delivered_turns, not a sibling test, so they snapshot the exact turn1/turn2 values the gate-rule assertions check"
  - "Every adapter goes through driver_for(kind).render_prompt, including codex and pi: their workflow_root() returns the literal string \"$HOME/...\" (never expanded), so the baselines are host-independent"

patterns-established:
  - "Before blessing a new snapshot, run it pinned (INSTA_UPDATE=no) with the baseline absent and require a printed FAILED with nothing written"

requirements-completed: [DECN-02, DECN-03]

coverage:
  - id: D1
    description: "Six named policy-carrying core baselines committed, all seven core .snap files carry the policy heading, both render styles represented"
    requirement: DECN-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::policy_carrying_full_execute_fix_prompt_snapshots"
        status: pass
      - kind: other
        ref: "47-04-PLAN.md Task 1 <automated> gate (tracked_core_snapshots=7 policy_carrying_snapshots=7 workflow_style_snapshots=2 claude_style_snapshots=5 check_test_exit=0 gate_rc=0)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The two delivered turns are separate named baselines, distinct, neither joined"
    requirement: DECN-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns"
        status: pass
      - kind: other
        ref: "47-04-PLAN.md Task 2 <automated> gate (joined_capture_files=0 turn_one_snapshot_files=1 turn_two_snapshot_files=1 two_snapshots_identical_exit=1 check_all_exit=0 gate_rc=0)"
        status: pass
    human_judgment: false
  - id: D3
    description: "A missing baseline fails the pinned run for both new tests rather than passing or writing a file"
    requirement: DECN-02
    verification:
      - kind: other
        ref: "env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test (each new test, before blessing): exit 101, 0 passed; 1 failed, 0 .snap/.snap.new written"
        status: pass
    human_judgment: false
  - id: D4
    description: "The text in the eight new baselines is the correct, intended wording (the reviewed-baseline property D-15 exists for)"
    verification: []
    human_judgment: true
    rationale: "Correctness of a golden text is a judgment. The executor read every file and byte-compared bodies, but the two workflow-style baselines (codex, pi) and the turn-2 baseline are text never baselined before this plan. Phase review should read them independently rather than trust the executor's reading."

duration: 9min
completed: 2026-09-11
status: complete
---

# Phase 47 Plan 04: D-15 Snapshot Suite Summary

**Named `insta` baselines for all six adapters' FullExecute fix prompts and for the two delivered resume turns, captured separately from the production constructors and taken against the final 47-03 text. The full `scripts/check.sh all` is green with them in place.**

## Performance

- **Duration:** 9 min
- **Started:** 2026-09-11T19:28:13Z
- **Completed:** 2026-09-11T19:37:14Z
- **Tasks:** 2
- **Files modified:** 10 (8 `.snap` created, 2 source files modified)

## Accomplishments

- A wording change to the FullExecute fix prompt on any of the six adapters now surfaces as a diff on
  that adapter's own `.snap` file. If an adapter switched render style, its file would change too.
- The two turns a resumed Claude session receives are baselined as two independently reviewable
  files. The turn-1 file holds the policy and carve-out, and the turn-2 file names `blocking-human`.
  No file holds both.
- Both new tests were observed failing on an absent baseline under the pinned invocation, so a
  deleted `.snap` does not pass silently.

## Task Commits

1. **Task 1: named baselines for the six policy-carrying prompts** - `8d71f4d` (test)
2. **Task 2: snapshot the two delivered turns separately** - `75c2a4c` (test)

## Every `.snap` file the phase now tracks

`git ls-files "*.snap"` returns 9 files. Each was read in full before its content was relied on:

| File | Created by | Read before staging |
|---|---|---|
| `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap` | 47-01 (re-blessed 47-02, 47-03) | Not staged by this plan. Read in this session, and its body is byte-identical (sha256 prefix `7ecb1e714d8dcb01`) to the four claude-style baselines below |
| `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap` | 47-04 Task 1 | Yes |
| `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap` | 47-04 Task 1 | Yes |
| `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap` | 47-04 Task 1 | Yes |
| `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap` | 47-04 Task 1 | Yes |
| `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap` | 47-04 Task 1 | Yes |
| `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap` | 47-04 Task 1 | Yes |
| `crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_one_code_prompt.snap` | 47-04 Task 2 | Yes |
| `crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap` | 47-04 Task 2 | Yes |

**What the reading checked, beyond reading by eye:**

- **Carve-out text.** Every core `.snap` contains the 47-03 pinned section (the package-verification
  prohibition followed by the blocking-human carve-out) exactly once. None contains the old combined
  phrase `gate or a package-verification checkpoint`.
- **Claude-style redundancy.** The five claude-style bodies (the four new ones plus the 47-01
  baseline) produce 1 distinct sha256. The turn-1 body has the same sha (`diff` exit 0 against the
  claude baseline).
- **Workflow-style pair.** codex and pi differ on exactly one line, the workflow root:
  `$HOME/.codex/gsd-core/workflows` vs `$HOME/.pi/agent/gsd-core/workflows`. As a control, claude vs
  codex bodies differ (`diff` exit 1).
- **Turn markers.**
  - Turn 1: policy heading 1, `headless DevFlow run` 0, `package-verification` 1, carve-out clause 1.
  - Turn 2: policy heading 0, `headless DevFlow run` 1, `blocking-human` 1, `package-verification`
    0, carve-out clause 0.
  - `cmp` of the two files exits 1.

## Automated gate counters (as printed)

| Gate | Printed counters | Exit |
|---|---|---|
| Task 1 (one run) | `tracked_core_snapshots=7` `policy_carrying_snapshots=7` `workflow_style_snapshots=2` `claude_style_snapshots=5` `check_test_exit=0` `gate_rc=0` | 0 |
| Task 2 (one run) | `tracked_cli_snapshots=2` `turn1_snapshots=1` `turn2_snapshots=2` `joined_capture_files=0` `turn_one_snapshot_files=1` `turn_two_snapshot_files=1` `pair_first=…__turn_one_code_prompt.snap` `pair_second=…__turn_two_resume_prompt.snap` `turn_one_policy_heading=1` `turn_two_policy_heading=0` `turn_two_blocking_human=1` `two_snapshots_identical_exit=1` `check_all_exit=0` `gate_rc=0` | 0 |

**`turn2_snapshots=2` is not a duplicate.** That counter counts files containing `blocking-human`. The
turn-1 file also contains it, inside the carve-out sentence, so the count is 2. The counter is weak by
construction. The name-selected checks (`turn_two_policy_heading=0`, `turn_two_blocking_human=1`,
`two_snapshots_identical_exit=1`) are the ones that discriminate.

**Every full-suite run in this plan, with its result.** Each gate ran exactly once, with no re-runs.
Neither pre-existing race recorded in `deferred-items.md` fired.

| Run | Command | Exit | Totals (from the log) |
|---|---|---|---|
| Task 1 gate | `scripts/check.sh test` | 0 | 30 `test result: ok`, 0 `FAILED`, 1282 passed, 0 failed, 0 `.snap.new` |
| Task 2 gate | `scripts/check.sh all` (fmt, clippy `--workspace --all-targets -D warnings`, pinned test) | 0 | fmt and clippy clean (0 warning/error lines), 30 ok, 0 FAILED, 1282 passed, 0 failed, `check.sh: all OK`, 0 `.snap.new` |

The totals reconcile. 47-03 recorded 1281, and this plan added one test function
(`policy_carrying_full_execute_fix_prompt_snapshots`). Task 2 added assertions to an existing test
and no new function.

**Targeted runs (not full-suite):**

| Run | Command | Exit | Result |
|---|---|---|---|
| T1 control | `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test -p devflow-core --lib prompt::tests::policy_carrying_full_execute_fix_prompt_snapshots`, no baselines yet | 101 | `snapshot assertion for 'full_execute_fix_prompt_claude' failed`, `0 passed; 1 failed; 769 filtered out`, 0 files written |
| T1 bless | `INSTA_UPDATE=always cargo test -p devflow-core --lib …` | 0 | 6× `updated snapshot`, `1 passed; 769 filtered out` |
| T1 pinned after | `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test -p devflow-core --lib …` | 0 | `1 passed; 769 filtered out` |
| T2 control | `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test -p devflow --bin devflow pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns`, no baselines yet | 101 | `snapshot assertion for 'turn_one_code_prompt' failed`, `0 passed; 1 failed; 363 filtered out`, snapshot dir not created |
| T2 bless | `INSTA_UPDATE=always cargo test -p devflow --bin devflow …` | 0 | 2× `updated snapshot`, `1 passed; 363 filtered out` |
| T2 pinned after | `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test -p devflow --bin devflow …` | 0 | `1 passed; 363 filtered out` |

Every targeted run printed `running 1 test` with a non-zero filtered-out count, so none was a
zero-match filter.

## Files Created/Modified

- `crates/devflow-core/src/prompt.rs` — adds `policy_carrying_full_execute_fix_prompt_snapshots`.
  The test walks every `AgentKind` through an exhaustive match. It snapshots
  `driver_for(kind).render_prompt(Code { phase 47, fix: FullExecute })` under the name
  `full_execute_fix_prompt_<adapter>`. The doc comment states that the four identical claude-style
  baselines are deliberate. `insta` is written fully qualified, with no `use insta`.
- `crates/devflow-cli/src/pipeline_launch.rs` — `the_gate_rule_holds_in_both_delivered_turns` now ends
  with `insta::assert_snapshot!("turn_one_code_prompt", turn1)` and
  `insta::assert_snapshot!("turn_two_resume_prompt", turn2)`, with a comment on why the two are never
  joined.
- Eight `.snap` baselines, listed above.

## Decisions Made

- **Snapshots inside the existing delivery test.** The plan allowed this or a sibling test. Inside the
  test, the snapshots use the exact `turn1`/`turn2` values the gate-rule and distinctness assertions
  already checked. No second capture can drift from the first.
- **All six adapters through `driver_for`.** Codex and Pi could have been rendered with
  `render_workflow_style` and a fixed root. Going through the drivers means the codex/pi baselines
  also catch a change in a driver's workflow root. That is hermetic because `workflow_root()` returns
  a literal `"$HOME/…"` string (read in `agents/mod.rs:129-131` and `agents/pi.rs:42-44`) and never
  expands it.
- **The 47-01 test and baseline are unchanged**, as the plan requires.
- **No `GapsOnly`/`AuditFix` baselines** (D-15 scope).

## Deviations from Plan

None - plan executed exactly as written.

The added negative controls (the missing-baseline runs) and the `visited.len() == 6` assertion are
additive checks inside the plan's own scope, not changes to it.

## Issues Encountered

None. The two deferred test races (`deferred-items.md`) did not fire in either full-suite run. Two
runs is no evidence about their rate.

## TDD Gate Compliance

`workflow.tdd_mode` is on. This plan is `type: execute`, and neither task carries `tdd="true"` or a
`<behavior>` block. `gsd-tools query task.is-behavior-adding` on this plan returned
`is_behavior_adding: false`, so the MVP+TDD gate did not apply. Both commits are `test(47-04)`:
test-only changes that baseline behaviour 47-02 and 47-03 already built. Discrimination is shown by
the two missing-baseline controls above, not by a RED/GREEN pair.

## Threat Model Coverage

- **T-47-13 (blessing without reading):** mitigated.
  - Both blesses ran only after 47-03's final text had landed.
  - All eight new files were read before `git add`.
  - The pinned carve-out section was counted in every core file.
  - Both gates pin required content.
- **T-47-14 (joined capture):** mitigated. `joined_capture_files=0`, `two_snapshots_identical_exit=1`,
  and turn 2 has no policy heading.
- **T-47-15 (scope creep):** mitigated. `policy_carrying_snapshots=7` equals
  `tracked_core_snapshots=7`, and no `GapsOnly`/`AuditFix` baseline exists.
- No new security surface: test code and golden text files only.

## What this does NOT establish

- **That the baseline text is right.** A snapshot makes drift visible; it does not certify the
  wording. The codex, pi and turn-2 texts had never been baselined before (coverage D4, human
  judgment).
- **CI container parity.** Every run was on the host toolchain. `scripts/check-in-container.sh` and
  the CI Test job have not run these baselines.
- **Repeatability.** Each gate ran once, and each control ran once.
- **A full adapter guarantee.** `visited.len() == 6` does not catch a seventh `AgentKind` whose arm
  compiles but that no other arm links to. Like the D-07 test, it would go unvisited, and the count
  would still read 6.
- **The redundancy guard's range.** It covers only the FullExecute Code intent. A render-style switch
  that left this one prompt unchanged would not show up here.
- **Model behaviour.** Which instruction a resumed model actually follows is Phase 49's to observe,
  not this plan's.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for 47-05. D-15's suite exists against the final text.
- DECN-02 and DECN-03 are still declared by 47-05, so `requirements.ready-ids` is expected to keep
  them unmarked until 47-05 has a SUMMARY.
- `graphify-out/` was refreshed locally and left uncommitted, and `.planning/user/errors/` was left
  uncommitted.

## Self-Check: PASSED

- FOUND: all 8 created `.snap` files and both modified source files (`git ls-files "*.snap"` lists 9, including the 47-01 baseline).
- FOUND: commits `8d71f4d` and `75c2a4c`, both in `git log 6f748e1..HEAD`. Measured count: 2.
- FOUND: `policy_carrying_full_execute_fix_prompt_snapshots ... ok` and `the_gate_rule_holds_in_both_delivered_turns ... ok` in the Task 2 gate log.
- Stub scan over added lines: `stub_hits=0`. The control scan of the same lines for `insta::assert_snapshot` returned 3.
- `.snap.new` files in `crates/`: 0.
