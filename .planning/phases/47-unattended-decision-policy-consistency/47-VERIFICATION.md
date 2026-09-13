---
phase: 47-unattended-decision-policy-consistency
verified: 2026-09-13T06:58:15Z
status: passed
score: 39/39 must-haves verified
covered_files:

  - .github/workflows/ci.yml
  - .gitignore
  - .planning/REQUIREMENTS.md
  - .planning/ROADMAP.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-01-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-01-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-02-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-02-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-03-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-03-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-04-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-04-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-05-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-05-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-06-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-06-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-07-PLAN.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-07-SUMMARY.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-CONTEXT.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-PHASE49-OBSERVATION.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-RESEARCH.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-REVIEW.md
  - .planning/phases/47-unattended-decision-policy-consistency/47-REVIEWS.md
  - .planning/user/DEV-SETUP-CHECKLIST.md
  - ARCHITECTURE.md
  - Cargo.lock
  - Cargo.toml
  - crates/devflow-cli/Cargo.toml
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_one_code_prompt.snap
  - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap
  - crates/devflow-cli/tests/check_script_run_test.rs
  - crates/devflow-cli/tests/ci_parity_guards.rs
  - crates/devflow-cli/tests/gitignore_coverage.rs
  - crates/devflow-core/Cargo.toml
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap
  - crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap
  - docs/guides/unattended-mode.md
  - scripts/check.sh
  - scripts/lint-phase-worktree.sh
  - scripts/test-phase-worktree-guard.sh

covered_digest: "v1:sha256:273202e8cf7cd5db6f032ff899a6c36819b0a03e28f763c1b40e97c79b6f0422"
behavior_unverified: 0
overrides_applied: 1
overrides:

  - must_have: "MUST NOT allow any text after the DEVFLOW_RESULT line, introduce a second record or marker format, or change the result parser or the gate detector (operator decision A1)."
    reason: "Review-fix commits bd7bd7c (match only Gate-labelled lines), ac3ee42 (recognise list and blockquote declarations) and 49bb324 (decode escaped captures) changed the gate detector after 47-06 to close code-review findings. Each carries its own regression test, all passing at 6ae0b77, and the result parser is untouched. bd7bd7c was already covered by the recorded 'fix every finding except IN-05' decision; the operator accepted all three on 2026-09-13."
    accepted_by: "operator"
    accepted_at: "2026-09-13T07:08:34Z"
re_verification:
  previous_status: passed
  previous_score: 23/23
  previous_verified_at: 9a2c7fe
  gaps_closed: []
  gaps_remaining: []
  regressions: []
deferred:

  - truth: "Which instruction a live Claude agent actually follows when CODE_STAGE_POLICY and the resume instruction are both present (the DECN-03 behavioural arm)."
    addressed_in: "Phase 49"
    evidence: "ROADMAP Phase 47 criterion 4 ('Deliberately NOT closed here'); Phase 49 criterion 4 carries the pointer to 47-PHASE49-OBSERVATION.md, which defines Followed / Not followed / Void."
human_verification:

  - test: "Decide whether the gate-detector changes made after plan 47-06 (bd7bd7c, ac3ee42, 49bb324) supersede 47-06's test-tier prohibition 'MUST NOT allow any text after the DEVFLOW_RESULT line, introduce a second record or marker format, or change the result parser or the gate detector'."
    expected: "Either accept the later detector changes (and record an overrides: entry for that prohibition), or ask for them to be reverted or re-reviewed."
    why_human: "Plan 06's own commits (b33406c..1691b12) honoured the prohibition: agent_result.rs changed only in tests. At HEAD the detector's non-test code (agent_result.rs 631-770) differs from b3b47ea; the result parser (182, 1736, 2074-2075, 2317) does not. REVIEW.md records an operator decision covering bd7bd7c (IN-02). REVIEWS.md records ac3ee42 and 49bb324 as fixes from the final external review but carries no operator-decision line. Whether a plan-scoped prohibition binds later review fixes is an operator decision, not a code fact."
  - test: "Confirm the 47-06 tests build their inputs from production text rather than hand-typed protocol text (judgment-tier prohibition)."
    expected: "The success line comes from COMPLETION_PROTOCOL and the declaration label from HUMAN_GATE_VALUE."
    why_human: "Judgment-tier; the verifier's verdict is non-authoritative. Verifier read (holds): decision_reasoning_above_the_result_line_parses_to_that_result uses crate::prompt::COMPLETION_PROTOCOL and checkpoint_auto_decide_prompt; resume_prompt_does_not_read_as_a_blocking_human_checkpoint builds its declaration from HUMAN_GATE_VALUE."
  - test: "Confirm no 47-06 commit was pushed before both Task 3 gates passed (judgment-tier prohibition)."
    expected: "The first push of any 47-06 commit came after both Task 3 gates were observed green."
    why_human: "Push timing is not recorded in the repository. The verifier cannot establish it either way."
  - test: "Confirm the 47-07 work never read, loaded or prompted for your real signing key, and did not modify your global git config or hooks (judgment-tier prohibition)."
    expected: "Only scratch configs naming a nonexistent key were used."
    why_human: "Judgment-tier. Verifier read (holds for the current script): the hostile config names a nonexistent key under $TMP, and fixtures run with global config nulled. That describes the script at HEAD, not what happened during execution."
---

# Phase 47: Unattended Decision Policy Consistency Verification Report

**Phase Goal:** An unattended agent receives the same merit-based decision instruction everywhere it is asked to make a Code-stage decision, and is never handed contradictory instructions about who may resolve a `blocking-human` gate inside one resumed session.
**Verified:** 2026-09-13T06:58:15Z against HEAD `6ae0b77` on `feature/phase-47`
**Status:** passed. No gaps. The four prohibition items this report raised were answered by the operator on 2026-09-13. The three post-47-06 gate-detector changes (`bd7bd7c`, `ac3ee42`, `49bb324`) are accepted as an override (frontmatter `overrides`). The operator also confirmed that the 47-06 tests build their inputs from production text; that no 47-06 commit was pushed before its gates passed (`feature/phase-47` has never been on origin); and that 47-07 never touched the real signing key, global git config or global hooks (their modification times all predate 2026-09-12).
**Re-verification:** Yes. The previous report (`9a2c7fe`, passed 23/23) predates review fixes `ac3ee42`, `c66cba2`, `2790678`, `49bb324`, guard fix `7400042`, and docs `0afbff5`..`6ae0b77`. It also collapsed plans 06 and 07 into four rows and did not assess any `prohibitions`. This report checks all 35 plan truths plus the 4 roadmap criteria and every prohibition, with evidence re-run at HEAD.

## What changed from the previous verdict

- **Status `passed` becomes `human_needed`.** No truth failed. Plans 47-06 and 47-07 declare prohibitions the previous report never assessed. Three are judgment-tier, and one test-tier prohibition's detector clause no longer holds literally at HEAD. The verification process treats each as a human-verification item, never a silent pass.
- **Four must-have texts are stale against HEAD but their protected property holds.** Checks added later changed a literal:
  - 47-06 truth 6's "agent_result.rs lines 1-3441 byte-identical" no longer holds, because the later detector fixes changed that range.
  - 47-07 truth 4 says "six calls"; there are now nine.
  - 47-07 truth 5 says "passed=12 failed=1"; the harness now prints 18/1.
  - 47-07 truth 6 says "count as 10"; the checklist now counts 19 checks, and the phrase `isolated from inherited hooks and commit signing` is gone.

  `gsd-tools query verify.artifacts` on 47-07 reports that missing phrase, and will keep reporting it until the plan text is amended.

## Goal Achievement

### Observable Truths

Roadmap criteria are rows R1-R4; plan truths are `PP-n`. All evidence below was produced in this verification run, at HEAD unless a commit is named.

| # | Truth | Status | Evidence |
|---|---|---|---|
| R1 | `fix_prompt` renders `CODE_STAGE_POLICY` for FullExecute; GapsOnly/AuditFix still omit it, with the existing omission test unmodified. | ✓ VERIFIED | Named tests `code_policy_reaches_the_full_execute_fix_arm_on_every_adapter`, `claude_style_fix_prompts_that_must_not_carry_the_policy_still_omit_it`, `code_policy_is_absent_from_prompts_that_must_not_carry_it` pass (8 passed; 766 filtered, one multi-name run). `git diff -U0 1dda004 HEAD -- prompt.rs` has no removed lines in the test module (both test hunks are `-N,0` pure additions). |
| R2 | The loop-back presence test fails against the pre-fix tree and passes after. | ✓ VERIFIED | Re-executed in a scratch clone: at `59e4334` (`test(47-02)`) `code_policy_reaches_the_full_execute_fix_arm_on_every_adapter` printed `test result: FAILED. 0 passed; 1 failed` (exit 101, compiled); at `062ebf7` (`feat(47-02)`) `1 passed`. Its omission twin passed at both, as a control. |
| R3 | A prompt with both `CODE_STAGE_POLICY` and `checkpoint_auto_decide_prompt` states one gate rule; a contradiction test fails before and passes after. | ✓ VERIFIED | At `94ef699` both `prompt::tests::the_gate_rule_has_one_definition_site` and `pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns` printed `test result: FAILED` (exit 101, compiled); at `cab8d54` both `1 passed`. Both pass at HEAD. |
| R4 | Which instruction a model follows is recorded as a Phase 49 observation item, not settled on a source read. | ✓ VERIFIED | `47-PHASE49-OBSERVATION.md` defines Followed / Not followed / Void and states that the event precedes the spawn. Phase 49 criterion 4 carries one pointer (ROADMAP:203). The live observation itself is deferred to Phase 49. |
| 01-1 | A wording change to a policy-carrying prompt fails `scripts/check.sh test`. | ✓ VERIFIED | Scratch clone, one drifted baseline (`..._codex.snap`): check.sh's literal prefix `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo test` exited 101 with `test result: FAILED`, baseline unchanged, 0 `.snap.new`. `check_script_run_test::cargo_failure_still_runs_the_harness_and_fails` shows check.sh returns cargo's 101. `check_script_pins_snapshots_against_env_override` pins that prefix as check.sh's invocation. |
| 01-2 | `INSTA_FORCE_UPDATE=1` cannot re-bless a drifted snapshot green. | ✓ VERIFIED | The 01-1 run exported `INSTA_FORCE_UPDATE=1 INSTA_UPDATE=always` and still failed. **Negative control:** the same drift and environment without the prefix exited 0 and rewrote the baseline, so the prefix is what holds the guard. |
| 01-3 | A clean tree passes `scripts/check.sh test`. | ✓ VERIFIED | `scripts/check.sh all` at `6ae0b77` exited 0: fmt, clippy `-D warnings`, full workspace tests (devflow-core lib 774 passed, devflow bin 364 passed, all other binaries green), harness `passed=19 failed=0`, `==> check.sh: all OK`. One run, no failures, no rerun. |
| 01-4 | `.snap.new` is ignored while `.snap` is not. | ✓ VERIFIED | `git check-ignore --no-index`: `x.snap.new` exit 0, `x.snap` exit 1. `gitignore_ignores_snapshot_scratch_but_not_the_baseline` passes (1 passed; 1 filtered). |
| 02-1 | FullExecute loop-back delivers the policy on claude, opencode, hermes, antigravity. | ✓ VERIFIED | Six-adapter test passes at HEAD; its red at `59e4334` (R2) shows it detects absence. |
| 02-2 | codex and pi still deliver it, as passing controls. | ✓ VERIFIED | Same six-adapter test covers both workflow-style adapters; it failed pre-fix, so it is not a tautology. |
| 02-3 | GapsOnly and AuditFix omit the policy on both render paths. | ✓ VERIFIED | Claude-style twin and workflow-style omission test both pass. |
| 02-4 | `workflow_code_prompt`'s existing tests pass unmodified. | ✓ VERIFIED | No removed test-module lines since fork `1dda004`; the omission test passes. |
| 03-1 | A resumed Claude session gets one gate rule, stated as a prohibition with exception and affirmatively. | ✓ VERIFIED | `GATE_RESOLUTION_RULE` is `gate_resolution_rule!()` (prompt.rs:88), interpolated into `CODE_STAGE_POLICY` (:124), and asserted in turn 1 by the CLI delivery test (pipeline_launch.rs:4075). Both gate-rule tests pass. |
| 03-2 | The carve-out covers `blocking-human` only; package verification stays unconditional. | ✓ VERIFIED | `package_verification_prohibition_is_unconditional` and `code_policy_excludes_blocking_human_and_package_checkpoints` pass. |
| 03-3 | `checkpoint_auto_decide_prompt` names `blocking-human`. | ✓ VERIFIED | Prompt text "stopped at a blocking-human gate" (prompt.rs ~616); the delivery test fails if turn 2 lacks `blocking-human` (pipeline_launch.rs, `!turn2.contains("blocking-human")`). |
| 03-4 | Both halves of the layered test went red pre-fix with a printed `test result: FAILED`, not a compile error. | ✓ VERIFIED | Re-executed at `94ef699`: both printed `test result: FAILED. 0 passed; 1 failed` (R3). |
| 03-5 | Each turn comes from its production constructor, never concatenated. | ✓ VERIFIED | The test takes turn 1 from `resolve_launch_shape`'s `MonitorLaunch::PipeOwning { prompt }` and turn 2 from `resume_launch_shape`'s `argv[1]`. It asserts `!turn1.contains(&turn2) && !turn2.contains(&turn1)` and snapshots each separately. `relaunch_checkpoint_session` emits `checkpoint_auto_decided` (:1126) before `resume_launch_shape` (:1142) and `spawn_agent_and_record` (:1153). |
| 04-1 | Every policy-carrying prompt has a committed reviewed baseline. | ✓ VERIFIED | 9 tracked `.snap` under `crates/` (7 core FullExecute, 2 CLI turns); snapshot tests pass under the pinned env. |
| 04-2 | The two turns are snapshotted separately. | ✓ VERIFIED | `turn_one_code_prompt` and `turn_two_resume_prompt` are two `assert_snapshot!` calls and two files. |
| 04-3 | The suite is scoped to policy-carrying prompts. | ✓ VERIFIED | The 9 tracked snapshots are exactly the policy-carrying set; each contains the completion-protocol line. |
| 05-1 | The guide says approving the parked gate calls `launch_stage_inner` directly and skips the adjudicated check. | ✓ VERIFIED | `docs/guides/unattended-mode.md:89-91`. |
| 05-2 | The never-auto-approved statement is qualified, not inverted. | ✓ VERIFIED | `unattended-mode.md:29-33` keeps "never auto-approved by any mode" and adds the resumed-after-human-approval exception. |
| 05-3 | ROADMAP and REQUIREMENTS name the four/two adapter split. | ✓ VERIFIED | REQUIREMENTS DECN-02 (:18-21) and ROADMAP Phase 47 goal (:97) name claude/opencode/hermes/antigravity vs codex/pi. |
| 05-4 | Phase 49 has one pointer, and nothing else in its criteria changed. | ✓ VERIFIED | One `47-PHASE49-OBSERVATION.md` reference in ROADMAP (:203); Phase 49 still lists criteria 1-5. |
| 05-5 | The DECN-03 behavioural question has a Followed / Not followed / Void standard. | ✓ VERIFIED | `47-PHASE49-OBSERVATION.md` "Evidence standard" section defines all three. |
| 05-6 | The two D-13 defects are on the backlog. | ✓ VERIFIED | ROADMAP `### Phase 999.125` (:604) and `### Phase 999.126` (:590). |
| 06-1 | Every prompt ending with `COMPLETION_PROTOCOL` puts `DEVFLOW_RESULT` last, with reasoning above it. | ✓ VERIFIED | prompt.rs:46 "the LAST line of your FINAL message must be exactly:" plus the "goes above the DEVFLOW_RESULT line" sentence; all 9 snapshots contain it; the tests assert the resume prompt `ends_with(COMPLETION_PROTOCOL)`. |
| 06-2 | Over 5000 characters of reasoning above the success line parses to Success at `evaluate_layer1` for plain text, an envelope, and a Claude stream with failed earlier turns. | ✓ VERIFIED | `decision_reasoning_above_the_result_line_parses_to_that_result` passes. It asserts `decision_record.len() > 5_000`, uses an envelope and `v3_stream_capture(MARKER_FAILED, MARKER_FAILED, …)`, and takes the success line from `COMPLETION_PROTOCOL`. |
| 06-3 | The same reasoning after the result line does not parse to Success; a short trailing line within the tail budget still does. | ✓ VERIFIED | Same test's long-trailing and within-budget controls pass (IN-03 later tightened the long case to "no result at all"). |
| 06-4 | The resume prompt tells the agent to name the gate in prose; the rendered prompt does not trip either detector, but the same text with the label appended trips both. | ✓ VERIFIED | Sentence at prompt.rs:622. `resume_prompt_does_not_read_as_a_blocking_human_checkpoint` passes against the post-review detector. |
| 06-5 | All nine baselines are byte-identical to `b3b47ea` except the expected substitutions: numstat 2/2 ×8 and 3/3 for turn two, and no `.snap.new`. | ✓ VERIFIED | `git diff --numstat b3b47ea HEAD` shows exactly that. The changed lines are the protocol opening line ×9, the closing line ×9, and one resume paragraph line. 0 `.snap.new` on disk. |
| 06-6 | Outside the two permitted `prompt.rs` regions nothing changed since `b3b47ea`, and the result parser is untouched. | ✓ VERIFIED (literal mechanism superseded) | `prompt.rs` hunks since `b3b47ea` lie only in `COMPLETION_PROTOCOL` (39-58) and `checkpoint_auto_decide_prompt` with its doc comment (596-625). The parser functions (`parse_devflow_result` :182, `parse_claude_event_result` :1736, `parse_marker_lines` :2074 with `TAIL_BUDGET_CHARS` :2075, `evaluate_layer1` :2317) are outside every changed range. Plan 06's commits touched `agent_result.rs` only in tests (hunks at 3443 and 3617). **At HEAD the "lines 1-3441 byte-identical" mechanism does not hold**: detector lines 631-770 were changed by `bd7bd7c`, `ac3ee42` and `49bb324`. That is the detector clause of the 06 prohibition, routed to Human Verification item 1. |
| 06-7 | ARCHITECTURE.md describes the result as the last line with reasoning above it. | ✓ VERIFIED | ARCHITECTURE.md:114-116. |
| 07-1 | The harness passes under global SSH signing plus a hooks directory, with no agent. | ✓ VERIFIED | Scratch clone, `env -u SSH_AUTH_SOCK GIT_CONFIG_GLOBAL=<hostile: failing pre-commit/post-checkout hooks, gpgsign=true, ssh format, nonexistent key>`: exit 0, `passed=19 failed=0`. |
| 07-2 | The scratch repository isolates hooks and signing through repository-local config before its first commit. | ✓ VERIFIED | `init_fixture_repo` sets `core.hooksPath` and `commit.gpgsign false` (test-phase-worktree-guard.sh:70-71) before its first commit; the 07-5 mutant proves these lines are load-bearing. |
| 07-3 | The harness passes with config injected through `GIT_CONFIG_COUNT` or `GIT_CONFIG_PARAMETERS`. | ✓ VERIFIED | Both injected runs: exit 0, 19/0. **Controls:** the same injections armed a failing hook in a plain scratch repository (commit exit 1 in both). `git rev-parse --local-env-vars` lists both carriers, and the harness unsets that list (:15-16). |
| 07-4 | The guard under test is still invoked directly. | ✓ VERIFIED (count superseded) | The six original `"$GUARD" --staged` calls (:86, 94, 97, 103, 112, 120) are unchanged; `7400042` added three more direct calls (:136, 144, 155). There is no wrapper or stub. |
| 07-5 | Removing the two isolation lines prints `FAIL 7c.` and exactly one failure. | ✓ VERIFIED (count superseded) | Mutant in a scratch clone (both lines removed, confirmed 2→0 matches): exit 1, `FAIL 7c. fixture bootstrap under hostile config -> allow  expected allow, got refuse`, `passed=18 failed=1`; restored copy 19/0. The plan's `passed=12` predates the checks added since. |
| 07-6 | The checklist states the harness's size accurately and states its isolation. | ✓ VERIFIED (text superseded) | DEV-SETUP-CHECKLIST.md:98 says "19 checks", matching the runtime `passed=19`. Its isolation text (:105-109) describes the unset of local git environment variables, nulled global/system config, and the local hook and signing pins. The plan's literal "10" and the phrase `isolated from inherited hooks and commit signing` were replaced in `bbf696d`; `verify.artifacts` flags the missing phrase. |

**Score:** 39/39 truths verified (0 present-but-behavior-unverified; 4 with a superseded literal noted).

### Prohibitions

| Plan | Prohibition (abridged) | Tier | Disposition | Evidence |
|---|---|---|---|---|
| 47-06 | Do not widen the carve-out, touch the pinned gate rule, or weaken the package-verification prohibition. | test | Verified | `package_verification_prohibition_is_unconditional`, `the_gate_rule_has_one_definition_site` pass. |
| 47-06 | Do not re-bless a baseline beyond the expected line substitutions. | test | Verified | Numstat and changed-line diff against `b3b47ea` (06-5). |
| 47-06 | No text after `DEVFLOW_RESULT`, no second marker format, no change to the result parser or gate detector. | test | **Flagged: human decision** | Plan-06 commits complied, and the parser is untouched at HEAD. The detector was changed after plan 06 (Human Verification item 1). |
| 47-06 | Do not build test inputs from hand-typed protocol text. | judgment | **Flagged: unverified-prohibition, human review recommended** | Non-authoritative read: holds (item 2). |
| 47-06 | Put the mitigation only in `checkpoint_auto_decide_prompt`, and do not give it the detector's match shape. | test | Verified | The sentence appears only at prompt.rs:622 inside that function (other "prose" hits are its doc comment and a test comment); the resume-prompt detector test passes. |
| 47-06 | Do not push before both Task 3 gates pass. | judgment | **Flagged: unverified-prohibition, human review recommended** | Not establishable from the repository (item 3). |
| 47-07 | Do not bypass, stub or wrap the guard. | test | Verified | Direct invocations (07-4); the mutant fails 7c while the guard cases still run. |
| 47-07 | Do not read the real signing key or modify global config or hooks. | judgment | **Flagged: unverified-prohibition, human review recommended** | Non-authoritative read of the script: holds (item 4). |
| 47-07 | Do not write a mutant or scratch file under the repository. | test | Verified (end state) | `git status` shows no tracked or untracked phase-47 files beyond pre-existing `graphify-out/` output and `.planning/user/errors/state-stale.json`; this run's own mutant lived in the scratchpad clone. |

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|---|---|---|
| 1 | Live model compliance with the decision and gate-rule wording | Phase 49 | Phase 47 criterion 4; Phase 49 criterion 4 → `47-PHASE49-OBSERVATION.md` |

### Advisory (New Scope, Unevidenced)

None. The four review fixes and `7400042` each carry a named regression test or harness case that this run executed green. No new-scope concern without deterministic evidence was raised.

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `crates/devflow-core/src/prompt.rs` | Shared arm helper, one gate rule, last-line protocol, prose mitigation | ✓ VERIFIED | Contains `the LAST line of your FINAL message must be exactly:`; all named prompt tests pass. |
| `crates/devflow-core/src/agent_result.rs` | Contract and detector tests | ✓ VERIFIED | Contains `fn resume_prompt_does_not_read_as_a_blocking_human_checkpoint`; the detector's post-review tests (`…sees_through_list_and_quote_markup`, `literal_backslash_n_in_agent_text_is_not_a_line_break`, live-rendering regressions) pass. |
| `crates/devflow-cli/src/pipeline_launch.rs` | `resume_launch_shape`, delivery test | ✓ VERIFIED | Emit → shape → spawn order at :1126/:1142/:1153. |
| Core and CLI snapshot directories | 9 reviewed baselines | ✓ VERIFIED | `gsd-tools verify.artifacts` errors with `EISDIR` on directory artifacts (a tool limitation), so they were checked by `git ls-files` and numstat instead. |
| `scripts/check.sh` | Hardened `run_test`, harness always runs | ✓ VERIFIED | Behavioural tests in `check_script_run_test.rs` (3 passed) plus the green `all` run. |
| `scripts/test-phase-worktree-guard.sh` | Hermetic harness, cases 0-8 | ✓ VERIFIED | 19/0 under four environments; mutant control fails 7c. |
| `.planning/user/DEV-SETUP-CHECKLIST.md` | Count and isolation note | ✓ VERIFIED (pattern superseded) | `verify.artifacts`: `Missing pattern: isolated from inherited hooks and commit signing`; the current text is accurate (07-6). |
| `ARCHITECTURE.md`, `docs/guides/unattended-mode.md`, ROADMAP, REQUIREMENTS, `47-PHASE49-OBSERVATION.md` | Record and handoff | ✓ VERIFIED | See 05-x, 06-7. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `scripts/check.sh run_test` | CI required Test job | `.github/workflows/ci.yml:63` `scripts/check.sh test` | ✓ WIRED | Parity guard passes. |
| `fix_prompt` | `CODE_STAGE_POLICY` | FullExecute arm helper | ✓ WIRED | The six-adapter test was red before the helper and is green after. |
| `GATE_RESOLUTION_RULE` | `CODE_STAGE_POLICY` and the CLI delivery test | `gate_resolution_rule!()` | ✓ WIRED | prompt.rs:88/124; pipeline_launch.rs:4075. |
| `relaunch_checkpoint_session` | `resume_launch_shape` → spawn | event emitted before spawn | ✓ WIRED | :1126 → :1142 → :1153. |
| `COMPLETION_PROTOCOL` | `evaluate_layer1` → `parse_marker_lines` | agent final message | ✓ WIRED | Contract test passes on three capture shapes. |
| `checkpoint_auto_decide_prompt` | `blocking_human_checkpoint_reported` | resumed capture | ✓ WIRED | Resume-prompt detector test passes against the HEAD detector. |
| harness entry | no inherited config | `--local-env-vars` unset, global nulled after case 0, local pins | ✓ WIRED | Hostile and injected runs 19/0; mutant fails 7c; source-order guard `worktree_guard_harness_queries_the_checkout_before_nulling_global_config` passes. |
| Phase 49 criterion 4 | evidence standard | `47-PHASE49-OBSERVATION.md` | ✓ WIRED | One pointer. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|---|---|---|---|---|
| `fix_prompt` | rendered loop-back prompt | `StageIntent::Code { fix: Some(FullExecute) }` through each driver | Yes | ✓ FLOWING |
| `resume_launch_shape` | resume `argv[1]` | `checkpoint_auto_decide_prompt(phase)` via `ClaudeDriver::exec_resume_command` | Yes | ✓ FLOWING |
| snapshots | prompt text | the two rendered strings above | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

All `cargo test` runs used `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no` and module-qualified `--exact` names, one run per binary, asserting passed == number of names.

| Behavior | Command (abridged) | Result | Status |
|---|---|---|---|
| 8 prompt tests | `-p devflow-core --lib -- --exact prompt::tests::…` | 8 passed; 766 filtered out | ✓ PASS |
| 7 contract/detector tests | `-p devflow-core --lib -- --exact agent_result::tests::…` | 7 passed; 767 filtered out | ✓ PASS |
| Two-turn delivery | `-p devflow --bin devflow -- --exact pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns` | 1 passed; 363 filtered out | ✓ PASS |
| Snapshot env pin + harness order guard | `--test ci_parity_guards -- --exact …` | 2 passed; 25 filtered out | ✓ PASS |
| Ignore split | `--test gitignore_coverage -- --exact …` | 1 passed; 1 filtered out | ✓ PASS |
| check.sh failure accounting | `--test check_script_run_test -- --exact` (all 3 names) | 3 passed; 0 filtered out (the binary holds exactly 3) | ✓ PASS |
| Negative control for the counting | `--exact prompt::tests::no_such_test_name_exists` | exit 0, `0 passed; 774 filtered out`; flagged as mismatch | ✓ control fired |
| Full gate | `scripts/check.sh all` | exit 0, harness 19/0 | ✓ PASS |
| Drift guard / its control | prefixed vs unprefixed cargo test on a drifted baseline (scratch clone) | 101 FAILED, baseline intact / 0 and rewritten | ✓ PASS |
| Historical red/green | tests at `59e4334`/`062ebf7`, `94ef699`/`cab8d54` (scratch clone) | FAILED → ok for all three | ✓ PASS |
| Harness hostile/injected/mutant | `test-phase-worktree-guard.sh` in scratch clone | 19/0 ×4; mutant 18/1 `FAIL 7c.`; restored 19/0 | ✓ PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` exists (0 found) and no Phase 47 PLAN or SUMMARY references one. Step 7c: not applicable.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| DECN-02 | 47-01, 02, 04, 05, 06 | Policy reaches the four claude-style adapters on the FullExecute loop-back; other arms excluded | ✓ SATISFIED | R1, R2, 02-1..02-4, snapshots |
| DECN-03 | 47-01, 03, 04, 05, 06 | A resumed session gets one consistent instruction about resolving a `blocking-human` gate | ✓ SATISFIED (prompt, parser and detector level; claude only per the recorded closure limit) | R3, 03-1..03-5, 06-1..06-4. The behavioural arm is deferred to Phase 49. |
| WR-01 | 47-07 | Review finding: worktree-guard fixture isolation | ✓ SATISFIED | 07-1..07-6. It is a review-finding ID, not a REQUIREMENTS.md entry. |

No orphaned requirements: REQUIREMENTS.md maps only DECN-02 and DECN-03 to Phase 47 (:172-173), and both are claimed by plans.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| — | — | `TBD`/`FIXME`/`XXX` | none | 0 hits across the 28 non-planning files changed since fork `1dda004`. |
| `47-06-PLAN.md`, `47-07-PLAN.md` | must_haves | Must-have literals superseded by later commits (byte range, call count 6, `passed=12`, count 10, checklist phrase) | ⚠️ Warning | Mechanical checks (`verify.artifacts`) will keep reporting the checklist phrase. The protected properties hold at HEAD. Amending the plan text is outside this verifier's remit. |
| `crates/devflow-core/src/agent_result.rs` | 631-770 | Detector changed after a plan prohibition forbade it | ⚠️ Warning | Routed to Human Verification item 1. The change carries its own regression tests, all green. |

### Human Verification Required

1. **Detector changes after plan 47-06.**
   - **Test:** Decide whether `bd7bd7c`, `ac3ee42` and `49bb324` supersede 47-06's "must not change the result parser or the gate detector" prohibition.
   - **Expected:** Accept the changes (and add an `overrides:` entry), or request a revert or re-review.
   - **Why human:** Plan 06 complied and the parser is untouched. Only `bd7bd7c` has a recorded operator decision (REVIEW.md, "fix every finding except IN-05"). `ac3ee42` and `49bb324` are recorded in REVIEWS.md without one.
2. **47-06 test inputs come from production text** (judgment-tier).
   - **Expected:** The success line comes from `COMPLETION_PROTOCOL`, the label from `HUMAN_GATE_VALUE`.
   - **Why human:** Non-authoritative verifier read says this holds.
3. **No 47-06 push before both Task 3 gates passed** (judgment-tier).
   - **Why human:** Push timing is not in the repository.
4. **47-07 never touched your real signing key, global config or hooks** (judgment-tier).
   - **Why human:** The script at HEAD uses scratch configs with a nonexistent key; what happened during execution is not observable now.

### Gaps Summary

No gaps. Phase 47's goal holds at HEAD:

- The FullExecute loop-back carries the policy on all six adapters, with real controls.
- A resumed Claude session is handed one gate rule in two production-built turns.
- The last-line completion contract lets required reasoning coexist with the result marker.
- The snapshot guard discriminates in both directions.
- The harness gate runs under hostile and injected configuration.

The review fixes since the previous report are exercised by named tests or harness cases, and all pass.

The status is `human_needed` only because of the four prohibition items above. They are decisions or unobservable history, not missing code.

**What these checks do not establish:** live model instruction-following (Phase 49); behaviour on GitHub's runner or inside the pinned CI container (the `all` run and harness runs were on this host); the detector against a fresh live checkpoint capture (the live-rendering tests use transcribed captures); reliability of the full suite beyond this single green run.

---

_Verified: 2026-09-13T06:58:15Z_
_Verifier: Claude (gsd-verifier)_
