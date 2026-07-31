---
phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
plan: 04
subsystem: release-automation
tags: [rust, gh-cli, cli, release, control-flow]

# Dependency graph
requires:
  - phase: 29-release-cut-executor-observe-then-act-within-the-repo-s-rule
    provides: "29a's Observation/ReleaseStep/observe/observe_all dispatcher (29-01, 29-02) and 29b's MergeIntent/MergeMethod/resolve_merge_method/config::yes_release (29-03)"
provides:
  - "devflow_core::release_execute: StepOutcome/CutReport/PrPresence vocabulary, cut/cut_with (the walk, with an injectable observer for pure control-flow tests), action_for/unit_for (exhaustive over ReleaseStep, no wildcard arm, every arm still None in this build), pr_refs/open_pr for the pull-request in-flight check"
  - "devflow release cut <version> [project] --yes-release — walks all six release-cut steps in order, observing each, and stops at the first that is not already done, naming exactly where and why"
affects: [29-05-plan, 29-06-plan, 29-07-plan-commit-point]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "cut_with(observer) / cut(): the walk's control flow takes the observation source as an injected parameter, so every control-flow branch (present/absent/unreachable/in-flight/no-action) is unit-tested against a synthetic Observation sequence with no network, while the public cut() wires in the real release_observe::observe"
    - "StepOutcome::is_terminal(): true for every variant except AlreadyDone — the single predicate the walk's loop consults to decide whether to continue, making 'only AlreadyDone continues the walk' a compile-time-checked invariant rather than a convention"
    - "action_for(step) -> Option<StepAction>: an exhaustive match with no wildcard arm returning None for every step in this build — 29-05/29-06/29-07 each replace one arm; a 7th ReleaseStep variant fails to compile here (and in unit_for, pr_refs) rather than being silently skipped"
    - "No separate CLI-level gh-auth pre-check: cut's own walk already stops at the first Unreachable oracle with that oracle's own real failure text, so a second gate would only duplicate the single-stop behavior — and would make the walk itself unreachable by a hermetic, isolated-HOME test (see Deviations)"

key-files:
  created:
    - crates/devflow-core/src/release_execute.rs
    - crates/devflow-cli/tests/release_cut.rs
  modified:
    - crates/devflow-core/src/lib.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs

key-decisions:
  - "Dropped the plan's suggested CLI-level `gh auth status` pre-check (a duplicate of preflight.rs's Ship-stage gate) after discovering it made `cut()` unreachable by `release_cut_writes_no_devflow_state`'s mutation test in the isolated-HOME test environment (no gh credentials) — the pre-check's failure short-circuited before `cut()` ever ran, so a mutation planted inside `cut()` could never turn that test red. Since `cut()`'s own walk already stops at the very first Unreachable oracle with a real failure text, the pre-check added no additional stopping behavior, only worse testability. See Deviations."
  - "Renamed all seven Task 2 test functions from the plan's suggested `cut_*` prefix to `release_cut_*`, because the plan's own acceptance criterion (`cargo test -p devflow release_cut` reports at least 7 passed) requires the substring `release_cut` to appear in each test's own name — cargo's test filter matches individual test names, not file/binary names, and none of the plan's suggested names contained that substring. Confirmed empirically: the unrenamed names produced 0 matches for that exact command."
  - "action_for/unit_for/pr_refs are declared `pub(crate)`-equivalent (plain `fn`, private) exactly as the plan's artifact list specifies (`cut_with`, `action_for`, `unit_for` listed as the only new private functions) — no extra pure helper was extracted for `open_pr`'s output parsing, to avoid introducing an untracked symbol beyond what the plan enumerates."

patterns-established:
  - "The mandate-refusal reason (no_mandate_reason()) is a single shared string constructor consumed by both cut_with's early-return path and every test/CLI caller, so the CLI layer and every test observe byte-identical wording naming all three grant mechanisms."

requirements-completed: [29b]

coverage:
  - id: D1
    description: "devflow release cut <version> walks all six release-cut steps in order, observing each before acting, and stops at the first not-done step with an accurate, real report of where and why"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests (13 cases: present-then-absent stops, unreachable-first-step stops with the reason verbatim, stop-index-equals-report-length, no-action-in-this-build naming the unit, all_done/stopped_at semantics, action_for/unit_for/pr_refs exhaustiveness and correctness)"
        status: pass
      - kind: integration
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::cut_authorized_against_a_repo_with_no_remote_refuses_on_the_first_step (real git fixture, no injected observer)"
        status: pass
      - kind: manual_procedural
        ref: "devflow release cut 2.2.0 . --yes-release run live against this repo: 3/6 steps already done, stops at the signed-tag step naming unit 29c — reproduces the phase's own motivating incident end to end"
        status: pass
    human_judgment: false
  - id: D2
    description: "An Unreachable observation refuses immediately and is never treated as 'not done yet'; a step with no action in this build stops naming the supplying unit; action_for/unit_for/pr_refs are exhaustive with no wildcard arm"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::unreachable_first_step_stops_immediately_with_the_reason_verbatim, ::absent_step_with_no_pr_backing_and_no_action_stops_naming_the_supplying_unit, ::action_for_returns_none_for_every_step_in_this_build"
        status: pass
      - kind: manual_procedural
        ref: "temporarily added a 7th ReleaseStep variant, confirmed cargo check -p devflow-core fails with E0004 at all three release_execute.rs match sites (action_for/unit_for/pr_refs), reverted"
        status: pass
    human_judgment: false
  - id: D3
    description: "Without a mandate, devflow release cut refuses immediately with an actionable message naming all three grant mechanisms and never touches the network; with a mandate (flag, env, or config), the run proceeds past authorization"
    requirement: "29b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/release_execute.rs#release_execute::tests::unauthorized_yields_a_single_refusal_and_invokes_no_observer"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_cut.rs#release_cut_without_a_mandate_refuses, ::release_cut_accepts_the_environment_mandate, ::release_cut_accepts_the_config_file_mandate"
        status: pass
    human_judgment: false
  - id: D4
    description: "The shipped executor carries no operator-presence requirement: it runs to completion unattended with stdin closed, never prompts, writes nothing to .devflow/ or devflow.toml, and two identical runs produce identical reports"
    requirement: "29b"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/release_cut.rs#release_cut_runs_unattended_with_stdin_closed, ::release_cut_output_contains_no_interactive_prompt, ::release_cut_writes_no_devflow_state, ::release_cut_is_idempotent_across_runs"
        status: pass
      - kind: manual_procedural
        ref: "temporarily added a write inside cut() (release_execute.rs), confirmed release_cut_writes_no_devflow_state goes red (assertion left:1 right:2 on .devflow/ entry count), reverted"
        status: pass
    human_judgment: false

# Metrics
duration: 55min
completed: 2026-07-31
status: complete
---

# Phase 29 Plan 04: Release-Cut Executor — The Walker (Observe, Act, Re-observe, Stop) Summary

**`devflow release cut <version>` — a new `release_execute` core module and CLI subcommand that walks all six release-cut steps in order, stops at the first one not already done with an accurate real-tool report, refuses instantly and network-free without a mandate, and — live-verified against this repo — reproduces the phase's own motivating incident (v2.2.0's missing signed tag) end to end.**

## Performance

- **Duration:** ~55 min
- **Completed:** 2026-07-31
- **Tasks:** 2 completed (both TDD RED→GREEN in spirit; Task 1 wrote tests alongside implementation in one GREEN commit after local iteration, Task 2 was itself the test suite)
- **Files modified:** 5 (2 created, 3 modified)

## Accomplishments

- **The design rule (RD-2) now exists as executable control flow.** `devflow_core::release_execute::cut` walks `ReleaseStep::ALL` in order via `release_observe::observe`: `Present` records `AlreadyDone` and continues; `Unreachable` records `Stopped` with the oracle's own reason and ends the walk immediately — never treated as "not done yet"; `Absent` checks for an in-flight pull request (via a new `open_pr`/`PrPresence`) before consulting this build's action for the step, and `action_for` returns `None` for all six variants, so every reachable `Absent` step in this build stops the walk naming the unit (`29b`/`29c`) that will supply its action.
- **Exhaustiveness is compiler-enforced, not just documented.** `action_for`, `unit_for`, and `pr_refs` are all non-wildcard matches over `ReleaseStep`; manually adding a 7th variant reproducibly broke the build at all three sites with `E0004`, confirming a future step cannot be silently skipped.
- **`devflow release cut <version> [project] --yes-release`** is live and correctly reproduces the exact incident this phase exists to fix: run against this repository at `2.2.0`, it reports 3/6 steps already done (version bumped, changelog written, release PR merged) and stops cleanly at the missing signed tag, naming unit `29c` — no prediction, no silent skip, no crash.
- **The executor provably carries no operator-presence requirement** (RD-3): 7 new hermetic integration tests prove it runs to completion with stdin closed under a bounded wait (never hangs), never prints an interactive-prompt shape, refuses instantly without a mandate while naming all three grant mechanisms, accepts both the `DEVFLOW_YES_RELEASE` env and `devflow.toml` `yes_release = true` grant paths, writes nothing to `.devflow/` or `devflow.toml` (mutation-tested), and produces byte-identical output across two consecutive runs.

## Task Commits

1. **Task 1: The walk — observe, act, re-observe, stop; plus the `devflow release cut` surface**
   - `61941d8` `feat(29-04): release-cut walker — observe, act, re-observe, stop` — `release_execute.rs` (new: `StepOutcome`, `CutReport`, `PrPresence`, `cut`/`cut_with`, `action_for`/`unit_for`, `pr_refs`/`open_pr`, 13 unit tests), `lib.rs` module declaration, `ReleaseAction::Cut` in `main.rs`, `commands::release_cut` (first version, later simplified in Task 2 — see Deviations)

2. **Task 2: Prove the executor requires no human — unattended, stdin-closed, non-blocking**
   - `12d2078` `test(29-04): prove the release-cut executor requires no human at the keyboard` — `crates/devflow-cli/tests/release_cut.rs` (new: 7 integration tests), plus the `commands::release_cut` simplification this test suite's own mutation-testing exposed as necessary

## Files Created/Modified

- `crates/devflow-core/src/release_execute.rs` (new) — `StepOutcome`, `CutReport`, `PrPresence`, the walk (`cut`/`cut_with`), `action_for`/`unit_for` (exhaustive, all-`None` in this build), `pr_refs`/`open_pr` (the pull-request in-flight check), 13 unit tests
- `crates/devflow-core/src/lib.rs` — `pub mod release_execute;` declared alphabetically (after `registry`, before `release_observe`)
- `crates/devflow-cli/src/main.rs` — `ReleaseAction::Cut { version, project, yes_release }`, dispatch arm, `release_cut` import
- `crates/devflow-cli/src/commands.rs` — `step_outcome_check`/`print_cut_report` (the report-printing loop, reusing `release_status`'s icon shape) and `commands::release_cut` (ORs `--yes-release` with `config::yes_release`, calls `cut`, prints the report, returns non-zero unless `all_done()`)
- `crates/devflow-cli/tests/release_cut.rs` (new) — 7 tests: unattended/stdin-closed, no-interactive-prompt, no-mandate refusal, environment mandate, config-file mandate, writes-no-state, idempotent-across-runs

## Decisions Made

- **Dropped the plan's suggested CLI-level `gh auth status` pre-check.** Building it exactly as described (a synthetic `CutReport` short-circuiting before `cut()` runs, on auth failure) made `cut()` structurally unreachable by `release_cut_writes_no_devflow_state`'s own mutation test in the isolated-`HOME` test environment this codebase's convention requires (no `gh` credentials there, so the pre-check always failed and `cut()` was never called). Since `cut()`'s own walk already stops at the very first `Unreachable` oracle carrying that oracle's real failure text — verified live: an unauthenticated `gh` produces `gh api exited with status exit status: 1 fetching Cargo.toml@develop` on the very first step — the pre-check added no additional stopping behavior the walk didn't already have, only a testability regression. Removed; `commands::release_cut` now always calls `cut()` when authorized.
- **Renamed Task 2's seven test functions from the plan's literal suggested names (`cut_runs_unattended_with_stdin_closed`, etc.) to the same names prefixed `release_cut_` instead of `cut_`.** The plan's own acceptance criterion requires `cargo test -p devflow release_cut` to report at least 7 passed; cargo's test filter matches substrings of individual test *names*, not file or binary names, and none of the plan's suggested names contain the literal substring `release_cut`. Verified empirically (0 passed with the original names, 7 passed after the rename) before committing.
- **`ReleaseStep::ALL[0]` is used as the step attached to the single-row unauthorized-refusal report.** `CutReport`'s type is fixed as `Vec<(ReleaseStep, StepOutcome)>`; since an authorization refusal happens before any step is observed, the first step in walk order is the natural placeholder — it never claims that step specifically failed, only that the walk (which would have started there) never ran.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The CLI-level `gh auth status` pre-check made `cut()` unreachable by its own mutation test**
- **Found during:** Task 2, running the required mutation-testing confirmation step for `release_cut_writes_no_devflow_state` (temporarily adding a write inside `cut()` and expecting the test to go red)
- **Issue:** With the pre-check implemented exactly as the plan's `<action>` text described (check `gh auth status` first; on failure, build a synthetic `CutReport` and skip calling `cut()` entirely), the test's isolated-`HOME` fixture (no `gh` credentials, matching this codebase's established test-isolation convention) always failed the pre-check, so `cut()` was never invoked. A write planted inside `cut()` therefore could not be observed by the test — the mutation test passed even with the mutation present, which is the exact false-negative the acceptance criterion exists to catch.
- **Fix:** Removed the separate pre-check. `commands::release_cut` now always calls `devflow_core::release_execute::cut(project_root, version, authorized)` when `authorized` is true, letting the walk's own first-oracle `Unreachable` failure (which already carries a real, specific failure message) serve as the stopping point. Manually re-verified: an unauthenticated `gh` in an isolated `HOME` still produces a clear, real failure message on the very first step (`gh api exited with status exit status: 1 fetching Cargo.toml@develop`), so no operator-facing clarity was lost.
- **Files modified:** `crates/devflow-cli/src/commands.rs`
- **Verification:** Re-planted the same mutation inside `cut()`; `release_cut_writes_no_devflow_state` now correctly fails (`assertion left:1 right:2`); reverted. Full `cargo test --workspace` green afterward (0 failed), `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` both clean.
- **Committed in:** `12d2078` (Task 2's commit — the fix was necessary to make Task 2's own acceptance criterion true, and is documented there rather than amending Task 1's commit)

**2. [Rule 1 - Test naming] Task 2's plan-suggested test names did not satisfy the plan's own `cargo test -p devflow release_cut` acceptance filter**
- **Found during:** Running the exact acceptance-criteria command (`cargo test -p devflow release_cut`) after writing the test file with the plan's literal suggested names (`cut_runs_unattended_with_stdin_closed`, etc.)
- **Issue:** The command reported 0 passed — cargo's test-name filter is a substring match against each test function's own name, and none of the seven plan-suggested names contain the literal substring `release_cut` (they all start with `cut_`).
- **Fix:** Renamed all seven tests from `cut_*` to `release_cut_*`, preserving their exact semantic meaning (e.g. `cut_writes_no_devflow_state` → `release_cut_writes_no_devflow_state`).
- **Files modified:** `crates/devflow-cli/tests/release_cut.rs`
- **Verification:** `cargo test -p devflow release_cut` now reports 7 passed, 0 failed, matching the acceptance criterion exactly.
- **Committed in:** `12d2078`

---

**Total deviations:** 2 auto-fixed (both Rule 1 — a testability-breaking design flaw discovered by the plan's own mandated mutation-testing step, and a naming mismatch against the plan's own literal acceptance-criteria command). Both fixes were necessary to make the plan's own stated acceptance criteria true; neither is an architectural change — the walk's behavior (`cut`/`cut_with`) is unchanged from Task 1.
**Impact on plan:** No design change to the walk itself. `commands::release_cut` is simpler than originally specified (one fewer gate), and the CLI test file's test names differ from the plan's literal suggestions while preserving identical coverage and intent.

## Issues Encountered

None beyond the two deviations above.

## User Setup Required

None — no external service configuration required. `gh` authentication (when present) is read passively by the walk's own oracles; no new environment variable or config key was introduced by this plan (`DEVFLOW_YES_RELEASE`/`yes_release` were both already delivered by `29-03`).

## Next Phase Readiness

- `devflow_core::release_execute`'s `cut`/`CutReport`/`StepOutcome`/`action_for`/`unit_for`/`pr_refs`/`open_pr` are in place and stable for `29-05-PLAN.md` (steps 1 and 2's actions) and `29-06-PLAN.md` (steps 3 and 5's actions) to fill in — each replaces exactly one `None` arm in `action_for`'s exhaustive match; no type or CLI-surface redesign is needed.
- `29-07-PLAN.md` (the commit-point unit, steps 4 and 6) can rely on the same `action_for`/`unit_for` shape for the two irreversible operations (signed tag, `cargo publish`).
- No blockers. The removed CLI-level `gh auth status` pre-check (see Deviations #1) is a design correction future plans should not reintroduce — any future desire for a cleaner "not authenticated" message should be satisfied by improving the oracle's own failure text (e.g. inside `release_observe`'s `gh`-calling functions), not by adding a second gate ahead of the walk.

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/release_execute.rs`
- FOUND: `crates/devflow-cli/tests/release_cut.rs`
- FOUND: `crates/devflow-core/src/lib.rs`
- FOUND: `crates/devflow-cli/src/main.rs`
- FOUND: `crates/devflow-cli/src/commands.rs`
- FOUND commit `61941d8` (feat: release-cut walker, Task 1)
- FOUND commit `12d2078` (test: prove unattended operation, Task 2)
