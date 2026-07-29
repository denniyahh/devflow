---
phase: 26-release-cut-automation
plan: 02
subsystem: release-automation
tags: [changelog, conventional-commits, git-conventional, keep-a-changelog, rust]

# Dependency graph
requires:
  - phase: 25-end-to-end-dogfood-blockers
    provides: "classify_range_bump / release_range_start / reachable_semver_baseline (the conventional-commit range-resolution machinery this plan's changelog generator reuses verbatim)"
provides:
  - "ChangelogHeading enum + changelog_sections/render_changelog_body (devflow_core::version), grouping baseline..HEAD commits into Breaking/Added/Fixed/Changed"
  - "sanitize_changelog_subject + CHANGELOG_SUBJECT_MAX_CHARS, neutralizing control characters and bounding length before commit-derived text reaches CHANGELOG.md or a tracing line"
  - "prepend_changelog(existing, version, date, body) — real generated content replaces the hardcoded 'Released phase via DevFlow.' placeholder"
  - "HookContext.shipped_changelog_body — the VersionBump -> ChangelogAppend handoff, computed strictly before the release tag is created"
affects: [26-01, 26-03]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Sibling function, not a modification: changelog_sections walks the identical git log --no-merges <range> --format=%H%x1f%B%x1e argv and git_conventional::Commit::parse call as classify_range_bump, but collects subjects into groups instead of folding to a single Bump value (RESEARCH.md Pitfall 1)."
    - "Compute-before-mutate ordering: version_bump computes the changelog body before git.tag() creates the release tag, mirroring the shipped_version/GAP-7 precedent — once the tag exists, the range this body was computed over collapses to empty."
    - "Sanitize-at-the-boundary: sanitize_changelog_subject is called at every changelog_sections push site (not once at the end), mirroring pipeline_outcomes.rs's render_gate_context precedent for contributor/attacker-influenced text."

key-files:
  created: []
  modified:
    - crates/devflow-core/src/version.rs
    - crates/devflow-core/src/ship.rs
    - crates/devflow-core/src/hooks.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs

key-decisions:
  - "Task 1 implemented changelog_sections with a deliberately minimal classification (feat->Added, everything else->Changed) so Task 2's tests could exercise a real RED->GREEN cycle against the complete per-type table, per the plan's own instruction that Task 1 only needs to exercise the feat/Added and everything-else arms."
  - "Used cargo test --workspace <name> -- --exact instead of cargo test -p devflow-core <name> -- --exact for all local verification: -p devflow-core alone does not enable the test-support feature devflow-cli's dev-dependency turns on via workspace feature unification, and fails to compile 3 integration-test targets. cargo test --workspace (matching scripts/check.sh's actual invocation) resolves features correctly. This is a pre-existing environment quirk unrelated to this plan's changes, confirmed by reproducing the same failure against the pre-plan commit."
  - "changelog_sections_treats_unparseable_messages_as_changed uses a message that genuinely fails git_conventional::Commit::parse (not the plan's literal 'wip: no colon here at all' example), because that string was empirically confirmed to parse successfully as a recognized-but-unlisted type 'wip' — using it would not exercise the parse-failure branch the test name and <behavior> intent both require."

requirements-completed: ["999.5"]

coverage:
  - id: D1
    description: "changelog_sections groups baseline..HEAD commits by ChangelogHeading (Breaking/Added/Fixed/Changed), covering every conventional-commit type the Phase 25 classifier recognizes plus breaking markers and unparseable messages"
    requirement: "999.5"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::changelog_sections_maps_every_recognized_type"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::changelog_sections_routes_breaking_changes_to_their_own_heading"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::changelog_sections_treats_unparseable_messages_as_changed"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::changelog_sections_returns_no_sections_for_an_empty_range"
        status: pass
    human_judgment: false
  - id: D2
    description: "prepend_changelog renders the generated body under the version heading, and the ChangelogAppend hook writes it end-to-end into CHANGELOG.md via a real VersionBump -> ChangelogAppend run"
    requirement: "999.5"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/ship.rs#ship::tests::prepend_changelog_uses_the_generated_body"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/hooks.rs#hooks::tests::changelog_append_writes_the_generated_body_end_to_end"
        status: pass
    human_judgment: false
  - id: D3
    description: "Commit-derived changelog text is neutralized of control characters and length-capped before reaching CHANGELOG.md, and the hardcoded placeholder bullet is fully retired"
    requirement: "999.5"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::sanitize_changelog_subject_neutralizes_controls_and_caps_length"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#version::tests::changelog_sections_sanitizes_subjects_before_grouping"
        status: pass
      - kind: other
        ref: "grep -rn 'Released phase via DevFlow' crates/ (no matches, exit 1)"
        status: pass
    human_judgment: false

duration: 40min
completed: 2026-07-29
status: complete
---

# Phase 26 Plan 02: CHANGELOG Content Generation Summary

**A DevFlow-written CHANGELOG entry now names what actually changed, grouped by Keep-a-Changelog heading (Breaking/Added/Fixed/Changed), derived from the exact same `baseline..HEAD` commit range and `git_conventional` parser the version-bump step already computes over — replacing the hardcoded `"Released phase via DevFlow."` bullet.**

## Performance

- **Duration:** ~40 min
- **Started:** 2026-07-29T20:57:29Z (session start, per STATE.md)
- **Completed:** 2026-07-29T21:28:23Z
- **Tasks:** 3
- **Files modified:** 4 (`version.rs`, `ship.rs`, `hooks.rs`, `pipeline_outcomes.rs`)

## Accomplishments

- `ChangelogHeading` enum (`Breaking`/`Added`/`Fixed`/`Changed`, declaration order = render order) plus `changelog_sections` and `render_changelog_body` — sibling functions to `classify_range_bump`, not modifications, per RESEARCH.md Pitfall 1's explicit warning against treating `classify_range_bump`'s aggregate `Bump` value as changelog content
- Complete per-type mapping locked with per-case tests: `feat`→Added, `fix`/`perf`→Fixed, breaking markers (`!` and `BREAKING CHANGE:` footer, both forms)→Breaking (checked before the type match), every other recognized type and unparseable messages→Changed
- `sanitize_changelog_subject`/`CHANGELOG_SUBJECT_MAX_CHARS` (200) neutralize control characters and bound length at every `changelog_sections` push site, mirroring `pipeline_outcomes.rs`'s `render_gate_context` precedent for the same attacker/contributor-influenced-text threat class (T-26-05)
- `prepend_changelog` gained a fourth `body: &str` parameter and a "no changes recorded" fallback line, with the header-insertion/no-header-fallback branches left byte-for-byte unchanged
- `HookContext.shipped_changelog_body` carries the generated body from `version_bump` to `changelog_append`, computed strictly before `git.tag(&tag)` (T-26-11) so the range is never collapsed by the tag it's about to create
- End-to-end proof: a fixture repo with one `feat:` commit produces a real `CHANGELOG.md` containing `### Added` and that commit's subject, written by the actual `ChangelogAppend` hook through the actual `prepend_changelog` — reverting only the `version_bump` body-capture hunk reproduces the documented RED failure (missing-`### Added` assertion, not a compile error)

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end generated changelog body — one commit type, every layer** - `a04b1bf` (feat, tdd tracer)
2. **Task 2: Complete the conventional-type to heading mapping (D-12)** - `b7252c3` (feat, tdd)
3. **Task 3: Sanitize commit-derived changelog text (ASVS V7) and retire the placeholder** - `b62d3e6` (feat, tdd)

_TDD flow per task: tests + implementation landed together per task commit (RED confirmed locally before each GREEN commit, per the acceptance criteria's explicit RED-message requirement); no separate `test(...)` commits were made since each task's own commit already carries both the failing-then-passing test and its minimal implementation, matching this plan's `tdd="true"` tasks' single-commit-per-task shape rather than the plan-level RED/GREEN/REFACTOR gate (this plan's frontmatter is `type: execute`, not `type: tdd`)._

## Files Created/Modified

- `crates/devflow-core/src/version.rs` - `ChangelogHeading`, `changelog_sections`, `render_changelog_body`, `sanitize_changelog_subject`, `CHANGELOG_SUBJECT_MAX_CHARS`, plus 8 new tests
- `crates/devflow-core/src/ship.rs` - `prepend_changelog` gains a `body: &str` fourth parameter and a fallback line; 2 existing tests updated for the new arity, 1 new test added
- `crates/devflow-core/src/hooks.rs` - `HookContext.shipped_changelog_body`; `version_bump` computes the body before tagging; `changelog_append` reads it; 1 new end-to-end test
- `crates/devflow-cli/src/pipeline_outcomes.rs` - `HookContext` construction site updated for the new field (`shipped_changelog_body: None`)

## Decisions Made

- Task 1's `changelog_sections` implementation was deliberately minimal (feat→Added, everything else→Changed) rather than pre-implementing the full mapping, so Task 2's four new tests could exercise a genuine RED (2 of 4 failed against Task 1's code, with a legible assertion mismatch, not a compile error) before Task 2's GREEN implementation.
- Local verification used `cargo test --workspace <name> -- --exact` rather than the plan's literal `cargo test -p devflow-core <name> -- --exact`: `-p devflow-core` alone does not pull in the `test-support` feature that `devflow-cli`'s dev-dependency enables via workspace feature unification, so 3 integration-test targets (`devflow_dir_gitignore.rs`, `monitor_e2e.rs`, and `git_env_hermeticity`-adjacent code) fail to compile with `cannot find test_support in devflow_core`. Confirmed via `git stash` that this failure is pre-existing and identical on the pre-plan commit — unrelated to this plan's changes. `cargo test --workspace` (which is what `scripts/check.sh`'s `cargo test --workspace --no-fail-fast` actually runs) resolves the feature correctly and was used for every full-suite run; per-test greps still confirm the required `1 passed`/`N passed` counts.
- `changelog_sections_treats_unparseable_messages_as_changed` does not use the plan's literal example string `"wip: no colon here at all"` — empirically confirmed (via a standalone `git_conventional` check) that this string parses successfully as type `"wip"` (a recognized-but-unlisted type), landing in `Changed` via rule 5, not rule 1. Since the test's name and the `<behavior>` intent both require exercising the actual parse-failure branch, the test instead uses `"just a plain message with no conventional type prefix!!!"` (the same message this codebase's own `compute_version` tests already use for this purpose), which does fail to parse. The observable outcome (`Changed`) is identical either way, so this substitution changes only which code path the test proves, not what it asserts.

## Deviations from Plan

None (Rules 1-4) requiring a code change beyond the plan's own written spec — the two items above are test-fixture and verification-methodology adjustments, not behavior changes, and are recorded as Decisions rather than deviations since they don't alter what was built.

## Issues Encountered

None beyond the two verification-methodology notes above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 999.5 (changelog placeholder) is fully closed: every conventional-commit type maps to a named heading, breaking changes get their own heading regardless of type, unparseable messages still contribute a bullet, commit-derived text is sanitized and bounded, and the body is captured before the tag exists.
- This plan's `ChangelogHeading`/`changelog_sections`/`render_changelog_body`/`sanitize_changelog_subject` are ready for 26-01/26-03 (the sync subcommand and the `--yes-release` executor) to consume unchanged — no further work on this file's public surface is implied by this plan.
- `prepend_changelog`'s new fourth parameter is a breaking signature change to a function this crate's own tests already covered; any other in-tree caller beyond `hooks.rs::changelog_append` would need updating, but `hooks.rs` was confirmed to be the only production call site.

---
*Phase: 26-release-cut-automation*
*Completed: 2026-07-29*

## Self-Check: PASSED

All 4 modified files and this SUMMARY.md file confirmed present on disk. All 3 task commit hashes (`a04b1bf`, `b7252c3`, `b62d3e6`) confirmed present in `git log --oneline --all`.
