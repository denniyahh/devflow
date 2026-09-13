---
phase: 47-unattended-decision-policy-consistency
plan: 02
subsystem: prompt-rendering
tags: [prompt, decision-policy, tdd, insta, snapshot, agents]

requires:
  - phase: 47-01
    provides: "insta wiring, hardened run_test, and the reviewed PRE-FIX FullExecute fix-prompt baseline this plan drifts and re-blesses"
provides:
  - "`code_policy_applies_to_fix_arm(Option<FixType>) -> bool` — the single definition of which Code arms carry CODE_STAGE_POLICY, consulted by both renderers"
  - "`fix_prompt`'s FullExecute arm delivers CODE_STAGE_POLICY on claude, opencode, hermes and antigravity (DECN-02)"
  - "Six-adapter presence test with codex/pi passing controls, walking AgentKind through an exhaustive match"
  - "Claude-style omission control for GapsOnly/AuditFix, observed failing under a temporary widening"
  - "Re-blessed snapshot baseline recording the fixed loop-back prompt"
affects: [47-03, 47-04, 47-05, "DECN-02", "render_claude_style adapters"]

actuals:
  tokens: 3384        # chars/4 over the realized diff 871c5f3..062ebf7 (13539 chars; prompt.rs + .snap)
  tasks: 3
  commits: 2          # MEASURED: git rev-list --count 871c5f3..HEAD at SUMMARY write (excludes docs commits)
plan_head_before: 871c5f3fee6c8cb3e59df257c538f86b2ccf2c20

tech-stack:
  added: []
  patterns:
    - "Arm-policy rule lives in one exhaustive-match helper; renderers build the arm text, then ask the helper whether to interpolate the policy"
    - "Multi-adapter tests enumerate AgentKind via an exhaustive successor match, not an array literal"
    - "D-05 byte-preservation checked by an out-of-tree scratch crate dumping every Code arm before and after"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap

key-decisions:
  - "fix_prompt consults code_policy_applies_to_fix_arm for EVERY arm, not only inside the FullExecute match arm, so the helper is the one decision point and a widening of it is observable in the claude-style render"
  - "D-05 extraction held: workflow_code_prompt output is byte-identical for all four Code arms; no fallback taken, no existing test edited"
  - "AgentKind is walked by an exhaustive successor match (no AgentKind::ALL, no derive providing one); the residual gap (an arm whose successor nothing points at) is stated in the test comment"

patterns-established:
  - "Presence test with passing controls + a completion-protocol precondition, so absence of the heading means absence of the policy"

requirements-completed: [DECN-02]  # copied verbatim from PLAN frontmatter; NOT marked in REQUIREMENTS.md — requirements.ready-ids reported 0/1 ready (sibling plans still declare DECN-02 without a SUMMARY)

coverage:
  - id: D1
    description: "A Validate loop-back dispatching Code { fix: Some(FullExecute) } carries the unattended decision policy on all six adapters; codex/pi are passing controls"
    requirement: DECN-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::code_policy_reaches_the_full_execute_fix_arm_on_every_adapter"
        status: pass
      - kind: other
        ref: "47-02-PLAN.md Task 1 gate (red pre-fix, gate_rc=0) and Task 2 gate (gate_rc=0)"
        status: pass
    human_judgment: false
  - id: D2
    description: "GapsOnly and AuditFix still omit the policy on the claude-style path; the control was observed red under a temporary widening of the helper and green after revert"
    requirement: DECN-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::claude_style_fix_prompts_that_must_not_carry_the_policy_still_omit_it"
        status: pass
      - kind: other
        ref: "Task 3 widening demonstration (exit 101 widened, exit 0 reverted)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Shared helper extraction changed no observable behaviour of workflow_code_prompt; the workflow-style omission control is byte-identical to the fork point"
    requirement: DECN-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::code_policy_is_absent_from_prompts_that_must_not_carry_it"
        status: pass
      - kind: other
        ref: "scratch render dump diff (11 sections, 0 removed lines) + Task 3 gate omission_control_unmodified_exit=0"
        status: pass
    human_judgment: false
  - id: D4
    description: "Snapshot baseline re-blessed; its diff adds only the CODE_STAGE_POLICY section between the fix command and the completion protocol"
    requirement: DECN-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#prompt::tests::claude_style_full_execute_fix_prompt_snapshot"
        status: pass
      - kind: other
        ref: "scripts/check.sh test (check_test_exit=0, 30 ok binaries, 0 failed, 1278 passed)"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-09-11
status: complete
---

# Phase 47 Plan 02: FullExecute Loop-Back Decision Policy Summary

**`fix_prompt`'s FullExecute arm now carries `CODE_STAGE_POLICY` on the four `render_claude_style` adapters, decided by a helper extracted from `workflow_code_prompt` whose output stayed byte-identical. A six-adapter test that was red on Claude, OpenCode, Antigravity and Hermes now passes.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-09-11T18:23:44Z
- **Completed:** 2026-09-11T18:36:04Z
- **Tasks:** 3 (Task 1 RED, Task 2 GREEN, Task 3 assert-only)
- **Files modified:** 2

## Accomplishments

- The Validate loop-back prompt delivers the unattended decision policy on claude, opencode,
  hermes and antigravity. Before this plan, only codex and pi received it.
- The rule "which Code arm carries the policy" has one definition. Both renderers consult it, so
  they cannot drift apart again.
- The claude-style omission control exists and has been observed failing in the over-broad
  direction, the direction D-08 exists to catch.
- The 47-01 drift guard caught its first real wording change (exit 101). The baseline was then
  re-blessed with the diff read first.

## Task Commits

1. **Task 1 (RED): six-adapter presence test + claude-style omission twin** - `59e4334` (test)
2. **Task 2 (GREEN): shared helper, FullExecute arm routed through it, baseline re-blessed** - `062ebf7` (feat)
3. **Task 3:** no commit. It only asserts; `prompt.rs` had no diff against HEAD after the widening revert.

## Criterion 2 evidence — pre-fix `test result: FAILED` (Task 1, tree at `59e4334`)

```
running 1 test
test prompt::tests::code_policy_reaches_the_full_execute_fix_arm_on_every_adapter ... FAILED
thread '...' panicked at crates/devflow-core/src/prompt.rs:950:9:
the FullExecute fix arm omits the unattended decision policy on [Claude, OpenCode, Antigravity, Hermes] (render_claude_style): a Validate loop-back never delivers it to those adapters
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 766 filtered out; finished in 0.00s
```

Neither control assertion (codex/pi) nor the completion-protocol precondition fired, so the failure
comes from the claude-style assertion alone. After Task 2 the same test prints
`test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 766 filtered out`.

## Automated gate counters (as printed)

| Gate | Printed counters | Exit |
|---|---|---|
| Task 1 | `cargo_exit=101` `failed_binaries=1` `zero_passed_lines=1` `presence_test_actually_ran=1` `cargo_exit_omit=0` `omission_control_one_passed=1` `gate_rc=0` | 0 |
| Task 2 | `cargo_exit=0` (`767 passed; 0 failed`) `failed_binaries=0` `presence_test_one_passed=1` `presence_filtered=766 filtered out` `helper_code_sites=3` `tracked_snapshots=1` `policy_heading_in_baseline=1` `unblessed_scratch_files=0` `gate_rc=0` | 0 |
| Task 3 | `fork_point=034f5b6…` `head_window_lines=30` `base_window_lines=30` `omission_control_unmodified_exit=0` `policy_heading_in_baseline=1` `check_test_exit=0` `gate_rc=0` | 0 |

`helper_code_sites=3` covers exactly the definition (`prompt.rs:463`), the `workflow_code_prompt` call (`:487`) and the
`fix_prompt` call (`:618`). A second count taken from the `check.sh` log gave 30 `test result: ok` binaries, 0
`FAILED`, and 1278 passed in total. That is 47-01's recorded 1276 plus the two tests this plan added.

## Task 2 — snapshot guard's first live catch, and the re-bless

With the fix in place, `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test -p devflow-core --lib
prompt::tests::claude_style_full_execute_fix_prompt_snapshot -- --exact` exited **101** and wrote no
`.snap.new` file:

```
test prompt::tests::claude_style_full_execute_fix_prompt_snapshot ... FAILED
-old snapshot
+new results
    3     3 │     /gsd-execute-phase 47 --auto
    4     4 │
          5 │+## Advisory incremental self-review
          ...  (+ the full CODE_STAGE_POLICY text, lines 5-16)
    5    17 │ ## Completion Protocol (REQUIRED)
snapshot assertion for 'claude_style_full_execute_fix_prompt_snapshot' failed in line 1233
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 766 filtered out
```

I re-blessed it with `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=always cargo test … -- --exact`, which exited 0
with `updated snapshot` and left 0 `.snap.new` files. **I read the `.snap` diff (`git diff`) before staging.** It
is a single hunk of +12 lines and 0 removed lines: the `## Advisory incremental self-review` and
`## Unattended decision checkpoints` sections, placed between the fix command and
`## Completion Protocol (REQUIRED)`. Nothing else in the file changed. The re-blessed baseline has 1
occurrence of the policy heading; 47-01's acceptance recorded 0.

## Task 3 — D-08 control, both directions

I temporarily widened `code_policy_applies_to_fix_arm` to return true for `Some(FixType::GapsOnly)`
(4 diff lines) and ran each control by module-qualified `--exact` name:

| Condition | `claude_style_fix_prompts_that_must_not_carry_the_policy_still_omit_it` | `code_policy_is_absent_from_prompts_that_must_not_carry_it` |
|---|---|---|
| widened | exit 101, `test result: FAILED. 0 passed; 1 failed; … 766 filtered out`, panic `only full-execute Code prompts may carry the shared policy` | exit 101, same `FAILED` line and panic |
| reverted (`git checkout -- prompt.rs`; `git diff --quiet` exit 0) | exit 0, `ok. 1 passed; … 766 filtered out` | exit 0, `ok. 1 passed; … 766 filtered out` |

The widening now turns the workflow-style control red as well, because both renderers consult the same helper.
Before this plan, a widening in `fix_prompt` alone would have left that control green. After the revert,
`prompt.rs:465-466` reads `Some(FixType::FullExecute) | None => true` /
`Some(FixType::GapsOnly) | Some(FixType::AuditFix) => false`, and `git diff HEAD -- crates/` lists 0 files.

## D-05 — did the behaviour-preserving extraction hold?

**Yes. The fallback was not taken, and no existing test was edited.** Three separate measurements:

- **Render dump.** A scratch crate outside the worktree, with a path dependency on `devflow-core`, printed every
  Code arm (`None`, `AuditFix`, `GapsOnly`, `FullExecute`) through `render_workflow_style` and
  `render_claude_style`, plus all three `fix_prompt` arms. Output was captured before and after Task 2. Both runs
  had 11 sections, and `diff` showed **0 removed lines**. The only additions are two 12-line hunks, one in
  `claude Some(FullExecute)` and one in `fix_prompt FullExecute`. All four workflow-style sections and every
  `AuditFix`/`GapsOnly` render are byte-identical. Policy heading count went from 3 to 5.
- **Diff against the fork point.** `git diff -U0 034f5b6 -- prompt.rs` removes 8 lines, all production code
  (old lines 454, 458, 463, 468, 472, 577-579). `mod tests` starts at old line 583, so 0 removed lines fall
  inside it.
- **Suite.** Every pre-existing `prompt.rs` test passes (Task 2 gate: 767 passed, 0 failed), and the
  `code_policy_is_absent_from_prompts_that_must_not_carry_it` window is byte-identical (Task 3 gate).

## TDD Gate Compliance

- Tasks 1 and 2 are TDD tasks: `task.is-behavior-adding` returned true for this plan. RED `59e4334` is an
  ancestor of GREEN `062ebf7`. No `feat(47-02)` commit existed before RED (count 0; the control count for
  `test(47-01)` was 2). REFACTOR was not needed.
- The RED record was verified with `gsd_run check tdd-red-evidence`. As in 47-01, the classifier parses node TAP only, so the
  record's `output` is a mechanical TAP transcription of the real cargo log (`not ok 1 - prompt::tests::code_policy_reaches_…`,
  `# tests 1 / # pass 0 / # fail 1`). Real record: **`RED_EVIDENCE_OK (target_test_failed)`**.
  Control with a wrong `targetTest`: **`INVALID_RED (no_target_test_failure)`**.
- The RED test is an inline `#[cfg(test)]` test in `src/prompt.rs`. It does not match the gate's
  `tests/**` / `*.test.*` path heuristic; that is Rust convention, not a missing test file.

## Files Created/Modified

- `crates/devflow-core/src/prompt.rs`:
  - adds `code_policy_applies_to_fix_arm`;
  - `workflow_code_prompt` now builds the arm instruction and then asks the helper;
  - `fix_prompt` routes every arm through the helper, with a signature-bridge comment and an extended doc comment naming DECN-02/D-05/D-06;
  - adds two tests.
- `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap`: re-blessed with the policy section.
- `.planning/phases/47-unattended-decision-policy-consistency/deferred-items.md`: one out-of-scope observation (see Issues Encountered).

## Decisions Made

- `fix_prompt` asks the helper on every arm rather than hard-coding the policy into the `FullExecute` match
  arm. The alternative would satisfy the Task 2 gate, but it would leave the helper consulted for
  `FullExecute` only. Task 3's widening would then change nothing in the claude-style render, so the D-08
  demonstration would be vacuous.
- AgentKind is enumerated with an exhaustive successor `match`. The crate has no `AgentKind::ALL` and no
  clap/strum derive that would provide one (`state.rs:400`). The test also asserts that the chain has no cycle
  and that at least one control was visited.
- New doc text names private items in plain backticks rather than intra-doc links, so it adds no new
  `private_intra_doc_links` sites.

## Deviations from Plan

None - plan executed exactly as written. There were no auto-fixes and no fallback. Task 3 made no commit because
it changes no file.

## Issues Encountered

- **The first RED record was malformed and read as `invalid_record` in both directions.** The classifier wants
  camelCase `targetTest`/`exitCode`. The problem surfaced because the wrong-target control returned the same
  verdict as the real record. After fixing the field names, the two verdicts diverged as shown above.
- **Out of scope, logged to `deferred-items.md`:** `cargo clippy -p devflow-core --all-targets` fails to compile
  `tests/monitor_e2e.rs` and `tests/devflow_dir_gitignore.rs`. They call `devflow_core::test_support`, which is
  configured out when only this package is selected. `cargo clippy --workspace --all-targets -- -D warnings`,
  the form CLAUDE.md and CI use, exited 0 both before and after Task 2. Neither failing file was touched by this
  plan, and the module they call is not `#[cfg(test)]` lib code.
- zsh `=word` expansion broke an `echo ===…` separator in an exploratory command. It had no effect on any gate.

## What this does NOT establish

- **Delivery, not compliance.** The tests prove the policy text is in the rendered prompt. They do not show that
  any agent follows it on a loop-back; that behavioural arm belongs to Phase 49's live run.
- **Only the `render_prompt` path.** The six-adapter test drives `driver_for(kind).render_prompt`. It does not
  exercise the launch path that carries the prompt to each CLI (stdin for Claude/Antigravity, argv for
  OpenCode/Hermes).
- **Enumeration is not fully exhaustive.** A new `AgentKind` variant with an arm that no successor points at
  would compile and go unvisited (the gap is stated in the test comment).
- **One observation per case.** Each demonstration ran once. They show the mechanism, not a flake rate.
- **Not run in the pinned CI container.** Every run was on the host toolchain.

## Threat Model Coverage

- **T-47-05:** mitigated by the claude-style omission control, observed red under widening.
- **T-47-06:** mitigated by the DECN-02 fix itself; the policy's record-the-reasoning instruction now reaches
  the loop-back.
- **T-47-07:** mitigated by the re-blessed snapshot, whose diff was read first.
- **T-47-08:** mitigated by the exhaustive match, within the residual gap stated above.
- No new security surface was added: this plan changes prompt text and tests only.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Ready for 47-03. `prompt.rs` line numbers cited by later plans have shifted: the helper added about 15 lines
  above `workflow_code_prompt`, and `fix_prompt` grew about 27 lines, so `mod tests` and everything after moved.
- `graphify-out/` was refreshed locally and deliberately left uncommitted (CLAUDE.md graphify rule).
- The scratch render-dump crate lives in the session scratchpad, not the worktree.

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/prompt.rs`, and the `.snap` baseline (git-tracked, 1 file under `snapshots/`).
- FOUND: commits `59e4334` and `062ebf7`, both ancestors of HEAD. Measured count `871c5f3..HEAD` = 2.
- FOUND: symbols `fn code_policy_applies_to_fix_arm`, `fn code_policy_reaches_the_full_execute_fix_arm_on_every_adapter`,
  and `fn claude_style_fix_prompts_that_must_not_carry_the_policy_still_omit_it`, 1 each.
- Stub scan over the added lines: `stub_hits_added_lines=0`. The control for the same scan, counting
  `Unattended decision checkpoints`, returned 2.
- `cargo fmt --check -p devflow-core` exit 0. `cargo clippy --workspace --all-targets -- -D warnings` exit 0.
