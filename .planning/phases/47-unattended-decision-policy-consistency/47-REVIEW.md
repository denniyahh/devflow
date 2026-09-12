---
phase: 47-unattended-decision-policy-consistency
reviewed: 2026-09-11T21:13:27Z
resolved: 2026-09-12T14:20:38Z
depth: deep
files_reviewed: 24
files_reviewed_list:
  - .gitignore
  - Cargo.lock
  - Cargo.toml
  - crates/devflow-cli/Cargo.toml
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_one_code_prompt.snap
  - crates/devflow-cli/src/snapshots/devflow__pipeline_launch__tests__turn_two_resume_prompt.snap
  - crates/devflow-cli/tests/ci_parity_guards.rs
  - crates/devflow-cli/tests/gitignore_coverage.rs
  - crates/devflow-core/Cargo.toml
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
  - scripts/hooks/pre-commit
  - scripts/lint-phase-worktree.sh
  - scripts/phase-worktree.sh
  - scripts/test-phase-worktree-guard.sh
findings:
  critical: 1
  warning: 1
  info: 0
  total: 2
status: resolved
---

# Phase 47: Code Review Report

**Reviewed:** 2026-09-11T21:13:27Z
**Depth:** deep
**Files Reviewed:** 24
**Status:** issues_found

## Summary

The shared policy reaches the intended FullExecute render paths, and the resume builder is connected to its production caller. However, the delivered prompts contain mutually impossible final-message requirements, so the requested decision audit trail cannot be produced reliably. The new end-to-end shell guard is also not hermetic: a developer's global Git hook can stop it before any positive or negative control runs.

The review traced the prompt renderers through `pipeline_launch` into the Claude resume argv and inspected the snapshot/CI/script guards. Targeted prompt, delivery, and guard tests passed, and `bash -n` passed for all changed shell scripts. Those static and constructor-level checks do **not** establish real model instruction following, a resumed Claude transcript, or an actual unattended run; Phase 49's live observation remains necessary for that behavior.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01: Decision-audit instruction contradicts the required completion message

**Classification:** BLOCKER

**File:** `crates/devflow-core/src/prompt.rs:46-55`, `crates/devflow-core/src/prompt.rs:116-120`, `crates/devflow-core/src/prompt.rs:605-615`

**Issue:** `COMPLETION_PROTOCOL` requires the final message to be exactly a `DEVFLOW_RESULT` line and to contain nothing after it. Both the FullExecute policy and the resume instruction require the agent to put a substantive decision comparison/reasoning in that same final message. No final message can satisfy both contracts. An agent that obeys the completion protocol leaves no audit record; one that supplies the required reasoning violates the machine-readable completion contract and risks the pipeline treating the stage as failed. The string/snapshot tests only assert each clause exists, so they presently encode rather than detect this contradiction.

**Fix:** Define one compatible final-message envelope and test it end-to-end at the prompt-parser boundary. For example, permit a `DEVFLOW_DECISION` record before a final, unchanged `DEVFLOW_RESULT` line, and change both instructions to require that record before the result. Alternatively extend the result JSON with a structured decision field only if the result parser accepts it. Update the completion-contract tests and snapshots to assert that the reasoning record and parseable terminal result coexist.

## Warnings

### WR-01: Worktree-guard test inherits and executes the operator's global Git hooks

**Classification:** WARNING

**File:** `scripts/test-phase-worktree-guard.sh:49-55`

**Issue:** The supposed hermetic scratch repository is initialized and committed without disabling inherited `core.hooksPath`. On this review host, the bootstrap `git commit` invoked the global gitleaks/SSH hook and failed while loading the operator's passphrase-protected key, before any of the guard's six cases executed. Thus the test is environment-dependent and cannot reliably validate the new pre-commit guard; a failing external hook can be mistaken for a guard failure, while its controls never run.

**Fix:** Disable hooks for the fixture bootstrap before its first commit, for example set a fixture-local empty hooks directory with `git config core.hooksPath "$TMP/no-hooks"` after `git init`, or use `git -c core.hooksPath=/dev/null commit -qm init`. Keep the guard under test invoked directly, as it is now, so this isolation does not bypass the behavior being tested.

---

## Finding Resolutions — 2026-09-12

- **CR-01 closed by 47-06.** The completion protocol now requires the result as the last line,
  permits decision reasoning above it, and has a production parser test with a long-trailing-text
  negative control. The resumed prompt is separately pinned against the gate detector.
- **WR-01 closed by 47-07.** The fixture clears both injected-config carriers and applies
  repository-local hooks/signing isolation. Its post-commit gate passed under every hostile state;
  removing only the two isolation lines fails the permanent 7c control.

The initial review remains above as the historical record of findings. These resolutions establish
the scoped code and harness behavior, not a live agent's instruction following; Phase 49 retains
that behavioral observation.

_Reviewed: 2026-09-11T21:13:27Z; resolved: 2026-09-12T14:20:38Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: deep_
