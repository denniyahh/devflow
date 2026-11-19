---
phase: 53-the-question-channel
plan: 06
subsystem: prompt
tags: [devflow_ask, prompt-policy, breaking-change, snapshots]
status: complete

requires:
  - phase: 53-03
    provides: "the checkpoint auto-decide relaunch is gone; a declared blocking-human checkpoint opens the stage failure gate"
provides:
  - "ask_policy!() / ASK_POLICY: every stage prompt (Plan, PlanGaps, every Code arm incl. GapsOnly, Validate; claude- and workflow-style; all six drivers) routes decision checkpoints, blocking-human gates, package verification and GSD discuss questions to devflow_ask, before the completion contract"
  - "CODE_STAGE_POLICY's `## Operator-only checkpoints` section: the pinned operator-answer-(1) text plus the checkpoint declaration reworded as the fallback for a failed devflow_ask call (D-13)"
  - "Deleted: the unattended positional decision policy, gate_resolution_rule!, GATE_RESOLUTION_RULE, checkpoint_auto_decide_prompt and their tests"
affects: [53-11, 53-13]

actuals:
  tokens: 18850
  tasks: 2
  commits: 3
plan_head_before: d16afe87e625c2cac94a5ba8ac14ac3a05f1527d
plan_head_after: c14bc94650c0c27499280a7ef4df784369db6d37

tech-stack:
  added: []
  patterns:
    - "Source-scan test cut at the first `\\nmod tests {` with a precondition that the cut happened, needles split mid-word with concat! so the test source does not hold them"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/prompt.rs
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap
    - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap

key-decisions:
  - "The pinned checkpoint sentences got their own heading, `## Operator-only checkpoints`, instead of falling under the self-review heading once the unattended section was deleted"
  - "The old section's last sentence (`This policy must not pause execution or request human input.`) was deleted with the section: it contradicts waiting on devflow_ask"
  - "The declaration's closing clause now reads `not for a gate the operator answered` (was `a gate you resolved`): the agent no longer resolves gates"
  - "ASK-03 and ASK-05 not marked complete: 53-11 carries both, and 53-09/10/13 carry ASK-05"

patterns-established: []

requirements-completed: []

duration: 11min
completed: 2026-11-19
---

# Phase 53 Plan 06: The question policy in every stage prompt Summary

**Every stage prompt on all six drivers now tells the agent to send decision checkpoints, blocking-human gates, package-verification checkpoints and GSD discuss questions to `devflow_ask` (naming `mcp__devflow__devflow_ask` and codex's `devflow_ask` of server `devflow`, with the `pending`/`question_id` re-call), and the positional self-resolution policy and the resume prompt are gone.**

## Performance

- **Duration:** 11 min
- **Started:** 2026-11-19T14:53:58Z
- **Completed:** 2026-11-19T15:04:24Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- `ask_policy!()` sits in every builder (`idempotent_stage_prompt`, `gaps_plan_prompt`, `code_stage_prompt`, `fix_prompt_with_notes` both arms, `validate_stage_prompt`, and the four `workflow_*` builders incl. the GapsOnly arm), placed before the notes and the completion contract.
- The pinned section is operator answer (1)'s text; the fallback `do not self-resolve or approve one under any circumstances, and report it instead` survives.
- 48 rendered prompts (6 drivers x 8 intents) carry the policy exactly once, before `## Completion Protocol (REQUIRED)`; none, and no line of prompt.rs's non-test source, carries a positional phrase.

## Task Commits

1. **Task 1: `ask_policy!()` in every stage prompt** - `89fdfe6` (test, RED), `7cc13a2` (feat, GREEN)
2. **Task 2: Re-bless the seven snapshots** - `c14bc94` (test)

## TDD Gate Compliance

RED -> GREEN: `89fdfe6` -> `7cc13a2`. No REFACTOR commit. Evidence from nextest JUnit reports (scratch `red` profile via `--config-file`), checked with `gsd-tools check tdd-red-evidence`; records in `target/red-evidence/53-06-*.json` (gitignored). The RED commit carried `ask_policy!()`/`ASK_POLICY` defined but not yet wired into any builder.

| Test | RED verdict | Failed on | Semantic assessment |
|------|-------------|-----------|---------------------|
| `prompt::tests::every_stage_prompt_routes_questions_through_devflow_ask` | RED_EVIDENCE_OK | every one of 48 prompts: "ask policy present 0 times" | Intended: no builder carried the policy |
| `prompt::tests::no_prompt_template_carries_positional_self_resolution` | RED_EVIDENCE_OK | Code prompts on every driver: `Unattended decision checkpoints`; non-test source: that heading, `nobody is coming`, `positional procedure` | Intended: the scan bites on the real file and real prompts |
| `prompt::tests::package_verification_prohibition_is_unconditional` | RED_EVIDENCE_OK | "CODE_STAGE_POLICY no longer carries the pinned gate-rule section verbatim" | Intended: old section still in place |
| `prompt::tests::the_positional_scan_trips_on_a_planted_copy` | INVALID_RED (unexpected_green) | n/a | Expected: a control on the helper, which exists in RED; mutation below fails it |
| `prompt::tests::the_ask_policy_names_the_namespaced_tool_and_the_pending_recall` | INVALID_RED (unexpected_green) | n/a | Expected: checks the macro text, present in RED; it carries its own in-test control (policy minus the `question_id` sentence reports `question_id` missing); mutation below fails it |

**Mutation runs** (each restored from a saved copy, confirmed with `cmp`; both records RED_EVIDENCE_OK):
- Heading needle misspelt (`...check`, `pointz`): the planted-copy test failed.
- `mcp__devflow__devflow_ask` dropped from the policy: the ask-policy test failed with `left: ["mcp__devflow__devflow_ask"]`.

## Snapshot diff (Task 2)

All seven baselines changed the same way (claude-style four byte-identical to each other and to the claude_style baseline after the header, checked with `diff`):
- Removed heading: `## Unattended decision checkpoints` (its three paragraphs, the old pinned section and the trailing "must not pause" sentence).
- Added headings: `## Operator-only checkpoints` (pinned sentences + reworded declaration) and `## Questions go to the operator` (the ask policy, two paragraphs).
- Unchanged: the fix command line, `## Advisory incremental self-review`, `## Completion Protocol (REQUIRED)`.

Before re-blessing, both snapshot tests failed under `INSTA_UPDATE=no` (full lib run); after, both pass under `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`; no `.snap.new` remains.

## Verification

- Task 1's five named gates: 1 passed each.
- Task 2's two named gates: 1 passed each.
- `cargo build --workspace`: no caller of a deleted item (only prompt.rs referenced them; 53-03 removed the CLI uses).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `scripts/check.sh all`: `==> check.sh: all OK`, exit 0.

### semver-checks

`cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop --release-type minor`: exit 100, `Checked 202 checks: 198 pass, 4 fail`, "semver requires new major version: 4 major and 0 minor checks failed". This plan's items, as expected:
- `function_missing`: `devflow_core::prompt::checkpoint_auto_decide_prompt`
- `pub_module_level_const_missing`: `GATE_RESOLUTION_RULE`

The rest are earlier 53-xx plans' planned breaks: `enum_variant_added` `GateKind::Question` (53-01); `function_missing` `verify::phase_has_blocking_human_checkpoint`, `agent_result::blocking_human_checkpoint_reported`, `agent_result::checkpoint_reported_in_capture`, `inherent_method_missing` `ClaudeDriver::exec_resume_command` (53-03).

## Deviations from Plan

### Test changes the plan did not list

**1. [Rule 1 - Bug] Tests asserting the deleted policy rewritten or removed**
- **Found during:** Task 1
- **Issue:** `code_policy_forbids_positional_option_selection` required "first option"/"merit" text that no longer exists; `code_policy_requires_the_reasoning_to_be_recorded` required "reasoning", which only the deleted sections carried; `the_pre_fix_prompt_builder_fails_the_verbatim_assertion`'s builder copy must equal today's no-note builder.
- **Fix:** removed the positional test (its intent is now `every_stage_prompt_routes_questions_through_devflow_ask` plus the positional scan); renamed the reasoning test to `code_prompt_requires_the_question_and_answer_to_be_recorded`, asserting the ask policy's record sentence; added `{ASK_POLICY}` to the pre-fix builder copy. `code_policy_reaches_the_full_execute_fix_arm_on_every_adapter` now looks for `## Operator-only checkpoints`.
- **Commit:** `7cc13a2`

**2. [Rule 2 - Correctness] New heading for the pinned section; contradictory sentence dropped**
- See key-decisions. Both are inside the diff the plan allowed (the removed block and the new pinned sentences); recorded so the reviewer reads them as choices.

## Issues Encountered

None.

## Next Phase Readiness

The prompt half of criterion 3 holds. The server side of `pending` (53-13) and the live closing run (53-VALIDATION.md M4) are not covered here.

## Self-Check: PASSED
