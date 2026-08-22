---
phase: 17-pipeline-dogfood-followup
plan: 13
subsystem: infra
tags: [rust, semver, git-hooks, changelog, versioning]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup (17-12)
    provides: ChangelogAppend reordered strictly after VersionBump in hooks_after_ship(), reads version::read_version instead of compute_version, commits its own write
provides:
  - replace_version_in_contents preserves the line remainder (trailing comma/comment) after the version token
  - HookContext.shipped_version threads the tag VersionBump cut into ChangelogAppend within one batch run
  - Hook::run takes &mut HookContext; run_checkout_hooks hoists context construction above the batch loop
affects: [17-VALIDATION.md row 12, any future phase touching hooks.rs or version.rs]

tech-stack:
  added: []
  patterns:
    - "Batch-scoped context state (shipped_version) instead of round-tripping a value through disk between hooks in the same run"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/version.rs
    - crates/devflow-core/src/hooks.rs
    - crates/devflow-cli/src/main.rs
    - .planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md

key-decisions:
  - "changelog_append's fallback chain is shipped_version -> read_version -> the unreleased literal, in that order, preserving the existing fallback for callers outside the after-ship batch"
  - "init_repo delegates to a new init_repo_with_options(root, write_version_file) rather than being edited in place, so every existing test's fixture is byte-for-byte unchanged"
  - "Hook::run's signature change to &mut HookContext was scoped to the minimum needed: branch_create/branch_cleanup/merge_feature/docs_update still take &HookContext internally via Rust's &mut-to-& reborrow coercion at the call site"

requirements-completed: [17d]

coverage:
  - id: D1
    description: "replace_version_in_contents preserves whatever follows the version token on the matched line (JSON trailing comma, TOML trailing comment)"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#write_version_preserves_trailing_comma_in_package_json"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#write_version_preserves_trailing_comment_in_toml"
        status: pass
    human_judgment: false
  - id: D2
    description: "version_bump hands the tagged version to changelog_append via batch-scoped context state, so the after-ship batch's tag and changelog heading stay in sync even with no version file present"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/hooks.rs#after_ship_batch_with_no_version_file_keeps_tag_and_changelog_in_sync"
        status: pass
    human_judgment: false
  - id: D3
    description: "17-VALIDATION.md records GAP-6 and GAP-7 as closed with their fix commits, and row 12 no longer reads partial"
    requirement: "17d"
    verification:
      - kind: other
        ref: ".planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md (GAP-6/GAP-7 sections, row 12) — reviewed directly, not automatable"
        status: pass
    human_judgment: false

duration: 15min
completed: 2026-07-20
status: complete
---

# Phase 17 Plan 13: GAP-6/GAP-7 Gap Closure Summary

**Fixed two live defects (silent JSON/TOML corruption on version write, and a tag/changelog desync with no version file) whose row-12 tests were vacuous against them; both fixes are RED-proven.**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-07-20 (session start)
- **Completed:** 2026-07-20T08:45:08Z
- **Tasks:** 3 completed
- **Files modified:** 4

## Accomplishments

- **GAP-6 closed:** `replace_version_in_contents` (`version.rs`) now captures and re-emits whatever
  follows the version token on the matched line — a JSON trailing comma or a TOML trailing comment —
  instead of silently discarding it. Previously, writing a version into any multi-key `package.json`
  where `version` was not the last key produced invalid JSON.
- **GAP-7 closed:** `version_bump` now hands the version it actually tagged to `changelog_append`
  through a new `HookContext.shipped_version: Option<String>` field, instead of `changelog_append`
  re-deriving the version from disk via `version::read_version` (which errors with no version file
  and silently falls back to the `"unreleased"` literal, desyncing the changelog heading from the
  git tag). `Hook::run` now takes `&mut HookContext`; `run_checkout_hooks` in `main.rs` hoists the
  `HookContext` construction above the batch loop instead of rebuilding a fresh, state-discarding
  context per hook iteration. The terminal-batch fail-fast (`terminal_batch && outcome.is_err()`)
  is unchanged.
- **17-VALIDATION.md updated:** both GAP sections marked closed with their fix commits and RED-proof
  summaries; row 12 restored from `⚠️ partial` to `✅ green` with the three new covering tests added.
  `nyquist_compliant` left untouched per the plan's explicit instruction — that flag is the
  auditor's to set.

## Task Commits

Each task was committed atomically:

1. **Task 1: GAP-6 — preserve trailing comma/comment in `replace_version_in_contents`** - `12b5b98` (fix)
2. **Task 2: GAP-7 — thread `shipped_version` through `HookContext`** - `e421ebd` (fix)
3. **Task 3: Record the disposition in `17-VALIDATION.md`** - `99ee090` (docs)

**Plan metadata:** commit pending (this SUMMARY + STATE.md/ROADMAP.md/REQUIREMENTS.md)

## Files Created/Modified

- `crates/devflow-core/src/version.rs` - `replace_version_in_contents` remainder-preservation fix + 2 new RED-proven tests
- `crates/devflow-core/src/hooks.rs` - `HookContext.shipped_version`, `Hook::run(&mut HookContext)`, `version_bump`/`changelog_append` threading, `init_repo_with_options`, 1 new RED-proven test, all existing call sites updated for the `&mut` signature
- `crates/devflow-cli/src/main.rs` - `run_checkout_hooks` hoists `HookContext` construction above the batch loop
- `.planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md` - GAP-6/GAP-7 closure records, row 12 restored to green

## Decisions Made

- Kept `changelog_append`'s existing three-level fallback chain intact (batch-scoped `shipped_version`,
  then `version::read_version`, then the `"unreleased"` literal) rather than removing the disk-read
  fallback — callers of `changelog_append` outside `hooks_after_ship()`'s batch (if any exist in the
  future) still get a best-effort version.
- Refactored `init_repo` in `hooks.rs` tests to delegate to `init_repo_with_options(root,
  write_version_file: bool)` rather than editing it in place, per the plan's explicit requirement
  that every existing test's fixture stay byte-for-byte unchanged.
- Left `branch_create`, `branch_cleanup`, `merge_feature`, and `docs_update` at `&HookContext` (not
  `&mut`) since none of them need to write `shipped_version` — Rust's implicit `&mut T -> &T`
  reborrow coercion at the `Hook::run` call site lets this work without touching their signatures,
  keeping the diff minimal.

## Deviations from Plan

None — plan executed exactly as written. Both RED tests were captured against the unfixed
implementation (real assertion failures, not compile errors) before their respective fixes landed,
per the plan's explicit RED-before-GREEN requirement.

## Issues Encountered

None. One verification wrinkle worth recording: this plan's frontmatter states the baseline as
"376 passed / 0 failed," while `17-12-SUMMARY.md` had separately recorded "377 passed." To resolve
the ambiguity honestly rather than assume either number, a temporary detached worktree was created
at `HEAD~1` (the commit immediately preceding this plan's changes) and `cargo test --workspace` was
run there directly: **376 passed, 0 failed** — confirming the plan's stated baseline was the
accurate one for this exact environment. The worktree was removed immediately after
(`git worktree remove --force`); no other git history was touched.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Both `write_version` (any format) and the after-ship hook batch (with or without a version file)
  are now correct and covered by RED-proven regression tests.
- `/gsd-validate-phase 17` should be re-run to re-audit `17-VALIDATION.md` and set
  `nyquist_compliant` on its own evidence, per the plan's explicit instruction that this plan does
  not self-certify that flag.
- No blockers identified for Phase 18 (Hermes Support).

---

## Verification

**`cargo test --workspace`:** 379 passed, 0 failed, 0 ignored, 0 filtered — across 10 targets
(baseline 376 + 3 new tests: `write_version_preserves_trailing_comma_in_package_json`,
`write_version_preserves_trailing_comment_in_toml`, `after_ship_batch_with_no_version_file_keeps_tag_and_changelog_in_sync`).

**`cargo clippy --workspace --all-targets -- -D warnings`:** clean, no warnings.

**`cargo fmt --check`:** clean (after one `cargo fmt` pass to normalize two lines the plan's new
test code introduced).

### RED output captured (GAP-6, before the fix)

```
thread 'version::tests::write_version_preserves_trailing_comma_in_package_json' panicked:
package.json no longer parses as JSON: expected `,` or `}` at line 4 column 3
{
  "name": "x",
  "version": "2.3.4"
  "private": true
}

thread 'version::tests::write_version_preserves_trailing_comment_in_toml' panicked:
expected trailing comment to survive, got: [package]
version = "2.3.4"

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 284 filtered out
```

### RED output captured (GAP-7, before the fix)

```
thread 'hooks::tests::after_ship_batch_with_no_version_file_keeps_tag_and_changelog_in_sync' panicked:
assertion `left != right` failed: changelog heading must name the tagged version, not fall back to the literal
  left: "unreleased"
 right: "unreleased"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 286 filtered out
```

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-20*

## Self-Check: PASSED

All files confirmed present on disk and all task commit hashes confirmed present in `git log`:
- `crates/devflow-core/src/version.rs` — FOUND
- `crates/devflow-core/src/hooks.rs` — FOUND
- `crates/devflow-cli/src/main.rs` — FOUND
- `.planning/phases/17-pipeline-dogfood-followup/17-VALIDATION.md` — FOUND
- `.planning/phases/17-pipeline-dogfood-followup/17-13-SUMMARY.md` — FOUND
- `12b5b98` — FOUND
- `e421ebd` — FOUND
- `99ee090` — FOUND
