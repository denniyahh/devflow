---
phase: 23-end-to-end-dogfood
plan: 12
subsystem: cli-preflight
tags: [devflow, preflight, guard, git, dogfood, gap-closure, rust]

requires:
  - phase: 23-end-to-end-dogfood
    provides: "23-VERIFICATION.md's identification of the one unmet truth (behavioral acceptance criterion) and the 2026-07-26 acceptance-run failure record (23-FINDINGS.md §B1)"
provides:
  - "PhaseReachability probe + ensure_phase_reachable_on_base guard in preflight.rs, refusing devflow start before any git mutation when a phase is unreachable from develop"
  - "Wired call site in commands::start, ahead of both fork paths (worktree mode and --no-worktree) and ahead of the Codex CONTEXT.md check"
  - "start_reachability_e2e.rs — 2 real end-to-end tests against the compiled binary"
  - "8 discrimination/fail-open unit tests in preflight::tests"
  - "CHANGELOG.md / OPERATIONS.md documentation of the new refusal"
affects: ["23-13", "23-14", "23-15"]

tech-stack:
  added: []
  patterns:
    - "Fail-open-where-blind preflight guard: Undeterminable (no repo, no base branch, no ROADMAP.md at all) always returns Ok — mirrors commands::phase_artifact_on_develop's existing contract"
    - "Pure message-builder function (unreachable_message) separated from the git-probing function (phase_reachability_on_base), so message content is unit-testable without spawning git"

key-files:
  created:
    - crates/devflow-cli/tests/start_reachability_e2e.rs
  modified:
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs
    - CHANGELOG.md
    - OPERATIONS.md

key-decisions:
  - "Guard fails open when develop carries no .planning/ROADMAP.md at all — required so phase7_cli.rs's ROADMAP-less fixtures (and any repo that doesn't keep a roadmap) stay green; the real 2026-07-26 failure is still caught because develop DID have a ROADMAP.md, just missing the Phase 24 heading."
  - "Guard call site placed after ensure_agent_binary and before the Codex CONTEXT.md check in commands::start, so it precedes both fork paths (ensure_phase_worktree and GitFlow::feature_start) and pre-empts the Codex leg's narrower, misleading diagnosis of the same root cause."
  - "Guard emits no event and writes no .devflow/ file — matches the existing pre-state Codex leg's CliError::Message contract; devflow start is always operator-typed or reached via devflow parallel, never the forked advance tail."

requirements-completed: [23f]

coverage:
  - id: D1
    description: "devflow start refuses, before any git mutation, when the target phase is unreachable from develop (missing ROADMAP heading and/or missing phase directory)"
    requirement: "23f"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_reachability_e2e.rs#start_refuses_a_phase_promoted_only_on_the_working_branch_and_scaffolds_nothing"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/tests/start_reachability_e2e.rs#start_refuses_before_creating_the_feature_branch_in_no_worktree_mode"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::reachability_is_unreachable_when_the_phase_dir_is_absent_from_base"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::reachability_is_unreachable_when_the_roadmap_entry_is_absent_from_base"
        status: pass
    human_judgment: false
  - id: D2
    description: "The guard fails open (never refuses) when develop cannot be inspected: no base branch, not a repo, or no .planning/ROADMAP.md at all — the pre-existing phase7_cli.rs suite (whose fixtures never commit a ROADMAP.md) passes unchanged"
    requirement: "23f"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::reachability_is_undeterminable_outside_a_git_repo"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::reachability_is_undeterminable_when_base_has_no_roadmap_at_all"
        status: pass
      - kind: integration
        ref: "cargo test -p devflow --test phase7_cli"
        status: pass
    human_judgment: false
  - id: D3
    description: "The refusal message names the base branch and each missing half, and leaks no absolute filesystem path or username (999.10 leak class)"
    requirement: "23f"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::unreachable_message_names_the_base_branch_and_each_missing_half"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#preflight::tests::unreachable_message_contains_no_absolute_path"
        status: pass
    human_judgment: false

duration: 30min
completed: 2026-07-26
status: complete
---

# Phase 23 Plan 12: Phase-Reachability Preflight Guard Summary

**`devflow start` now refuses — before creating any worktree or branch — when the target phase's ROADMAP heading or `.planning/phases/NN-*/` directory is absent from `develop`, closing the exact 2026-07-26 acceptance-run failure by construction.**

## Performance

- **Duration:** ~30 min
- **Started:** 2026-07-26T16:34:00-04:00 (approx.)
- **Completed:** 2026-07-26T16:42:00-04:00
- **Tasks:** 3/3 completed
- **Files modified:** 5 (2 created/modified source, 1 new test file, 2 docs)

## Accomplishments

- Added `PhaseReachability` (`Reachable` / `Undeterminable` / `Unreachable { roadmap_entry_found, phase_dir_found }`), `phase_reachability_on_base`, `unreachable_message`, and `ensure_phase_reachable_on_base` to `crates/devflow-cli/src/preflight.rs` — a probe run before `devflow start` forks anything.
- Wired the guard into `commands::start` immediately after `ensure_agent_binary` and before the Codex CONTEXT.md pre-flight, so it precedes **both** fork paths (`ensure_phase_worktree` in worktree mode, `GitFlow::feature_start` in `--no-worktree` mode).
- Proved the guard end-to-end against the real compiled binary in a new `crates/devflow-cli/tests/start_reachability_e2e.rs`, including a genuine RED-then-GREEN cycle (RED captured below) and a second test proving the refusal precedes `--no-worktree`'s branch creation too.
- Added 8 discrimination/fail-open unit tests in `preflight::tests`, proving the guard distinguishes "roadmap heading missing" from "phase dir missing" from "cannot see at all," and that a phase-number prefix (`### Phase 240:`) cannot be confused with `### Phase 24:`.
- Documented the refusal in `CHANGELOG.md` (new `### Added` subsection) and `OPERATIONS.md` (amended `devflow start` row + new `## Preflight refusals` section).
- Confirmed zero regressions: the pre-existing `phase7_cli.rs` suite (whose fixtures never commit a `ROADMAP.md`) reports the identical `17 passed; 0 failed` both before and after the Task 3 doc changes; full workspace suite is `608 passed; 0 failed; 0 ignored`.

## RED Evidence (Task 1, Step 2 — required by change-acceptance requirement 3)

Ran `cargo test -p devflow --test start_reachability_e2e start_refuses_a_phase_promoted_only_on_the_working_branch_and_scaffolds_nothing -- --exact` against the tree with the test committed but the guard **not yet wired**. It failed on the intended assertion — not a compile error, missing import, or fixture panic:

```
thread 'start_refuses_a_phase_promoted_only_on_the_working_branch_and_scaffolds_nothing' panicked at crates/devflow-cli/tests/start_reachability_e2e.rs:136:5:
devflow start must refuse an unreachable phase, but exited successfully
stdout:
created worktree: /tmp/.tmpgUmctZ/.worktrees/phase-24 (branch feature/phase-24)
warning: build provenance staleness check did not confirm a fresh build for stage define — proceeding (only DevFlow's own workspace is ever hard-blocked, D-18)
stage define → launched Claude Code (monitor pid 2203592)
started phase 24 in auto mode at 1785098155 — monitor will auto-advance
  watch live: devflow logs -f --phase 24

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

The failing assertion was exactly `!output.status.success()` — proof the real, unwired binary proceeded to create a worktree, save state, and spawn a monitor for the unreachable phase. This is the intended-reason RED: the guard was absent, so `start` did exactly what the pre-2026-07-26-fix binary did. After wiring (Step 3-4), the same invocation reports `test result: ok. 1 passed; 0 failed`.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end "devflow start refuses an unreachable phase and scaffolds nothing"** — `fdc0a3d` (feat)
2. **Task 2: Discrimination — the guard must refuse for the right reason and fail open where it cannot see** — `fbf535b` (test)
3. **Task 3: Reconcile the operator docs and prove the guard broke nothing** — `9ebde0b` (docs)

**Plan metadata:** SUMMARY commit follows this file's creation (worktree mode — orchestrator handles STATE.md/ROADMAP.md).

## Files Created/Modified

- `crates/devflow-cli/src/preflight.rs` — `PhaseReachability` enum, `phase_reachability_on_base`, `unreachable_message`, `ensure_phase_reachable_on_base`, plus 8 new unit tests
- `crates/devflow-cli/src/commands.rs` — one new call to `ensure_phase_reachable_on_base(project_root, phase, DEVELOP)` in `start()`, plus the import
- `crates/devflow-cli/tests/start_reachability_e2e.rs` (new) — 2 real end-to-end tests against the compiled binary
- `CHANGELOG.md` — new `### Added` subsection under `## 2.0.0 — 2026-07-26`
- `OPERATIONS.md` — amended `devflow start` row + new `## Preflight refusals` section

## Decisions Made

- **Fail-open on no-ROADMAP-at-all** (design decision baked into the plan, confirmed against live source): verified `phase7_cli.rs`'s `init_repo` commits phase directories but never a `ROADMAP.md` — exactly the fixture shape the fail-open decision protects. Re-ran `phase7_cli` after wiring the guard: unchanged `17 passed; 0 failed`.
- **Call-site placement**: placed the guard call between `ensure_agent_binary` and the `if agent == AgentKind::Codex` block, per the plan's explicit instruction, with an inline comment documenting both properties this placement buys (precedes both fork paths; pre-empts the narrower Codex diagnosis). Left the `dry_run` early return untouched, per plan scope.
- **Zero new dependencies** — the probe uses only `std::process::Command` (already imported patterns in this file), consistent with the plan's threat-model disposition (T-23f-05).

## Deviations from Plan

None — plan executed exactly as written. All acceptance criteria for all three tasks were met without needing Rule 1-4 deviations.

## Issues Encountered

None. The RED capture in Task 1 fired for the intended reason on the first attempt (no fixture debugging needed). All discrimination tests in Task 2 passed on first run. The full gate chain in Task 3 was green on first run.

**Observed unrelated gap (noted, not fixed, per plan's explicit scope boundary):** `OPERATIONS.md`'s `devflow start` row does not document the `--yes-ship` flag — a pre-existing gap left by plan 23-09. Confirmed still absent (`rg -n "yes-ship" OPERATIONS.md` — no match) and deliberately left unfixed here, as instructed.

## Baseline Comparison (Task 3)

- **`phase7_cli` (the guard's real over-fire regression net) — before and after the Task 3 doc changes:** `17 passed; 0 failed` both times (docs-only change, no behavior difference expected or observed).
- **Full workspace suite:** `cargo test --workspace --no-fail-fast` → `608 passed; 0 failed; 0 ignored` (final aggregate across all binaries/integration-test targets). This is higher than the `592 passed / 0 failed / 0 ignored` baseline recorded in `23-VERIFICATION.md` — that baseline predates this plan's own 10 new tests (2 e2e + 8 preflight unit tests) and also predates a handful of already-landed gap-closure fix/test commits on this branch since 23-VERIFICATION was written (`fac8f7e`, `a5487dc`, `73887ce`) that are outside this plan's scope. No `test result: FAILED` line appeared anywhere in the log.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean (exit 0).
- `cargo fmt --check`: clean (exit 0).

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

The guard is wired and proven end-to-end on this branch (`worktree-agent-a2bc6de1d10af42cd`), but **not yet on `develop`** — this plan's commits live only in this worktree. Per the plan's own sequencing note (`STATE.md`), `23-13` is responsible for merging this guard behind a blocking operator checkpoint and proving the *shipped binary* refuses. `23-14` re-measures every precondition on the post-merge tree, and `23-15` retries the Phase 24 acceptance run. No blockers identified for that sequencing — this plan's `must_haves.truths` are all satisfied by the committed code and tests, and the full gate chain is green.

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-26*
