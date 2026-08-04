---
phase: 13-mvp-core-loop
plan: 02
subsystem: ship-pipeline
tags: [rust, ship, prompt, gsd-native, code-review, tdd]

# Dependency graph
requires:
  - phase: 13-mvp-core-loop
    provides: "13-01's prepare_loop_back_to_code / launch_stage split and handle_ship_failure's review: reason parsing"
provides:
  - "ship.rs stripped of dead v1 LastShip/PR-body/test-summary bookkeeping (only cron-instructions, shell_quote, prepend_changelog remain)"
  - "Ship-stage prompt that sequences /gsd-code-review before /gsd-ship and mandates NOT shipping on Critical REVIEW.md findings"
  - "The review: ReviewFailed reason-prefix contract, defined on the prompt side (consumed by 13-01's handle_ship_failure)"
affects: [13-mvp-core-loop plan 06 (pre-flight dogfood checkpoint must observe real /gsd-ship headless behavior on the clean-review path)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Stage-specific prompt branch (ship_stage_prompt) instead of the generic single-command template, for stages needing multi-step / conditional agent instructions"
    - "reason string convention (review: prefix, trim + case-fold) instead of a new AgentStatus enum variant, to signal ReviewFailed without a serde-format break"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/ship.rs
    - crates/devflow-core/src/prompt.rs

key-decisions:
  - "Deleted LastShip, save/load/delete, build_pr_body, extract_goal, extract_section, changed_files, test_summary, count_passed_tests, mark_phase_complete — zero live callers, all fed the removed devflow confirm/rejectpr commands"
  - "Kept ShipError::Missing (still returned by load_cron_instructions), CronInstructions/ResumeCommand/HermesCronJob, cron_instructions_path, write/load/delete_cron_instructions, build_cron_instructions, cron_schedule_from_retry_after + date-math helpers, shell_quote, prepend_changelog"
  - "Ship prompt sidesteps /gsd-ship's interactive optional_review AskUserQuestion entirely (rather than assuming it's skipped) by mandating no-ship-on-Critical — closes RESEARCH Pitfall 2 for the headless dogfood run"

patterns-established:
  - "Ship-stage prompt as a dedicated function (ship_stage_prompt) branched on Stage::Ship inside stage_prompt, keeping other stages on the generic template"

requirements-completed: [13a]

coverage:
  - id: D1
    description: "Dead v1 LastShip/PR-body/test-summary bookkeeping removed from ship.rs without breaking any live caller (cron-instructions, prepend_changelog, shell_quote and their callers in hooks.rs/main.rs/phase7_cli.rs)"
    requirement: "13a"
    verification:
      - kind: unit
        ref: "cargo test -p devflow-core ship:: (11 tests)"
        status: pass
      - kind: integration
        ref: "cargo build -p devflow && cargo test -p devflow --test phase7_cli (6 tests)"
        status: pass
      - kind: other
        ref: "cargo clippy -p devflow-core -- -D warnings"
        status: pass
    human_judgment: false
  - id: D2
    description: "Ship-stage prompt sequences /gsd-code-review {N} before /gsd-ship {N}, defines the Critical-severity gate, mandates no-ship-on-Critical, and emits the review:-prefixed ReviewFailed contract"
    requirement: "13a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#ship_prompt_sequences_code_review_before_ship"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#ship_prompt_defines_critical_gate_and_review_failed_contract"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/prompt.rs#non_ship_stage_prompts_are_unchanged_single_command_template"
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-07-14
status: complete
---

# Phase 13 Plan 02: Ship.rs Dead-Code Deletion + Headless-Safe Ship Prompt Summary

**Deleted the dead v1 LastShip/PR-body/test-summary bookkeeping from `ship.rs` and gave the Ship-stage prompt a dedicated branch that sequences `/gsd-code-review {N}` before `/gsd-ship {N}`, mandating the agent never run `/gsd-ship` when `REVIEW.md` reports a Critical finding.**

## Performance

- **Duration:** 10 min
- **Started:** 2026-07-14T20:25:00Z
- **Completed:** 2026-07-14T20:28:20Z
- **Tasks:** 2 completed
- **Files modified:** 2

## Accomplishments
- `ship.rs` shrunk from 789 to 446 lines by removing `LastShip` and all PR-body/goal-extraction/test-summary machinery written for the deleted `devflow confirm`/`devflow rejectpr` commands, while every live caller (cron-instructions in `main.rs`, `prepend_changelog` in `hooks.rs`) keeps compiling
- `ShipError::Missing` explicitly preserved (still returned by `load_cron_instructions`)
- Ship-stage prompt now has its own branch (`ship_stage_prompt`) that instructs the agent to run code review first, defines the Critical-severity gate, and mandates a `review:`-prefixed `DEVFLOW_RESULT` failure with no `/gsd-ship` invocation when Critical findings exist — closing RESEARCH Pitfall 2 (undefined behavior on `/gsd-ship`'s interactive `AskUserQuestion` step under headless `--dangerously-skip-permissions`)
- Full TDD cycle for the prompt change: RED (2 failing tests) → GREEN (implementation) → `cargo fmt` cleanup (no logic change)

## Task Commits

Each task was committed atomically:

1. **Task 1: Delete dead v1 bookkeeping from ship.rs** - `a008b48` (refactor)
2. **Task 2: Sequence code-review before ship in the Ship-stage prompt** - `c68522d` (test, RED) → `258bf9f` (feat, GREEN)

**Plan metadata:** (this commit, docs: complete plan)

_Note: Task 2 was TDD — test commit precedes feat commit._

## Files Created/Modified
- `crates/devflow-core/src/ship.rs` - Removed `LastShip`/PR-body/test-summary dead code and its tests; kept cron-instructions machinery, `shell_quote`, `prepend_changelog`, and their tests
- `crates/devflow-core/src/prompt.rs` - Added `ship_stage_prompt`, branched `stage_prompt` on `Stage::Ship`, added 3 new tests + kept all 6 existing tests passing

## Decisions Made
- Followed the plan's exact keep-list for `ship.rs` — no additional deletions or extra hardening beyond what was specified
- Made "do NOT run `/gsd-ship` on Critical findings" a hard MUST in the prompt text (not merely "run review first"), per the plan's explicit instruction that this is what actually sidesteps the interactive `optional_review` step rather than assuming it's skipped
- Ran `cargo fmt` after the GREEN commit to reformat a multi-line `||` assertion chain in the new test; purely cosmetic, no functional change, included as fmt-clean before commit

## Deviations from Plan

None - plan executed exactly as written. The two acceptance-criteria greps for surviving symbol counts (`fn prepend_changelog|fn shell_quote|fn build_cron_instructions|fn cron_schedule_from_retry_after`) matched 8 lines instead of the plan's stated "4" because the pattern also substring-matches test function names (e.g. `shell_quote_leaves_common_safe_chars_unquoted`); this is a looseness in the plan's grep pattern, not a functional gap — `rg -n` confirms all 4 named functions are defined exactly once and all listed survivor tests pass.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `ship.rs` is now leaner and headless-safe groundwork is in place for the Ship stage
- Plan 06's pre-flight dogfood checkpoint must still observe `/gsd-ship`'s real headless behavior on a clean-review path before the live dogfood run (this plan sidesteps the interactive step on the Critical-findings path but does not itself execute `/gsd-ship` to prove the clean path is hang-free)
- No blockers for subsequent 13-mvp-core-loop plans

---
*Phase: 13-mvp-core-loop*
*Completed: 2026-07-14*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/ship.rs
- FOUND: crates/devflow-core/src/prompt.rs
- FOUND: .planning/phases/13-mvp-core-loop/13-02-SUMMARY.md
- FOUND: commit a008b48 (Task 1)
- FOUND: commit c68522d (Task 2 RED)
- FOUND: commit 258bf9f (Task 2 GREEN)
