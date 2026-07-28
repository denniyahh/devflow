---
phase: 25-end-to-end-dogfood-blockers
plan: 01
subsystem: infra
tags: [rust, semver, git-conventional, version-derivation, release-tooling]

# Dependency graph
requires: []
provides:
  - "reachable_semver_baseline / highest_semver_tag: D-07 reachable-tag baseline resolution over 'git tag --merged HEAD'"
  - "release_range_start: D-08 commit-range anchor across squash-merge + sync-back release topology"
  - "classify_range_bump / Bump: conventional-commit precedence classification via git-conventional"
  - "compute_version rewritten to compose the above; version file no longer an input (D-11)"
  - "VersionError::UnreachableBaseline: D-10 refuse-on-unreachable-highest-tag, naming the tag and repair command"
  - "count_git_tags / commits_since_last_minor_tag deprecated (not deleted) — published pub API, D-13 precedent"
affects: [25-06, release-cut-executor]

# Tech tracking
tech-stack:
  added: ["semver = \"1\" (devflow-core)", "git-conventional = \"1\" (devflow-core)"]
  patterns:
    - "git subprocess idiom: Command::new(\"git\") → .current_dir → .output() → map_err(VersionError::Git) → status check — reused verbatim for all four new helpers"
    - "git tag --merged HEAD for O(1)-spawn reachability filtering (mirrors GitFlow::cleanup_merged's branch --merged precedent)"
    - "%H%x1f%B%x1e git log format for record-separated raw commit messages, safe against arbitrary message content"

key-files:
  created: []
  modified:
    - "crates/devflow-core/src/version.rs"
    - "crates/devflow-core/Cargo.toml"
    - "Cargo.lock"

key-decisions:
  - "compute_version's baseline is the highest semver tag reachable from HEAD (git tag --merged HEAD), never a raw tag count and never git describe distance (D-07)"
  - "The classification range is anchored via a measured two-branch rule (tag vs. C1's first-parent ancestry), not the literal D-08 baseline..HEAD range — the literal range re-includes all pre-release history because every release here squash-merges develop into main (measured 677 vs 5 commits on v2.0.0..HEAD)"
  - "compute_version refuses (VersionError::UnreachableBaseline) rather than silently falling back to the highest reachable tag when the true highest tag is unreachable from HEAD (D-10)"
  - "count_git_tags and commits_since_last_minor_tag are deprecated, not deleted — both are pub API of a published crate with no publish = false, so removal is a breaking change (D-13 precedent)"
  - "Cargo.toml is no longer read by compute_version at all (D-11); read_version's doc comment now states this explicitly to keep the two functions' roles distinguishable"

requirements-completed: ["25c"]

coverage:
  - id: D1
    description: "compute_version derives baseline from the highest reachable semver tag (never raw tag count, never git describe distance)"
    requirement: "25c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#docs_only_commits_after_tag_yield_patch_floor"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#feat_commit_after_tag_yields_minor_bump"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#fix_commit_after_tag_yields_patch_bump"
        status: pass
    human_judgment: false
  - id: D2
    description: "Classification range is anchored past the squash-merge + sync-back topology (single and double release cycles)"
    requirement: "25c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#squash_sync_topology_classifies_only_post_merge_commits"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#two_squash_sync_cycles_anchor_to_the_second_merge_only"
        status: pass
    human_judgment: false
  - id: D3
    description: "D-10 floors and refusal: no-bump patch floor, malformed-message patch floor, breaking markers (both spellings/positions) yield major, refuse (not fallback) on unreachable highest tag"
    requirement: "25c"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#unreachable_highest_tag_refuses_rather_than_falling_back"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#range_with_no_bumping_commits_yields_patch_floor"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#malformed_commit_message_yields_patch_not_crash_or_major"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#exclamation_before_colon_yields_major"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#breaking_change_footer_yields_major_even_with_fix_subject"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/version.rs#exclamation_only_in_description_does_not_yield_major"
        status: pass
    human_judgment: false
  - id: D4
    description: "Manual sanity check: compute_version against this repository's real history resolves to 2.0.1, not 2.1.0 (naive D-08 range) or 1.11.359 (old algorithm)"
    requirement: "25c"
    verification:
      - kind: manual_procedural
        ref: "ad-hoc #[test] run with --nocapture against project root, removed before commit (see Deviations)"
        status: pass
    human_judgment: false

duration: ~35min
completed: 2026-07-27
status: complete
---

# Phase 25 Plan 01: Reachable-Tag Version Derivation Summary

**Rewrote `compute_version` to derive major/minor/patch entirely from `(highest reachable semver tag, conventional-commit classification of an anchored commit range)`, replacing `Cargo.toml` major + raw tag count + `git describe` distance — verified against this repository's real history to resolve `2.0.1`, not the previous `~1.11.359`.**

## Performance

- **Duration:** ~35 min
- **Completed:** 2026-07-27
- **Tasks:** 2 (Task 1 tracer, Task 2 TDD)
- **Files modified:** 3 (`version.rs`, `Cargo.toml`, `Cargo.lock`)

## Accomplishments

- Added `semver` and `git-conventional` dependencies to `devflow-core` (both pre-approved in 25-RESEARCH.md's Package Legitimacy Audit — OK/Approved, no `[ASSUMED]`/`[SUS]`/`[SLOP]`)
- New `highest_semver_tag`/`reachable_semver_baseline`: D-07 baseline resolution via `git tag --merged HEAD` (one spawn, mirrors `GitFlow::cleanup_merged`'s `branch --merged` precedent), filtering non-semver tags (e.g. this repo's `archive-planning-docs-2026-07-24`) via `filter_map(...ok())` so a malformed tag can never crash the path
- New `release_range_start`: implements the measured two-branch anchor rule from the plan's `<measured_correction>` — resolves the sync-merge commit as the range start instead of the raw tag, restoring D-08's evident intent ("commits added since the last release") against a squash-merge + `-X ours` sync-back topology
- New `classify_range_bump`/`Bump`: conventional-commit precedence classification via `git_conventional::Commit::parse` over `%H%x1f%B%x1e`-formatted commit records — breaking markers (both `!`-before-colon and the two `BREAKING CHANGE:`/`BREAKING-CHANGE:` footer spellings) always win regardless of type
- Rewrote `compute_version` to compose all of the above; deleted the `detect_version_file` call from its body entirely (D-11 — the version file is no longer a computation input)
- New `VersionError::UnreachableBaseline{tag}`: D-10's refuse-on-unreachable-highest-tag, naming the tag and the repair command (`scripts/sync-main-to-develop.sh` or merge the tag's branch), with no filesystem path in the message (WR-02)
- `count_git_tags`/`commits_since_last_minor_tag` marked `#[deprecated]` (bodies unchanged) rather than deleted — both are `pub` API of a published crate with no `publish = false` (D-13 precedent)
- Rewrote the test suite: 6 new Task 1 fixtures (docs/feat/fix-after-tag, no-tag-at-all, single and double squash+sync release-cycle topologies) plus 6 new Task 2 fixtures (unreachable-baseline refusal, no-bump floor, malformed-message floor, and the three breaking-marker edge cases) — 36/36 `version::` tests pass, up from the prior 20

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end version derivation (tracer)** — `c033aad` (feat)
2. **Task 2: D-10 refusal and floors, retirement of superseded helpers (TDD)**:
   - RED — `d8def90` (test): added the 6 D-10 fixtures; failed to compile (`VersionError::UnreachableBaseline` variant did not exist yet)
   - GREEN — `9d33489` (feat): added the variant, wired the refusal into `compute_version`, deprecated the two superseded helpers; all 36 tests pass

**Plan metadata:** (this commit)

## Files Created/Modified

- `crates/devflow-core/src/version.rs` — rewritten `compute_version`; four new public helpers (`highest_semver_tag`, `reachable_semver_baseline`, `release_range_start`, `classify_range_bump`) plus `Bump` enum; `VersionError::UnreachableBaseline` variant; `#[deprecated]` on `count_git_tags`/`commits_since_last_minor_tag`; rewritten test module (12 new tests, 1 old test removed, 1 stale comment corrected)
- `crates/devflow-core/Cargo.toml` — added `semver = "1"` and `git-conventional = "1"` to `[dependencies]`
- `Cargo.lock` — locked `semver 1.0.28`, `git-conventional 1.1.0`, and the new transitive `unicase 2.9.0`

## Decisions Made

- **Baseline reconstruction via `format!("v{version}")`:** `reachable_semver_baseline`/`highest_semver_tag` return a parsed `semver::Version`, not the original tag string. `release_range_start` and the `UnreachableBaseline` error both need the tag *string*, so the plan's helpers reconstruct it as `v{version}`. This round-trips exactly for every tag in this repository (all plain `vMAJOR.MINOR.PATCH`, no pre-release/build suffixes) and is the same reconstruction the plan's own D-10 refusal message uses.
- **Empty-string sentinel for "no baseline tag" in `classify_range_bump`:** rather than adding a second function signature for the no-tag-at-all case, `range_start = ""` means "classify the whole history reachable from HEAD" (`git log --no-merges HEAD`, no exclusion). This lets `compute_version`'s "no tag at all → baseline 0.0.0" behavior fall out of the same classification path used for every other case, rather than a separate special-cased branch.
- **`"ci"` type classified via string comparison, not a `git_conventional::Type` constant:** the crate does not export a `Type::CI` constant (only `FEAT`/`FIX`/`REVERT`/`DOCS`/`STYLE`/`REFACTOR`/`PERF`/`TEST`/`CHORE`), so `ty == "ci"` (a `PartialEq<&str>` the crate implements on `Type`) is used directly, matching the plan's own listed floor group (`docs`, `test`, `chore`, `ci`, `refactor`, `style` → `Bump::None`).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Corrected a stale comment describing the old algorithm**

- **Found during:** Task 1 (rewriting `compute_version`)
- **Issue:** `read_version_does_not_recompute_from_git_tags`'s test comment said "`compute_version` would see 1 tag + 2 commits since => 2.1.2" — a literal claim about the now-removed old algorithm's output, left factually wrong by the rewrite even though the test itself doesn't call `compute_version`.
- **Fix:** Reworded to describe the property generically ("recompute from git history... instead of reporting the version file") without asserting a specific stale number.
- **Files modified:** `crates/devflow-core/src/version.rs`
- **Committed in:** `c033aad` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug — stale comment)
**Impact on plan:** Cosmetic only; no test behavior changed.

### Noted, Not Fixed (documented plan-level tensions — not auto-fixed, no code change)

**A. Task 1's whole-file "no `describe`" acceptance check cannot literally return 0 while Task 2's own instruction retains `commits_since_last_minor_tag`'s body**

Task 1's acceptance criteria include: `rg -v '^\s*//|^\s*///' crates/devflow-core/src/version.rs | rg -c '"describe"'` returns 0. This currently returns 1, because `commits_since_last_minor_tag` (which shells `git describe --tags --abbrev=0`) is still present in the file — Task 2 explicitly instructs *deprecating*, not deleting, that function (D-13 precedent: it's `pub` API of a published crate; removal would be a breaking change). Deleting it to satisfy the literal grep would contradict Task 2's own explicit instruction and CONTEXT.md's D-13 precedent. The substantive truth the check is protecting — `compute_version` itself no longer calls `git describe` anywhere — is independently confirmed by the "`compute_version` contains no call to `detect_version_file`" assertion and by direct inspection of `compute_version`'s body (four calls: `highest_semver_tag`, `reachable_semver_baseline`, `release_range_start`, `classify_range_bump` — none of which shell `describe`). Not fixed; flagging as a self-contradictory pair of acceptance criteria across Task 1 and Task 2 in this plan.

**B. The `#[deprecated]` lint promotes `cargo clippy --workspace --all-targets -- -D warnings` to a hard error at the one out-of-crate caller, `pipeline_gate.rs:836`**

Task 2's action text says to "check `cargo clippy --workspace --all-targets -- -D warnings` before declaring done," but the same task's action text also says "the one out-of-crate caller lives in `crates/devflow-cli/src/pipeline_gate.rs`... do not touch it here" — and the plan's own `<verification>` section separately states this exact fixture is "EXPECTED to fail after this plan lands... do not chase it as a new regression here." All three statements are true simultaneously and consistent with each other once read together: this is the *same* anticipated pipeline_gate.rs coupling (25-RESEARCH.md Pitfall 2), now surfacing one step earlier — as a `deprecated`-lint compile error under `-D warnings` in addition to the runtime assertion failure. Confirmed both forms live:
  - `cargo clippy --package devflow-core --all-targets --features test-support -- -D warnings` — **clean** (this plan's actual scoped acceptance criterion; the `--features test-support` flag is needed only because Cargo does not unify `devflow-cli`'s dev-dependency feature request when building `devflow-core` in isolation — `cargo clippy --workspace --all-targets` naturally receives this unification and needs no flag)
  - `cargo clippy --workspace --all-targets -- -D warnings` — fails at `pipeline_gate.rs:836` (`use of deprecated function`)
  - `cargo test --package devflow --bin devflow finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` — **fails** (assertion, not compile error, when built without `-D warnings`): confirms the plan's predicted runtime coupling too
  - `cargo build --workspace` / `cargo test --workspace --no-run` — **succeed** (the `deprecated` lint is warn-only without `-D warnings`)

  Not fixed; `pipeline_gate.rs` is plan 25-06's scope per this plan's own text. Both couplings (clippy-under-`-D-warnings` and the runtime assertion) are the identical root cause and will close together when 25-06 rewrites that consumer.

## Issues Encountered

None beyond the two noted tensions above (both pre-flagged by the plan itself, not new discoveries).

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- `compute_version`'s algorithm is correct and independently verified against this repository's real tag/commit history (baseline `v2.0.0`, range start `c92229e` — the sync-merge commit — bump `Patch` from one `fix` commit in range, result `2.0.1`).
- `read_version`/`write_version`/`detect_version_file` are untouched, as required.
- Plan 25-06 must: (a) add the D-09 preflight major-bump gate consuming this algorithm, and (b) rewrite `pipeline_gate.rs`'s `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` fixture, which independently re-derives the old algorithm and now both fails at runtime and fails `cargo clippy -- -D warnings` (deprecated-function use) — both are the same anticipated coupling, not a new regression from this plan.
- No conventional-commit type in the real measured range (`c92229e..HEAD`, 11 non-merge commits: 10 `docs`, 1 `fix`) fell through D-10's unrecognised-type floor — every commit message parsed as a recognised, listed type (`docs` → `None`, `fix` → `Patch`). The floor's "any other recognised-but-unlisted type → patch" branch and the "malformed/unparseable → patch" branch are both covered only by the new synthetic fixtures, not by this repository's real history at measurement time.

---
*Phase: 25-end-to-end-dogfood-blockers*
*Completed: 2026-07-27*
