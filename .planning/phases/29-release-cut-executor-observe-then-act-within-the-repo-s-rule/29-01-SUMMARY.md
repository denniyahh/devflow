---
phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
plan: 01
subsystem: release-automation
tags: [rust, git, gh-cli, curl, crates-io, cli, release]

# Dependency graph
requires:
  - phase: 27-scrub-redirecting-git-environment-from-production-calls
    provides: "git_command/hermetic_command hermeticity substrate every new git/cargo invocation must use"
provides:
  - "devflow release status <version> [project] — a new read-only observer subcommand"
  - "devflow_core::release_observe: Observation/ReleaseStep/TagRefs/TagSignature vocabulary the rest of Phase 29 (29-02..29-07) builds on"
  - "Two of the six release-cut questions (signed tag on origin, crates published) answered from authoritative external sources"
affects: [29-02-plan-observe-remaining-four-questions, 29-03-plan-recoverable-actions, 29-06-plan, 29-07-plan-commit-point]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Three-valued Observation enum (Present/Absent/Unreachable), mirroring git.rs's AncestorStatus/SigningViability convention — never a boolean, Unreachable always carries the real tool's own failure text"
    - "Pure classify_* functions separated from thin I/O wrapper functions, so every classification branch is unit-testable without a network or a subprocess"
    - "curl shell-out (never a new HTTP crate) for crates.io's /api/v1 JSON endpoint, pinned cwd, descriptive User-Agent, never the CDN-cached sparse index"

key-files:
  created:
    - crates/devflow-core/src/release_observe.rs
    - crates/devflow-cli/tests/release_status.rs
  modified:
    - crates/devflow-core/src/lib.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - OPERATIONS.md

key-decisions:
  - "gh api .../git/tags/<sha> must be called with the tag OBJECT's own sha (ls-remote's unpeeled entry), not the peeled ^{} entry — the peeled entry dereferences to the underlying commit, a different object with no .verification field. Found and fixed during Task 1's manual end-to-end verification, before the GREEN commit."
  - "Module placed alphabetically after registry, before ship (not literally 'between recover and registry' as PLAN.md's action text stated) — matches this file's own established alphabetical convention; a wording imprecision in the plan, not a design decision."

patterns-established:
  - "release_observation_check(step, observation) -> Check: the single conversion point from the new three-valued oracle vocabulary to the pre-existing Check-list-then-report CLI shape; Unreachable maps to the failing 'fail' status (RD-8), diverging deliberately from 29-PATTERNS.md's softer suggestion."

requirements-completed: [29a]

coverage:
  - id: D1
    description: "devflow release status <version> observes the signed-tag-on-origin question end to end (git ls-remote + gh api), three-valued, Unreachable never collapsed into Absent"
    requirement: "29a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_observe.rs#release_observe::tests (10 cases covering classify_tag_refs/classify_signed_tag)"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_status.rs#release_status_no_remote_is_unreachable_not_absent"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_status.rs#release_status_absent_tag_on_reachable_remote_warns"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/release_status.rs#signed_tag_live_smoke (#[ignore], run with --ignored against this repo's real origin)"
        status: pass
    human_judgment: false
  - id: D2
    description: "devflow release status <version> observes the crates-published question end to end (crates.io /api/v1), reusing publish_order, never hardcoding crate names"
    requirement: "29a"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_observe.rs#release_observe::tests (8 cases covering classify_http_status/combine_crate_observations)"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/release_status.rs#crates_published_live_smoke (#[ignore], run with --ignored against the real crates.io registry)"
        status: pass
    human_judgment: false
  - id: D3
    description: "devflow release --check (the pre-existing 20d preflight) keeps working exactly as it did before this plan"
    requirement: "29a"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_status.rs#release_check_still_passes_on_matching_pins"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs (pre-existing suite, 9 passed — same count as before this plan)"
        status: pass
    human_judgment: false

duration: 40min
completed: 2026-07-31
status: complete
---

# Phase 29 Plan 01: Release-Cut Executor Tracer — Signed-Tag + Crates-Published Observers Summary

**`devflow release status <version>` end-to-end: a new `release_observe` core module answers two of the six release-cut questions (signed tag on `origin`, both workspace crates published to crates.io) via `git ls-remote`/`gh api`/`curl`, every observation three-valued with `Unreachable` carrying the real tool's own failure text.**

## Performance

- **Duration:** ~40 min
- **Completed:** 2026-07-31
- **Tasks:** 2 completed (Task 1 tracer + Task 2)
- **Files modified:** 7 (2 created, 5 modified)

## Accomplishments

- Built the tracer for Phase 29: `devflow release status <version>` runs end to end from CLI arg parsing through a new `devflow_core::release_observe` oracle module out to real external tools (`git`, `gh`, `curl`) and back to a printed, `release --check`-styled report.
- `Observation` (`Present`/`Absent`/`Unreachable`), `ReleaseStep`, `TagRefs`, and `TagSignature` — the vocabulary the rest of Phase 29 (29-02 through 29-07) builds on — are now real, tested types.
- Both oracle kinds this phase needs are proven live against real infrastructure: remote git refs (signed tag on `origin`, reproducing the exact "tag never created" incident that motivated this phase — `v2.2.0` genuinely has no signed tag today) and the crates.io registry HTTP API (both `devflow-core` and `devflow` confirmed live at `2.2.0`).
- The legacy `devflow release --check` contract (20d) is unchanged — same four checks, same pass/fail behavior, verified both by a regression test and a live manual run.

## Task Commits

Each task followed a RED (failing test) → GREEN (implementation) TDD cycle, per its `tdd="true"` attribute:

1. **Task 1: signed-tag-on-remote oracle** (tracer)
   - `d75cc43` `test(29-01): add failing tests for signed-tag observation classifiers` — RED: `classify_tag_refs`/`classify_signed_tag` stubbed wrong, 7/10 tests failed for the intended reason
   - `7694d2a` `feat(29-01): devflow release status — signed-tag-on-remote oracle (29a)` — GREEN: real classifiers, `signed_tag_on_remote`/`tag_signature_via_gh` I/O wrappers, CLI wiring (`ReleaseAction::Status`), integration tests, `--help` snapshot + OPERATIONS.md updated

2. **Task 2: crates.io publish oracle**
   - `e521025` `test(29-01): add failing tests for the crates.io publish oracle` — RED: `classify_http_status`/`combine_crate_observations` stubbed wrong, 6/18 tests failed for the intended reason
   - `d0291cc` `feat(29-01): devflow release status — crates.io publish oracle (29a)` — GREEN: real classifiers, `crate_version_http_status`/`crates_published` I/O wrappers (reusing `publish_order`), wired into `release_status`, live smoke test added, two pre-existing fixture tests reworked to account for the second check row

## Files Created/Modified

- `crates/devflow-core/src/release_observe.rs` (new) — `Observation`, `ReleaseStep`, `TagRefs`, `TagSignature`, the six pure `classify_*`/`combine_*` functions, and the four I/O wrapper functions (`signed_tag_on_remote`, `tag_signature_via_gh`, `crate_version_http_status`, `crates_published`), plus 18 unit tests
- `crates/devflow-core/src/lib.rs` — `pub mod release_observe;` declared alphabetically (after `registry`, before `ship`)
- `crates/devflow-cli/src/main.rs` — new `ReleaseAction` subcommand enum (`Status { version, project }`), `Command::Release` gains `action: Option<ReleaseAction>`; the omitted-`--check` rejection message (naming DEN-50) is byte-for-byte preserved
- `crates/devflow-cli/src/commands.rs` — `release_observation_check` (Observation+ReleaseStep -> Check) and `release_status` (the new command handler, reusing `release_check`'s report loop verbatim)
- `crates/devflow-cli/tests/release_status.rs` (new) — 6 tests: `--check` regression, no-remote-is-unreachable, reachable-but-absent-tag, summary-line, and two `#[ignore]`-gated live smoke tests
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerated for the changed `release` subcommand help text
- `OPERATIONS.md` — new table row documenting `devflow release status <version> [project]`

## Decisions Made

- **Tag-object-sha vs. peeled-sha bug, found and fixed mid-Task-1:** manual end-to-end verification against the real repo (`devflow release status 2.1.0 .`) initially reported the signed tag as unreachable (`gh api` returned 404). Root cause: `gh api .../git/tags/<sha>` expects the tag OBJECT's own sha (`git ls-remote`'s unpeeled `refs/tags/vX.Y.Z` entry), not the peeled `^{}` entry (which dereferences to the commit the tag points to — a different object). Fixed in `signed_tag_on_remote` before the GREEN commit; this is a Rule 1 auto-fix (bug in the plan's I/O wrapper design, caught by actually running it against live data rather than trusting the design read-through).
- **Module placement:** `pub mod release_observe;` was placed after `registry`, before `ship` — the file's own true alphabetical order. PLAN.md's action text said "between recover and registry," which is not actually alphabetical (`recover` < `registry` < `release_observe`); treated as a wording imprecision in the plan rather than a design instruction to follow literally, since the intent stated ("alphabetical") is unambiguous and the file's existing convention is already strictly alphabetical.
- **Two fixture tests reworked for the second check row:** `release_status_absent_tag_on_reachable_remote_warns_and_exits_zero` and `release_status_summary_line_names_version_and_count` were written during Task 1 against a single wired question. Task 2 added a second row (`crates published`) that both fixtures — having no `Cargo.toml` — report as `Unreachable` (empty `publish_order`, correctly so per this task's own spec: "an empty slice is Unreachable"). Rather than weaken the assertions, added a `row_icon()` test helper so each test asserts on the one row it actually targets, independent of how many other rows the command also prints. The absent-tag test was renamed (dropped "_and_exits_zero" from its name, since overall exit code is now also influenced by the independent crates-published row) to describe what it actually verifies.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] gh api tag-verification call used the wrong sha (peeled instead of object)**
- **Found during:** Task 1, manual end-to-end verification after the unit tests passed
- **Issue:** `signed_tag_on_remote` passed `refs.peeled` (the dereferenced commit sha) to `tag_signature_via_gh`, but `gh api repos/{owner}/{repo}/git/tags/<sha>` requires the tag OBJECT's own sha — `refs.object` for an unpeeled `refs/tags/vX.Y.Z` line. The peeled sha returned a real 404 from GitHub (a genuinely different, non-tag object), which correctly surfaced as `Unreachable` (the safety net worked) but meant `v2.1.0`'s real, correctly-signed tag reported unreachable instead of Present.
- **Fix:** `signed_tag_on_remote` now branches on `refs.is_annotated()` and passes `refs.object` (not `refs.peeled`) to `tag_signature_via_gh`, with an explanatory comment distinguishing the two shas.
- **Files modified:** `crates/devflow-core/src/release_observe.rs`
- **Verification:** `devflow release status 2.1.0 .` now reports the signed-tag row `✓ tag is annotated and signed`, confirmed against this repo's real, historically-signed `v2.1.0` tag; `signed_tag_live_smoke` (`#[ignore]`) passes.
- **Committed in:** `7694d2a` (part of the GREEN commit for Task 1 — caught before that commit, not a follow-up fix)

**2. [Rule 1/3 - Grep-discipline wording] Two doc comments contained acceptance-criteria-forbidden literal strings**
- **Found during:** Running each task's acceptance-criteria greps before committing GREEN
- **Issue:** A doc comment on `signed_tag_on_remote` literally quoted `` `Command::new("git")` `` (to say it is never used), and a doc comment on `crate_version_http_status` literally named `` `index.crates.io` `` (to say it is never queried) — both tripped their own task's "must not appear in this file" grep, even though the surrounding sentence's *meaning* was exactly what the grep exists to enforce.
- **Fix:** Reworded both doc comments to convey the same meaning without the literal forbidden substring (e.g. "never a direct, unscrubbed git invocation" instead of quoting the exact `Command::new(...)` call; "the registry's CDN-cached sparse-index mirror" instead of naming the host literally).
- **Files modified:** `crates/devflow-core/src/release_observe.rs`
- **Verification:** Both greps (`rg -n 'Command::new\("git"\)'` and `rg -n 'index\.crates\.io|cargo (info|search)'`) return zero matches.
- **Committed in:** `7694d2a`, `d0291cc` (part of each task's respective GREEN commit)

---

**Total deviations:** 2 auto-fixed (1 Rule 1 bug, 1 Rule 1/3 wording fix). Both directly caused by this plan's own new code; no scope creep, nothing touched outside the plan's `files_modified` list.
**Impact on plan:** Both fixes were necessary for correctness (bug) or for meeting the plan's own stated acceptance criteria (wording). No design change; no architectural deviation.

## Issues Encountered

- **`cargo test -p devflow-core <filter>` (no `--features test-support`) fails to compile**, unrelated to this plan: two pre-existing `devflow-core` integration test files (`tests/devflow_dir_gitignore.rs`, `tests/monitor_e2e.rs`) reference `devflow_core::test_support`, which is only compiled in under `#[cfg(any(test, feature = "test-support"))]`. Running `cargo test -p devflow-core` in isolation (not `--workspace`) does not pull in that feature via Cargo's workspace feature unification, so those two integration test binaries fail to compile regardless of any change in this plan. Confirmed pre-existing by reproducing the identical failure with this plan's `lib.rs` change reverted. Per the executor's scope boundary (only auto-fix issues directly caused by the current task's changes), this was **not** fixed — instead, all scoped `-p devflow-core` verification in this plan was run with `--features test-support` added, and the plan's own final gate (`cargo test --workspace`, which unifies features across the workspace) was also run and is fully green. No code change was made for this issue.

## User Setup Required

None — no external service configuration required. (`gh` and network reachability to `github.com`/`crates.io` were both already available in this environment and were used for live verification, but neither is a new requirement introduced by this plan; both were already-assumed tools per `29-RESEARCH.md`'s Environment Availability table.)

## Next Phase Readiness

- `devflow_core::release_observe`'s `Observation`/`ReleaseStep` vocabulary is in place and stable for `29-02-PLAN.md` to extend with the remaining four release-cut questions (`VersionBumped`, `ChangelogWritten`, `ReleasePrMerged`, `SyncMerged`).
- `ReleaseStep::ALL`/`label()` already enumerate all six steps in release-sequence order, so `29-02` only needs to wire the remaining four oracles and extend `release_status`'s check list — no type or CLI-surface redesign needed.
- No blockers. The `Unreachable`-dominates-everything discipline (`combine_crate_observations`, `classify_signed_tag`'s `Undetermined` arm) is directly unit-tested and available as a pattern for 29-02's own multi-source observations (e.g. `SyncMerged`'s `gh api compare` call).

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/release_observe.rs`
- FOUND: `crates/devflow-cli/tests/release_status.rs`
- FOUND: `crates/devflow-core/src/lib.rs`
- FOUND: `crates/devflow-cli/src/main.rs`
- FOUND: `crates/devflow-cli/src/commands.rs`
- FOUND: `OPERATIONS.md`
- FOUND commit `d75cc43` (test: signed-tag classifiers, RED)
- FOUND commit `7694d2a` (feat: signed-tag oracle, GREEN)
- FOUND commit `e521025` (test: crates.io oracle, RED)
- FOUND commit `d0291cc` (feat: crates.io oracle, GREEN)
