---
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
plan: 03
subsystem: Code-stage prompt rendering
tags: [rust, prompts, unattended-mode, checkpoint-policy, tdd]
requires: []
provides:
  - One shared Code-stage policy delivered by Claude/OpenCode and Codex/Pi renderers
  - Rendered-prompt regression coverage for merit-based unattended decision instructions
affects: [unattended-auto-mode, code-stage, checkpoint-auditability]
tech-stack:
  added: []
  patterns:
    - Keep full-execute Code guidance in a single private prompt constant shared by both renderers.
    - Pair rendered-prompt presence checks with excluded-prompt negative controls.
key-files:
  created: []
  modified:
    - crates/devflow-core/src/prompt.rs
key-decisions:
  - "Keep the unattended decision policy static and shared by both Code renderers so agent-family prompts cannot drift."
  - "Preserve human-only handling for blocking-human gates and package-verification checkpoints."
requirements-completed: []
requirements-partial:
  - id: DECN-01
    reason: >-
      code_policy_is_identical_across_both_renderers exercises fix: None only.
      render_claude_style's Code { fix: Some(_) } arm routes to fix_prompt,
      which carries no policy in any arm (including FullExecute, a live
      production dispatch) — so a Claude/OpenCode loop-back re-run receives no
      unattended-decision policy while a Codex/Pi run in the same state does
      (CR-03, 45-REVIEW.md; confirmed independently in 45-VERIFICATION.md).
      CODE_STAGE_POLICY also textually contradicts checkpoint_auto_decide_prompt
      for a blocking-human gate in the same resumed session (CR-04). Both
      deferred by recorded operator decision to backlog 999.115 / 999.116
      rather than reopening this plan.
coverage:
  - id: D1
    description: Both full-execute Code renderers deliver the same shared policy while excluded prompts do not.
    requirement: DECN-01
    verification:
      - kind: unit
        ref: crates/devflow-core/src/prompt.rs#code_policy_is_identical_across_both_renderers
        status: pass
      - kind: unit
        ref: crates/devflow-core/src/prompt.rs#code_policy_is_absent_from_prompts_that_must_not_carry_it
        status: pass
    human_judgment: false
  - id: D2
    description: The delivered policy forbids positional choice, requires recorded reasoning, and excludes human-only checkpoints.
    requirement: DECN-01
    verification:
      - kind: unit
        ref: crates/devflow-core/src/prompt.rs#code_policy_forbids_positional_option_selection
        status: pass
      - kind: unit
        ref: crates/devflow-core/src/prompt.rs#code_policy_requires_the_reasoning_to_be_recorded
        status: pass
      - kind: unit
        ref: crates/devflow-core/src/prompt.rs#code_policy_excludes_blocking_human_and_package_checkpoints
        status: pass
    human_judgment: false
actuals:
  tokens: 2319
  tasks: 2
  commits: 4
duration: ~10min from first RED commit through final verification
completed: 2026-09-02
status: complete
---

# Phase 45 Plan 03: Unattended Decision Policy Summary

**Full-execute Code prompts now share a merit-based unattended-decision policy across all four supported agent families, with explicit human-only carve-outs and auditable-reasoning instructions.**

## Accomplishments

- Moved the existing advisory self-review text and the new unattended-decision policy into one private `CODE_STAGE_POLICY` constant.
- Routed both the Claude/OpenCode and Codex/Pi full-execute Code prompts through that one policy while leaving Validate, Ship, GapsOnly, and AuditFix excluded.
- Kept `COMPLETION_PROTOCOL` last in both Code prompt families and retained the existing no-interactive-token checks.
- Added policy-content, determinism, terminator, and excluded-prompt tests with explanatory assertions.

## TDD Evidence

- **Task 1 RED:** before the shared symbol existed, the three new renderer tests failed to compile with `E0425: cannot find value CODE_STAGE_POLICY in this scope`. This proves the specified shared source of truth was absent; it is structural RED evidence, not a runtime assertion failure.
- **Task 1 GREEN:** the three renderer/negative-control/terminator tests each reported `1 passed`; the module prompt suite reported `21 passed` at that checkpoint.
- **Task 2 RED:** `code_policy_requires_the_reasoning_to_be_recorded` failed against the first policy draft because it recorded a comparison but did not explicitly require the word `reasoning`.
- **Task 2 GREEN:** the policy now says "record the reasoning and comparison," and all five policy/pinning target tests reported `1 passed`.

## Verification

- `cargo test -p devflow-core --lib prompt::`: 25 passed.
- All named policy/pinning tests: each 1 passed, including the Validate negative control for positional-policy keywords.
- `cargo test --workspace`: `cargo_exit=0`; `^test result: FAILED` count 0. The suite was run in a bounded transient user service because the normal agent command window kills Cargo after 30 seconds.
- `cargo clippy --workspace --all-targets -- -D warnings`: `clippy_exit=0`; `^error` count 0.
- `cargo fmt --all -- --check`: exit 0 after formatting two line-wrap-only changes in the new tests.

These rendered-prompt tests establish instruction delivery and the tested prompt boundaries. They do **not** establish that an LLM obeys the instruction, that it will make correct merit judgments in every live checkpoint, or that the upstream GSD positional decision procedure has changed; that workflow remains outside this repository.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Policy wording] Explicit recorded reasoning was missing from the first policy draft.**

- **Found during:** Task 2 RED test.
- **Fix:** Changed the policy from recording only a comparison to recording "the reasoning and comparison."
- **Commit:** `237d63f`

**2. [Rule 1 - Formatting] The formatter wrapped two new renderer-test calls.**

- **Found during:** final `cargo fmt --check`.
- **Fix:** Applied `cargo fmt --all`; no logic changed.
- **Commit:** `d56cc96`

## Workspace Note

`.opencode/opencode.json` is modified outside this plan's scope by a one-line `$schema` addition. This executor did not edit, stage, or commit it; its provenance is not established here.

## Known Stubs

None.

## Self-Check: PASSED

- Confirmed `crates/devflow-core/src/prompt.rs` exists and declares exactly one `CODE_STAGE_POLICY` with two renderer interpolation sites.
- Confirmed all four task commits exist: `ac3ec45`, `13dc82f`, `d56cc96`, and `237d63f`.
