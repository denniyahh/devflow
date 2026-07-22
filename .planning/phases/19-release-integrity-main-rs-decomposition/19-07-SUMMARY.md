---
phase: 19-release-integrity-main-rs-decomposition
plan: 07
subsystem: cli
tags: [rust, main-rs-split, pure-move, cargo-test, clippy]

# Dependency graph
requires:
  - phase: 19-release-integrity-main-rs-decomposition
    provides: 19-06 (pub(crate) visibility pass, test_support.rs ENV_MUTEX/fixtures hoist, committed baseline SHA + name lists)
provides:
  - "crates/devflow-cli/src/staleness.rs and preflight.rs as flat sibling modules, proving the mechanical extraction procedure before the larger 19-08/19-09 clusters bet on it"
  - "A wider-than-estimated pub(crate) surface finding (worktree_writable_roots, ensure_agent_binary, agent_program, phase_artifact_on_develop) that 19-08/19-09 should expect when extracting pipeline/CLI-cluster helpers"
affects: [19-08, 19-09, 19-11]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Bidirectional cluster coupling (preflight <-> pipeline) preserved as direct calls and documented in the module doc comment, not abstracted behind a callback/trait (D-09, D-18f)"
    - "Judgment-call test attribution decided by reading the test body's actual assertions, not by its physical location in the pre-move file"

key-files:
  created:
    - crates/devflow-cli/src/staleness.rs
    - crates/devflow-cli/src/preflight.rs
  modified:
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root left in main.rs for 19-08: its body asserts exclusively on hook_context_root (a pipeline-cluster function), not on any staleness or preflight function, despite sitting physically inside the staleness test region."
  - "Wider pub(crate) surface than the plan's two-function estimate (run_preflight/launch_stage_inner): the compiler also demanded worktree_writable_roots, ensure_agent_binary, and agent_program (called directly from main.rs's own launch_stage_inner/run_agent_blocking), plus phase_artifact_on_develop (a CLI-cluster function preflight_interactivity_check calls back into main.rs for). All four added per the plan's compiler-driven minimal-surface rule; recorded here as the real architectural finding the plan asked to flag rather than pass unremarked."
  - "Corrected the plan's own literal cargo test --workspace -- --list | rg '::tests::' | sed 's/.*::tests:://' extraction command: it silently drops every test in main.rs's own top-level mod tests (lines like 'tests::foo: test' have no leading '::' before 'tests', so the '::tests::' substring never matches). Used sed 's/.*:://' (strip up to the last '::' occurrence) instead, verified byte-identical against the committed 19-SPLIT-BASELINE-names.txt (438/438, diff empty) both after Task 1 and Task 2."

patterns-established:
  - "Per-cluster module doc comment states not just what the cluster owns but which cross-cluster call it participates in, so a future reader of the two-way preflight/pipeline coupling doesn't have to reconstruct the coupling from two separate diffs"

requirements-completed: [19c, 19d]

coverage:
  - id: D1
    description: "Staleness cluster (embedded_commit_is_stale..enforce_build_staleness + Staleness/StalenessOutcome enums + BUILD_AFFECTING_FILES) moved into staleness.rs, byte-identical modulo pub(crate)/use-path/indentation"
    requirement: "19c"
    verification:
      - kind: other
        ref: "diff -u against baseline SHA f35d6c1's extracted range (911-1240): 4 hunks, all pub(crate) additions (Staleness, StalenessOutcome, run_git_stdout, enforce_build_staleness), zero unexplained hunks"
        status: pass
      - kind: unit
        ref: "cargo test --workspace after Task 1: all 11 targets match baseline pass counts exactly (438 total), 0 failed"
        status: pass
    human_judgment: false
  - id: D2
    description: "Preflight cluster (worktree_writable_roots..run_preflight) moved into preflight.rs, bidirectional pipeline coupling preserved as direct calls"
    requirement: "19c"
    verification:
      - kind: other
        ref: "diff -u against baseline SHA f35d6c1's extracted range (643-884): 4 hunks, all pub(crate) additions (worktree_writable_roots, agent_program, ensure_agent_binary, run_preflight), zero unexplained hunks"
        status: pass
      - kind: other
        ref: "rg -c 'launch_stage_inner' preflight.rs == 4; rg -c 'run_preflight' main.rs == 9; rg -c 'Box<dyn Fn|callback|Arc<dyn' preflight.rs == 0 (no indirection introduced)"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow x3 consecutive runs post-move: 0 failed each time, no D-12 ENV_MUTEX finding"
        status: pass
    human_judgment: false
  - id: D3
    description: "Zero tests lost, gained, renamed, or split across both tasks"
    requirement: "19d"
    verification:
      - kind: other
        ref: "cargo test --workspace -- --list, trailing-name-set (438 entries) diffed empty against committed 19-SPLIT-BASELINE-names.txt after Task 1 and again after Task 2"
        status: pass
    human_judgment: false

duration: 71min
completed: 2026-07-22
status: complete
---

# Phase 19 Plan 07: staleness + preflight cluster extraction Summary

**Extracted the build-staleness/provenance cluster (`staleness.rs`) and the agent preflight cluster (`preflight.rs`) out of `main.rs` as the shakedown run for the mechanical extraction procedure — 438/438 tests byte-identical to the 19-06 baseline, every moved function diffs clean modulo `pub(crate)`, and the intentional preflight/pipeline call cycle is preserved and documented rather than abstracted away.**

## Performance

- **Duration:** 71 min
- **Started:** 2026-07-21T22:07:38-04:00 (immediately after 19-05's completion commit)
- **Completed:** 2026-07-22T ~22:40 (this commit)
- **Tasks:** 2
- **Files modified:** 3 (`main.rs`, plus 2 new files created)

## Accomplishments
- Re-derived both clusters' current line ranges live at each task's own HEAD (never reused the stale `19-SPLIT-BASELINE.md` ranges, which predate both 19-06's own fixture hoist and this plan's Task 1): staleness at 914–1243 (pre-Task-1 HEAD), preflight at 649–890 (post-Task-1 HEAD).
- Extracted the staleness cluster (`embedded_commit_is_stale` through `enforce_build_staleness`, the `Staleness`/`StalenessOutcome` enums, `BUILD_AFFECTING_FILES`) into `crates/devflow-cli/src/staleness.rs`, together with all 16 attributed tests and the two shared fixtures (`worktree_staleness_fixture`, `init_repo_with_diverged_commit`), in one commit.
- Extracted the preflight cluster (`worktree_writable_roots` through `run_preflight`) into `crates/devflow-cli/src/preflight.rs`, together with the isolated `ensure_agent_binary_diagnoses_missing_program` test and the contiguous 9-test preflight block (including all 4 cross-cluster `PATH`-mutating tests), in one commit.
- Preserved the preflight ↔ pipeline bidirectional call (D-18f, 18-07) as two direct calls — `run_preflight`'s `Advance` arm calls `crate::launch_stage_inner` directly, `crate::launch_stage` calls `preflight::run_preflight` — with no callback/trait indirection, documented explicitly in `preflight.rs`'s module doc comment.
- Ran the per-function equivalence proof against the committed baseline SHA (`f35d6c1`) for both clusters immediately after each task's commit (not deferred to phase end): every moved function's body diffs clean, with the only permitted hunk type (`pub(crate)` additions) appearing exactly where the compiler demanded it.

## Task Commits

Each task was committed atomically:

1. **Task 1: Extract the staleness cluster into `staleness.rs`** — `f101280` (refactor)
2. **Task 2: Extract the preflight cluster into `preflight.rs`** — `0c23ef3` (refactor)

**Plan metadata:** (this commit, docs: complete plan)

## Files Created/Modified
- `crates/devflow-cli/src/staleness.rs` — New module: build staleness/provenance decision + enforcement (17d/18c), `pub(crate)`-only, 16 tests + 2 fixtures moved verbatim
- `crates/devflow-cli/src/preflight.rs` — New module: agent preflight readiness gate (17c, D-13–D-16), `pub(crate)`-only, 10 tests moved verbatim, bidirectional pipeline coupling documented
- `crates/devflow-cli/src/main.rs` — `mod staleness;`/`mod preflight;` declarations added, both clusters' production + test code removed, `phase_artifact_on_develop` made `pub(crate)` (called from `preflight.rs`)

## Decisions Made
- **`content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root`** — the plan's one flagged judgment call — left in `main.rs` for 19-08. Read the test body directly: every assertion is against `hook_context_root` (a pipeline-cluster function), not against anything in staleness or preflight, even though the test sits physically inside the staleness test region in the pre-move file. Per `<test_attribution_rule>`, attribution follows what a test actually asserts, not its location.
- **Wider `pub(crate)` surface than the plan's two-function estimate.** The plan named `run_preflight`/`launch_stage_inner` as the one anticipated coupling. The compiler additionally demanded `pub(crate)` on `worktree_writable_roots`, `ensure_agent_binary`, and `agent_program` — all three called directly from `main.rs`'s own `launch_stage_inner` and `run_agent_blocking` (not merely from within the preflight cluster itself) — plus `phase_artifact_on_develop` (a "CLI arg types and entry" cluster function that `preflight_interactivity_check` calls back into `main.rs` for). All four were added following the plan's own rule ("add `pub(crate)` only where the compiler demands it"); this is recorded per the plan's explicit instruction that a wider-than-expected surface is a real architectural finding to write down, not a reason to abandon the move. 19-08/19-09 should expect a similar handful of cross-cluster call-backs when they extract the pipeline and commands/display clusters.
- **Corrected a latent bug in the plan's own literal verification command.** `cargo test --workspace -- --list | rg '::tests::' | sed 's/.*::tests:://'` silently drops every test that lives in `main.rs`'s own top-level `mod tests` (their `--list` lines read `tests::foo: test`, which has no leading `::` before `tests`, so the `::tests::` substring pattern never matches — this is a materially different bug class from the trailing-name collision risk the baseline document anticipated). Used `sed 's/.*:://'` (take the segment after the last `::`, matching regardless of module nesting depth) instead, and confirmed it reproduces the committed baseline list byte-for-byte (438/438, diff empty) both after Task 1 and after Task 2. Flagging for 19-08/19-09/19-11, which cite the same literal command in their own `<verify>` blocks.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - blocking] Compiler-demanded `pub(crate)` surface wider than the plan's estimate**
- **Found during:** Task 2
- **Issue:** `worktree_writable_roots`, `ensure_agent_binary`, `agent_program` (called from main.rs's own pipeline functions) and `phase_artifact_on_develop` (called from preflight.rs back into main.rs) all failed to compile without `pub(crate)`.
- **Fix:** Added `pub(crate)` to all four, per the plan's own extraction procedure step 7 ("add `pub(crate)` only where the compiler demands it"). No function body was touched.
- **Files modified:** `crates/devflow-cli/src/preflight.rs`, `crates/devflow-cli/src/main.rs`
- **Commit:** `0c23ef3`

**2. [Rule 1 - bug] Corrected the plan's own name-set extraction command**
- **Found during:** Task 1 (equivalence-proof step)
- **Issue:** The literal `rg '::tests::' | sed 's/.*::tests:://'` command specified in the plan's own `<verify>` blocks silently drops every top-level `mod tests` test (no leading `::` before `tests` in those `--list` lines), producing a false "lost 123 tests" diff against the baseline that isn't a real regression.
- **Fix:** Used `sed 's/.*:://'` instead (strip to the segment after the last `::`), confirmed empty diff against the committed baseline (438/438) at both checkpoints.
- **Files modified:** none (verification-only correction, not a source change)
- **Commit:** n/a (documented here for 19-08/19-09/19-11's benefit, since they cite the same command)

### Auto-fixed Issues (blank-line cosmetics)
- Four double-blank-line artifacts left by the `sed`-range deletions (at the boundaries where a cluster was cut out from between two adjacent items) were collapsed to single blank lines in `main.rs` — a whitespace-only cleanup, not a body change, verified by `cargo fmt --check` passing clean afterward.

## Issues Encountered
None beyond the two deviations above — no compile errors survived past the pub(crate) pass, no clippy or fmt issues in the final state, and no D-12 `ENV_MUTEX` finding across three consecutive `cargo test -p devflow` runs after Task 2.

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- The mechanical extraction procedure (re-derive range → `sed`-extract → assemble → delete highest-line-first → `mod` declaration → compiler-driven `pub(crate)` pass → per-function equivalence proof) is proven on two clusters and ready to scale to 19-08 (pipeline, the largest cluster) and 19-09 (parallel + commands/display).
- 19-08/19-09 should expect a similar "compiler demands more `pub(crate)` than the plan's estimate" pattern — cross-cluster helper calls are common in this file, not isolated to preflight.
- 19-08's first task should decide `content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root`'s move now that `hook_context_root` is the function it actually exercises.
- All 11 targets (438 tests) remain green; `cargo build -p devflow`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` all exit 0 at this plan's final commit.

---
*Phase: 19-release-integrity-main-rs-decomposition*
*Completed: 2026-07-22*

## Self-Check: PASSED

All claimed files verified present on disk (`crates/devflow-cli/src/staleness.rs`, `crates/devflow-cli/src/preflight.rs`, `crates/devflow-cli/src/main.rs`, this SUMMARY). Both task commits (`f101280`, `0c23ef3`) verified present in `git log --oneline --all`.
