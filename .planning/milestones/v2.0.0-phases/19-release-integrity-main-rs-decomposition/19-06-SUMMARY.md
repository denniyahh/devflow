---
phase: 19-release-integrity-main-rs-decomposition
plan: 06
subsystem: testing
tags: [rust, cargo-test, env-mutex, visibility, test-fixtures]

# Dependency graph
requires:
  - phase: 19-release-integrity-main-rs-decomposition
    provides: 19-01/19-02/19-03 (19a .devflow/ hygiene, 19b commit_path idempotence) landed at wave 1, satisfying D-20 sequencing
provides:
  - "A committed, durable pre-split baseline (SHA, per-target pass counts, both name lists, collision-check result, 9 cluster line ranges) that 19-07/08/09/11 diff against"
  - "pub(crate) visibility on CliError and 9 other cross-cluster types + fields, unblocking the first cluster move"
  - "A single crate::test_support module hosting the one ENV_MUTEX static and 9 shared test fixtures, importable by every future split cluster"
affects: [19-07, 19-08, 19-09, 19-10, 19-11]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Durable baseline artifacts (committed .txt name lists) instead of /tmp-only equivalence-proof inputs"
    - "#[cfg(test)] mod test_support; declared on the mod item (not #![cfg(test)] inside the file) to avoid dead_code false positives on a binary-only crate"
    - "Mechanical sed-range extraction for pure test-fixture moves, never retyping bodies"

key-files:
  created:
    - crates/devflow-cli/src/test_support.rs
    - .planning/phases/19-release-integrity-main-rs-decomposition/19-SPLIT-BASELINE.md
    - .planning/phases/19-release-integrity-main-rs-decomposition/19-SPLIT-BASELINE-names.txt
    - .planning/phases/19-release-integrity-main-rs-decomposition/19-SPLIT-BASELINE-names-full.txt
  modified:
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "D-20 sequencing verified at plan start: 19-01/19-02/19-03 commits and SUMMARYs confirmed present in git log before any main.rs edit began."
  - "Baseline captured from live cargo test --workspace runs at wave-2 HEAD (f35d6c1), not from a remembered/pre-wave-1 figure — 438 tests total, up from the ~424 pre-phase reference."
  - "No trailing-name collisions found (sort | uniq -d on 438 entries returns empty) — downstream diffs (19-07/08/09/11) may safely use the trailing-name list."
  - "EnvOverride RAII adoption explicitly deferred, per 19-PATTERNS.md's ruling — wrapping the 18 existing manual set/remove sites would be a behavioral change to test code inside the one plan whose job is to prove nothing changed."
  - "No D-12 ENV_MUTEX finding: three consecutive cargo test -p devflow runs post-hoist were stable with 0 failed each time."

patterns-established:
  - "Committed baseline artifacts (not /tmp) as the source of truth for cross-session/cross-executor equivalence proofs"
  - "#[cfg(test)] mod declaration on the mod item itself to keep a binary-only crate's non-test build free of test-only dead code"

requirements-completed: [19c]

coverage:
  - id: D1
    description: "Durable, committed baseline (SHA, per-target pass counts, name lists, collision check, cluster ranges) for the equivalence proof"
    requirement: "19c"
    verification:
      - kind: other
        ref: "git ls-files --error-unmatch .planning/phases/19-release-integrity-main-rs-decomposition/19-SPLIT-BASELINE-names.txt && 19-SPLIT-BASELINE-names-full.txt"
        status: pass
    human_judgment: false
  - id: D2
    description: "pub(crate) visibility on CliError and 9 other cross-cluster types/fields, pure-token diff"
    requirement: "19c"
    verification:
      - kind: other
        ref: "git diff -U0 crates/devflow-cli/src/main.rs (Task 2 commit) | rg '^[-+]' | rg -v '^(\\+\\+\\+|---)' — every hunk differs only by pub(crate)"
        status: pass
      - kind: unit
        ref: "cargo test --workspace — 438/438, 0 failed"
        status: pass
    human_judgment: false
  - id: D3
    description: "Single ENV_MUTEX hoisted into test_support.rs, all 18 lock sites intact, D-04 invariant documented, no D-12 finding"
    requirement: "19c"
    verification:
      - kind: other
        ref: "rg -c 'static ENV_MUTEX' crates/devflow-cli/src/ == 1; rg -n 'ENV_MUTEX\\.lock' crates/devflow-cli/src/main.rs == 18 lines"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow x3 consecutive runs — 0 failed each time"
        status: pass
    human_judgment: false

duration: 22min
completed: 2026-07-22
status: complete
---

# Phase 19 Plan 06: main.rs split foundation Summary

**Committed pre-split baseline (438 tests, 9 cluster ranges), pub(crate) visibility pass on 10 cross-cluster types, and a single hoisted ENV_MUTEX in a new test_support.rs module — the foundation the cluster-move plans (19-07..19-11) build on.**

## Performance

- **Duration:** 22 min
- **Started:** 2026-07-22T01:13:38Z
- **Completed:** 2026-07-22T01:35:44Z
- **Tasks:** 3
- **Files modified:** 2 (`main.rs`, plus 4 new files created)

## Accomplishments
- Captured and committed a durable pre-split baseline at SHA `f35d6c1`: 438 tests across 11 targets (0 failed), a sorted trailing-name list and a full-path list (both 438 entries, no collisions), and the measured line range of all 9 clusters plus `workflow_started_payload` at current HEAD.
- Added `pub(crate)` to `CliError`, `Staleness`, `StalenessOutcome`, `ValidateOutcome`, `ValidateResult`, `Liveness` (+ `describe`), `Check` (+ 4 fields), `Severity` (+ `label`), `PhaseFacts` (+ 9 fields), `PhaseFinding` (+ 4 fields) — a pure token-level diff verified line-by-line.
- Created `crates/devflow-cli/src/test_support.rs` (`#[cfg(test)] mod test_support;`) and mechanically relocated `ENV_MUTEX` and 9 shared test fixtures (`init_repo`, `init_repo_no_version_file`, `AlwaysFailAdapter`+impl, `FailOnceAdapter`+impls, `agent_free_git_only_path_dir`, `agent_free_dir_with_agent_stub`, `stub_agent_binary`, `prepend_path`, `stage_launched_count`) via sed-range extraction — zero bodies retyped.

## Task Commits

Each task was committed atomically:

1. **Task 1: Capture and commit the pre-split baseline** - `93b0f44` (docs)
2. **Task 2: pub(crate) visibility pass on cross-cluster types** - `98da212` (refactor)
3. **Task 3: Hoist ENV_MUTEX and shared fixtures into test_support.rs** - `a4785aa` (refactor)

**Plan metadata:** (this commit, docs: complete plan)

## Files Created/Modified
- `.planning/phases/19-release-integrity-main-rs-decomposition/19-SPLIT-BASELINE.md` - Committed baseline document: SHA, per-target pass-count lines, collision-check result, 9 cluster line ranges, both name lists reproduced inline
- `.planning/phases/19-release-integrity-main-rs-decomposition/19-SPLIT-BASELINE-names.txt` - Sorted trailing test-function names (438), the durable diff target for 19-07/08/09/11
- `.planning/phases/19-release-integrity-main-rs-decomposition/19-SPLIT-BASELINE-names-full.txt` - Sorted full module-qualified test paths (438), the fallback comparison form
- `crates/devflow-cli/src/test_support.rs` - New shared `#[cfg(test)]` module: `ENV_MUTEX` (with D-04 invariant doc) + 9 relocated test fixtures
- `crates/devflow-cli/src/main.rs` - `#[cfg(test)] mod test_support;` declaration, `pub(crate)` visibility pass on 10 types/fields, hoisted-item bodies removed from `mod tests`, `use crate::test_support::*;` added, unused `use std::sync::Mutex;` removed

## Decisions Made
- Verified D-20 sequencing invariant (19-01/19-02/19-03 landed at wave 1) via `git log` before touching `main.rs`, per the plan's mandatory halt-and-report branch — no violation found, proceeded.
- Recorded per-target pass counts freshly (438 total) rather than reusing the stale ~424 pre-phase reference, since 19-01/19-02/19-03 added tests since that figure was written.
- Kept the pre-existing `// 17-08 gap closure (CR-01)` section-separator comment in `main.rs` (it documents the broader CR-01 test group, most of which stays in `main.rs`) rather than moving it with `FailOnceAdapter` — only the item-level `///` doc comments moved with their items.
- Ran `cargo fmt` once after the mechanical extraction: dedenting the hoisted `stage_launched_count` body changed its optimal line-wrap width, which `rustfmt --check` flagged. This is a pure reformatting consequence of moving code to a shallower indentation level, not a body edit — reformatting was necessary to pass the plan's own `cargo fmt --check` acceptance criterion.

## Deviations from Plan

None - plan executed exactly as written. No D-12 `ENV_MUTEX` finding was raised (three consecutive `cargo test -p devflow` runs post-hoist were stable, `0 failed` each time), so the plan completes rather than halts.

## Issues Encountered
- The plan's acceptance criterion `rg -c 'ENV_MUTEX' crates/devflow-cli/src/main.rs returns 18` measures the bare substring, which also matches 44 doc-comment/SAFETY-comment mentions of `ENV_MUTEX` beyond the 18 actual `.lock()` call sites (62 total matches). Verified the real invariant directly instead: `rg -n 'ENV_MUTEX\.lock'` returns exactly 18 lines, matching the pre-hoist count exactly — no lock site was lost.
- My first draft of the `ENV_MUTEX` doc comment's D-04 invariant sentence line-wrapped across two `///` lines, which caused the required `rg -c 'exactly one mutex'` single-line-match acceptance criterion to return 0. Reworded to keep "exactly one mutex" on one line; re-verified.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- `crate::test_support::ENV_MUTEX` and its 9 companion fixtures are ready for every future cluster-move plan (19-07..19-11) to import via `use crate::test_support::*;`.
- All 10 cross-cluster types are `pub(crate)`, so the first cluster-move plan can relocate functions returning `CliError` without a separate visibility pass.
- The committed `19-SPLIT-BASELINE-names.txt`/`-names-full.txt` are ready for 19-07/08/09/11's equivalence-proof diffs; no collision-handling fallback is needed (trailing names are unique).
- No blockers. `EnvOverride` RAII adoption remains an explicitly deferred follow-up, not part of this phase.

---
*Phase: 19-release-integrity-main-rs-decomposition*
*Completed: 2026-07-22*

## Self-Check: PASSED

All claimed files verified present on disk (`19-SPLIT-BASELINE.md`, `19-SPLIT-BASELINE-names.txt`, `19-SPLIT-BASELINE-names-full.txt`, `test_support.rs`, `main.rs`, this SUMMARY). All three task commits (`93b0f44`, `98da212`, `a4785aa`) verified present in `git log --oneline --all`.
