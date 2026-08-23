---
phase: 19-release-integrity-main-rs-decomposition
plan: 02
subsystem: infra
tags: [rust, serde_json, telemetry, privacy, wr-02]

# Dependency graph
requires:
  - phase: 19-release-integrity-main-rs-decomposition (19-01)
    provides: "ensure_devflow_dir() single chokepoint for .devflow/ creation (19a), landed first per D-13 sequencing"
provides:
  - "workflow_started_payload's exe_path field redacted to a bare binary filename (WR-02, closes 19a's second half)"
  - "Strengthened regression test that fails if the absolute path ever returns"
affects: [19-06, 19-07, 19-08, 19-09, 19-11]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Path redaction pattern: .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())) preserves the string-or-null failure-signal contract while dropping the directory component"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs

key-decisions:
  - "Used to_string_lossy().into_owned() rather than to_str() so a non-UTF-8 binary name still produces a string, keeping null as the field's distinct 'could not determine the executable' signal"
  - "Left worktree field byte-identical — T-19-09 accepts it as out of D-15's decision scope (operator-chosen project-relative path, not host identity); surfaced here per the plan's requirement, not fixed"
  - "Shortened the WR-02 source comment to 2 lines so it stays inside the plan's rg -A14 verification window (see Issues Encountered)"

requirements-completed: [19a]

coverage:
  - id: D1
    description: "exe_path in workflow_started_payload emits only the binary filename, never a directory-bearing path"
    requirement: "19a"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#workflow_started_payload_carries_build_provenance"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-07-22
status: complete
---

# Phase 19 Plan 02: Redact `exe_path` to Binary Filename Summary

**`workflow_started_payload`'s `exe_path` now emits only `current_exe().file_name()` instead of the full absolute path — closing WR-02, the second half of the `.devflow/` artifact-hygiene fix (19a), so `events.jsonl` no longer leaks the operator's home directory and OS username into a file `OPERATIONS.md` tells operators to tail and paste.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-07-22T00:44:20Z
- **Completed:** 2026-07-22T00:56:00Z
- **Tasks:** 1
- **Files modified:** 1

## Accomplishments
- `exe_path` value expression in `workflow_started_payload` (`main.rs:894`) redacted to the binary's bare filename via `.and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))`, replacing `.map(|p| p.display().to_string())`.
- `workflow_started_payload_carries_build_provenance` strengthened with two new assertions: (1) the `exe_path` key still exists (guards against a future refactor satisfying the redaction by deletion — T-19-10), and (2) when `exe_path` is a string it contains neither `/` nor `\`.
- RED-first evidence captured live (see Issues Encountered) before the production fix landed.

## Task Commits

Each task was committed atomically (TDD RED → GREEN):

1. **Task 1 (RED):** Strengthen `workflow_started_payload_carries_build_provenance` - `671b97b` (test)
2. **Task 1 (GREEN):** Redact `exe_path` to binary filename - `3dd9185` (fix)

**Plan metadata:** committed alongside this SUMMARY.

_TDD task: RED (`test`) then GREEN (`fix`), no refactor commit needed._

## Files Created/Modified
- `crates/devflow-cli/src/main.rs` - `workflow_started_payload`'s `exe_path` expression redacted; `workflow_started_payload_carries_build_provenance` strengthened with key-presence and no-separator assertions.

## Decisions Made
- `to_string_lossy().into_owned()` over `to_str()` — a non-UTF-8 binary name must still produce a string, not silently collapse to the field's documented null failure signal (matches the plan's explicit instruction).
- `worktree` field left untouched — out of D-15's scope; see Threat Flags / next section for the T-19-09 observation this plan was required to surface.

## Deviations from Plan

### Auto-fixed Issues

None — Rules 1-3 were not triggered; no bugs, missing functionality, or blockers were found beyond the plan's own scope.

**1. [Verification-command note, not a code deviation] `rg -A14` window required a shorter source comment**

- **Found during:** Task 1, drafting the production change.
- **Issue:** The plan's `<action>` asked for a `//` comment "naming the decision as the WR-02 redaction." A first draft (5 lines) pushed the `file_name` occurrence past line 14 of the acceptance criterion's `rg -A14 'fn workflow_started_payload' ... | rg -c 'file_name'` window, which would have returned `0` instead of the required `1`.
- **Fix:** Condensed the comment to 2 lines carrying the same reasoning (redaction rationale + `to_string_lossy` justification), which restores `file_name` inside the 14-line window. No functional change — comment content only.
- **Files modified:** `crates/devflow-cli/src/main.rs` (comment text only, same commit as the production fix).
- **Verification:** `rg -A14 'fn workflow_started_payload' crates/devflow-cli/src/main.rs | rg -c 'file_name'` → `1`.
- **Committed in:** `3dd9185` (Task 1 GREEN commit).

**2. [Acceptance-criteria observation, not fixed] The `exe_path` count criterion cannot return `1` as literally specified**

- **Found during:** Task 1, running the acceptance-criteria verification commands.
- **Issue:** `rg -A14 'fn workflow_started_payload' crates/devflow-cli/src/main.rs | rg -c 'exe_path'` returns `3`, not the `1` the acceptance criteria expect. Root cause: the unanchored pattern `fn workflow_started_payload` also matches the test function `fn workflow_started_payload_carries_build_provenance` (substring match), so `rg -A14` emits **two** 14-line blocks — the production function (1 `exe_path` occurrence) plus the test function (2 occurrences, from the new key-presence assertion and its failure message). Confirmed via `git stash` that this ambiguity **predates this plan's changes** — it returns `3` even on the RED-commit-only state, before the GREEN fix — so it is not a regression introduced here.
- **Fix:** Not fixed — this is a property of the plan's own verification command interacting with a pre-existing test function name, not a defect in the code. The underlying invariant it was meant to check (the `exe_path` key still exists in the production function) is independently confirmed by the `file_name` count (`1`, correct) and by the strengthened test's explicit key-presence assertion.
- **Files modified:** none.
- **Verification:** N/A (documented, not fixed).
- **Committed in:** N/A.

---

**Total deviations:** 1 auto-fixed (comment-length adjustment, no functional change), 1 documented-not-fixed (stale acceptance-criteria grep count, pre-existing).
**Impact on plan:** Zero scope creep. Both items are verification-tooling notes, not code defects.

## Issues Encountered

**RED-first evidence (required by the plan's acceptance criteria):**

With the strengthened test in place and the production fix reverted (i.e. immediately after committing `671b97b`, before `3dd9185`), `cargo test -p devflow workflow_started_payload_carries_build_provenance` failed with:

```
thread 'tests::workflow_started_payload_carries_build_provenance' panicked at crates/devflow-cli/src/main.rs:6889:13:
WR-02: exe_path must be a bare filename with no directory separator — OPERATIONS.md documents events.jsonl as safe to
tail and paste, so a full absolute path here leaks the operator's home directory and OS username; got
"/var/home/denniyahh/Github/devflow/target/debug/deps/devflow-a7e2a5c74821aa39"
```

This confirms the test genuinely exercises the vulnerability (the observed path contains the real home directory `/var/home/denniyahh`) before the fix, and passes cleanly after (`1 passed; 0 failed`).

**T-19-09 (surfaced, not absorbed):** the sibling `worktree` field in the same `workflow_started_payload` JSON literal also carries a full path (a project-relative working directory the operator chose via `--worktree`). Per the threat register's explicit `accept` disposition and the plan's own instruction, this was **not** touched — D-15 names only `exe_path`. Recorded here as the required surfaced-not-absorbed finding for any future follow-up.

## Verification Results

1. `cargo test -p devflow workflow_started_payload_carries_build_provenance` → `test result: ok. 1 passed; 0 failed` (unittest binary target; other filtered targets report `0 passed; 0 filtered out` correctly, per the known `cargo test --exact`/name-filter false-green trap — confirmed the actual unittests target ran `1 test`, not `0`).
2. `cargo test --workspace` → 435 total passed across all targets, 0 failed (up from the 424 baseline recorded before Phase 19; 19-01 and this plan both added tests since).
3. `cargo clippy --workspace --all-targets -- -D warnings` → exit 0, no warnings.
4. `cargo fmt --check` → exit 0.
5. `git diff --stat crates/devflow-cli/src/main.rs` across the whole plan (both commits) → confined to exactly two hunks: `workflow_started_payload`'s body and the one test function. No other function in the diff.
6. Source assertions: `file_name` count in the production function region = `1` (correct). `exe_path` count in the combined `rg -A14` regions = `3` (see Deviations item 2 — a pre-existing grep-pattern ambiguity, not a functional gap).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 19a (`.devflow/` artifact hygiene) is now fully closed: 19-01 stopped the file from being swept into a user's commit (WR-01), and this plan stops the sensitive data from being written in the first place (WR-02).
- This was the last plan to touch `main.rs` before the wave-2 split begins (19-06 onward) — the diff against `main.rs` stayed small and confined to two regions, per D-20.
- T-19-09 (the `worktree` field's full-path exposure) remains open and accepted — available as a future finding if priorities change, not part of this milestone's scope.

---
*Phase: 19-release-integrity-main-rs-decomposition*
*Completed: 2026-07-22*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/src/main.rs
- FOUND: 671b97b (test commit)
- FOUND: 3dd9185 (fix commit)
