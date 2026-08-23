---
phase: 13-mvp-core-loop
plan: 04
subsystem: cli
tags: [rust, clap, cli, worktree, git-worktree, unattended-safety]

# Dependency graph
requires:
  - phase: 13-mvp-core-loop
    provides: "13-01's stage-launch split (prepare_loop_back_to_code / launch_stage) that this plan's start() call site builds on"
provides:
  - "`devflow start` defaults to an isolated git worktree at `.worktrees/phase-NN/` unless `--no-worktree` is passed"
  - "Hidden, deprecated `--worktree` no-op alias preserved for one release for backward compatibility"
  - "Regression tests asserting `state.worktree_path` is `Some(_)` by default and `None` with `--no-worktree`"
affects: [15-oss-readiness]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Deprecated CLI flags retained as hidden (`#[arg(long, hide = true)]`) no-op aliases for one release instead of breaking removal"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/phase7_cli.rs

key-decisions:
  - "Retained --worktree as a hidden deprecated no-op alias rather than removing it, per cross-AI review consensus (#6) — avoids breaking existing scripts/docs for one release"
  - "Computed effective flag as `!no_worktree` in the Start match arm, leaving start()'s internal worktree: bool parameter and parallel()/sequentagent() call sites completely unchanged"

requirements-completed: [13d]

coverage:
  - id: D1
    description: "devflow start with no worktree flag creates an isolated git worktree by default, with state.worktree_path: Some(_)"
    requirement: "13d"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#start_defaults_to_worktree"
        status: pass
    human_judgment: false
  - id: D2
    description: "--no-worktree opts out, running on a feature branch in the primary checkout, with state.worktree_path: None"
    requirement: "13d"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#start_no_worktree_uses_feature_branch"
        status: pass
    human_judgment: false
  - id: D3
    description: "Hidden --worktree flag survives as a deprecated no-op alias; parallel/sequentagent unaffected by the default flip"
    requirement: "13d"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#reference_and_cleanup_worktree_cli_flow"
        status: pass
      - kind: unit
        ref: "cargo clippy -p devflow -- -D warnings (source assertion: start(project_root, phase, agent, mode, force, true, false) unchanged in parallel())"
        status: pass
    human_judgment: false

# Metrics
duration: 7min
completed: 2026-07-14
status: complete
---

# Phase 13 Plan 04: Worktree-by-Default Summary

**Flipped `devflow start`'s default from opt-in (`--worktree`) to opt-out (`--no-worktree`), with the old flag retained as a hidden deprecated no-op so existing scripts still parse.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-07-14T20:46:14Z
- **Completed:** 2026-07-14T20:53:08Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- `Command::Start` gained a `no_worktree: bool` opt-out flag; the effective worktree flag is computed as `!no_worktree` in the match arm, so `start()`'s internal signature and the `parallel()`/`sequentagent()` call sites are untouched
- The old `worktree: bool` field is retained with `#[arg(long, hide = true)]` and a deprecation doc comment — it's parsed but intentionally ignored, so `devflow start --worktree` from existing scripts/docs still works (and still yields a worktree, since worktree is now the default anyway)
- Two new integration tests assert the behavior end-to-end via the real `state.json`: default run → `worktree_path: Some(_)` and `.worktrees/phase-NN/` created; `--no-worktree` → `worktree_path: None` and no worktree directory
- Repo-wide `--worktree` audit (`rg -n -- '--worktree' crates CHANGELOG.md ARCHITECTURE.md`) confirmed every remaining hit still parses under the hidden alias — no hard errors; doc rewrite deferred to Phase 15 as planned

## Task Commits

Each task was committed atomically:

1. **Task 1: Invert the Start worktree default to opt-out** - `99daed3` (feat)
2. **Task 2: Integration tests for worktree-by-default and opt-out** - `2682157` (test)

**Plan metadata:** (pending — final docs commit below)

## Files Created/Modified
- `crates/devflow-cli/src/main.rs` - Added `no_worktree` opt-out flag, hid the deprecated `worktree` flag, inverted the default computation in the `Start` match arm, updated `--help` text
- `crates/devflow-cli/tests/phase7_cli.rs` - Added `start_defaults_to_worktree` and `start_no_worktree_uses_feature_branch`, both asserting on `state.worktree_path` via `devflow_core::workflow::load_state`

## Decisions Made
- Retained `--worktree` as a hidden no-op rather than removing it outright (cross-AI review consensus #6) — a full flag removal would break scripts/docs referencing it before the Phase 15 documentation rewrite lands
- Used the existing `wait_for()` polling helper for worktree/pid-file assertions rather than adding new synchronization, consistent with the rest of the integration suite (start returns before its detached monitor finishes)

## Deviations from Plan

None - plan executed exactly as written. Both tasks' acceptance criteria were verified directly:
- `no_worktree` present, `hide = true` on the deprecated `worktree` arg, `!no_worktree` computed at the call site, `parallel()` still calls `start(project_root, phase, agent, mode, force, true, false)` unchanged (all confirmed via `rg`)
- `cargo run -p devflow -- start --help` output contains `--no-worktree` and omits `--worktree` (hidden)
- `cargo test -p devflow --test phase7_cli` (full file, 8 tests) and `cargo test --workspace` (18 + 8 + 160 + 2 = 188 tests) all pass
- `cargo clippy --workspace -- -D warnings` and `cargo fmt --check` both exit 0 (one `cargo fmt` auto-format pass applied to the new test code before the final check)

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- 13d (unattended-safety worktree default) is now fully implemented and tested; no known blockers for subsequent Phase 13 plans that assume worktree-isolated `start` behavior
- Phase 15's planned README/ARCHITECTURE.md rewrite should update the remaining `--worktree` documentation references (ARCHITECTURE.md:98, CHANGELOG.md:37) to describe the new default and the deprecated alias

---
*Phase: 13-mvp-core-loop*
*Completed: 2026-07-14*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/main.rs
- FOUND: crates/devflow-cli/tests/phase7_cli.rs
- FOUND: .planning/phases/13-mvp-core-loop/13-04-SUMMARY.md
- FOUND: 99daed3 (feat(13-04) commit)
- FOUND: 2682157 (test(13-04) commit)
