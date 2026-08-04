---
phase: 23-end-to-end-dogfood
plan: 06
subsystem: infra
tags: [devflow-core, devflow-cli, event-log, layer0-probe, verification, git]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood
    provides: "23-05's prior wave establishing the pipeline/hooks state this plan builds a read-only oracle on top of"
provides:
  - "A terminal-only `workflow_shipped` event, emitted at exactly one site (`finish_workflow_with_gate_timeout`), that is the strict, code-enforced answer to 'did this phase actually ship?'"
  - "`devflow_core::ship_evidence::{ShipEvidence, collect}` — a strictly read-only oracle reading that event"
  - "`devflow evidence --phase N [--json] [--require-shipped] [--root]` CLI verb, declarable as a Layer 0 external_verify probe"
  - "A post-merge ancestry re-assertion inside `hooks::merge_feature`, run before `BranchCleanup` deletes the feature branch, with a documented no-rollback failure policy"
affects: ["23-10 (recovery-path artifact must cite merge_feature's no-rollback doc comment)", "any future phase whose attestation claims a completed Ship"]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Terminal-only event as a structural predicate: a new, single-emission-site event name, not a payload convention on an existing multi-site event, closes a false-green class permanently rather than by discipline."
    - "Read-only oracle module returning a value (never Result), every field degrading to its safest value, so a reviewer can never skip it due to an error."
    - "Layer 0 probe: a CLI verb whose --require-shipped flag is exit-code-stable on one strict field, declarable via PLAN frontmatter external_verify + DEVFLOW_TRUST_EXTERNAL_VERIFY approval."

key-files:
  created:
    - crates/devflow-core/src/ship_evidence.rs
  modified:
    - crates/devflow-core/src/events.rs
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/hooks.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/snapshots/devflow-help.txt
    - OPERATIONS.md

key-decisions:
  - "The shipped predicate is a distinct, terminal-only `workflow_shipped` event with exactly one emission site — NOT `workflow_finished`, which is emitted at two sites (real Ship finalization, and `transition`'s `--until` clean-stop branch with a `stopped_at` reason) and would have shipped a false green for a phase that only stopped after one stage."
  - "Git ancestry is corroboration only, never the predicate: it is shape-sensitive and goes false for every successfully shipped phase once `BranchCleanup` deletes the feature branch later in the same batch."
  - "The merge post-condition re-checks ancestry inside `merge_feature`, immediately after `merge_feature_into_develop` returns Ok and before `BranchCleanup` runs — the only point in the batch where the assertion is both meaningful and safe."
  - "No rollback on a failed merge post-condition: the merge commit stays, the batch fails, the Ship gate reopens for a human. Documented in a `///` doc comment for plan 23-10's recovery path to rely on."
  - "ship_evidence.rs never references `external_verify_enabled` or decides whether Layer 0 is active — it only reports facts; declaring the probe is a per-phase PLAN-author choice."

patterns-established:
  - "Pattern: a false-green detector's own predicate must itself be re-verified against source before landing (this plan's own first draft used `workflow_finished` as the predicate and was proven wrong by cross-AI review before landing)."

requirements-completed: [23e]

coverage:
  - id: D1
    description: "Terminal-only workflow_shipped event emitted at exactly one site, before the pre-existing workflow_finished emission"
    requirement: "23e"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#advance_ship_success_emits_workflow_shipped_and_ship_evidence_reports_shipped"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_gate.rs#until_stop_never_emits_workflow_shipped_and_ship_evidence_reports_not_shipped"
        status: pass
    human_judgment: false
  - id: D2
    description: "ship_evidence::collect is a strictly read-only oracle whose shipped field consults only the workflow_shipped event, never git ancestry or workflow_finished"
    requirement: "23e"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/ship_evidence.rs#shipped_predicate_consults_no_git_field"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/ship_evidence.rs#stopped_at_phase_reports_not_shipped_but_corroborates_finished"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/ship_evidence.rs#missing_devflow_dir_degrades_safely_without_panicking"
        status: pass
    human_judgment: false
  - id: D3
    description: "devflow evidence --phase N [--json] [--require-shipped] CLI verb, exit-code-stable and declarable as a Layer 0 probe"
    requirement: "23e"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#evidence_require_shipped_exits_ok_iff_the_phase_has_shipped"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#evidence_require_shipped_names_stopped_at_rather_than_generic_not_shipped"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#evidence_require_shipped_failure_message_is_single_line_and_names_phase"
        status: pass
      - kind: manual_procedural
        ref: "manual CLI run: devflow evidence --phase 1 --json / --require-shipped against a fresh fixture (see Verification section)"
        status: pass
    human_judgment: false
  - id: D4
    description: "merge_feature re-asserts ancestry after a reported-successful merge, before BranchCleanup deletes the branch, with a no-rollback failure policy documented at the source"
    requirement: "23e"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/hooks.rs#merge_through_hook_records_true_merged_result_after_ancestry_reconfirmed"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/hooks.rs#merge_fails_closed_when_branch_absent_emits_no_merge_result_event"
        status: pass
    human_judgment: false

duration: ~2h30m
completed: 2026-07-26
status: complete
---

# Phase 23 Plan 06: End-to-end shipped oracle Summary

**A terminal-only `workflow_shipped` event with exactly one emission site, a read-only `ship_evidence` oracle that reads only that event, and a `devflow evidence --require-shipped` CLI verb declarable as a code-enforced Layer 0 probe — closing the false-green attestation class this plan's own first draft nearly reintroduced.**

## Performance

- **Tasks:** 3/3 completed
- **Files created:** 1 (`crates/devflow-core/src/ship_evidence.rs`)
- **Files modified:** 7 (`events.rs`, `lib.rs`, `hooks.rs`, `pipeline_gate.rs`, `commands.rs`, `main.rs`, `OPERATIONS.md`, plus the regenerated `--help` snapshot)
- **New/changed tests:** devflow-core `hooks::` 13 → 15 passed; devflow-core `ship_evidence`/`events::` +16 new tests; devflow-cli `commands::tests`/`pipeline_gate::tests` +5 new tests. Full workspace: 177+3+4+4+1+1+1+3+20+8+9+1+370+2+2+0 = all green, 0 failed.

## Accomplishments

- **Task 1** — Emitted a new terminal-only `workflow_shipped` event from `finish_workflow_with_gate_timeout`, strictly after the `hooks_after_ship` batch succeeds and strictly before the pre-existing (unchanged) `workflow_finished` emission. Added `events::last_event_of_kind_for_phase`/`has_event_for_phase` as the single scanner for one event name/phase. Added `devflow_core::ship_evidence::{ShipEvidence, collect}` — a strictly read-only oracle whose `shipped` field reads only the new event (never git ancestry, never `workflow_finished`). Added `devflow evidence --phase N [--json] [--require-shipped] [--root]`.
- **Task 2** — `hooks::merge_feature` re-asserts `is_merged_into_develop` immediately after `merge_feature_into_develop` reports success, while the feature branch still exists (before `BranchCleanup` deletes it in the same batch). On failure it emits `merge_result` with `merged: false` and returns `Err`, routing into the existing reopen-the-Ship-gate path. No rollback — documented in a `///` doc comment on `merge_feature`.
- **Task 3** — Confirmed `evaluate_layer0`/`external_verify_commands` treat a declared, approved command's exit status as authoritative. Made `devflow evidence --require-shipped` exit-code-stable (exits 0 iff `shipped` is true, independent of every other field), with a single-line failure message that explicitly names the stopped-at case. Documented all three disproven predicates/placements (pre-gate merge check, post-batch ancestry check, `workflow_finished` as predicate) in a doc comment on `commands::evidence`, with source citations, so they are not re-derived and re-attempted.

## Task Commits

Each task was committed atomically:

1. **Task 1: End-to-end "did this phase actually ship?" — one terminal-only event, one command, one answer** — `c650cd6` (feat)
2. **Task 2: Enforce the merge post-condition where the branch still exists** — `147c43f` (test — TDD-shaped commit; see TDD Gate Compliance below)
3. **Task 3: Make the oracle usable as a code-enforced probe, and document the contract** — `49b981e` (feat)

**Plan metadata:** (this commit, made by the orchestrator after all wave agents complete — not made by this plan directly per its worktree-mode instructions)

## Files Created/Modified

- `crates/devflow-core/src/ship_evidence.rs` — new: `ShipEvidence` struct + `collect()`, the read-only shipped oracle, with 6 unit tests
- `crates/devflow-core/src/events.rs` — added `last_event_of_kind_for_phase`/`has_event_for_phase`; added 2 unit tests using a generic marker event name (kept decoupled from the `workflow_shipped` literal per Task 1's own acceptance criteria)
- `crates/devflow-core/src/lib.rs` — `pub mod ship_evidence;` declared between `ship` and `stage`
- `crates/devflow-core/src/hooks.rs` — `merge_feature` gains the post-merge ancestry re-check + `///` doc comment; 2 new tests
- `crates/devflow-cli/src/pipeline_gate.rs` — `finish_workflow_with_gate_timeout` emits `workflow_shipped`; 2 new regression tests (shipped-path, stopped-at-path)
- `crates/devflow-cli/src/commands.rs` — `commands::evidence()` handler + doc comment recording the three dead ends; 3 new unit tests
- `crates/devflow-cli/src/main.rs` — `Command::Evidence` variant + dispatch
- `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerated (new `evidence` subcommand line)
- `OPERATIONS.md` — new `devflow evidence` row in the Commands table (required by `doc_check::source_devflow_env_vars_and_subcommands_are_documented`)

## Decisions Made

- **The shipped predicate is `workflow_shipped`, not `workflow_finished`.** This plan's own PLAN.md documents (in its `<objective>`) that an earlier revision defined the predicate as `workflow_finished` and was proven wrong by cross-AI review before this plan was even executed — `workflow_finished` is emitted at two sites, the second being `transition`'s `--until` clean-stop branch with a `stopped_at` reason. This plan's Task 1 implements the corrected design directly (a distinct, terminal-only event); no rediscovery was needed, but the regression test (`until_stop_never_emits_workflow_shipped_and_ship_evidence_reports_not_shipped`) proves it holds in the shipped code.
- **`ship_evidence.rs` must never reference `external_verify_enabled`.** Initially the module-level doc comment named that identifier in prose while explaining Layer 0's gating; removed and reworded during Task 3 to satisfy the literal acceptance criterion (`rg -c 'external_verify_enabled' ship_evidence.rs` returns 0) — the module reports facts and must not even *appear* to decide whether Layer 0 is active.
- **The `events.rs` generic-scanner tests use a marker event name, not `"workflow_shipped"`.** Task 1's acceptance criteria required `rg -c 'workflow_shipped' crates/devflow-core/src/` to return hits only in `ship_evidence.rs`. The initial test draft in `events.rs` used the real literal for realism; switched to a `MARKER_EVENT` constant so the generic scanner's tests stay decoupled from any one caller's event-name choice, and `ship_evidence::tests` exercises the same scanner against the real literal instead.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `--help` snapshot regenerated after adding the `evidence` subcommand**
- **Found during:** Task 1
- **Issue:** `crates/devflow-cli/tests/help_snapshot.rs` compares `devflow --help` against a committed snapshot; adding `Command::Evidence` changed the CLI surface and broke the test.
- **Fix:** Ran `cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt` per the test's own documented regeneration instructions.
- **Files modified:** `crates/devflow-cli/tests/snapshots/devflow-help.txt`
- **Verification:** `cargo test -p devflow --test help_snapshot` passes.
- **Committed in:** `c650cd6` (Task 1 commit)

**2. [Rule 3 - Blocking] `devflow evidence` documented in OPERATIONS.md**
- **Found during:** Task 1
- **Issue:** `doc_check::source_devflow_env_vars_and_subcommands_are_documented` (`crates/devflow-core/src/doc_check.rs`) asserts every `Command` enum variant is named in one of the scoped operator docs. The new `Command::Evidence` variant was not yet documented, so this pre-existing repo-level invariant test failed.
- **Fix:** Added a `devflow evidence` row to OPERATIONS.md's Commands table, describing the strict predicate, the corroboration fields, and the Layer 0 probe usage.
- **Files modified:** `OPERATIONS.md`
- **Verification:** `cargo test -p devflow-core --features test-support doc_check::` — all 6 doc_check tests pass (including the reverse-direction `doc_referenced_identifiers_exist_in_source`).
- **Committed in:** `c650cd6` (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (both Rule 3 — blocking pre-existing repo invariants that my own change tripped, both required to complete the task, neither expanding scope beyond what Task 1 already touched).
**Impact on plan:** No scope creep. Both fixes are the documented, mechanical remediation steps for pre-existing repo-level tests, triggered directly by adding the new CLI surface Task 1 already specified.

## Issues Encountered

- **`-p devflow-core` alone does not enable the `test-support` feature.** `devflow-core`'s two integration test files (`tests/devflow_dir_gitignore.rs`, `tests/monitor_e2e.rs`) reference `devflow_core::test_support` unconditionally, but that module is `#[cfg(any(test, feature = "test-support"))]`-gated and the feature is off by default. Running `cargo test -p devflow-core <filter>` (as the plan's literal `<verify>` blocks specify) fails to *compile* those two integration binaries — a pre-existing repo condition unrelated to this plan's changes (confirmed: `git diff --stat HEAD -- crates/devflow-core/tests/` is empty for this plan's changes). Resolved by running `-p devflow-core` invocations with `--features test-support` explicitly; `-p devflow` invocations pick the feature up automatically because `devflow-cli`'s own `[dev-dependencies]` entry for `devflow-core` already declares `features = ["test-support"]`. `cargo test --workspace` (used for Task 3's gate and the plan's own `<verification>` block) is unaffected either way, since workspace-level feature unification enables it automatically. All three tasks' literal `<verify>` commands pass once this is accounted for; recorded here so a future reader isn't surprised by the same compile failure.
- **A new untracked file is invisible to `copy_tracked_worktree_into`-style test fixtures.** `crates/devflow-cli/tests/build_provenance.rs`'s `build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` test snapshots the workspace via `git ls-files -z` before building in an isolated temp dir — while `ship_evidence.rs` was still untracked (before Task 1's commit), that snapshot omitted it and the isolated build failed with "file not found for module `ship_evidence`". This resolved itself automatically once Task 1 was committed (making the file tracked); confirmed green on the final `cargo test --workspace` run. Not a defect in this plan's code — a normal consequence of `git add`-then-`git commit` sequencing during execution, noted here in case a future run hits it mid-task and wonders why.

## User Setup Required

None — no external service configuration required. `devflow evidence --require-shipped` is opt-in per phase (a PLAN author must declare it via `external_verify` frontmatter, and the operator must approve execution via `DEVFLOW_TRUST_EXTERNAL_VERIFY`); no project-wide config changes were made.

## Verification

Ran the plan's `<verification>` block against this repository (`.planning`'s own project, itself mid-Phase-23):

```
$ devflow evidence --phase 22 --json
{
  "phase": 22, "shipped": false, "workflow_finished_seen": false,
  "finished_reason": null, "stage": null, "state_present": false,
  "feature_branch_exists": false, "merged_into_develop": false, "has_remote": true
}

$ devflow evidence --phase 23 --json
{
  "phase": 23, "shipped": false, "workflow_finished_seen": false,
  "finished_reason": null, "stage": null, "state_present": false,
  "feature_branch_exists": true, "merged_into_develop": false, "has_remote": true
}
```

Both report `shipped: false` — the expected fail-closed result, not a defect: neither phase has ever emitted the new terminal-only `workflow_shipped` event (phase 22 shipped as v1.8.1 before this event existed; phase 23 is the currently-executing phase and has not reached a finalized Ship). This is exactly the "phases that finalized before this event existed report NOT shipped" behavior documented on `ShipEvidence::shipped`.

- `rg -c 'is_merged_into_develop' crates/devflow-core/src/hooks.rs` → `2` (the pre-existing short-circuit + the new post-condition; confirmed no stray comment references inflate the count).
- Exactly one `events::emit` call in the workspace names `workflow_shipped`, at `crates/devflow-cli/src/pipeline_gate.rs:234`, inside `finish_workflow_with_gate_timeout`, after the hook-success loop `break` and before the `workflow_finished` emission — confirmed by grep and by reading the surrounding code.
- Named regression test proving a `--until`-stopped phase reports NOT shipped: `pipeline_gate::tests::until_stop_never_emits_workflow_shipped_and_ship_evidence_reports_not_shipped` (drives `transition()` through the real `stop_until` clean-stop branch, not a synthetic event line) and `ship_evidence::tests::stopped_at_phase_reports_not_shipped_but_corroborates_finished` (unit-level, direct event injection).
- Final gate: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check` — all pass, 0 failed, 0 warnings, 0 formatting diffs.

## TDD Gate Compliance

Task 2 (`tdd="true"`) implements a defensive post-condition (re-verify ancestry immediately after a reported-successful `git merge --no-ff`) that is, by the mathematical semantics of a `--no-ff` merge, unreachable via real git within a single synchronous function call: if `merge_feature_into_develop` returns `Ok`, the merged branch's tip is *necessarily* an ancestor of `develop`'s new tip by construction of the merge commit — there is no sequence of real git operations that makes the merge report success while the immediately-following ancestry check (same branch name, same function call, no external interleaving possible) reports false. This codebase's `GitFlow` has no mock/injection seam (it shells out to real `git` directly), and the plan's own Task 2 acceptance criteria do not require a test that manufactures this branch — only that the happy path (real merge, `merged: true` event) and the pre-existing missing-branch refusal continue to hold with the new check present. Classic RED (a test that fails before the code exists, passes after) could not be constructed for the failure branch itself for this reason; instead, RED/GREEN was applied to the two behaviors the acceptance criteria do specify:

- **RED:** `merge_through_hook_records_true_merged_result_after_ancestry_reconfirmed` and `merge_fails_closed_when_branch_absent_emits_no_merge_result_event` were written against the pre-Task-2 code first; both already passed (the assertions describe pre-existing, unaffected behavior), confirming there was nothing to accidentally break.
- **GREEN:** added the post-condition; re-ran both tests plus the full `hooks::` suite (13 → 15 passed, strictly greater as required) — all pass, no regression.

The added code's own correctness (the second `is_merged_into_develop` call, its ordering relative to `BranchCleanup`, its no-rollback failure path, and its doc comment) is verified via the plan's own source-assertion acceptance criteria (grep-confirmed above and in-line), which is the verification mechanism the plan itself specifies for this defensive branch — no `test(...)`-then-`feat(...)` RED/GREEN commit pair exists for Task 2 because the observable-behavior tests were already green; this is recorded here rather than silently passed over.

## Known Stubs

None.

## Next Phase Readiness

- `devflow evidence --phase N --require-shipped` is ready to be declared as an `external_verify` probe by any future phase's PLAN whose own attestation claims a completed Ship — closing the exact false-green class `23-ORPHAN-FORENSICS.md` documented.
- Plan 23-10 (recovery-path artifact) can now cite `hooks::merge_feature`'s `///` doc comment directly for the no-rollback-on-merge-post-condition-failure policy, as this plan's cross-AI review incorporation required.
- No blockers. All three tasks' acceptance criteria are met and grep-verified; the full workspace gate (`cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`) is green.

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-26*

## Self-Check: PASSED

- `crates/devflow-core/src/ship_evidence.rs` — FOUND
- Task commit `c650cd6` — FOUND
- Task commit `147c43f` — FOUND
- Task commit `49b981e` — FOUND
- SUMMARY commit `b0e0f51` — FOUND
