---
phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
plan: 03
subsystem: release-automation
tags: [rust, config, gh-cli, merge-policy, release]

# Dependency graph
requires:
  - phase: 29-01-plan-observe-first-two-questions
    provides: "Established alphabetical `pub mod` convention in lib.rs (release_observe placed after registry, before ship) and the D-12 yes_ship three-source-precedence resolver shape this plan mirrors"
provides:
  - "config::yes_release(project_root) / DevflowConfig::yes_release — the release-cut executor's standing authorization mandate, read-only, not consumed, defaulting to false"
  - "devflow_core::release_policy — MergeIntent/MergeMethod/MergePolicyError vocabulary plus required_method/resolve_merge_method/intersect_allowed/discover_allowed_merge_methods, the pure policy layer 29-05 and 29-06's `gh pr merge` call sites will consume"
affects: [29-04-plan-cli-surface-and-recoverable-actions, 29-05-plan, 29-06-plan, 29-07-plan-commit-point]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Read-only authorization mandate: a boolean resolved from flag/file/env with a false default, structurally incapable of becoming a progress ledger because the resolving module (config.rs) contains zero write calls outside test code"
    - "Merge intent (not branch name) as the primary noun for merge-method policy — MergeIntent -> required_method(intent) -> resolve_merge_method(intent, discovered_allowed) -> MergeMethod::flag(), never a branch-name lookup"
    - "Refuse-never-substitute error shape: MergePolicyError::NotAllowed carries the intent, the required method, and the discovered allowed set so the caller can print an actionable refusal instead of silently taking the other method"

key-files:
  created:
    - crates/devflow-core/src/release_policy.rs
  modified:
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/lib.rs
    - OPERATIONS.md

key-decisions:
  - "release_policy.rs placed alphabetically after release_observe, before ship (not literally 'between recover and registry' as PLAN.md's action text stated) — matches 29-01's own precedent decision on the same wording imprecision; the file's established convention is strictly alphabetical."
  - "resolve_merge_method's allowed-set matching is case-insensitive and whitespace-trimmed on both sides (the discovered value and the fixed policy's api_name()), satisfying the plan's explicit case/whitespace-tolerance behavior bullet without adding a normalization step to discover_allowed_merge_methods itself."
  - "discover_allowed_merge_methods reads repo-level allow_merge_commit/allow_squash_merge via two gh api --jq boolean calls (mirroring release_observe.rs's tag_signature_via_gh single-field pattern) rather than one call parsing a combined JSON object — keeps each gh invocation reviewable line by line, per this plan's own stated review bar."

patterns-established:
  - "gh_bool_field(project_root, path, jq): a one-field gh api --jq boolean reader, reusable wherever a future release-cut step needs a single true/false fact from the GitHub API without embedding gh's raw output in an error (T-29-03)."

requirements-completed: [29b]

coverage:
  - id: D1
    description: "Release authorization (yes_release) resolves from --yes-release-equivalent config precedence — DEVFLOW_YES_RELEASE env, devflow.toml key, false default — read-only, not consumed by reading, independent of yes_ship"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#config::tests (10 new yes_release cases: default false, missing file, file true/false, unrelated keys, env override, unparseable env fallback, independence from yes_ship, not-consumed-by-reading)"
        status: pass
    human_judgment: false
  - id: D2
    description: "No DevFlow code path writes devflow.toml or any .devflow/ file for release purposes, and yes_release is deliberately absent from State"
    requirement: "29b"
    verification:
      - kind: other
        ref: "rg -n 'fs::write|File::create|write_all' crates/devflow-core/src/config.rs (zero matches outside #[cfg(test)]); rg -n 'yes_release' crates/devflow-core/src/state.rs (zero matches)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Merge intent (VersionBump/ReleaseCut/SyncBack) resolves to a merge method against a discovered allowed set, refusing loudly rather than substituting when the required method is absent — the sync-back case specifically requires Merge even when squash is also allowed"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_policy.rs#release_policy::tests (14 cases: sync-back-yields-merge, sync-back-refuses-on-squash-only naming intent+method+set, release-cut-yields-squash, release-cut-refuses-on-merge-only, version-bump-yields-squash, empty-set-is-AllowedSetUnknown for all three intents, case/whitespace tolerance, MergeMethod flag/api_name, intersect_allowed)"
        status: pass
    human_judgment: false
  - id: D4
    description: "No merge-method literal, no required-check name, and no branch-name conditional exists in release_policy.rs — the discovered allowed set is the only source of truth, keyed on intent"
    requirement: "29b"
    verification:
      - kind: other
        ref: "rg -n 'if .*branch *== *\"main\"|if .*branch *== *\"develop\"' (zero matches); rg -n '\"Test\"|\"Clippy\"|\"Format\"|\"Build \\+ test in devcontainer\"' (zero matches), both against release_policy.rs"
        status: pass
    human_judgment: false

# Metrics
duration: 55min
completed: 2026-07-31
status: complete
---

# Phase 29 Plan 03: Release Authorization Mandate + Merge-Intent Resolution Summary

**`config::yes_release` — a read-only, unconsumed, structurally-non-ledger authorization mandate mirroring `yes_ship` exactly — plus a new `release_policy` module resolving `MergeIntent` (VersionBump/ReleaseCut/SyncBack) to a required `MergeMethod` against a discovered allowed set, refusing loudly rather than silently substituting the method that destroyed this repo's own sync ancestry on 2026-07-27.**

## Performance

- **Duration:** ~55 min
- **Completed:** 2026-07-31
- **Tasks:** 2 completed (both TDD RED→GREEN)
- **Files modified:** 4 (1 created, 3 modified)

## Accomplishments

- **The authorization-record boundary is pinned and structurally enforced, not just documented.** `DevflowConfig::yes_release` / `config::yes_release(project_root)` resolve from `devflow.toml`'s `yes_release` key and `DEVFLOW_YES_RELEASE`, defaulting to `false`, mirroring `yes_ship`'s D-12 precedent field for field. `config.rs` contains zero write calls outside test code, and `yes_release` is deliberately absent from `State` — the two facts a source-level grep now proves rather than merely asserts.
- **One mandate covers the whole release sequence**, per RD-4: no second, narrower flag was added for the irreversible steps, which would have been a self-imposed gate no repository rule imposes (RD-2).
- **Merge method is resolved from intent, never from branch name.** `release_policy::resolve_merge_method(MergeIntent::SyncBack, &["merge", "squash"])` returns `Ok(MergeMethod::Merge)` — the sync-back PR takes a real merge commit even though squash is also allowed on `develop`, reproducing the correct outcome of the incident CONTRIBUTING.md documents (an unspecified-method auto-merge defaulted to squash on 2026-07-27, destroying the ancestry link and requiring a repair PR).
- **A required method absent from the discovered set produces a loud, three-fact refusal** (`MergePolicyError::NotAllowed { intent, required, allowed }`) — never a silent substitution. `MergeMethod` has exactly two variants and no "unspecified" state, so a method-less merge cannot be constructed by any future call site.
- **Live discovery, never a cache or a compiled-in copy.** `discover_allowed_merge_methods` reads `gh api repos/{owner}/{repo}` (repo-level settings) and `gh api repos/{owner}/{repo}/rules/branches/<branch>` (branch ruleset), combining via `intersect_allowed`; no required-check name or merge-method literal exists anywhere in the module (source-level grep confirms both).

## Task Commits

Each task followed a RED (failing test) → GREEN (implementation) TDD cycle, per its `tdd="true"` attribute:

1. **Task 1: the authorization mandate**
   - `cf76c9e` `test(29-03): add failing tests for the release authorization mandate` — RED: `yes_release` field/accessor added (compiles), resolver stubbed with `todo!()`, 9/24 config tests failed for the intended reason (2 additional `yes_ship` tests were incidentally poisoned by the RED-phase mutex panic, an expected artifact of ENV_MUTEX-guarded tests)
   - `e7f6ddc` `feat(29-03): release authorization mandate — yes_release resolver` — GREEN: real `DEVFLOW_YES_RELEASE`-over-file-over-default resolver, mirroring `yes_ship` exactly; 24/24 config tests pass

2. **Task 2: merge-intent resolution**
   - `66c7ed2` `test(29-03): add failing tests for merge-intent resolution` — RED: new `release_policy.rs` module with real types (`MergeIntent`, `MergeMethod`, `MergePolicyError`) and correct trivial methods (`flag`, `api_name`, `required_method`), but `resolve_merge_method`/`intersect_allowed` stubbed with `unimplemented!()`; 10/14 tests failed for the intended reason
   - `6b8faf0` `feat(29-03): merge intent resolved against a discovered allowed set` — GREEN: real resolution logic, `discover_allowed_merge_methods` I/O wrapper, and a Rule 3 doc fix (see below); 14/14 release_policy tests pass, `cargo test --workspace` 496/496 in devflow-core with no regressions

## Files Created/Modified

- `crates/devflow-core/src/release_policy.rs` (new) — `MergeIntent`, `MergeMethod`, `MergePolicyError`, `required_method`, `resolve_merge_method`, `intersect_allowed`, `discover_allowed_merge_methods`, `gh_bool_field`/`repo_level_allowed_methods`/`branch_level_allowed_methods` I/O helpers, plus 14 unit tests
- `crates/devflow-core/src/config.rs` — `DevflowConfig::yes_release` field (with a doc comment naming all four boundary properties: read-only, not consumed, single boolean carrying no progress, defaults to false), `DevflowConfig::yes_release()` accessor, module-level `config::yes_release(project_root)` resolver, 10 new tests
- `crates/devflow-core/src/lib.rs` — `pub mod release_policy;` declared alphabetically after `release_observe`, before `ship`
- `OPERATIONS.md` — new `DEVFLOW_YES_RELEASE` row in the environment-variables table (Rule 3 fix, see below)

## Decisions Made

- **Module placement:** `release_policy.rs` was declared after `release_observe`, before `ship` — the file's true alphabetical order, matching 29-01's own precedent on this exact wording imprecision in PLAN.md's action text ("between recover and registry" is not itself alphabetical order).
- **Two `gh_bool_field` calls instead of one combined-JSON `gh api` call** for repo-level merge settings: keeps each `gh` invocation a single reviewable line, matching this plan's own stated bar ("small enough to review line by line — which is the gate that actually works on this code") and the existing `tag_signature_via_gh` one-field-per-call convention from 29-01.
- **Case-insensitive, whitespace-trimmed matching lives in `resolve_merge_method`**, not in `discover_allowed_merge_methods` — the plan's behavior bullet ("`&["SQUASH"]` and `&[" squash "]` both resolve") is about resolution tolerance of whatever the discovered set contains, not a normalization step on the discovery I/O itself.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `DEVFLOW_YES_RELEASE` missing from operator documentation**
- **Found during:** Task 2, running `cargo test --workspace` before the GREEN commit
- **Issue:** `crates/devflow-core/src/doc_check.rs`'s `source_devflow_env_vars_and_subcommands_are_documented` test scans source for every `DEVFLOW_*` environment variable and asserts it is documented in one of the scoped operator docs (README.md/ARCHITECTURE.md/CONTRIBUTING.md/OPERATIONS.md). Task 1's new `DEVFLOW_YES_RELEASE` env var (added to `config.rs` in the GREEN commit for Task 1) tripped this check, failing the workspace test suite.
- **Fix:** Added a `DEVFLOW_YES_RELEASE` row to `OPERATIONS.md`'s environment-variables table, mirroring the existing `DEVFLOW_YES_SHIP` row's shape and stating the read-only/not-a-ledger boundary explicitly.
- **Files modified:** `OPERATIONS.md`
- **Verification:** `cargo test --workspace` — `doc_check::source_devflow_env_vars_and_subcommands_are_documented` passes; full workspace suite green (496/496 in devflow-core, no regressions elsewhere).
- **Committed in:** `6b8faf0` (bundled with Task 2's GREEN commit — the failure surfaced while verifying Task 2's own work, though its root cause was Task 1's new env var)

---

**Total deviations:** 1 auto-fixed (Rule 3, blocking — a workspace-wide doc-coverage test, not scoped to either task's own `<files>` list, but directly caused by this plan's new environment variable).
**Impact on plan:** Necessary to keep `cargo test --workspace` green, which this plan's own `<verification>` section requires. No design change; no architectural deviation.

## Issues Encountered

None beyond the deviation above.

## User Setup Required

None — no external service configuration required. `yes_release` and `DEVFLOW_YES_RELEASE` are operator-facing knobs an operator may choose to set later (in `29-04` and beyond, when the CLI surface and `commands::release_cut` land), but nothing in this plan requires any setup to function; every test uses a fixture `devflow.toml`, never the real one.

## Next Phase Readiness

- `config::yes_release(project_root)` is ready for `29-04-PLAN.md`'s `commands::release_cut` to OR-combine with a new `--yes-release` CLI flag, exactly as `commands::start` ORs `config::yes_ship` with `--yes-ship`.
- `release_policy::resolve_merge_method` and `MergeMethod::flag()` are ready for every `gh pr merge` call site in `29-05-PLAN.md` and `29-06-PLAN.md` to consume — each call site is expected to pass an explicit method flag obtained from this module, never a bare `--auto`.
- `discover_allowed_merge_methods(project_root, branch)` is untested against the live GitHub API in this plan (per its own action text: "no test calls `gh`"); `29-04`/`29-05` should exercise it against the real repository as part of their own end-to-end verification, the same way 29-01's tracer task did for `release_observe`'s live oracles.
- No blockers.

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/release_policy.rs`
- FOUND: `crates/devflow-core/src/config.rs`
- FOUND: `crates/devflow-core/src/lib.rs`
- FOUND: `OPERATIONS.md`
- FOUND commit `cf76c9e` (test: release authorization mandate, RED)
- FOUND commit `e7f6ddc` (feat: yes_release resolver, GREEN)
- FOUND commit `66c7ed2` (test: merge-intent resolution, RED)
- FOUND commit `6b8faf0` (feat: merge-intent resolution + doc fix, GREEN)
