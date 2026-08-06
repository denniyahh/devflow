---
phase: 17-pipeline-dogfood-followup
plan: 06
subsystem: infra
tags: [rust, cli, state-machine, git, build-provenance, gap-closure]

# Dependency graph
requires:
  - phase: 17-pipeline-dogfood-followup
    provides: "Plan 01's infra_failures counter + MAX_INFRA_FAILURES ceiling, Plan 04's transition()/handle_infra_outcome wiring, Plan 05's embedded_commit_is_stale/combined_staleness/enforce_build_staleness self-dogfood staleness gate"
provides:
  - "transition() resets infra_failures alongside consecutive_failures — the infra-fault ceiling now bounds a stuck loop, not a phase's lifetime"
  - "embedded_commit_is_stale distinguishes an exact HEAD match (Fresh) from a strict ancestor of HEAD (Stale), closing the false-negative on the most common real staleness case"
  - "An attributed overrides: entry in 17-VERIFICATION.md formally accepts the AC-4 scope narrowing (security-artifact + reviewer-set checks deferred to Phase 18)"
affects: [18-hermes-support]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "PATH-neutralization-under-ENV_MUTEX idiom (fresh empty tempdir, not empty string) extended to a direct transition() call, not just handle_stage_failure/run_gate paths"
    - "Attributed overrides: frontmatter block (must_have/reason/accepted_by/accepted_at) as the recorded mechanism for accepting a disclosed scope narrowing without touching the gaps: list it responds to"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/mode.rs
    - .planning/phases/17-pipeline-dogfood-followup/17-VERIFICATION.md
    - .planning/ROADMAP.md

key-decisions:
  - "infra_failures reset is placed in transition() only (not in loop_back_to_code or handle_stage_failure's GateAction::Advance/LoopBack retry branches) — those paths retry the SAME stage after a gate, which is not a 'successful transition' in the D-08 sense the ceiling is meant to bound; only a genuine forward stage transition resets the counter, matching the plan's exact fix site"
  - "The new WR-01 regression test is named without an 'embedded_commit_is_stale'/'combined_staleness'/'enforce_build_staleness' substring (wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks) — run explicitly during verification since it exercises all three functions together rather than being a filter target of any single one"

requirements-completed: [17b, 17c, 17d]

coverage:
  - id: D1
    description: "transition() resets state.infra_failures = 0 alongside state.consecutive_failures = 0, closing CR-01"
    requirement: "17b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#transition_resets_infra_failures"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#infra_ceiling_aborts_instead_of_gating"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#rate_limited_at_infra_ceiling_stops_resuming_and_aborts"
        status: pass
    human_judgment: false
  - id: D2
    description: "embedded_commit_is_stale classifies a clean-tree strict-ancestor-of-HEAD build as Stale (only exact HEAD match is Fresh), closing WR-01"
    requirement: "17d"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#embedded_commit_is_stale_maps_ancestry_exit_codes"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks"
        status: pass
    human_judgment: false
  - id: D3
    description: "AC-4 scope narrowing recorded as an attributed override in 17-VERIFICATION.md; ROADMAP.md's Requirements line updated to match"
    requirement: "17c"
    verification:
      - kind: other
        ref: "grep -c '^overrides:'/'accepted_by:'/'overrides_applied: 1' 17-VERIFICATION.md; grep -c '17-VERIFICATION.md' ROADMAP.md"
        status: pass
    human_judgment: false

duration: 25min
completed: 2026-07-19
status: complete
---

# Phase 17 Plan 06: Gap Closure (CR-01, WR-01, AC-4 Override) Summary

**transition() now resets infra_failures alongside consecutive_failures, embedded_commit_is_stale distinguishes exact-HEAD-match from strict-ancestor, and 17-VERIFICATION.md records an attributed override for the disclosed AC-4 scope narrowing**

## Performance

- **Duration:** ~25 min
- **Completed:** 2026-07-19
- **Tasks:** 3 completed
- **Files modified:** 5

## Accomplishments

- **CR-01 closed:** `transition()` now sets `state.infra_failures = 0` in the same statement group as `state.consecutive_failures = 0`, before `workflow::save_state` persists it. A new regression test (`transition_resets_infra_failures`) proves the reset is both in-memory and persisted (reload via `workflow::load_state`), and that a fresh infra fault after a clean transition starts counting from 1, not the pre-transition `MAX_INFRA_FAILURES - 1` count. `state.rs`'s `infra_failures` doc comment and `mode.rs`'s `MAX_INFRA_FAILURES` doc comment now describe the reset-on-transition semantics they previously only aspired to.
- **WR-01 closed:** `embedded_commit_is_stale`'s `merge-base --is-ancestor` exit-0 branch now runs a second check — `git rev-parse HEAD` compared against the trimmed `embedded_commit` — so only an EXACT match to HEAD is Fresh; a strict ancestor of HEAD (the "committed new commits, forgot to rebuild" incident class) is now Stale. The pre-existing `embedded_commit_is_stale_maps_ancestry_exit_codes` test's `base` assertion was corrected from Fresh to Stale (it previously encoded the WR-01 bug), with a new genuine-HEAD-equality Fresh assertion added alongside it. A new test (`wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`) reproduces the verifier's exact linear two-commit, clean-tree fixture and proves `combined_staleness` reports Stale and `enforce_build_staleness` hard-blocks a self-dogfood workspace, with the `self_dogfood_stale_blocked` event recorded before the error.
- **AC-4 scope narrowing formally recorded:** `17-VERIFICATION.md`'s frontmatter gained a top-level `overrides:` list (one entry: `must_have`/`reason`/`accepted_by`/`accepted_at`) in the exact shape the verifier proposed in its Gaps Summary, and `overrides_applied: 0` was bumped to `1`. Gap 1 (CR-01) and Gap 2 (WR-01)'s `gaps:` entries were left byte-for-byte untouched — they close only via a future re-verification pass confirming this plan's code fixes. `ROADMAP.md`'s Phase 17 Requirements line now states the narrowed AC-4 scope (plan-interactivity + Ship-scoped `gh auth` only) and cites `17-VERIFICATION.md` as the source of the accepted override.

## Task Commits

Each task was committed atomically:

1. **Task 1: CR-01 — reset infra_failures on every successful transition** - `cb9ddab` (fix)
2. **Task 2: WR-01 — classify a clean-tree strict-ancestor build as Stale, not Fresh** - `f73a968` (fix)
3. **Task 3: Record the AC-4 scope-narrowing override and update ROADMAP.md's Requirements line** - `d307f72` (docs)

**Plan metadata:** (this commit) `docs(17-06): complete gap-closure plan`

## Files Created/Modified

- `crates/devflow-cli/src/main.rs` - `transition()` gains `state.infra_failures = 0` + doc comment; `embedded_commit_is_stale`'s exit-0 branch gated on `git rev-parse HEAD` equality; `Staleness` enum doc comment updated; corrected `embedded_commit_is_stale_maps_ancestry_exit_codes`; two new regression tests (`transition_resets_infra_failures`, `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`)
- `crates/devflow-core/src/state.rs` - `infra_failures` field doc comment extended with reset-on-transition semantics
- `crates/devflow-core/src/mode.rs` - `MAX_INFRA_FAILURES` doc comment extended with the same
- `.planning/phases/17-pipeline-dogfood-followup/17-VERIFICATION.md` - `overrides:` frontmatter entry added; `overrides_applied: 0` -> `1`
- `.planning/ROADMAP.md` - Phase 17 Requirements line documents the narrowed AC-4 scope

## Decisions Made

- Reset `infra_failures` only in `transition()` (the forward-stage-transition path), not in the gate-driven Advance/LoopBack retry branches of `handle_stage_failure` — those retry the *same* stage after a human gate resolves, which is not the "successful transition" `MAX_INFRA_FAILURES` is scoped to bound. This matches the plan's exact fix site (`transition()` lines 1613-1624) and keeps the ceiling's semantics precisely "bounds a stuck loop across forward progress," not "bounds every retry."
- The new WR-01 regression test's name (`wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`) doesn't share a substring with `embedded_commit_is_stale`/`combined_staleness`/`enforce_build_staleness`/`is_self_dogfood_workspace`, so none of the plan's four filtered `cargo test` invocations pick it up individually — it was run explicitly by name during verification (and is part of the full `cargo test -p devflow` / `cargo test --workspace` runs, both green) to confirm it passes.

## Deviations from Plan

None - plan executed exactly as written. All three tasks' `<action>` and `<behavior>` specifications were implemented literally; no auto-fixes were needed beyond what the plan itself specified.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- All three disclosed/functional gaps from `17-VERIFICATION.md`'s initial pass (score 9/12) are addressed: CR-01 and WR-01 with code fixes + regression tests, AC-4 with a formally attributed override.
- Full workspace test suite (`cargo test --workspace`: 276 devflow-core unit tests + 2 monitor e2e + 61 devflow-cli unit tests + 20 integration tests across 6 suites), `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` are all green on final HEAD.
- Gap 1 (CR-01) and Gap 2 (WR-01) in `17-VERIFICATION.md`'s `gaps:` list still read `status: failed` — that is intentional per the plan's prohibitions; they close only via a future re-verification pass against this plan's commits, not via this plan's own SUMMARY.
- No blockers for Phase 18 (Hermes Support), which the recorded AC-4 override explicitly routes the deferred security-artifact and reviewer-set sub-checks to.

## Self-Check: PASSED

- `crates/devflow-cli/src/main.rs` — FOUND
- `crates/devflow-core/src/state.rs` — FOUND
- `crates/devflow-core/src/mode.rs` — FOUND
- `.planning/phases/17-pipeline-dogfood-followup/17-VERIFICATION.md` — FOUND
- `.planning/ROADMAP.md` — FOUND
- Commit `cb9ddab` — FOUND in `git log --oneline`
- Commit `f73a968` — FOUND in `git log --oneline`
- Commit `d307f72` — FOUND in `git log --oneline`

---
*Phase: 17-pipeline-dogfood-followup*
*Completed: 2026-07-19*
