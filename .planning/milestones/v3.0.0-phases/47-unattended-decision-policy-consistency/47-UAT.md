---
status: complete
phase: 47-unattended-decision-policy-consistency
source: [47-01-SUMMARY.md, 47-02-SUMMARY.md, 47-03-SUMMARY.md, 47-04-SUMMARY.md, 47-05-SUMMARY.md, 47-06-SUMMARY.md, 47-07-SUMMARY.md]
started: 2026-09-12T23:33:10Z
updated: 2026-09-13T00:09:19Z
---

<!--
Automated entries re-checked at HEAD 8d60ff8 on 2026-09-12 (env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no, --exact, module-qualified names):
  devflow-core lib, 7 prompt::tests named tests: 7 passed, 767 filtered out
  devflow bin, 2 pipeline_launch::tests named tests: 2 passed, 362 filtered out
  devflow --test ci_parity_guards, 1 named test: 1 passed, 26 filtered out
  devflow --test gitignore_coverage, 1 named test: 1 passed, 1 filtered out
  devflow-core lib, 2 agent_result::tests named tests from 47-06: 2 passed, 772 filtered out
  No .snap.new written; no tracked snapshot changed.
Not re-run in this session (executor evidence only): 47-01 D1 plan gate, 47-01 D3 four-condition
INSTA_FORCE_UPDATE demonstration, 47-01 D5 cargo deny/machete, 47-04 D3 missing-baseline
demonstration, and the "other"-kind plan-gate refs attached to 47-02 and 47-03 entries.
-->

## Current Test

[testing complete]

## Tests

### 1. (47-01 D1) insta wired at workspace level and in both crates; run_test hardened with env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no; *.snap.new ignored; checklist § 8 corrected with the baseline recipe
expected: insta wired at workspace level and in both crates; run_test hardened with env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no; *.snap.new ignored; checklist § 8 corrected with the baseline recipe
result: pass
source: automated
coverage_id: D1

### 2. (47-01 D2) Committed reviewed pre-fix baseline for claude_style_full_execute_fix_prompt_snapshot
expected: Committed reviewed pre-fix baseline for claude_style_full_execute_fix_prompt_snapshot (contains /gsd-execute-phase, no decision-policy heading)
result: pass
source: automated
coverage_id: D2

### 3. (47-01 D3) Guard observed failing in the INSTA_FORCE_UPDATE direction
expected: Guard observed failing in the INSTA_FORCE_UPDATE direction: un-hardened force-update 0 (defect), hardened drift 101, hardened drift+force-update 101, hardened clean 0
result: pass
source: automated
coverage_id: D3

### 4. (47-01 D4) Env-override and .snap ignore-split guards each fail under reverting mutations
expected: check_script_pins_snapshots_against_env_override and gitignore_ignores_snapshot_scratch_but_not_the_baseline, each shown to fail under reverting mutations
result: pass
source: automated
coverage_id: D4

### 5. (47-01 D5) cargo deny check and cargo machete green; cargo deny list discriminates dev vs regular dependency
expected: cargo deny check and cargo machete green on the real workspace; cargo deny list control discriminates dev vs regular dependency
result: pass
source: automated
coverage_id: D5

### 6. Snapshot-baseline recipe in the setup checklist is accurate
expected: `.planning/user/DEV-SETUP-CHECKLIST.md` § 8, the "Snapshot baselines (`insta`)" item (around lines 437-445), is enough for a contributor on another machine to work with the snapshot guard without asking anyone: cargo-insta is not required; `scripts/check.sh test` runs under `env -u INSTA_FORCE_UPDATE INSTA_UPDATE=no`, so a missing or mismatched baseline fails; the re-bless recipe is `INSTA_UPDATE=always cargo test -p <crate> <testname>` followed by reading the generated `.snap` before committing; `*.snap.new` is gitignored; and it warns never to export INSTA_UPDATE or INSTA_FORCE_UPDATE. Nothing in it is wrong or stale.
result: pass

### 7. (47-02 D1) Validate loop-back FullExecute fix prompt carries the decision policy on all six adapters
expected: A Validate loop-back dispatching Code { fix: Some(FullExecute) } carries the unattended decision policy on all six adapters; codex/pi are passing controls
result: pass
source: automated
coverage_id: D1

### 8. (47-02 D2) GapsOnly and AuditFix still omit the policy on the claude-style path
expected: GapsOnly and AuditFix still omit the policy on the claude-style path; the control was observed red under a temporary widening of the helper and green after revert
result: pass
source: automated
coverage_id: D2

### 9. (47-02 D3) Shared helper extraction changed no observable behaviour of workflow_code_prompt
expected: Shared helper extraction changed no observable behaviour of workflow_code_prompt; the workflow-style omission control is byte-identical to the fork point
result: pass
source: automated
coverage_id: D3

### 10. (47-02 D4) Snapshot baseline re-blessed adding only the CODE_STAGE_POLICY section
expected: Snapshot baseline re-blessed; its diff adds only the CODE_STAGE_POLICY section between the fix command and the completion protocol
result: pass
source: automated
coverage_id: D4

### 11. (47-03 D1) A resumed Claude session gets one gate rule
expected: A resumed Claude session gets one gate rule: the layered contradiction test failed on the pre-fix tree for both halves and passes after the fix
result: pass
source: automated
coverage_id: D1

### 12. (47-03 D2) Carve-out covers blocking-human only; package-verification prohibition pinned verbatim
expected: The carve-out covers blocking-human only; the package-verification prohibition is pinned verbatim and the pin was observed failing under a widening
result: pass
source: automated
coverage_id: D2

### 13. (47-03 D3) resume_launch_shape extracted with the audit emission unmoved
expected: resume_launch_shape extracted with the audit emission unmoved; the three relaunch_checkpoint_session tests are byte-identical to the fork point and pass
result: pass
source: automated
coverage_id: D3

### 14. The 47-03 gate-rule re-bless changed one line and grants no new authority
expected: `git show cab8d54 -- crates/devflow-core/src/snapshots/devflow_core__prompt__tests__claude_style_full_execute_fix_prompt_snapshot.snap` shows exactly one line removed and one added. The new line limits the gate exception to `blocking-human`, and nothing in that diff hands the agent any authority beyond that — package verification stays prohibited without exception.
result: pass

### 15. (47-04 D1) Six named policy-carrying core baselines committed; all seven core .snap files carry the policy heading
expected: Six named policy-carrying core baselines committed, all seven core .snap files carry the policy heading, both render styles represented
result: pass
source: automated
coverage_id: D1

### 16. (47-04 D2) The two delivered turns are separate named baselines, distinct, neither joined
expected: The two delivered turns are separate named baselines, distinct, neither joined
result: pass
source: automated
coverage_id: D2

### 17. (47-04 D3) A missing baseline fails the pinned run rather than passing or writing a file
expected: A missing baseline fails the pinned run for both new tests rather than passing or writing a file
result: pass
source: automated
coverage_id: D3

### 18. The committed prompt baselines read as the wording you intend
expected: Reading the nine tracked `.snap` files at HEAD — seven in `crates/devflow-core/src/snapshots/`, two in `crates/devflow-cli/src/snapshots/` — all seven core fix-prompt snapshots carry the decision-policy section; codex and pi use the workflow-style render and the other five the claude-style render; turn one (Code prompt) and turn two (resume prompt) are separate files and neither contains the other; and the text is what you want an unattended agent to receive. The codex, pi and turn-two texts had never been baselined before this phase.
result: pass

### 19. Unattended-mode guide describes the real parked-gate recovery
expected: `docs/guides/unattended-mode.md` no longer calls a parked gate a final refusal. It explains that approving a parked preflight gate launches the run directly via `GateAction::Advance`, skipping the adjudicated preflight check, and says what that costs. The "never auto-approved" rule is still stated, qualified by the resumed-to-resolve exception after human approval rather than inverted. The reviewers' false-positive objection is still there, reworded against this mechanism.
result: pass

### 20. Phase 49 has a usable evidence standard for DECN-03
expected: `47-PHASE49-OBSERVATION.md` defines three verdicts — Followed, Not followed, Void — explains that the resume audit event is emitted before the agent is spawned and so is not evidence of a decision, and limits the claim to Claude. In ROADMAP.md, Phase 49 still has exactly five success criteria, and criterion 4 carries exactly one pointer to that file.
result: pass

### 21. Adapter split and the two preflight defects are on record
expected: ROADMAP.md (Phase 47 entry) and REQUIREMENTS.md (DECN-02/DECN-03) name claude, opencode, hermes and antigravity as the adapters that gained the policy on loop-back, identify codex and pi as workflow-style controls, and record DECN-03 as claude-only. Backlog entries 999.125 (predicate mismatch) and 999.126 (late-plan re-scan window) each have a Fix shape and an Acceptance. No adapter-independent-resume follow-on was filed.
result: pass

### 22. Completion protocol lets decision reasoning sit above the result line
expected: In the policy-carrying snapshots (e.g. `crates/devflow-core/src/snapshots/devflow_core__prompt__tests__full_execute_fix_prompt_claude.snap`) and in ARCHITECTURE.md, the completion protocol says `DEVFLOW_RESULT` must be the LAST line and that decision reasoning goes above it. No "final message must be exactly …" wording remains in any of the nine snapshots. An agent reading it could meet the decision-reasoning mandate and the result protocol at the same time.
result: pass

### 23. Resumed agent is told not to copy the gate declaration
expected: `crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap` tells the resumed agent to describe the resolved gate in prose and not to copy the checkpoint's gate-declaration line, clearly enough that you would expect an agent to comply. Whether a live agent actually complies is Phase 49's observation, not this test.
result: pass

### 24. Worktree-guard harness passes on this host with your real git config
expected: Running `bash scripts/test-phase-worktree-guard.sh` from this worktree, with your normal global git config (hooks, SSH signing) in place, ends with `failed=0`, prints rows 7a and 7b (controls) and 7c (fixture bootstrap under hostile config -> allow), and the passed count matches the 13 checks that DEV-SETUP-CHECKLIST § 3 says the script runs.
result: pass

## Summary

total: 24
passed: 24
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

[none yet]
