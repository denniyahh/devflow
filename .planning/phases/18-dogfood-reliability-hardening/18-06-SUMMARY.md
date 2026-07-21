---
phase: 18-dogfood-reliability-hardening
plan: 06
subsystem: infra
tags: [rust, git-worktree, staleness, provenance, dogfood]

# Dependency graph
requires:
  - phase: 18-dogfood-reliability-hardening (18-05)
    provides: 417/417 green baseline; consecutive_failures/Layer-0 fixes landed, main.rs stable to edit
provides:
  - "enforce_build_staleness evaluates ancestry/dirty-tree checks against execution_root (the phase's worktree when one is set, else project_root), closing Round 4 CR-01"
  - "A self-dogfood binary behind its worktree's HEAD is a hard BLOCK, not a warn"
  - "Block error message names the tree actually evaluated and states whether a worktree was in play"
  - "Real git-worktree test fixture (worktree_staleness_fixture) reusable by future staleness tests"
affects: [18-07, future worktree-aware staleness or provenance work]

tech-stack:
  added: []
  patterns:
    - "execution_root = state.worktree_path.as_deref().unwrap_or(project_root) — same idiom evaluate_layer0 uses in agent_result.rs, now also used by enforce_build_staleness"
    - "Sibling (not nested) tempdir fixture layout for worktree tests, so path-containment assertions on block messages are unambiguous"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "is_self_dogfood_workspace and events::emit deliberately kept project_root-scoped (Assumption A3, 18-RESEARCH.md Pitfall 4) — documented in the enforce_build_staleness doc comment, not left implicit"
  - "Verification used cargo test -p devflow <name> (no --lib) per the 18-01 precedent — devflow is binary-only, --lib hard-errors; the plan's own verify blocks still say --lib and should be corrected in future 18-0N plans"
  - "cargo test -p devflow --lib staleness pass-count acceptance criterion (>=11) does not literally hold under a bare substring filter — only 7 of the 12 staleness-related tests contain the literal substring 'staleness' in their name (embedded_commit_is_stale_uses_worktree_head and 3 other pre-existing tests don't); verified substantively instead via full cargo test --workspace (0 failed) plus individually confirming every staleness/worktree test by name"

requirements-completed: [18c]

coverage:
  - id: D1
    description: "enforce_build_staleness evaluates ancestry against the worktree HEAD when state.worktree_path is set, not project_root's HEAD"
    requirement: "18c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::embedded_commit_is_stale_uses_worktree_head"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::enforce_build_staleness_blocks_self_dogfood_behind_worktree_head"
        status: pass
    human_judgment: false
  - id: D2
    description: "A self-dogfood binary behind its worktree's HEAD is BLOCKed (hard error), not warned — closing the Round 4 CR-01 false-green"
    requirement: "18c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::enforce_build_staleness_blocks_self_dogfood_behind_worktree_head"
        status: pass
    human_judgment: false
  - id: D3
    description: "A phase with no worktree is unaffected: execution_root falls back to project_root and every pre-existing staleness test passes unchanged"
    requirement: "18c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::staleness_without_worktree_is_unchanged"
        status: pass
      - kind: unit
        ref: "cargo test --workspace (420/420, 0 failed — includes all 9 pre-existing staleness-related tests unmodified)"
        status: pass
    human_judgment: false
  - id: D4
    description: "is_self_dogfood_workspace remains project_root-anchored, with the reasoning documented in source rather than left implicit"
    requirement: "18c"
    verification:
      - kind: unit
        ref: "static: crates/devflow-cli/src/main.rs enforce_build_staleness doc comment (Assumption A3) + is_self_dogfood_workspace(project_root) call site unchanged"
        status: pass
    human_judgment: false
  - id: D5
    description: "Block message names execution_root and states whether a worktree was in play (T-18-23); message is not persisted into events.jsonl beyond the existing truncate_reason path (T-18-25/WR-02 class)"
    requirement: "18c"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::enforce_build_staleness_blocks_self_dogfood_behind_worktree_head (asserts worktree path present in message)"
        status: pass
    human_judgment: true
    rationale: "The 'not persisted beyond truncate_reason' half of this claim is a code-inspection fact (events::emit's json! payload is structurally unchanged, still routes the message through truncate_reason), not something a unit test asserts directly — flagged for a human/verifier to confirm the events.jsonl payload shape is unchanged."

duration: 21min
completed: 2026-07-21
status: complete
---

# Phase 18 Plan 06: Worktree-Aware Build Staleness Enforcement (18c) Summary

**`enforce_build_staleness` now evaluates ancestry and dirty-tree checks against the phase's worktree HEAD (not `project_root`'s), hard-blocking a self-dogfood binary that's behind the worktree branch instead of misclassifying it `Ahead`/warn — closing the Round 4 CR-01 root cause.**

## Performance

- **Duration:** 21 min
- **Started:** 2026-07-21T04:38:00Z
- **Completed:** 2026-07-21T04:57:02Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments

- Built a real `git worktree add` test fixture (`worktree_staleness_fixture`) proving the exact Round 4 CR-01 asymmetry: the same embedded commit reads `Fresh` against `project_root` and `Stale` against the worktree HEAD.
- Threaded `execution_root` through `embedded_commit_is_stale`, `tree_has_modified_build_inputs`, and `combined_staleness` — a parameter rename plus call-site change, with the ancestry exit-code contract and reverse-probe logic left untouched.
- `enforce_build_staleness` now derives `execution_root = state.worktree_path.as_deref().unwrap_or(project_root)` (the same idiom `evaluate_layer0` already uses in `agent_result.rs`) and passes it to `combined_staleness`, while keeping `is_self_dogfood_workspace` and `events::emit` on `project_root` — documented in source as Assumption A3.
- Block error message now names `execution_root` and adds a clause stating whether a worktree was in play, so an operator knows which HEAD to rebuild against.
- RED-then-GREEN proven live: manually reverted the `execution_root` derivation to `project_root` and confirmed `enforce_build_staleness_blocks_self_dogfood_behind_worktree_head` fails (`Ok` instead of the expected `Err`) — then restored the fix and confirmed it passes.

## Task Commits

1. **Task 1: Build a real git-worktree staleness fixture and RED-prove the wrong-tree evaluation** - `a80079f` (test)
2. **Task 2: Thread execution_root through the ancestry and dirty-tree checks** - `10730ea` (fix)

**Plan metadata:** (this commit)

_Note: Task 1 is intentionally green pre-fix (it parameterizes both calls by a root, proving the fixture is correct, not that the defect is fixed); Task 2 carries the RED-then-GREEN proof of the actual `enforce_build_staleness` entry point._

## Files Created/Modified

- `crates/devflow-cli/src/main.rs` - `worktree_staleness_fixture` test helper; 3 new tests (`embedded_commit_is_stale_uses_worktree_head`, `enforce_build_staleness_blocks_self_dogfood_behind_worktree_head`, `staleness_without_worktree_is_unchanged`); `execution_root`-threaded `embedded_commit_is_stale`/`tree_has_modified_build_inputs`/`combined_staleness`; `enforce_build_staleness` derives and uses `execution_root`, updated block message, updated doc comments

## Decisions Made

- **`is_self_dogfood_workspace` stays `project_root`-scoped** (Assumption A3, per 18-RESEARCH.md Pitfall 4, adopted as-is from the plan): it answers "is this workspace DevFlow's own repo at all," not "is the binary stale relative to tree X." Documented directly above `enforce_build_staleness` in source, including the residual risk (a PLAN modifying the root `Cargo.toml`'s `members` array on the feature branch mid-flight could make the two roots disagree).
- **Worktree test fixture uses sibling directories, not nested ones.** `worktree_staleness_fixture` places `project_root` and the worktree as siblings under one outer tempdir (`<tempdir>/project`, `<tempdir>/worktree`) rather than nesting the worktree under `project_root` (e.g. `project_root/.worktrees/phase-NN`, matching production layout). A nested worktree path would contain `project_root`'s path as a string prefix, making "message contains worktree path" and "message does not contain project_root path" mutually exclusive assertions — the plan's own Task 2 action explicitly requires asserting both. This is a test-fixture-only deviation from production's actual `.worktrees/` nesting; it does not affect the production code path, which only ever receives whatever path `state.worktree_path` already holds.
- **`cargo test -p devflow --lib <name>` verification commands corrected to `cargo test -p devflow <name>`** (no `--lib`) per the 18-01 precedent recorded in STATE.md — `devflow` (the `devflow-cli` package) has no `[lib]` target, so `--lib` hard-errors (exit 101) rather than filtering. The plan's own `<verify>` blocks and 18-RESEARCH.md still specify `--lib`; flagging again for a future pass to correct at the source, as 18-01 already flagged once.
- **`combined_staleness`'s single-line signature wraps to multi-line under `cargo fmt`** once `project_root` (11 chars) became `execution_root` (14 chars) — the line exceeds rustfmt's 100-column width. This means the plan's own acceptance-criteria regex `rg -n 'fn combined_staleness' ... | rg -c 'execution_root'` returns 0, not 1, because the parameter name is no longer on the same line as `fn combined_staleness(`. Verified the underlying rename is correct via `rg -n -A3 'fn combined_staleness'` instead; `cargo fmt --check` passes, so the wrapped form is the canonical formatting, not a style violation.

## Deviations from Plan

### Auto-fixed Issues

None — no bugs, missing functionality, or blocking issues were found beyond the plan's own scope. The three items above are documentation/verification-script corrections (Rule-adjacent but not code auto-fixes), recorded as decisions rather than deviations since they don't change behavior.

---

**Total deviations:** 0 code-level auto-fixes. 3 verification-script/documentation corrections recorded under Decisions Made (mirrors the 18-01 `--lib` precedent).
**Impact on plan:** None on behavior — `combined_staleness`'s rename and the `--lib`/pass-count acceptance-criteria mismatches are artifacts of the plan's verification scripts not accounting for rustfmt's line-width wrapping and this crate's binary-only nature; the actual code changes match the plan's action text and must_haves exactly.

## Issues Encountered

None beyond the acceptance-criteria/verification-script mismatches documented above under Decisions Made.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 18c (Requirement) is closed: worktree-aware staleness enforcement is live, RED-then-GREEN proven, and the one deliberately-unchanged check (`is_self_dogfood_workspace`) is documented in source.
- `cargo test --workspace`: 420/420 passed, 0 failed (up from 417 at 18-05's close — 3 new tests). `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both clean.
- Ready for 18-07 (final plan of Phase 18, wave 6: 18f preflight-gate re-run wedge fix).

---
*Phase: 18-dogfood-reliability-hardening*
*Completed: 2026-07-21*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/main.rs
- FOUND: .planning/phases/18-dogfood-reliability-hardening/18-06-SUMMARY.md
- FOUND commit: a80079f (test(18-06): add real git-worktree staleness fixture)
- FOUND commit: 10730ea (fix(18-06): evaluate staleness against worktree HEAD)
- FOUND commit: 9b5e011 (docs(18-06): add plan summary)
