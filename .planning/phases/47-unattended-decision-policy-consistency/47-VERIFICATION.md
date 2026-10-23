---
phase: 47-unattended-decision-policy-consistency
verified: 2026-10-23T10:17:44Z
status: passed
score: 39/39 must-haves verified
covered_files:
  - ".github/workflows/ci.yml"
  - ".gitignore"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-01-PLAN.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-01-SUMMARY.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-02-PLAN.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-02-SUMMARY.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-03-PLAN.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-03-SUMMARY.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-04-PLAN.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-04-SUMMARY.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-05-PLAN.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-05-SUMMARY.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-06-PLAN.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-06-SUMMARY.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-07-PLAN.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-07-SUMMARY.md"
  - ".planning/phases/47-unattended-decision-policy-consistency/47-PHASE49-OBSERVATION.md"
  - ".planning/user/DEV-SETUP-CHECKLIST.md"
  - "ARCHITECTURE.md"
  - "Cargo.lock"
  - "Cargo.toml"
  - "crates/devflow-cli/Cargo.toml"
  - "crates/devflow-cli/src/pipeline_launch.rs"
  - "crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_one_code_prompt.snap"
  - "crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap"
  - "crates/devflow-cli/tests/ci_parity_guards.rs"
  - "crates/devflow-cli/tests/gitignore_coverage.rs"
  - "crates/devflow-cli/tests/worktree_guard_harness.rs"
  - "crates/devflow-core/Cargo.toml"
  - "crates/devflow-core/src/agent_result.rs"
  - "crates/devflow-core/src/agents/claude.rs"
  - "crates/devflow-core/src/prompt.rs"
  - "crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap"
  - "crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_antigravity.snap"
  - "crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap"
  - "crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_codex.snap"
  - "crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_hermes.snap"
  - "crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_opencode.snap"
  - "crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_pi.snap"
  - "docs/guides/unattended-mode.md"
  - "scripts/check.sh"
  - "scripts/lint-phase-worktree.sh"
  - "scripts/test-phase-worktree-guard.sh"
covered_digest: "v3:sha256:10e77006d2d6a00a8e5eee8d599ed7a0ec329c2ac07a07c858566230d7d9c24d"
behavior_unverified: 0
overrides_applied: 1
overrides:
  - must_have: "MUST NOT allow any text after the DEVFLOW_RESULT line, introduce a second record or marker format, or change the result parser or the gate detector (operator decision A1)."
    reason: "Review-fix commits bd7bd7c (match only Gate-labelled lines), ac3ee42 (recognise list and blockquote declarations) and 49bb324 (decode escaped captures) changed the gate detector after 47-06 to close code-review findings. Each carries its own regression test, all passing at 6ae0b77, and the result parser is untouched. bd7bd7c was already covered by the recorded 'fix every finding except IN-05' decision; the operator accepted all three on 2026-09-13."
    accepted_by: "operator"
    accepted_at: "2026-09-13T07:08:34Z"
re_verification:
  previous_status: passed
  previous_score: 39/39
  previous_verified_at: 6ae0b77
  reason: "stale: covered files changed by phases 48-50 (prompt.rs, agent_result.rs, pipeline_launch.rs, ci.yml, snapshots, check.sh)"
  gaps_closed: []
  gaps_remaining: []
  regressions: []
deferred:
  - truth: "Which instruction a live Claude agent actually follows when CODE_STAGE_POLICY and the resume instruction are both present (the DECN-03 behavioural arm)."
    addressed_in: "Phase 49"
    evidence: "ROADMAP Phase 47 criterion 4 ('Deliberately NOT closed here'); Phase 49 criterion 4 points at 47-PHASE49-OBSERVATION.md. 49-VERIFICATION.md row 4 records one outcome, Followed (one model, one sample)."
---

# Phase 47: Unattended Decision Policy Consistency Verification Report

**Phase Goal:** An unattended agent receives the same merit-based decision instruction everywhere it is asked to make a Code-stage decision (including the claude, opencode, hermes and antigravity Validate loop-back), and is never handed two contradictory instructions about who may resolve a `blocking-human` gate inside one resumed session.
**Verified:** 2026-10-23T10:17:44Z against HEAD `91d7064` on `workspace/denniyahh` (code identical to `78178f6` apart from `graphify-out/`)
**Status:** passed
**Re-verification:** Yes. The 2026-09-13 report (`passed`, 39/39, HEAD `6ae0b77`) went stale because phases 48-50 changed 22 covered files. Every must-have was re-checked against current code; the prior report was used only as a map.

## What changed since the previous verdict

Later phases changed phase 47's code in three ways. None of them breaks the phase goal.

1. **Phase 49 (CR-01 / inserted 49.1, `ab6f6f2`..`e980201`) rewrote the gate detector and added a field to the result marker.**
   - `AgentResult` gained a `checkpoint` field.
   - `CODE_STAGE_POLICY` gained one sentence, appended after the pinned gate-rule section. It tells the agent how to report a blocking-human gate it is not resolving (`"checkpoint": "blocking-human"` on the final marker), and says the field must not be used for a gate it resolved.
   - The detector now reads the latest stated outcome. `text_reports_human_gate` was replaced by `line_reports_human_gate` plus `latest_stated_outcome`.
   - All eight `CODE_STAGE_POLICY`-carrying snapshots were re-blessed with that one line.
   - Verifier read of the new sentence: it does not contradict the single gate rule. It applies only when the agent is reporting a gate, which is the rule's own "otherwise report it instead" branch. It grants no new authority.
2. **`8c41c46` moved the worktree-guard harness out of `scripts/check.sh`** into `crates/devflow-cli/tests/worktree_guard_harness.rs`, and deleted `check_script_run_test.rs`, which had tested only the removed wiring. `check.sh`'s snapshot-guard prefix is unchanged.
3. **Four must-have literals are now stale, but the property each protects still holds** (rows 06-4, 06-5 and 06-6, plus the 07-6 checklist phrase).

## Goal Achievement

### Observable Truths

Roadmap criteria are rows R1-R4; plan truths are `PP-n`. All test evidence was produced in this run at `91d7064`, unless a historical commit is named.

| # | Truth | Status | Evidence |
|---|---|---|---|
| R1 | `fix_prompt` renders `CODE_STAGE_POLICY` for FullExecute; GapsOnly/AuditFix still omit it; the existing omission test is unmodified | ✓ VERIFIED | `fix_prompt` (prompt.rs:708) gates the policy on `code_policy_applies_to_fix_arm` (:546). The named tests `code_policy_reaches_the_full_execute_fix_arm_on_every_adapter`, `claude_style_fix_prompts_that_must_not_carry_the_policy_still_omit_it` and `code_policy_is_absent_from_prompts_that_must_not_carry_it` pass. The omission test's body is byte-identical to fork `1dda004` (29 lines, `diff` empty). The fork test module starts at line 583, and no hunk since the fork removes a line at or after it. |
| R2 | The loop-back presence test fails pre-fix and passes after | ✓ VERIFIED | Historical: red at `59e4334` and green at `062ebf7`, re-executed by the 2026-09-13 verifier. Both commits still exist and are immutable. Passes at HEAD. |
| R3 | A prompt with both texts states one gate rule; a contradiction test fails before and passes after | ✓ VERIFIED | `prompt::tests::the_gate_rule_has_one_definition_site` and `pipeline_launch::tests::the_gate_rule_holds_in_both_delivered_turns` pass at HEAD. Historical red/green: `94ef699` and `cab8d54`. Turn 1 carries `GATE_RESOLUTION_RULE` once, plus the phase-49 reporting sentence (read: no conflicting condition). Turn 2 says "resumed you specifically to resolve that gate". |
| R4 | The behavioural question is recorded for Phase 49, not settled on a source read | ✓ VERIFIED | `47-PHASE49-OBSERVATION.md` defines Followed, Not followed and Void (lines 12-18). ROADMAP Phase 49 criterion 4 carries the pointer (ROADMAP.md:286). |
| 01-1 | A wording change to a policy-carrying prompt fails `scripts/check.sh test` | ✓ VERIFIED | This run, scratch copy of HEAD: one word drifted in `..._codex.snap`, then the test was run under check.sh's literal prefix `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`. Result: `1 test run: 0 passed, 1 failed`, "snapshot assertion for 'full_execute_fix_prompt_codex' failed", baseline unchanged, 0 `.snap.new`. check.sh:60 still carries the prefix, and `ci_parity_guards::check_script_pins_snapshots_against_env_override` passes. CI runs `scripts/check.sh test` (ci.yml:71). |
| 01-2 | `INSTA_FORCE_UPDATE=1` cannot re-bless drift green | ✓ VERIFIED | The 01-1 run exported `INSTA_FORCE_UPDATE=1 INSTA_UPDATE=always` before the prefix and still failed. **Negative control:** the same drift and environment without the prefix exited 0 and rewrote the baseline (drift erased). insta is 1.48.0 at both `6ae0b77` and HEAD. |
| 01-3 | A clean tree passes `scripts/check.sh test` | ✓ VERIFIED (inherited regression gate) | Per the orchestrator, `scripts/check.sh all` passed in the pinned CI container at `78178f6` (2026-10-23). This verifier did not re-run the full suite. Every named test below passes at HEAD. |
| 01-4 | `.snap.new` is ignored; `.snap` is not | ✓ VERIFIED | `gitignore_coverage::gitignore_ignores_snapshot_scratch_but_not_the_baseline` passes (1 run, 1 passed). |
| 02-1 | FullExecute loop-back delivers the policy on claude, opencode, hermes and antigravity | ✓ VERIFIED | The six-adapter test passes. It walks `AgentKind` exhaustively and requires `COMPLETION_PROTOCOL` as a non-empty-search precondition. |
| 02-2 | codex and pi still deliver it, as controls | ✓ VERIFIED | Covered by the same test; red pre-fix (R2). |
| 02-3 | GapsOnly and AuditFix omit the policy on both render paths | ✓ VERIFIED | Claude-style twin and workflow-style omission test both pass. |
| 02-4 | `workflow_code_prompt`'s existing tests pass unmodified | ✓ VERIFIED | No line removed from the test module since the fork; the tests pass. |
| 03-1 | One gate rule: a prohibition with exception in the policy, stated affirmatively in the resume instruction | ✓ VERIFIED | `gate_resolution_rule!` (prompt.rs:65) feeds `GATE_RESOLUTION_RULE` (:88) and `CODE_STAGE_POLICY` (:150). `checkpoint_auto_decide_prompt` (:664) states the same condition affirmatively. Both gate-rule tests pass. |
| 03-2 | The carve-out covers `blocking-human` only; package verification stays unconditional | ✓ VERIFIED | `package_verification_prohibition_is_unconditional` (pinned section still contained verbatim) and `code_policy_excludes_blocking_human_and_package_checkpoints` pass. |
| 03-3 | `checkpoint_auto_decide_prompt` names `blocking-human` | ✓ VERIFIED | Text: "stopped at a blocking-human gate" (prompt.rs:667). The delivery test fails if turn 2 lacks it. |
| 03-4 | Both halves of the layered test went red pre-fix with `test result: FAILED` | ✓ VERIFIED (historical) | Re-executed at `94ef699` by the 2026-09-13 verifier; the commit is immutable and unchanged. |
| 03-5 | Each turn comes from its production constructor, never concatenated | ✓ VERIFIED | Test (pipeline_launch.rs:7210): turn 1 from `resolve_launch_shape` → `PipeOwning { prompt }`; turn 2 from `resume_launch_shape(...).1[1]`. It asserts mutual non-containment. `relaunch_checkpoint_session` emits `checkpoint_auto_decided` (:2070) before `resume_launch_shape` (:2086) and before `spawn_agent_and_record` (:2097). `relaunch_checkpoint_session_emits_exactly_one_audit_event` passes. |
| 04-1 | Every policy-carrying prompt has a committed, reviewed baseline | ✓ VERIFIED | 9 tracked `.snap` under `crates/*/src/snapshots`; snapshot tests pass under the pinned env; 0 `.snap.new` on disk. |
| 04-2 | The two turns are snapshotted separately | ✓ VERIFIED | Two `assert_snapshot!` calls (pipeline_launch.rs:7287-7288), two files. |
| 04-3 | The suite is scoped to policy-carrying prompts | ✓ VERIFIED | The 9 files are 7 FullExecute fix prompts, turn 1 and turn 2. Each contains the last-line protocol; 8 carry the policy heading, and turn 2 is the resume instruction. |
| 05-1 | The guide says approving the parked gate calls `launch_stage_inner` directly and skips the check | ✓ VERIFIED | `docs/guides/unattended-mode.md:92-94`. |
| 05-2 | The never-auto-approved statement is qualified, not inverted | ✓ VERIFIED | `unattended-mode.md:29-33`. |
| 05-3 | ROADMAP and REQUIREMENTS name the four/two adapter split | ✓ VERIFIED | REQUIREMENTS.md:18-21; ROADMAP Phase 47 goal. |
| 05-4 | Phase 49 has one pointer and nothing else changed | ✓ VERIFIED | ROADMAP.md:286, a single reference. |
| 05-5 | The behavioural question has a Followed / Not followed / Void standard | ✓ VERIFIED | `47-PHASE49-OBSERVATION.md:12-18`. |
| 05-6 | The two D-13 defects are on the backlog | ✓ VERIFIED | ROADMAP 999.125 (:1880) and 999.126 (:1866), both since promoted to Phase 48. |
| 06-1 | Every prompt ending with `COMPLETION_PROTOCOL` puts `DEVFLOW_RESULT` last, with reasoning above | ✓ VERIFIED | prompt.rs:46 and :54-55. All 9 snapshots carry the last-line sentence. The contract test asserts that both the fix and resume prompts `ends_with(COMPLETION_PROTOCOL)`. |
| 06-2 | Over 5000 characters of reasoning above the success line parses to Success at `evaluate_layer1` (plain, envelope, stream with failed earlier turns) | ✓ VERIFIED | `decision_reasoning_above_the_result_line_parses_to_that_result` passes; the success line is taken from `COMPLETION_PROTOCOL`. |
| 06-3 | Reasoning after the line does not parse to Success; a short trailing line still does | ✓ VERIFIED | Same test's controls; `TAIL_BUDGET_CHARS = 4000` (agent_result.rs:2212). |
| 06-4 | The resume prompt names the gate in prose; the rendered prompt trips neither detector; with the label appended it trips both | ✓ VERIFIED (detector names superseded) | `resume_prompt_does_not_read_as_a_blocking_human_checkpoint` passes against the HEAD detector. `text_reports_human_gate` no longer exists; the test now uses `line_reports_human_gate` plus `blocking_human_checkpoint_reported`. Both are negative on the bare prompt and positive with the `HUMAN_GATE_VALUE` declaration appended. |
| 06-5 | Nine baselines byte-identical to `b3b47ea` except the expected substitutions | ✓ VERIFIED (plan-scoped; HEAD literal superseded) | Plan 06's re-bless was verified on 2026-09-13. At HEAD the numstat against `b3b47ea` is 3/3 for all nine. Eight files carry plan 06's 2/2 plus phase 49's one checkpoint-declaration line. Turn 2 is unchanged since `6ae0b77` (still 3/3). The extra line is the sentence quoted in "What changed", committed in `ab6f6f2`. |
| 06-6 | Outside the two permitted regions, `prompt.rs` is unchanged since `b3b47ea`; agent_result.rs 1-3441 is byte-identical; the parser is untouched | ✓ VERIFIED (plan-scoped; HEAD literal superseded) | Re-checked this run: `git diff b33406c^ 1691b12`, which is plan 06's five commits. agent_result.rs gets only test-module additions (`@@ -3445,0`, `@@ -3619,0`; tests start at 3441). prompt.rs changes only the protocol (42-55), the doc comment (599) and `checkpoint_auto_decide_prompt` (613). At HEAD, phase 49 later changed `CODE_STAGE_POLICY` (one appended sentence), the parser (a `checkpoint` field) and the detector. See Prohibitions. |
| 06-7 | ARCHITECTURE.md describes the result as the last line with reasoning above | ✓ VERIFIED | ARCHITECTURE.md:115-116. |
| 07-1 | The harness passes under global SSH signing plus a hooks dir, with no agent | ✓ VERIFIED | This run, scratch clone at `91d7064`, `env -u SSH_AUTH_SOCK GIT_CONFIG_GLOBAL=<failing pre-commit/post-checkout hooks, gpgsign=true, ssh format, nonexistent key>`: exit 0, `passed=19 failed=0`. |
| 07-2 | The scratch repo isolates hooks and signing through local config before its first commit | ✓ VERIFIED | test-phase-worktree-guard.sh:70-71 run before the first commit; the 07-5 mutant shows these lines are load-bearing. |
| 07-3 | The harness passes with config injected through `GIT_CONFIG_COUNT` or `GIT_CONFIG_PARAMETERS` | ✓ VERIFIED | Both injected runs: 19/0. **Control:** the same `GIT_CONFIG_PARAMETERS` hook injection made a plain scratch-repo commit exit 1. The harness unsets `git rev-parse --local-env-vars` (:15-16). |
| 07-4 | The guard is still invoked directly | ✓ VERIFIED (count superseded) | Script blob `1485b44` is identical at `6ae0b77` and HEAD: the six original direct calls plus three added in `7400042`, with no wrapper. |
| 07-5 | Removing the two isolation lines prints `FAIL 7c.` and exactly one failure | ✓ VERIFIED (count superseded) | Mutant in the scratch clone (diff shows exactly lines 70-71 removed): exit 1, `FAIL 7c. fixture bootstrap under hostile config -> allow  expected allow, got refuse`, `passed=18 failed=1`. Restored copy: 19/0. |
| 07-6 | The checklist states the harness size accurately and states its isolation | ✓ VERIFIED (text superseded) | DEV-SETUP-CHECKLIST.md:125 says "19 checks", matching runtime 19. Lines 127-140 describe the cargo-test wiring, the nulled global/system config and the local isolation. `verify.artifacts` still reports the plan's old phrase `isolated from inherited hooks and commit signing` as missing. |

**Score:** 39/39 truths verified (0 present-but-behavior-unverified). Eight carry a superseded literal or a historical/inherited basis, as noted.

### Prohibitions

| Plan | Prohibition (abridged) | Tier | Disposition | Evidence |
|---|---|---|---|---|
| 47-06 | Do not widen the carve-out, touch the pinned gate rule, or weaken the package-verification prohibition | test | Verified | `package_verification_prohibition_is_unconditional` passes: the pinned section is still contained verbatim and `GATE_RESOLUTION_RULE` equals the pin. Phase 49's sentence sits after the section and grants nothing. |
| 47-06 | Do not re-bless beyond the expected substitutions | test | Verified (plan-scoped) | Plan 06's re-bless was verified on 2026-09-13. The later +1 line per file is phase 49's own reviewed change. |
| 47-06 | No text after `DEVFLOW_RESULT`, no second marker format, no parser or detector change | test | PASSED (override) | The override covers `bd7bd7c`, `ac3ee42` and `49bb324`. **Its reason does not name the later phase-49 changes** (`ab6f6f2`..`e980201`). Those trace to a separate operator decision: STATE.md:989 inserted Phase 49.1 to replace the heuristic "with an explicit checkpoint field in the DEVFLOW_RESULT marker", and 49-REVIEW-DISPOSITION.md:137 records "operator decision 2026-10-09". The A1 contract itself holds: the protocol still says "Output nothing after it", the tail budget is unchanged, and `checkpoint` is an extra field in the same JSON marker, not a second format. |
| 47-06 | Test inputs come from production text | judgment | Resolved (operator, 2026-09-13; re-read holds at HEAD) | The contract test takes the success line from `COMPLETION_PROTOCOL`; the resume test builds its declaration from `HUMAN_GATE_VALUE`. |
| 47-06 | Mitigation only in `checkpoint_auto_decide_prompt`, without the detector's match shape | test | Verified | The "plain prose" sentence is only at prompt.rs:673, and the resume-prompt test passes. |
| 47-06 | No push before both Task 3 gates passed | judgment | Resolved (operator, 2026-09-13) | A historical fact; nothing since can change it. |
| 47-07 | Do not bypass, stub or wrap the guard | test | Verified | Direct invocations; the mutant fails 7c while the guard cases still run. |
| 47-07 | Never touch the real signing key, global config or hooks | judgment | Resolved (operator, 2026-09-13) | Historical. This run's hostile config was a scratch file naming a nonexistent key. |
| 47-07 | No mutant or scratch file under the repository | test | Verified (end state) | `git status --porcelain` printed 0 lines after this run. |

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|---|---|---|
| 1 | Live model compliance with the gate-rule wording | Phase 49 | 49-VERIFICATION.md row 4: one outcome, **Followed** (one model, one sample). STATE.md:989 notes that the run used the pre-49.1 heuristic binary. |

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `crates/devflow-core/src/prompt.rs` | Arm helper, one gate rule, last-line protocol, prose mitigation | ✓ VERIFIED | Contains `the LAST line of your FINAL message must be exactly:`; 11 named prompt tests pass. |
| `crates/devflow-core/src/agent_result.rs` | Contract and detector tests | ✓ VERIFIED | Contains `fn resume_prompt_does_not_read_as_a_blocking_human_checkpoint`; 5 named tests pass. |
| `crates/devflow-cli/src/pipeline_launch.rs` | `resume_launch_shape`, delivery test | ✓ VERIFIED | :246 and :7210; emit → shape → spawn order. |
| Core and CLI snapshot dirs | 9 baselines | ✓ VERIFIED | `verify.artifacts` reports "Not a file: path is a directory" (a tool limitation), so they were checked with `git ls-files` and numstat. |
| `scripts/check.sh` | Snapshot env pin in `run_test` | ✓ VERIFIED | :60. The harness wiring moved to `worktree_guard_harness.rs` (`8c41c46`). |
| `scripts/test-phase-worktree-guard.sh` | Hermetic harness | ✓ VERIFIED | 19/0 under four environments; the mutant fails 7c. |
| `crates/devflow-cli/tests/worktree_guard_harness.rs` | Runs the harness inside `cargo test` | ✓ VERIFIED | Both tests pass (2 run, 2 passed). |
| `.planning/user/DEV-SETUP-CHECKLIST.md` | Count and isolation note | ✓ VERIFIED (pattern superseded) | See 07-6. |
| ARCHITECTURE.md, unattended-mode.md, ROADMAP, REQUIREMENTS, 47-PHASE49-OBSERVATION.md | Record and handoff | ✓ VERIFIED | See rows 05-x and 06-7. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `check.sh run_test` | CI Test job | ci.yml:71 `scripts/check.sh test` | ✓ WIRED | Parity guard passes |
| `fix_prompt` | `CODE_STAGE_POLICY` | `code_policy_applies_to_fix_arm` | ✓ WIRED | Six-adapter test |
| `GATE_RESOLUTION_RULE` | `CODE_STAGE_POLICY` and the CLI delivery test | `gate_resolution_rule!()` | ✓ WIRED | prompt.rs:88/150; pipeline_launch.rs `turn1.contains(prompt::GATE_RESOLUTION_RULE)` |
| `relaunch_checkpoint_session` | `resume_launch_shape` → spawn | event before spawn | ✓ WIRED | :2070 → :2086 → :2097 |
| resume route | Claude only | `state.agent == AgentKind::Claude` | ✓ WIRED | pipeline_launch.rs ~2639; matches the REQUIREMENTS closure limit |
| `COMPLETION_PROTOCOL` | `evaluate_layer1` → `parse_marker_lines` | final message | ✓ WIRED | Contract test, three capture shapes |
| `checkpoint_auto_decide_prompt` | `blocking_human_checkpoint_reported` | resumed capture | ✓ WIRED | Resume-prompt detector test against the HEAD detector |
| Phase 49 criterion 4 | evidence standard | `47-PHASE49-OBSERVATION.md` | ✓ WIRED | One pointer |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|---|---|---|---|---|
| `fix_prompt` | rendered loop-back prompt | `StageIntent::Code { fix: Some(FullExecute) }` through each driver | Yes | ✓ FLOWING |
| `resume_launch_shape` | resume `argv[1]` | `checkpoint_auto_decide_prompt` via `ClaudeDriver::exec_resume_command` | Yes | ✓ FLOWING |
| snapshots | prompt text | the two rendered strings above | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

Each run used `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no cargo nextest run --no-tests=fail` with a union of exact `test(=path)` filters. The verdict required rc 0 **and** "N tests run: N passed", where N is the number of names given.

| Behavior | Command (abridged) | Result | Status |
|---|---|---|---|
| 11 prompt + 5 agent_result tests | `-p devflow-core --lib` | 16 run, 16 passed | ✓ PASS |
| Delivery test + audit-event test | `-p devflow --bin devflow` | 2 run, 2 passed | ✓ PASS |
| Snapshot env pin | `--test ci_parity_guards` | 1 run, 1 passed | ✓ PASS |
| Ignore split | `--test gitignore_coverage` | 1 run, 1 passed | ✓ PASS |
| Harness in cargo test + source-order guard | `--test worktree_guard_harness` | 2 run, 2 passed | ✓ PASS |
| Negative control for the counting | one real name + `prompt::tests::no_such_test_exists_47` | rc 0, `1 test run` → flagged MISMATCH | ✓ control fired |
| Drift guard / its control | prefixed vs unprefixed on a drifted codex snap (scratch copy) | 1 failed, baseline intact / passed, baseline rewritten | ✓ PASS |
| Harness hostile / injected ×2 / mutant | `test-phase-worktree-guard.sh` in a scratch clone | 19/0 ×3; mutant 18/1 `FAIL 7c.`; restored 19/0 | ✓ PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` exists, and no Phase 47 PLAN or SUMMARY declares one. Not applicable.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| DECN-02 | 47-01, 02, 04, 05, 06 | The policy reaches the four claude-style adapters on the FullExecute loop-back; other arms excluded | ✓ SATISFIED | R1, R2, 02-1..02-4, snapshots |
| DECN-03 | 47-01, 03, 04, 05, 06 | A resumed session gets one consistent instruction about resolving a `blocking-human` gate | ✓ SATISFIED (prompt level; claude only, per the closure limit) | R3, 03-1..03-5, 06-1..06-4; behavioural arm in Phase 49 |
| WR-01 | 47-07 | Review finding: worktree-guard fixture isolation | ✓ SATISFIED | 07-1..07-6. A review-finding ID, not a REQUIREMENTS.md entry |

No orphaned requirements: REQUIREMENTS.md maps only DECN-02 and DECN-03 to Phase 47 (:197-198).

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| none | none | `TBD`/`FIXME`/`XXX` | none | 0 hits across the 11 covered source/doc/script files. Control: `rg -c -w COMPLETION_PROTOCOL prompt.rs` = 19 |
| `47-06-PLAN.md`, `47-07-PLAN.md` | must_haves | Literals superseded by later commits | ℹ️ Info | `verify.artifacts` will keep flagging the checklist phrase; the protected properties hold |
| `crates/devflow-core/src/agent_result.rs` | detector | Changed after a 47-06 prohibition forbade it | ℹ️ Info | Covered by the override plus the later 49.1 operator decision; see Prohibitions |

### Human Verification Required

None. The three judgment-tier prohibitions concern execution history from 2026-09-11..13. The operator answered them on 2026-09-13, and no later change can alter them. The one test-tier prohibition whose literal no longer holds is covered by the accepted override for the 47-era commits, and by the recorded 49.1 operator decision for the later ones.

### Gaps Summary

No gaps. At `91d7064`:
- The FullExecute loop-back carries the policy on all six adapters, and the GapsOnly/AuditFix exclusions hold with their original test unmodified.
- A resumed Claude session gets one gate rule, in two separately built turns.
- The last-line completion contract survives phase 49's detector rewrite.
- The snapshot drift guard discriminates in both directions.
- The worktree-guard harness holds under hostile and injected config.

**What these checks do not establish:**
- That the full workspace suite is green: this run relies on the orchestrator's report of the pinned-container `check.sh all` at `78178f6`.
- The historical red runs (R2, 03-4): they were not re-executed this run. They rest on the 2026-09-13 re-execution at immutable commits.
- That a model obeys the wording: Phase 49 has one sample, run on the pre-49.1 binary.
- That the gate-rule tests would catch a *new* contradictory sentence added elsewhere in the policy: they check the old contradiction, the presence of the shared rule, and the pinned section. A new sentence is caught only by reviewing the snapshot diff. This verifier read the one sentence phase 49 added and found no contradiction.

---

_Verified: 2026-10-23T10:17:44Z_
_Verifier: Claude (gsd-verifier)_
