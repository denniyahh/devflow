---
phase: 17-pipeline-dogfood-followup
plan: 03
subsystem: devflow-core-completion-cascade
tags: [rust, agent-result, external-verify, layer0, layer3, fail-closed]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: "17-01's decided_by_layer field and typed-outcome taxonomy (ResourceKilled/AgentUnavailable)"
  - phase: 16-pipeline-reliability-hardening
    provides: "16-01's Layer 0 external post-condition probe machinery (TRUST_EXTERNAL_VERIFY_ENV, external_verify_commands, run_external_verification)"
provides:
  - "evaluate_layer3 fail-closed split: zero-commit/no-declaration is Failed (human review flag); commits-present stays Unknown (gated downstream)"
  - "evaluate_layer0 evaluates every stage (not just Code) and returns affirmative Success when all approved declared probes pass, even with zero commits"
  - "evaluate_layer0's two-root split: PLAN discovery from project_root, probe execution from execution_root (worktree-safe)"
affects: [17-04-typed-outcome-taxonomy-integration, 17-05]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Layer 0 affirmative-Success branch: an approved all-passing external post-condition is completion evidence in its own right, ranked above self-reported agent status"
    - "Discovery root vs execution root split for worktree-safe PLAN.md parsing"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/agent_result.rs

key-decisions:
  - "evaluate_layer3's zero-commit sub-case now returns AgentStatus::Failed (reused variant, not a new one) instead of the prior blanket Unknown, per D-02/D-03 case 3"
  - "evaluate_layer0's early-return guard now only checks external_verify_enabled — the Stage::Code restriction is removed so Layer 0 runs on Define/Plan/Code/Validate/Ship alike"
  - "external_verify_commands (PLAN discovery) is called with project_root; run_external_verification (probe execution) keeps execution_root — previously both used execution_root, which silently returned zero commands (and mis-fired the PLAN-removed veto) for worktree-based phases"
  - "The all-probes-pass path now returns Some(AgentStatus::Success) instead of falling through to None, so an external-only stage with zero commits can complete cleanly without relying on Layer 1/2 to independently confirm success"

requirements-completed: [17a]

coverage:
  - id: D1
    description: "evaluate_layer3 zero-commit/no-declaration case reclassified from Unknown to Failed, flagging human review; commits-present case unchanged (still Unknown, gated downstream)"
    requirement: "17a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#evaluate_layer3_zero_commits_is_failed_and_flags_human_review"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#evaluate_layer3_falls_back_to_commit_count"
        status: pass
    human_judgment: false
  - id: D2
    description: "evaluate_layer0 evaluates for every stage (not only Code) and returns affirmative Success with decided_by_layer(0) when all approved declared probes pass, even on a non-Code stage with zero commits"
    requirement: "17a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#layer0_affirmative_success_on_non_code_stage_with_zero_commits"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree"
        status: pass
    human_judgment: false
  - id: D3
    description: "PLAN discovery reads project_root while probe execution reads execution_root (the worktree), preventing a worktree-based phase from mis-firing the PLAN-removed veto"
    requirement: "17a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree"
        status: pass
    human_judgment: false
  - id: D4
    description: "Layer 0 affirmative Success short-circuits the cascade before Layer 1, outranking a self-reported agent failure marker"
    requirement: "17a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#layer0_affirmative_success_outranks_layer1_failure_marker"
        status: pass
    human_judgment: false
  - id: D5
    description: "Multiple declared probes: all must pass for Success; the first failing probe vetoes regardless of declaration order"
    requirement: "17a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#multiple_declared_probes_first_failure_vetoes_regardless_of_order"
        status: pass
    human_judgment: false
  - id: D6
    description: "TRUST_EXTERNAL_VERIFY_ENV approval-mismatch behavior (not-approved, PLAN-removed, commands-changed vetoes) is byte-for-byte unchanged after the stage-scope lift"
    requirement: "17a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#changed_external_probe_never_inherits_prior_approval"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#removed_external_probe_fails_closed_against_prior_approval"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#failing_external_probe_outranks_success_marker"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-07-18
status: complete
---

# Phase 17 Plan 03: Fail-Closed Completion Cascade Rework Summary

**Split evaluate_layer3's blanket Unknown into a fail-closed Failed/Unknown pair and extended evaluate_layer0 to vouch for a passing external post-condition on every stage, not just Code.**

## Performance

- **Duration:** 5 min
- **Started:** 2026-07-18T19:51:33-04:00
- **Completed:** 2026-07-18T19:56:18-04:00
- **Tasks:** 2 completed
- **Files modified:** 1

## Accomplishments
- `evaluate_layer3`'s zero-commit "process gone, nothing accounted for" case now returns `AgentStatus::Failed` (with a reason flagging human review) instead of a blanket advanceable `Unknown`; the commits-present case is unchanged and stays `Unknown` for Plan 04's never-advance dispatch to gate.
- `evaluate_layer0`'s early-return guard no longer restricts to `Stage::Code` — it now evaluates for Define, Plan, Code, Validate, and Ship alike (still gated by the `external_verify_enabled` feature toggle).
- `evaluate_layer0` now returns `Some(AgentStatus::Success)` (with `decided_by_layer: Some(0)`) when every declared, approved probe passes — previously this case fell through to `None` and relied on Layer 1/2 to independently confirm success, which meant a legitimately external-only stage with zero commits could not complete cleanly.
- Split the two roots `evaluate_layer0` previously conflated: PLAN declaration discovery now reads `project_root` (`.planning/phases/` lives there, not in a worktree checkout); probe execution still reads `execution_root` (the worktree, when set). This closes a pre-existing bug where a worktree-based phase's PLAN declarations were invisible to discovery and silently mis-fired the "PLAN removed" veto.
- All four pre-existing approval-mismatch veto branches (not-approved, PLAN-removed, commands-changed, failing-probe) are byte-for-byte unchanged.

## Task Commits

Each task followed the TDD RED → GREEN cycle:

1. **Task 1: Split Layer 3 into a fail-closed typed outcome (D-02/D-03 case 3)**
   - `d7e452a` (test) — failing test for the zero-commit → Failed reclassification
   - `82b69fe` (feat) — `evaluate_layer3` status split implemented
2. **Task 2: Layer 0 runs every stage and vouches for a passing declared probe (D-05/D-06)**
   - `41f3f96` (test) — failing tests for every-stage evaluation, the two-root split, and the cascade-level short-circuit
   - `305e267` (feat) — `evaluate_layer0` reworked: stage-scope lift, project_root/execution_root split, affirmative Success branch
   - `70c0cee` (test) — supplementary coverage for the multi-probe ordering edge (17a), confirming the existing `.find()`-based implementation already satisfies it

**Plan metadata:** (this commit) `docs(17-03): complete completion-cascade-fail-closed-split plan`

## Files Created/Modified
- `crates/devflow-core/src/agent_result.rs` — `evaluate_layer3` status split; `evaluate_layer0` stage-scope lift, root split, affirmative-Success branch; 7 new/rewritten unit tests

## Decisions Made
- Reused `AgentStatus::Failed`/`AgentStatus::Unknown` for the Layer 3 split rather than adding a new variant, per the plan's explicit prohibition (D-02 realized structurally via status, not a new variant).
- Kept the multi-probe ordering test as a standalone supplementary commit rather than folding it into Task 2's GREEN commit, since the underlying `Iterator::find`-based implementation already satisfied the must-have without further code changes — the test documents/locks in that guarantee rather than driving new production code.
- Rewrote the pre-existing `external_probe_runs_only_after_code_and_reads_execution_worktree` test (which encoded the old, incorrect worktree-as-discovery-root assumption) into `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree`, matching the plan's explicit review disposition (Plan 03 MEDIUM, OpenCode) rather than leaving a test that would otherwise assert stale behavior.

## Deviations from Plan

None - plan executed exactly as written. The only test-file change beyond what Task 2's action described was updating the one pre-existing test whose fixture setup embodied the bug being fixed (discovery root under the worktree) — this is exactly what the plan's own "Review dispositions" section calls out as RESOLVED for Plan 03 MEDIUM, not an unplanned deviation.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `evaluate_layer3` and `evaluate_layer0` are ready for Plan 04 to wire the never-advance dispatch against the newly-split `Failed`/`Unknown` outcomes and the new Layer 0 `Success` decision.
- Full `devflow-core` test suite (273 tests) and workspace-wide `cargo test`/`clippy -D warnings`/`fmt --check` all green.

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-18*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/agent_result.rs
- FOUND: .planning/phases/17-pipeline-dogfood-followup/17-03-SUMMARY.md
- FOUND commit: d7e452a
- FOUND commit: 82b69fe
- FOUND commit: 41f3f96
- FOUND commit: 305e267
- FOUND commit: 70c0cee
