---
phase: 19-release-integrity-main-rs-decomposition
plan: 08
subsystem: cli
tags: [rust, main-rs-split, pure-move, cargo-test, clippy, pipeline]

# Dependency graph
requires:
  - phase: 19-release-integrity-main-rs-decomposition
    provides: "19-06 (pub(crate) visibility pass, test_support.rs ENV_MUTEX/fixtures hoist, committed baseline SHA + name lists); 19-07 (staleness.rs/preflight.rs extraction, proved the mechanical extraction procedure, deferred content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root's attribution to this plan)"
provides:
  - "crates/devflow-cli/src/pipeline_launch.rs, pipeline_outcomes.rs, pipeline_gate.rs as the three D-06 pipeline seams, byte-identical to their pre-move bodies modulo pub(crate)/use-path/rustfmt-reflow"
  - "The three-way module cycle (launch -> outcomes -> gate -> launch) preserved as direct pub(crate) calls, no callback/trait indirection, documented in each module's doc comment"
  - "main.rs reduced from the phase's 8,467-line starting figure to 3,313 lines — the pipeline state machine's largest cluster is fully evacuated"
affects: [19-09, 19-10, 19-11, phase-20-pipeline-work]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Three-way cyclic module reference across sibling modules (pipeline_launch <-> pipeline_outcomes <-> pipeline_gate), legal in Rust since only the crate dependency graph must be acyclic — documented explicitly as NOT a wave-parallelism promise for future pipeline work (19-RESEARCH.md Pitfall 1, asserted in pipeline_gate.rs's own source)"
    - "Test-only imports (AgentKind, Mode, Stage duplicates, Gates, GitFlow, prompt module) added inside each module's own #[cfg(test)] mod tests block rather than at production-code module scope, since a binary-only crate's plain `cargo build` never sees #[cfg(test)] content and would otherwise flag them unused"
    - "rustfmt-driven multi-line signature wrap (plus idiomatic trailing comma) is an accepted, mechanical, explained hunk type when a moved function's signature crosses rustfmt's line-length limit only because of the added pub(crate) prefix — distinct from and narrower than a real edit"

key-files:
  created:
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/preflight.rs

key-decisions:
  - "Test attribution for tests directly exercising more than one seam's functions followed the plan's own attribution rule (\"by the production function each test calls,\" verified by reading the body) rather than the plan's illustrative name-wildcard lists where the two conflicted: consecutive_failures_are_independent_across_phases moved to seam C (pipeline_gate) alongside the plan's explicitly-named repeated_code_to_validate_transition_is_idempotent_on_the_counter, since both call transition() directly with no handle_*_outcome involvement, structurally identical to the plan's own reasoning for the named test."
  - "content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root (19-07's deferred attribution) landed in pipeline_outcomes.rs (seam B) in Task 2 — its assertions are exclusively against hook_context_root, confirmed by reading the body, closing the 19-07 deferral as instructed."
  - "hook_context_root, transition, and loop_back_to_code each have exactly one rustfmt-driven multi-line signature wrap (plus the idiomatic trailing comma rustfmt adds) as their only non-pub(crate) delta — recorded as an explained hunk class, not a retype: the added `pub(crate) ` prefix pushed each already-near-limit single-line signature over rustfmt's wrap threshold."

patterns-established:
  - "Per-module equivalence proof: extract each moved function/enum/test from the committed baseline SHA and from the new module by AST-adjacent brace-matching (not line-range guessing), normalize only the pub(crate) prefix, and diff — run once per task/seam immediately after that task's commit, not deferred to plan end."

requirements-completed: [19d, 19e]

coverage:
  - id: D1
    description: "Seam A (launch/advance: launch_stage_inner, launch_stage, resume, single_active_phase, resolve_sole_active_phase, advance) moved into pipeline_launch.rs, byte-identical modulo pub(crate)/use-path"
    requirement: "19d"
    verification:
      - kind: other
        ref: "per-function diff vs baseline SHA f35d6c1: 6/6 functions IDENTICAL modulo pub(crate); 4 attributed tests IDENTICAL byte-for-byte"
        status: pass
      - kind: unit
        ref: "cargo test --workspace after Task 1: 438/438 trailing test names match 19-SPLIT-BASELINE-names.txt, 0 failed across all 11 targets"
        status: pass
    human_judgment: false
  - id: D2
    description: "Seam B (handle_*_outcome family, ValidateOutcome/ValidateResult, checkout-hook batch, rendering helpers) moved into pipeline_outcomes.rs; 19-07's deferred test attribution resolved"
    requirement: "19d"
    verification:
      - kind: other
        ref: "per-function diff vs baseline SHA f35d6c1: 14/15 IDENTICAL modulo pub(crate), hook_context_root has 1 explained rustfmt-wrap hunk; 30/30 moved tests+helpers IDENTICAL byte-for-byte"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow x3 consecutive runs post-move: 0 failed each time, no D-12 ENV_MUTEX finding; cargo test --workspace 438/438 names match baseline"
        status: pass
    human_judgment: false
  - id: D3
    description: "Seam C (transition, loop_back_to_code, prepare_loop_back_to_code, finish_workflow, run_gate, abort, print_dry_run) moved into pipeline_gate.rs, closing the three-way module cycle with a direct pipeline_launch::launch_stage call"
    requirement: "19d"
    verification:
      - kind: other
        ref: "per-function diff vs baseline SHA f35d6c1: 5/7 IDENTICAL modulo pub(crate), transition and loop_back_to_code have the same explained rustfmt-wrap hunk class; 7/7 moved tests IDENTICAL byte-for-byte"
        status: pass
      - kind: other
        ref: "rg -c 'pipeline_launch::launch_stage' pipeline_gate.rs == 2 (cycle closed by direct call); rg -c 'Box<dyn Fn|Arc<dyn|trait .*Callback' across all three modules == 0 (no indirection introduced); rg -c 'two or three of these files' pipeline_gate.rs == 1 (Pitfall 1 caveat recorded in source)"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow x3 consecutive runs post-move: 0 failed each time; cargo test --workspace 438/438 names match baseline; wc -l main.rs == 3313 (down from phase-start 8,467)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Zero tests lost, gained, renamed, or split across all three tasks; final tree green (build/clippy/fmt) after each task"
    requirement: "19e"
    verification:
      - kind: other
        ref: "cargo test --workspace -- --list trailing-name-set (438 entries) diffed empty against committed 19-SPLIT-BASELINE-names.txt after Task 1, Task 2, and Task 3"
        status: pass
      - kind: other
        ref: "cargo build -p devflow, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --check all exit 0 after each of the three task commits"
        status: pass
    human_judgment: false

duration: 37min
completed: 2026-07-22
status: complete
---

# Phase 19 Plan 08: pipeline state machine split (seams A/B/C) Summary

**Split the pipeline state machine — main.rs's largest and most serialization-heavy cluster — into three sibling modules (pipeline_launch.rs, pipeline_outcomes.rs, pipeline_gate.rs) at the D-06 seams, taking main.rs from 8,467 to 3,313 lines while preserving the launch→outcomes→gate→launch cycle as direct pub(crate) calls across all three tasks with zero unexplained diffs against the committed baseline.**

## Performance

- **Duration:** 37 min
- **Started:** 2026-07-22T11:46:14Z
- **Completed:** 2026-07-22T12:23:04Z (this commit)
- **Tasks:** 3
- **Files modified:** 5 (`main.rs`, `preflight.rs`, plus 3 new files created)

## Accomplishments
- Re-derived each seam's current production line range live at its own task's HEAD (never reused the stale `19-SPLIT-BASELINE.md` ranges) for all three tasks: seam A at 677–1002, seam B at 677–1153 (post-Task-1 HEAD), seam C at 677–921 (post-Task-2 HEAD).
- Extracted seam A (`launch_stage_inner`, `launch_stage`, `resume`, `single_active_phase`, `resolve_sole_active_phase`, `advance`) into `pipeline_launch.rs` with its 4 attributed tests, repointing `preflight.rs`'s bidirectional 18-07 coupling to `crate::pipeline_launch::{launch_stage, launch_stage_inner}`.
- Extracted seam B (the `handle_*_outcome` family, `ValidateOutcome`/`ValidateResult`, the checkout-hook batch, and the rendering helpers) into `pipeline_outcomes.rs` with 27 attributed tests plus 3 shared test helpers, closing 19-07's deferred `content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root` attribution.
- Extracted seam C (`transition`, `loop_back_to_code`, `prepare_loop_back_to_code`, `finish_workflow`, `run_gate`, `abort`, `print_dry_run`) into `pipeline_gate.rs` with its 7 attributed tests, adding `use crate::pipeline_launch::launch_stage;` — the import that closes the three-way cycle — and writing the Pitfall 1 caveat directly into the module's doc comment (asserted by a source grep).
- Ran the per-function equivalence proof against the committed baseline SHA (`f35d6c1`) for each of the three seams immediately after its own commit: 28 production items and 41 tests/helpers, all clean modulo `pub(crate)` or one explained rustfmt-reflow class (3 functions: `hook_context_root`, `transition`, `loop_back_to_code`), zero unexplained hunks.
- Verified the trailing test-name set (438 entries) empty-diffs against the committed `19-SPLIT-BASELINE-names.txt` after every one of the three tasks, and `cargo test -p devflow` was run 3 consecutive times after Tasks 2 and 3 with 0 failed each time — no D-12 `ENV_MUTEX` finding.

## Task Commits

Each task was committed atomically:

1. **Task 1: Extract seam A (launch/advance) into `pipeline_launch.rs`** — `cdb7368` (refactor)
2. **Task 2: Extract seam B (the `handle_*_outcome` family) into `pipeline_outcomes.rs`** — `94d9a1c` (refactor)
3. **Task 3: Extract seam C (transition/gate/finish) into `pipeline_gate.rs` and close the cycle** — `33f7962` (refactor)

**Plan metadata:** (this commit, docs: complete plan)

## Files Created/Modified
- `crates/devflow-cli/src/pipeline_launch.rs` — New module: launching a stage's agent and driving the `advance` decision (D-06 seam A), `pub(crate)`-only, 4 tests moved verbatim
- `crates/devflow-cli/src/pipeline_outcomes.rs` — New module: deciding what happens after a stage produces a result — the `handle_*_outcome` family, checkout-hook execution, rendering helpers (D-06 seam B), `pub(crate)`-only, 27 tests + 3 shared helpers moved verbatim
- `crates/devflow-cli/src/pipeline_gate.rs` — New module: stage transitions, gate firing/resolution, loop-backs, workflow completion, abort (D-06 seam C), `pub(crate)`-only, 7 tests moved verbatim, closes the three-way module cycle
- `crates/devflow-cli/src/main.rs` — Three `mod` declarations added, all three pipeline clusters' production + test code removed (8,467 → 3,313 lines across the whole phase), stale imports pruned
- `crates/devflow-cli/src/preflight.rs` — Its 18-07 bidirectional coupling repointed from `crate::{launch_stage, launch_stage_inner, abort, run_gate, truncate_reason}` to the three new modules

## Decisions Made
- **Test attribution for cross-seam-calling tests followed the plan's own stated rule over its illustrative wildcard lists where they'd otherwise diverge.** `consecutive_failures_are_independent_across_phases` calls `transition()` directly with no `handle_*_outcome` involvement — structurally identical to the plan's explicitly-named `repeated_code_to_validate_transition_is_idempotent_on_the_counter` (also a pure `transition()` unit test) — so both moved to seam C (`pipeline_gate.rs`) together, even though Task 2's action text generically bucketed "the consecutive-failure tests" under seam B. Verified by reading each test body directly, per the plan's own attribution rule ("re-verify each by reading the test body — do not attribute from the name alone").
- **19-07's deferred attribution closed.** `content_hooks_target_the_worktree_while_terminal_hooks_stay_on_project_root` exercises `hook_context_root` exclusively — moved to `pipeline_outcomes.rs` (seam B) in Task 2, as 19-07-SUMMARY.md anticipated.
- **rustfmt line-wrap is an accepted, explained hunk class, not a retype.** Three moved functions (`hook_context_root`, `transition`, `loop_back_to_code`) had single-line signatures sitting just under rustfmt's wrap threshold in the baseline; adding the required `pub(crate) ` prefix pushed each over the limit, so `cargo fmt` mechanically reflowed them to multi-line (with the idiomatic trailing comma). This is a compiler/tool-driven consequence of the one permitted delta (added visibility keyword), not a manual edit — confirmed by diffing with all whitespace collapsed, which matches exactly.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - blocking] `run_preflight`'s `Advance` arm import fix required splitting a grouped `use` into separate lines to satisfy the plan's literal source-grep acceptance criterion**
- **Found during:** Task 1
- **Issue:** `rg -c 'pipeline_launch::launch_stage_inner' crates/devflow-cli/src/preflight.rs` (an acceptance criterion) does not match a grouped-brace import (`pipeline_launch::{launch_stage, launch_stage_inner}`), since the literal substring `pipeline_launch::launch_stage_inner` is broken by the `{`.
- **Fix:** Split into two ungrouped `use` statements (`use crate::pipeline_launch::launch_stage;` / `use crate::pipeline_launch::launch_stage_inner;`) and updated the module doc comment's cross-references to the fully-qualified paths.
- **Files modified:** `crates/devflow-cli/src/preflight.rs`
- **Commit:** `cdb7368`

**2. [Rule 3 - blocking] Test-only types needed inside each new module's own `mod tests` block, not at production-code module scope**
- **Found during:** Tasks 1, 2, 3
- **Issue:** `AgentKind`, `Mode`, `Gates`, `GitFlow`, `prompt`, `GateResponse` are needed only by moved test bodies, not by the moved production functions. Importing them at the module's top level triggered `cargo clippy --workspace --all-targets -- -D warnings` "unused import" failures on the plain (non-test) binary target, since `devflow-cli` is binary-only and a plain `cargo build` never compiles `#[cfg(test)]` content.
- **Fix:** Added these imports directly inside each module's own `#[cfg(test)] mod tests { ... }` block (matching the pattern already established in `staleness.rs` by 19-07), not at the enclosing module's top level.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`, `pipeline_outcomes.rs`, `pipeline_gate.rs`
- **Commits:** `cdb7368`, `94d9a1c`, `33f7962`

**3. [Rule 3 - blocking] Wider cross-module import surface than each task's minimal estimate, following the compiler**
- **Found during:** Tasks 2 and 3
- **Issue:** As each task moved out, the still-resident seam's functions (`transition`/`run_gate`/`abort`/`finish_workflow`/`loop_back_to_code`, `checkout_lock_timeout`/`retry_after_from_reason`) needed explicit `use` paths repointed in the already-moved sibling modules and in `preflight.rs`, beyond the plan's illustrative import lists.
- **Fix:** Repointed every affected `use` statement per the compiler's `E0425`/`E0432` errors; no function body touched.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`, `pipeline_outcomes.rs`, `preflight.rs`
- **Commits:** `94d9a1c`, `33f7962`

---

**Total deviations:** 3 auto-fixed (all Rule 3 — blocking import/visibility fixes required to compile, no function body changed)
**Impact on plan:** All three are mechanical consequences of the extraction procedure itself (import repointing, test-only type placement, and one acceptance-criterion-literal fix). No scope creep; zero behavioral change.

## Issues Encountered
None beyond the three deviations above — no compile errors survived past the import-repointing pass, no clippy or fmt issues in any of the three final states, and no D-12 `ENV_MUTEX` finding across six total consecutive `cargo test -p devflow` runs (three after Task 2, three after Task 3).

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- The pipeline state machine — the phase's highest-value cut and the cluster that absorbed 3 of Phase 18's 7 plans — is fully split. `main.rs` contains no pipeline code at all.
- 19-09 (parallel + commands/display clusters) and 19-10 can proceed against a `main.rs` that is now 3,313 lines, down from the phase-start 8,467.
- Per the plan's explicit prohibition, the three pipeline modules are NOT independently editable — a future pipeline-logic change is likely to touch two or three of them together (documented in `pipeline_gate.rs`'s own module doc comment and asserted by a source grep, so a future planner will read it where the decision is actually made).
- All 438 tests remain green; `cargo build -p devflow`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` all exit 0 at this plan's final commit.
- Per D-11, this plan's local-green result is not the phase gate — the CI-on-branch gate is owned by plan 19-11.

---
*Phase: 19-release-integrity-main-rs-decomposition*
*Completed: 2026-07-22*

## Self-Check: PASSED

All claimed files verified present on disk (`pipeline_launch.rs`, `pipeline_outcomes.rs`, `pipeline_gate.rs`, `main.rs`, `preflight.rs`, this SUMMARY). All three task commits (`cdb7368`, `94d9a1c`, `33f7962`) verified present in `git log --oneline --all`.
