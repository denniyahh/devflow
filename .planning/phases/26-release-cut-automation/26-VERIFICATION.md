---
phase: 26-release-cut-automation
verified: 2026-07-29T23:45:00Z
status: gaps_found
score: 6/11 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "`devflow release` executes the release-cut sequence (version bump -> direct push develop -> release PR -> signed tag -> sync -> publish), not just the read-only `--check` preflight."
    status: failed
    reason: "`devflow release` (crates/devflow-cli/src/main.rs:572-586) still hard-rejects any invocation without `--check` with the message \"the release-cut executor (merge PR -> tag -> sync develop -> publish) is deferred (DEN-50) and not yet built.\" Behavior is byte-identical to Phase 20's 20d --check-only preflight; this is the phase's headline deliverable and it does not exist."
    artifacts:
      - path: "crates/devflow-cli/src/main.rs"
        issue: "Command::Release has no --execute path; only `check: bool` exists, and a false value is rejected outright."
    missing:
      - "A `--execute` code path on `devflow release` that actually runs version bump -> push -> tag -> sync -> publish."
      - "The `Release { execute, yes_release }` fields and `--execute`/`--yes-release` flags that 26-03-PLAN.md's own \"Artifacts this phase produces\" section declares as in-scope for this phase."
  - truth: "A `devflow sync` subcommand exists, both standalone and executor-internal (999.52)."
    status: failed
    reason: "No `crates/devflow-core/src/sync.rs` file exists anywhere in the repository. `crates/devflow-core/src/lib.rs` has no `mod sync;` declaration. `crates/devflow-cli/src/main.rs`'s `Command` enum has no `Sync` variant. `commands.rs` has no `sync_cmd`. Every plan in this phase (26-02-PLAN.md, 26-03-PLAN.md) lists `devflow_core::sync::{SyncError, SyncOutcome, SYNC_MERGE_MESSAGE, sync_main_to_develop}` and `Command::Sync { project }` under \"Artifacts this phase produces\" as in-scope for Phase 26 — none of it was built."
    artifacts:
      - path: "crates/devflow-core/src/sync.rs"
        issue: "MISSING — file does not exist"
    missing:
      - "crates/devflow-core/src/sync.rs with SyncError, SyncOutcome, SYNC_MERGE_MESSAGE, sync_main_to_develop"
      - "Command::Sync { project } CLI variant and sync_cmd in commands.rs"
  - truth: "A `--yes-release` flag exists, separate from `--yes-ship`, that per-invocation authorizes the bump->tag->sync->publish sequence (mirrors the `--yes-ship` dangerous-operation pattern)."
    status: failed
    reason: "`rg -n \"yes.release|yes_release\" crates/ -g '*.rs'` returns zero matches anywhere in the workspace. The flag referenced in ROADMAP.md's Phase 26 goal, 999.25-BACKLOG-DOSSIER.md, and both 26-02-PLAN.md/26-03-PLAN.md's must-produce artifact lists does not exist."
    missing:
      - "--yes-release CLI flag on Command::Release, independently settable per-invocation only (D-03 parity with --yes-ship)"
  - truth: "cargo publish primitives (pre-publish existence check, actual publish call) exist and are ready to be driven by the executor (999.25, D-04)."
    status: failed
    reason: "`rg -n \"cargo_publish|PublishCheck|PublishError|classify_cargo_info_result|crate_already_published\" crates/ -g '*.rs'` returns zero matches. None of the publish-side primitives 26-01/26-02/26-03's \"Artifacts this phase produces\" sections declare (`PublishCheck`, `PublishError`, `classify_cargo_info_result`, `crate_already_published`, `cargo_publish`) were written. Decision 2 in 26-01-SUMMARY.md explicitly authorized this capability, but no code implementing it exists."
    missing:
      - "classify_cargo_info_result, crate_already_published, cargo_publish, PublishCheck, PublishError in devflow_core::git"
  - truth: "`push_ref`, `release_tag_state`, and `create_signed_release_tag` (added by 26-03) are consumed by production code that assembles them into the release-cut sequence, not merely exercised by their own unit tests."
    status: failed
    reason: "`rg -n \"push_ref\" crates/ -g '*.rs'` (11 hits), `release_tag_state` (13 hits), and `create_signed_release_tag` (7 hits) show every call site is inside `crates/devflow-core/src/git.rs`'s own `#[cfg(test)] mod tests` block or a doc-comment cross-reference. No caller exists in `hooks.rs`, `commands.rs`, `main.rs`, or `ship.rs`. These three primitives are exactly as production-uncalled today as `GitFlow::push`/`delete_remote_branch` were before this phase (the state RESEARCH.md identified as the problem to fix) — the phase added new primitives without wiring any of them to a caller."
    artifacts:
      - path: "crates/devflow-core/src/git.rs"
        issue: "push_ref, release_tag_state, create_signed_release_tag exist and are substantively implemented, but have zero non-test callers — orphaned artifacts."
    missing:
      - "The executor (26-06/26-07 per 26-03-PLAN.md's own dependency notes) that calls push_ref for the version-bump push, release_tag_state + create_signed_release_tag for the tag step, and sync_main_to_develop for the sync step."
deferred: []
human_verification: []
---

# Phase 26: Release-Cut Automation Verification Report

**Phase Goal:** Make `devflow release` *execute* the release-cut sequence —
version bump -> direct push to `develop` -> develop->main release PR
(human-merged) -> signed tag -> sync back to `develop` (direct push) ->
publish `devflow-core` then `devflow` — not just the read-only `--check`
preflight Phase 20's 20d delivered. Adds a real `devflow sync` subcommand
(999.52, both standalone and executor-internal) and fixes the changelog's
placeholder content (999.5) by generating it from the conventional-commit
classification Phase 25's version-bump step already computes.

**Verified:** 2026-07-29T23:45:00Z
**Status:** gaps_found
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Operator explicitly re-authorized direct-push-to-develop capability (D-01/D-08) before any implementing code exists, with the selected option recorded | ✓ VERIFIED | `26-01-SUMMARY.md` records `direct-push` selected, with operator response text, for both the original discuss-phase session and a live re-confirmation this dogfood run |
| 2 | Operator explicitly re-authorized unattended `cargo publish` (D-04, one-way/irreversible) before any implementing code exists, with the selected option recorded | ✓ VERIFIED | `26-01-SUMMARY.md` records `automate-publish` selected, same double-confirmation |
| 3 | A DevFlow-written CHANGELOG entry lists what actually changed, derived from the same conventional-commit classification the version bump computes (999.5), replacing the hardcoded placeholder | ✓ VERIFIED | `crates/devflow-core/src/version.rs` `ChangelogHeading`/`changelog_sections`/`render_changelog_body`/`sanitize_changelog_subject` exist, are wired through `hooks.rs::version_bump` -> `ctx.shipped_changelog_body` -> `hooks.rs::changelog_append` -> `ship.rs::prepend_changelog`; `grep -rn 'Released phase via DevFlow' crates/` returns no matches; `cargo test -p devflow-core --features test-support hooks::tests::changelog_append_writes_the_generated_body_end_to_end -- --exact` passes (spot-checked live) |
| 4 | DevFlow can push a ref to `origin` with no force flag, and a rejected non-fast-forward push leaves the remote unmoved (D-05 foundation for 999.25) | ✓ VERIFIED (as primitive) | `GitFlow::push_ref` at `git.rs:243`; `cargo test --workspace git::tests::push_ref_lands_a_branch_on_the_remote -- --exact` passes (spot-checked live). See Truth 9 — never called by production code. |
| 5 | DevFlow correctly classifies an existing tag's release state, refusing to treat a stray lightweight tag as an already-cut release (D-06 foundation for 999.25) | ✓ VERIFIED (as primitive) | `ReleaseTagState`/`release_tag_state` at `git.rs:531-593`, 5 passing unit tests including the lightweight-collision case. See Truth 9 — never called by production code. |
| 6 | DevFlow can create the maintainer-signed release tag using CONTRIBUTING.md's exact documented invocation form and report git's own real result (D-10 foundation for 999.25) | ✓ VERIFIED (as primitive) | `create_signed_release_tag` at `git.rs:698`, 3 passing unit tests including the end-to-end push+release_tag_state round trip. See Truth 9 — never called by production code. |
| 7 | `devflow release` *executes* the release-cut sequence, not just the read-only `--check` preflight (the phase's headline goal) | ✗ FAILED | `main.rs:572-586`: a bare `devflow release` (omitted `--check`) is still rejected with the message "the release-cut executor ... is deferred (DEN-50) and not yet built." Behavior is unchanged from Phase 20's 20d. |
| 8 | A `devflow sync` subcommand exists, both standalone and executor-internal (999.52) | ✗ FAILED | No `crates/devflow-core/src/sync.rs`, no `mod sync;` in `lib.rs`, no `Command::Sync` variant, no `sync_cmd` — confirmed absent by direct grep/find. |
| 9 | `push_ref`, `release_tag_state`, `create_signed_release_tag` are wired into a production caller (the release executor), not only exercised by their own tests | ✗ FAILED | Every call site of all three symbols is inside `git.rs`'s own `#[cfg(test)]` module or a doc comment. Zero production callers. |
| 10 | A `--yes-release` flag exists and is required per-invocation to authorize the automated sequence | ✗ FAILED | `rg "yes.release|yes_release" crates/ -g '*.rs'` — zero matches anywhere. |
| 11 | `cargo publish` primitives (existence check, publish call) exist for the executor's final step (999.25, D-04) | ✗ FAILED | `rg "cargo_publish|PublishCheck|PublishError|classify_cargo_info_result|crate_already_published" crates/ -g '*.rs'` — zero matches. |

**Score:** 6/11 truths verified (0 present-but-behavior-unverified)

Note: Truths 1-3 are genuinely, independently complete deliverables. Truths 4-6
are real, well-tested code, but only as unwired foundation — grouped separately
from Truths 7-11 (which represent the phase's actual stated goal: an
*executing* `devflow release`, a real `devflow sync`, and the publish step)
because presence of a primitive is not the same claim as achievement of the
goal that primitive was built to serve.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/version.rs` — `ChangelogHeading`, `changelog_sections`, `render_changelog_body`, `sanitize_changelog_subject`, `CHANGELOG_SUBJECT_MAX_CHARS` | 999.5 changelog content | ✓ VERIFIED | Present, substantive (384 new lines), wired end-to-end through hooks.rs/ship.rs, data-flow confirmed by a passing integration test |
| `crates/devflow-core/src/ship.rs` `prepend_changelog(existing, version, date, body)` | 4th `body` param replacing the hardcoded bullet | ✓ VERIFIED | Signature changed as specified; 3 passing tests |
| `crates/devflow-core/src/hooks.rs` `HookContext.shipped_changelog_body` | VersionBump -> ChangelogAppend handoff | ✓ VERIFIED | Field present, populated before `git.tag()` per the documented ordering rationale |
| `crates/devflow-core/src/git.rs` `GitFlow::push_ref` | fast-forward-only push primitive | ✓ VERIFIED (substantive, tested) | ⚠️ ORPHANED — no non-test caller |
| `crates/devflow-core/src/git.rs` `ReleaseTagState`/`release_tag_state` | already-released predicate | ✓ VERIFIED (substantive, tested) | ⚠️ ORPHANED — no non-test caller |
| `crates/devflow-core/src/git.rs` `create_signed_release_tag` | signed-tag creator | ✓ VERIFIED (substantive, tested) | ⚠️ ORPHANED — no non-test caller |
| `crates/devflow-core/src/sync.rs` | new sync module (999.52) | ✗ MISSING | File does not exist |
| `crates/devflow-cli/tests/release_execute.rs` | integration tests for `--execute`/`--yes-release` | ✗ MISSING | File does not exist |
| `crates/devflow-core/src/git.rs` `PublishCheck`/`PublishError`/`classify_cargo_info_result`/`crate_already_published`/`cargo_publish` | cargo publish primitives (999.25) | ✗ MISSING | Zero matches anywhere in `crates/` |
| `crates/devflow-cli/src/main.rs` `Command::Sync`, `Command::Release{execute,yes_release}` | new CLI surface | ✗ MISSING | `main.rs` unmodified by this phase (confirmed via `git diff --stat`) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `hooks.rs::version_bump` | `ctx.shipped_changelog_body` | direct field assignment before `git.tag()` | ✓ WIRED | Ordering rationale documented; end-to-end test passes |
| `hooks.rs::changelog_append` | `ship::prepend_changelog` | 4th `body` argument | ✓ WIRED | Confirmed by passing test asserting on CHANGELOG.md bytes |
| release executor | `GitFlow::push_ref` | (does not exist) | ✗ NOT_WIRED | No executor exists to call it |
| release executor | `release_tag_state` / `create_signed_release_tag` | (does not exist) | ✗ NOT_WIRED | No executor exists to call it |
| `devflow sync` (executor-internal) | `sync_main_to_develop` | (does not exist) | ✗ NOT_WIRED | Neither side exists |
| `devflow release --execute` | version bump / push / tag / sync / publish sequence | (does not exist) | ✗ NOT_WIRED | `--execute` flag itself does not exist |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| `push_ref` lands a branch on a real (bare) remote | `cargo test --workspace git::tests::push_ref_lands_a_branch_on_the_remote -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| Generated changelog body reaches `CHANGELOG.md` end-to-end via the real hook chain | `cargo test -p devflow-core --features test-support hooks::tests::changelog_append_writes_the_generated_body_end_to_end -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| Hardcoded changelog placeholder fully retired | `grep -rn 'Released phase via DevFlow' crates/` | no matches (exit 1) | ✓ PASS |
| `devflow release` executes (vs. only `--check`) | inspection of `main.rs:572-586` | bare invocation still hard-rejected, citing DEN-50 deferred | ✗ FAIL |
| `devflow sync` subcommand present | `find crates -iname sync.rs`, `rg "Sync" crates/devflow-cli/src/main.rs` | no file, no match | ✗ FAIL |
| `--yes-release` flag present | `rg "yes.release|yes_release" crates/ -g '*.rs'` | no matches | ✗ FAIL |

### Requirements Coverage (backlog IDs, no REQ-IDs in this project)

| Backlog ID | Source Plan(s) | Description | Status | Evidence |
|------------|-----------------|-------------|--------|----------|
| 999.25 | 26-01, 26-03 | Release-cut executor: `devflow release` executes bump->push->PR->tag->sync->publish | ✗ BLOCKED | Only 3 of the ~8 git primitives an executor needs exist (`push_ref`, `release_tag_state`, `create_signed_release_tag`); none are called by any executor; no `--execute`/`--yes-release` CLI surface; `devflow release` behavior is unchanged from Phase 20. `26-01-SUMMARY.md` and `26-03-SUMMARY.md` both list `requirements-completed: ["999.25", ...]` — this is an overclaim (see Anti-Patterns below); the backlog dossier itself (`999.25-BACKLOG-DOSSIER.md`) is still marked `status: backlog`, unresolved. |
| 999.5 | 26-02 | Changelog placeholder content, generated from conventional-commit classification | ✓ SATISFIED | Fully implemented, tested end-to-end, hardcoded placeholder confirmed removed. `26-02-SUMMARY.md`'s `requirements-completed: ["999.5"]` is accurate. (Note: `999.5-BACKLOG-DOSSIER.md` frontmatter is still `status: backlog` — a documentation-lag issue, not a functional gap.) |
| 999.52 | (declared in 26-02/26-03 plan frontmatter's `requirements`, but never given its own implementing plan) | `devflow sync` subcommand, standalone and executor-internal | ✗ BLOCKED | Zero implementation. `26-01-SUMMARY.md` lists `requirements-completed: ["999.25", "999.52"]` for a plan whose own frontmatter states `files_modified: []` and that produced no code — this is a clear overclaim (see Anti-Patterns below). |

**Orphaned requirements check:** ROADMAP.md's Phase 26 section names only 999.25, 999.5, and 999.52 in scope (999.54/999.50/999.4 explicitly dropped, 999.31/999.15/999.21 explicitly declined). No orphaned requirements found beyond the three already covered above.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `.planning/phases/26-release-cut-automation/26-01-SUMMARY.md` | 30 | `requirements-completed: ["999.25", "999.52"]` on a plan with `files_modified: []` and zero code commits | 🛑 Blocker-adjacent (self-report overclaim) | A plan that only records an authorization decision cannot mark two large, unimplemented backlog items "completed." Both 999.25 and 999.52 remain functionally open at phase end. This SUMMARY field, if trusted uncritically by a downstream `/gsd-ship` or `/gsd-complete-milestone` pass, would misreport the phase as having closed backlog items it did not close. |
| `.planning/phases/26-release-cut-automation/26-03-SUMMARY.md` | 37 | `requirements-completed: ["999.25"]` after building 3 of ~8 needed primitives, none wired to a caller | ⚠️ Warning (partial-progress overclaim) | 26-03's own `coverage` block (D1/D2/D3) is scoped and accurate as *plan-level* coverage of the primitives it built — but the top-level `requirements-completed: ["999.25"]` field reads as "this backlog item is done," which it is not: no executor exists yet that calls any of these primitives, and the publish-side primitives were never written at all. |
| `crates/devflow-core/src/git.rs` | 243, 593, 698 | `push_ref`, `release_tag_state`, `create_signed_release_tag` have zero non-test callers | ⚠️ Warning (orphaned artifact) | Functionally identical to the "zero non-test callers" defect RESEARCH.md identified in the pre-phase `GitFlow::push`/`delete_remote_branch` methods — this phase added new primitives in the same unwired state rather than closing that class of gap. |
| N/A | N/A | No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers found in any of the 5 files this phase modified (`git.rs`, `version.rs`, `ship.rs`, `hooks.rs`, `pipeline_outcomes.rs`) | ℹ️ Info | Consistent with `26-REVIEW.md`'s own finding of "no `TODO`/debug artifacts." |

`26-REVIEW.md` (code review, standard depth) independently found 0 critical, 3 warning, 3 info findings in the code actually written (WR-01 Validate-outcome status/verdict decoupling in `pipeline_outcomes.rs`, WR-02 misleading `merged: false` event on the idempotent merge no-op, WR-03 TOML comment-line scanning gap in workspace-pin rewriting) — these are pre-existing-file issues incidentally touched by this phase's edits, not new debt this phase introduced, and are out of scope for this phase's own goal (they affect Validate/Merge/version-bump machinery, not the release-cut sequence). Recorded here for completeness; not treated as phase-blocking for Phase 26's own goal.

### Human Verification Required

None. Every gap above is directly observable in the codebase (missing files, missing CLI variants, zero call sites) and required no runtime/visual/external-service judgment to resolve. The `26-VALIDATION.md`'s two documented "Manual-Only Verifications" (real signing-key tag creation, real `cargo publish`) are moot for this verification pass since the code paths that would exercise them (`create_signed_release_tag`'s production caller, `cargo_publish`) were never built.

### Gaps Summary

Phase 26's own upfront planning artifacts (26-01-PLAN.md's `key_links`, 26-03-PLAN.md's
`key_links` and "Artifacts this phase produces" section, and 26-VALIDATION.md's
Per-Task Verification Map) all describe a seven-plan arc: 26-01 (authorization) ->
26-02 (changelog) -> 26-03 (git primitives) -> 26-04 (sync module) -> 26-05
(publish primitives) -> 26-06 (executor's bump/push/tag step) -> 26-07
(executor's publish step, `--execute`/`--yes-release` CLI surface). Only 26-01
through 26-03 were ever planned into `-PLAN.md` files and executed. ROADMAP.md's
own Phase 26 entry (`Plans: 3/3 plans executed`, followed by
`- [ ] TBD (run /gsd-discuss-phase 26, then /gsd-plan-phase 26 to break down)`)
independently confirms the phase was left mid-arc, not closed.

The result: **999.5 (changelog) is genuinely, fully done.** 999.25 (the executor)
and 999.52 (`devflow sync`) — the two items the phase's own goal statement leads
with — are not done. `devflow release` today behaves identically to Phase 20's
`--check`-only preflight; no `devflow sync` subcommand exists in any form; no
`--yes-release` flag exists; and the three new git primitives 26-03 built
(`push_ref`, `release_tag_state`, `create_signed_release_tag`) are fully tested
but called by nothing outside their own test module. Two SUMMARY.md files
(`26-01-SUMMARY.md`, `26-03-SUMMARY.md`) mark `requirements-completed: 999.25`
and/or `999.52` in a way that overstates what was actually delivered against
those backlog IDs — both remain open by the codebase's own evidence, and both
backlog dossiers are still frontmatter-tagged `status: backlog`.

This is not a "gaps in an otherwise-complete phase" situation — it is a phase
that stopped after its first three (of seven forward-referenced) plans. The
recommended path is `/gsd-plan-phase 26 --gaps` to plan the remaining
26-04..26-07 work (sync module, publish primitives, and the executor itself
that composes everything already built), rather than treating this as a small
fix-up pass.

---

*Verified: 2026-07-29T23:45:00Z*
*Verifier: Claude (gsd-verifier)*
