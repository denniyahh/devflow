---
phase: 20-release-correctness-operator-control
plan: 01
subsystem: release-tooling
tags: [rust, cargo, semver, versioning, release-correctness]

# Dependency graph
requires:
  - phase: 19-release-integrity-main-rs-decomposition
    provides: commit_path empty-commit guard, .devflow/ artifact hygiene (unrelated but same release-hook surface)
provides:
  - "write_version rewrites every local-path [workspace.dependencies] self-pin alongside [workspace.package] version, in one write"
  - "PR #17 regression guard (workspace_version_pin.rs) is now green by construction — no manual pin edit required on release"
affects: [20-04-release-preflight, ship-hooks, versionbump]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Additive-pass rewrite: a second, independent scan over [workspace.dependencies] runs after the existing single-field replace_version_in_contents, rather than modifying that function's starts_with('{') guard — keeps single-field callers (pyproject.toml/package.json/plain Cargo.toml) untouched."
    - "Token-anchored inline-table rewrite: locate path=/version= sub-values by splitting the inline table on top-level commas and matching each fragment's key, not by column offset — makes the rewrite key-order-independent."

key-files:
  created: []
  modified:
    - crates/devflow-core/src/version.rs

key-decisions:
  - "Hand-rolled the inline-table split (comma-delimited fragments with byte offsets) instead of pulling in the `toml` crate for this rewrite, even though `toml` is already a workspace dependency — its serializer would not preserve GAP-6 formatting (comments, quote style, trailing commas) that this project's existing single-field rewrite already guarantees."
  - "Scoped the additive pass to run only when field_for() resolves to workspace.package.version — pyproject.toml/package.json/plain-package Cargo.toml never reach the new code path."

requirements-completed: [20a]

coverage:
  - id: D1
    description: "write_version rewrites both [workspace.package] version and every local-path [workspace.dependencies] self-pin in one write"
    requirement: "20a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::write_version_rewrites_workspace_dependency_self_pin"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/workspace_version_pin.rs#workspace_member_pins_match_the_workspace_version"
        status: pass
    human_judgment: false
  - id: D2
    description: "Third-party (path-less) deps and non-workspace file formats are provably untouched"
    requirement: "20a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::write_version_leaves_third_party_version_only_dep_untouched"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::inline_table_version_does_not_shadow_workspace_package"
        status: pass
    human_judgment: false
  - id: D3
    description: "Empty/no-version-key edges no-op cleanly without panic; comment/quote style preserved; key-order independence proven"
    requirement: "20a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::write_version_no_ops_on_missing_workspace_dependencies_section"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::write_version_no_ops_on_member_with_no_version_key"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::write_version_preserves_comment_and_quote_in_workspace_dependency_pin"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::write_version_rewrites_self_pin_regardless_of_key_order"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-07-23
status: complete
---

# Phase 20 Plan 01: VersionBump Workspace Self-Pin Rewrite Summary

**`write_version` now rewrites both `[workspace.package] version` and every local-path `[workspace.dependencies]` self-pin in one write, via an additive, key-order-independent inline-table pass — closing the defect that shipped `cargo publish`-breaking stale pins on v1.5.0 and v1.6.0.**

## Performance

- **Duration:** 8 min
- **Started:** 2026-07-23T04:22:00-04:00 (approx, worktree checkout)
- **Completed:** 2026-07-23T04:29:31-04:00
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments
- `write_version` (`crates/devflow-core/src/version.rs`) rewrites the `[workspace.package] version` field AND every `[workspace.dependencies]` entry pinning a local workspace member by `path`, in one write — the second, additive pass only runs when the version file resolves to a workspace `Cargo.toml`.
- The rewrite is anchored to the `path=`/`version=` tokens within each single-line inline table (not column offsets), so it correctly handles `version` appearing either before or after `path`.
- Third-party (path-less) dependencies and non-workspace version files (`package.json`, `pyproject.toml`, plain-package `Cargo.toml`) are provably untouched.
- The pre-existing PR #17 regression guard (`crates/devflow-cli/tests/workspace_version_pin.rs`) now passes with zero manual pin edits — the exact scenario that shipped broken twice (v1.5.0, v1.6.0) is fixed by construction.
- 5 new unit tests in `version.rs` plus the untouched guard lock the empty-section, no-version-key, adjacency, comment/quote-preservation, and key-order edges.

## Task Commits

Each task was committed atomically:

1. **Task 1: RED — prove write_version leaves the self-pin stale** - `4b8bbbe` (test)
2. **Task 2: Additive inline-table rewrite pass — make RED + guard green** - `2a9a5f3` (feat)
3. **Task 3: Edge coverage — empty section, no-version member, quote/comment preservation** - `bc32d5a` (test)

_TDD tasks: test (RED) → feat (GREEN) → test (edge coverage), all three GREEN on first run after implementation._

## Files Created/Modified
- `crates/devflow-core/src/version.rs` - Added `rewrite_workspace_member_pins`, `inline_table_fragments`, `workspace_dependency_has_local_path`, `rewrite_inline_table_version` helpers, wired into `write_version`; added 6 new tests (1 RED-proving, 5 edge-locking).

## Decisions Made
- Hand-rolled the inline-table token split (comma-delimited fragments + byte offsets) rather than using the `toml` crate (already a workspace dependency) for this rewrite — the `toml` serializer would not preserve GAP-6 formatting guarantees (comments, quote style, trailing commas/whitespace) that the existing single-field `replace_version_in_contents` already provides and that this new pass must match.
- Scoped the additive pass strictly to `field == "workspace.package.version"` so `pyproject.toml`/`package.json`/plain-package `Cargo.toml` callers never execute the new code path — matches the plan's read_first scoping note.

## Deviations from Plan

None - plan executed exactly as written. All three tasks (RED, additive rewrite pass, edge coverage) match their `<action>` and `<behavior>` specs; no architectural changes, no missing critical functionality found, no blocking issues encountered.

## Issues Encountered

`cargo fmt` reformatted two multi-line `.contains(...)` assertions onto single lines that exceeded the reader's expectation of pre-formatted style (Task 2 and Task 3 commits both required a `cargo fmt` pass before commit). No functional change; re-ran the full test suite after each formatting pass to confirm no regression.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 20a is fully resolved: the `VersionBump` ship hook (`hooks::version_bump`, unmodified — it calls `write_version` once and commits the same `Cargo.toml`) will now capture both the package version and every local self-pin in a single commit on the next release.
- 20d (`devflow release --check` preflight) was noted in ROADMAP.md as "Blocks on 20a" — that dependency is now satisfied; 20d's preflight can assume `write_version` produces a self-consistent workspace `Cargo.toml`.
- No blockers for downstream plans in this phase (20-02 through 20-05 touch disjoint files per the wave-1 parallel dispatch).

---
*Phase: 20-release-correctness-operator-control*
*Completed: 2026-07-23*

## Self-Check: PASSED

- FOUND: crates/devflow-core/src/version.rs
- FOUND: crates/devflow-cli/tests/workspace_version_pin.rs
- FOUND: .planning/phases/20-release-correctness-operator-control/20-01-SUMMARY.md
- FOUND commit: 4b8bbbe (test: RED)
- FOUND commit: 2a9a5f3 (feat: additive rewrite pass)
- FOUND commit: bc32d5a (test: edge coverage)
- FOUND commit: 6a7d2d6 (docs: summary)
