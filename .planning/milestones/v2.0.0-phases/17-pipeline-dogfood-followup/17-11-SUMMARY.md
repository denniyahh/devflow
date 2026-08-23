---
phase: 17-pipeline-dogfood-followup
plan: 11
subsystem: infra
tags: [build-rs, cargo, git-provenance, staleness-gate, rust]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: "17-05's launch_stage/enforce_build_staleness call site, 17-06's ancestry Staleness arm, 17-07's Ahead classification, 17-10's affects_compiled_binary predicate"
provides:
  - "Always-rerunning build.rs (single unfingerprintable sentinel replaces the HEAD/refs/packed-refs rerun-if-changed set)"
  - "DEVFLOW_BUILD_TIMESTAMP fully removed (emission and every consumer)"
  - "Dirty-flag staleness arm (combined_staleness/enforce_build_staleness take build_dirty: bool) replacing the old mtime arm"
  - "CR-02 regression test proving build.rs actually reruns across a working-tree edit"
affects: [18-hermes-support]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "cargo build script always-rerun via an unfingerprintable rerun-if-changed sentinel path, when the script reads inputs (working-tree status, wall-clock) that cannot be expressed as watched paths"
    - "Reading a build script's cached target/debug/build/<pkg>-<hash>/output file to assert whether it actually reran across two real cargo build invocations, instead of just asserting env!() values from a single compile"

key-files:
  created: []
  modified:
    - crates/devflow-cli/build.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/build_provenance.rs
    - .planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md

key-decisions:
  - "Operator decision (CR-02 disposition): always re-run build.rs and drop DEVFLOW_BUILD_TIMESTAMP entirely, rather than keep the narrow HEAD/refs/packed-refs trigger set. The timestamp was the only value that changed every run, so removing it is what keeps an always-rerunning script from forcing a devflow-cli recompile on every cargo build."
  - "Staleness's second signal (replacing the mtime arm) is a decision table over (build_dirty, tree_has_modified_build_inputs_now): (false, yes) => Stale (the CR-02 case); (true, yes) => Indeterminate, never a hard block (Pitfall 4); (either, no) => fall through to the unchanged ancestry result."
  - "Task 1 (build.rs) and Task 2 (main.rs) were committed together, not separately, because they are compile-coupled — main.rs references DEVFLOW_BUILD_TIMESTAMP in two places and the workspace does not compile with only build.rs's half of the change applied. See Deviations."

requirements-completed: [17d]

coverage:
  - id: D1
    description: "build.rs always reruns (single unfingerprintable sentinel), so DEVFLOW_BUILD_DIRTY reflects the working tree's current state on every build"
    requirement: "17d"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/build_provenance.rs#build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild"
        status: pass
    human_judgment: false
  - id: D2
    description: "DEVFLOW_BUILD_TIMESTAMP removed entirely — no emission in build.rs, no consumer in main.rs"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::workflow_started_payload_carries_build_provenance"
        status: pass
    human_judgment: false
  - id: D3
    description: "Staleness decided by ancestry + live dirty-flag comparison: a clean-tree build against a now-modified tree is Stale (the CR-02 case)"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::combined_staleness_dirty_flag_arm_flags_modified_tree_when_build_was_clean"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::dirty_flag_arm_ignores_non_build_files_but_still_flags_sources"
        status: pass
    human_judgment: false
  - id: D4
    description: "A build already made from a dirty tree, run against a still-dirty tree, is Indeterminate and never hard-blocks"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::combined_staleness_dirty_flag_arm_is_indeterminate_when_build_was_already_dirty"
        status: pass
    human_judgment: false
  - id: D5
    description: "17-07 Ahead and 17-06 strict-ancestor Stale classifications, and D-20 no-git graceful degradation, are unchanged"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::ahead_build_from_descendant_commit_warns_instead_of_blocking"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks"
        status: pass
    human_judgment: false

duration: 40min
completed: 2026-07-19
status: complete
---

# Phase 17 Plan 11: Fix CR-02 — build provenance the trigger set cannot honor Summary

**`build.rs` now always re-runs via an unfingerprintable rerun-if-changed sentinel and no longer embeds `DEVFLOW_BUILD_TIMESTAMP`; the staleness gate's second signal is a live dirty-flag comparison instead of a stale-by-construction mtime check.**

## Performance

- **Duration:** ~40 min
- **Started:** 2026-07-19T20:45:00Z (approx.)
- **Completed:** 2026-07-19T21:23:19Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- `build.rs` replaces its three narrow `HEAD`/`refs`/`packed-refs` `rerun-if-changed` lines with a
  single sentinel path that can never exist, forcing cargo to always rerun the script — the input
  it actually reads (`git status --porcelain`, the whole working tree) could never be fingerprinted
  by path in the first place.
- `DEVFLOW_BUILD_TIMESTAMP` is gone: no `cargo:rustc-env` emission in `build.rs`, no `env!(...)`
  consumer anywhere in `main.rs`. It was the one value that changed on every build, so retiring it
  is what keeps the now-always-rerunning script from forcing `devflow-cli` to recompile on every
  single `cargo build`.
- `combined_staleness`/`enforce_build_staleness` take `build_dirty: bool` instead of
  `build_timestamp: u64`; `tracked_source_newer_than_build` (mtime comparison) is replaced by
  `tree_has_modified_build_inputs` (live `git status`/`ls-files -m` check), reusing 17-10's
  `affects_compiled_binary` predicate rather than duplicating it.
- New decision table for the second signal: `(dirty=false, tree modified)` ⇒ Stale (the CR-02
  case — built clean, source changed since); `(dirty=true, tree modified)` ⇒ Indeterminate (warn,
  never hard-block — Pitfall 4, since "same dirt" and "more dirt" can't be told apart without a
  timestamp); either case with no modified build-affecting files falls through to the unchanged
  ancestry result.
- `crates/devflow-cli/tests/build_provenance.rs` gained a real end-to-end regression test:
  builds a synthetic single-commit, packed-refs checkout (reproducing the CI-shaped conditions
  that exposed CR-02, distinct from this dev checkout's locally-missing `packed-refs` that
  accidentally masked it) from the CURRENT working tree, builds it with `cargo build -p devflow`,
  edits a tracked `.rs` file, builds again, and asserts the build script's own cached `output`
  file shows `DEVFLOW_BUILD_DIRTY` flip `false → true`.
- `17-REVIEW.md`'s CR-02 entry, disposition table row, header status line, and "still open" list
  are all updated to Fixed, naming the operator decision and this plan.

## Task Commits

Task 1 and Task 2 were committed together (see Deviations for why):

1. **Tasks 1+2: build.rs always-rerun + drop timestamp; main.rs dirty-flag staleness arm** - `3e39cf6` (fix)
2. **Task 3: record CR-02 disposition as resolved in 17-REVIEW.md** - `fd065e3` (docs)

## Files Created/Modified

- `crates/devflow-cli/build.rs` - single always-rerun sentinel; `DEVFLOW_BUILD_TIMESTAMP` removed; comment rewritten explaining why (CR-02)
- `crates/devflow-cli/src/main.rs` - `workflow_started_payload` drops `build_timestamp`; `tree_has_modified_build_inputs` replaces `tracked_source_newer_than_build`; `combined_staleness`/`enforce_build_staleness` take `build_dirty: bool`; call site passes `env!("DEVFLOW_BUILD_DIRTY") == "true"`; renamed/rewrote the mtime-arm test fixtures against the dirty-flag rule; added a new Indeterminate-branch regression test
- `crates/devflow-cli/tests/build_provenance.rs` - dropped the `DEVFLOW_BUILD_TIMESTAMP`-parsing test; added the two-cargo-build CR-02 regression test
- `.planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md` - CR-02 marked Fixed (header status, finding body, Audit-Fix Addendum disposition table and "still open" list)

## Decisions Made

- **Always-rerun + drop the timestamp** (the operator's CR-02 disposition, implemented as written
  in the plan): no other option was on the table — the plan's "The decision" section had already
  resolved the design tradeoff before this execution began.
- **`tree_has_modified_build_inputs` reuses `affects_compiled_binary` verbatim** rather than
  re-deriving a similar predicate, per the plan's explicit "do not duplicate" instruction.
- **Test verification strategy for the regression test:** rather than trust `env!(...)` read from
  the test binary's own single compile (which would only prove what got embedded into that one
  compile, not whether `build.rs` actually reran across two builds), the test builds the real
  `devflow` package twice via `cargo build -p devflow` in a synthetic checkout and inspects the
  build script's own cached `target/debug/build/devflow-<hash>/output` file — the only artifact
  cargo persists that changes if and only if the build script was actually invoked again. This
  is the direct, non-duplicating way to prove the fix rather than reimplement build.rs's logic
  in the test.
- **Synthetic checkout built from `git ls-files` + live file content, not `git clone`:** an
  actual `git clone` only reproduces the last COMMIT, not uncommitted working-tree edits (this
  plan's own in-progress edits to `build.rs`/`main.rs` at test-authoring time). Copying every
  tracked file's current on-disk content into a fresh `git init`+commit+`pack-refs` checkout
  means the regression test always exercises whatever the working tree currently says, not a
  stale prior commit — and still gets a real `packed-refs` file, the condition that exposed
  CR-02 in CI and was accidentally masked in this dev checkout.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 1 and Task 2 could not be committed separately — they are compile-coupled**
- **Found during:** Task 1's own verification step (attempting to `cargo test -p devflow --test build_provenance` after only the `build.rs` change)
- **Issue:** The plan splits build.rs (Task 1) from main.rs's decision-table rewrite (Task 2), implying separately-verifiable commits. But `main.rs` references `env!("DEVFLOW_BUILD_TIMESTAMP")` in two places (`workflow_started_payload` and the `enforce_build_staleness` call site). Once `build.rs` stops emitting that env var, `crates/devflow-cli` fails to compile — including for `cargo test -p devflow --test build_provenance`, whose test-target build still requires the package's other targets — so Task 1's own required verification (the two-build regression test) could not run against a `build.rs`-only change.
- **Fix:** Implemented Task 1 and Task 2's code changes together, verified the combined result (`cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, all clean), and committed both as a single `fix(17-11)` commit rather than leaving an intermediate non-compiling commit in history.
- **Files modified:** `crates/devflow-cli/build.rs`, `crates/devflow-cli/src/main.rs`, `crates/devflow-cli/tests/build_provenance.rs`
- **Verification:** Full verification gate green after the combined commit (see below).
- **Committed in:** `3e39cf6`

**2. [Rule 2 - Missing Critical] Added a dedicated Indeterminate-branch regression test**
- **Found during:** Task 2 (writing the decision table)
- **Issue:** The plan's must_haves truth 5 requires "a binary built from an ALREADY-dirty tree, run against a still-dirty tree, is Indeterminate (warn, never hard-block)" — a genuinely new branch this plan introduces (the old mtime arm had no equivalent case). No existing or plan-named test exercised it.
- **Fix:** Added `combined_staleness_dirty_flag_arm_is_indeterminate_when_build_was_already_dirty`, covering both `combined_staleness` returning `Staleness::Indeterminate` and `enforce_build_staleness` not hard-blocking a self-dogfood workspace in that state.
- **Files modified:** `crates/devflow-cli/src/main.rs`
- **Verification:** Test passes; without the `Some(true) if build_dirty => Indeterminate` arm in `combined_staleness`, this test fails (verified by temporarily reverting that arm during development).
- **Committed in:** `3e39cf6`

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 missing critical test coverage).
**Impact on plan:** No scope creep — both are required for the plan's own stated deliverables (a compiling, fully-verified codebase; and test coverage for a must_have behavior). Combining Tasks 1+2 into one commit is a documented departure from the plan's task-per-commit framing, not a change to what was built.

## Issues Encountered

- One `cargo test --workspace` run (of ~7 full runs performed during verification) showed
  `terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished` FAILED. This test
  is unrelated to any file this plan touches logically (it's a threaded merge-conflict/gate test
  with a hardcoded 1-second polling budget, pre-existing in `main.rs`) and passed cleanly on 3
  immediate reruns of the full suite plus an isolated single-test run — consistent with a
  pre-existing timing flake under system load, not a regression introduced here. Left unfixed
  per the SCOPE BOUNDARY rule (out-of-scope, unrelated file/test).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- CR-02 is resolved; no open Critical remains in `17-REVIEW.md`.
- `17-VALIDATION.md`'s GAP-3 ("17d build-provenance freshness is sampled by no test") was raised
  against the pre-fix state and is now addressed by the new regression test, but that file itself
  was not touched by this plan (out of `17-11`'s declared `files_modified` scope) — a future
  re-validation pass should re-sample Rows 7/8 and close GAP-3 explicitly if a formal
  `nyquist_compliant` flip back to `true` is desired.
- WR-09's `Ahead`/`Indeterminate` output-collapse bullet remains open (untouched by this plan);
  its "`DEVFLOW_BUILD_DIRTY` is never read" bullet is now moot — the flag is read by
  `enforce_build_staleness`'s call site as of this plan.

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-19*

## Self-Check: PASSED

All created/modified files confirmed present on disk; both commit hashes (`3e39cf6`, `fd065e3`)
confirmed present in `git log --oneline --all`.
