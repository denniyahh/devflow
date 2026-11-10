---
phase: 52-subtract-and-stabilize
plan: 07
subsystem: cli
tags: [cut, parallel, worktree, rust, clap]

requires:
  - phase: 52-06
    provides: devflow release removed (previous plan in the cut order)
provides:
  - "crates/devflow-cli/src/phase_worktree.rs holding ensure_phase_worktree and its fork-base test (start's worktree fork)"
  - "retry_after_from_reason and its test in pipeline_outcomes.rs, its only kept caller (52-09 and 52-31 import it from there)"
  - "`devflow parallel` removed from the CLI, its tests, help snapshot and scoped docs"
affects: [52-09 rate-limit rework, 52-31 rate-limit rework, start, CUT-04]

actuals:
  tokens: 12944
  tasks: 2
  commits: 3
plan_head_before: 1d374ab058367ed040c8f9fcdee9094770ef159c
plan_head_after: 27cb028586fcb51f8946824dfcaeb95aab8e9baa

tech-stack:
  added: []
  patterns:
    - "A helper that outlives the command it lived in moves first, with its test, and the move is proved by running the test at its new path (nextest exits 4 before the move, 0 after)"

key-files:
  created:
    - crates/devflow-cli/src/phase_worktree.rs
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/fallback_agent_config.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/recover.rs
    - crates/devflow-core/src/ship.rs
    - crates/devflow-core/src/workflow.rs
    - README.md
    - OPERATIONS.md
    - ARCHITECTURE.md
    - docs/guides/configuration.md
  deleted:
    - crates/devflow-cli/src/parallel.rs

key-decisions:
  - "FallbackSource::NoFallback was deleted with parallel, its only constructor; FallbackSource keeps its single FlagOrConfig variant rather than being collapsed into Option<AgentKind>, to keep the change to the call sites at zero"
  - "retry_after_from_reason stays pub(crate) in pipeline_outcomes.rs because 52-09 and 52-31 import it"

patterns-established:
  - "Removed verbs are appended to phase7_cli REMOVED_SUBCOMMANDS as the RED step of each cut plan"

requirements-completed: [CUT-04]

coverage:
  - id: D1
    description: "ensure_phase_worktree lives in phase_worktree.rs, unchanged in behaviour, and start uses it"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/phase_worktree.rs#phase_worktree::tests::ensure_phase_worktree_forks_from_the_supplied_base"
        status: pass
    human_judgment: false
  - id: D2
    description: "retry_after_from_reason lives in pipeline_outcomes.rs, unchanged in behaviour"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_outcomes.rs#pipeline_outcomes::tests::retry_after_from_reason_strips_prefix"
        status: pass
    human_judgment: false
  - id: D3
    description: "devflow parallel is rejected by clap and absent from --help"
    requirement: CUT-04
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#removed_subcommands_are_unknown"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/help_snapshot.rs#help_output_matches_committed_snapshot"
        status: pass
    human_judgment: false
  - id: D4
    description: "No scoped doc names devflow parallel, and every doc-referenced identifier still exists"
    requirement: CUT-04
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: other
        ref: "rg -n 'devflow parallel|parallel --phases|`parallel`' (outside .planning/ and CHANGELOG.md) -> only the phase_worktree.rs module comment naming the removed module"
        status: pass
    human_judgment: false

duration: 11min
completed: 2026-11-10
status: complete
---

# Phase 52 Plan 07: Remove devflow parallel Summary

**`devflow parallel` is gone: its two helpers that kept code still uses moved out first (`ensure_phase_worktree` to a new `phase_worktree.rs`, `retry_after_from_reason` to `pipeline_outcomes.rs`), then the verb, `parallel.rs`, the three tests that ran it and every scoped-doc mention were deleted.**

## Performance

- **Duration:** 11 min
- **Started:** 2026-11-10T21:01:32Z
- **Completed:** 2026-11-10T21:13:04Z
- **Tasks:** 2 (tracer + 1)
- **Files modified:** 17 (1 created, 1 deleted)

## Accomplishments

- `phase_worktree.rs` holds `ensure_phase_worktree` and `ensure_phase_worktree_forks_from_the_supplied_base` byte-for-byte; `commands.rs` imports it from there.
- `retry_after_from_reason` and `retry_after_from_reason_strips_prefix` sit in `pipeline_outcomes.rs` beside their only caller, `handle_rate_limited_outcome`.
- `Command::Parallel`, its dispatch, `mod parallel;` and `parallel.rs` are deleted. `FallbackSource::NoFallback` went with them, since `parallel` was its only constructor and the build warned it was dead.
- Tests deleted: `phase7_cli::parallel_creates_two_worktrees_and_spawns_two_monitors`, `fallback_agent_config::parallel_ignores_the_config_key`, `fallback_agent_config::parallel_refuses_a_config_key_that_names_no_agent`. The sweep `rg -n 'parallel' crates/devflow-cli/tests/` found no other test that runs the verb (the remaining hits are comments about tests running in parallel, plus `REMOVED_SUBCOMMANDS`).
- Docs: the README command table's Multi-Agent section and fallback parenthetical, the OPERATIONS command row, ARCHITECTURE's worktree list entry and two concurrency sentences, and the configuration guide's `parallel` sentence. Seven source comments that named `devflow parallel` (in `config.rs`, `recover.rs` x2, `ship.rs`, `workflow.rs`, `commands.rs`, `pipeline_launch.rs`) were reworded; the `config.rs` one described behaviour that no longer exists. CHANGELOG history was left as is.

## Task Commits

1. **Task 1 (tracer): move the kept helpers out of parallel.rs** - `0c9d3d6` (refactor)
2. **Task 2: remove devflow parallel** - `f6f7379` (test, RED), `27cb028` (feat!, GREEN)

## TDD Gate Compliance

- **Task 1 RED:** `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=phase_worktree::tests::ensure_phase_worktree_forks_from_the_supplied_base)'` and the same for `pipeline_outcomes::tests::retry_after_from_reason_strips_prefix` both exited 4 (`0 tests run ... error: no tests to run`) before the move. Control: the old path `parallel::tests::retry_after_from_reason_strips_prefix` exited 0 before the move and 4 after it. Semantic assessment: this is the plan's named RED signal for a pure move (the test cannot exist at the new path without the function, so no compiling test-only commit is possible). There is no `test(52-07)` commit before the Task 1 commit, and it is a `refactor`, not a `feat`, as the plan's commit subjects specify. The tracer gate re-ran the automated verify after the move: both exited 0.
- **Task 2 RED:** appending `parallel` to `REMOVED_SUBCOMMANDS` made `phase7_cli::removed_subcommands_are_unknown` exit 100, failing on its own assertion "`devflow parallel` must be rejected by clap as unknown" (clap printed `Usage: devflow parallel --phases <PHASES> [PROJECT]`). Semantic assessment: the target test ran and failed on the planned assertion for the intended reason. The libtest/nextest text output is not a format `check tdd-red-evidence` supports, so the classifier was not run; the assessment above is from the log. Committed as `f6f7379`.
- **Task 2 GREEN:** `27cb028`, after which the test exits 0.

## Verification

| Command | Exit |
|---------|------|
| `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=phase_worktree::tests::ensure_phase_worktree_forks_from_the_supplied_base)'` | 0 |
| `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=pipeline_outcomes::tests::retry_after_from_reason_strips_prefix)'` | 0 |
| `cargo nextest run -p devflow --test phase7_cli --no-tests=fail -E 'test(=removed_subcommands_are_unknown)'` | 0 |
| `cargo nextest run -p devflow --test help_snapshot --no-tests=fail -E 'test(=help_output_matches_committed_snapshot)'` | 0 |
| `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::doc_referenced_identifiers_exist_in_source)'` | 0 |
| `scripts/check.sh all` (fmt, clippy `-D warnings`, `cargo test --workspace`) | 0 (45 test binaries, 2124 passed, no `failed` line; the failure pattern was checked against the Task 2 RED log, where it matches) |

What this does not establish: `check.sh all` ran on this host, not in the pinned container (`scripts/check-in-container.sh`, run by the pre-push hook). No `devflow-core` public item changed (comment-only edits there), so `cargo semver-checks` was not run.

## Decisions Made

- `FallbackSource` keeps one variant instead of being replaced by `Option<AgentKind>`; that would touch every `start` call site for no behaviour change.
- `retry_after_from_reason` stays `pub(crate)` for the 52-09 and 52-31 imports.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Deleted FallbackSource::NoFallback**
- **Found during:** Task 2
- **Issue:** with `parallel` gone, `cargo build` warned `variant NoFallback is never constructed`, which fails clippy `-D warnings` in `scripts/check.sh all`.
- **Fix:** removed the variant, its match arm in `start`, and the comment sentences that described it.
- **Files modified:** crates/devflow-cli/src/commands.rs
- **Verification:** clippy clean; `scripts/check.sh all` exit 0
- **Committed in:** 27cb028

**2. [Rule 1 - Bug] Stale source comments naming the removed verb**
- **Found during:** Task 2 doc sweep
- **Issue:** `config.rs` said `devflow parallel` reads `fallback_agent` to refuse a broken key, which is no longer true; six other comments justified per-phase files "under `devflow parallel`".
- **Fix:** reworded to describe phases running at the same time (still possible with several `devflow start` runs). Comment-only, no API change.
- **Files modified:** crates/devflow-core/src/{config,recover,ship,workflow}.rs, crates/devflow-cli/src/{commands,pipeline_launch}.rs, docs/guides/configuration.md
- **Committed in:** 27cb028

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug). **Impact on plan:** both are cleanup forced by the removal; no scope beyond `parallel`.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 52-09 and 52-31 can import `retry_after_from_reason` from `crate::pipeline_outcomes`.
- Backlog 999.181 and 999.26 were already marked OBSOLETE in ROADMAP.md with CUT-04; nothing further changed there.

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/phase_worktree.rs; ABSENT as intended: crates/devflow-cli/src/parallel.rs
- FOUND on HEAD: 0c9d3d6, f6f7379, 27cb028
- Acceptance criteria re-run: `parallel.rs` absent; `fallback_agent_config.rs` has neither deleted `fn` (rg exit 1); `commands.rs:16` imports `crate::phase_worktree::ensure_phase_worktree`; `scripts/check.sh all` exit 0

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-10*
