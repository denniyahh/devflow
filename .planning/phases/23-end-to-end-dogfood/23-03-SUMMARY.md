---
phase: 23-end-to-end-dogfood
plan: 03
subsystem: infra
tags: [rust, cli, registry, fnv-1a, gates, observability]

# Dependency graph
requires:
  - phase: 23-end-to-end-dogfood (23-01, 23-02)
    provides: scratch-repo probe isolation and probe-output evidence discipline that this plan's manual smoke test reused
provides:
  - "devflow_core::registry — machine-global (project_root, phase) registry, one file per registration under ~/.cache/devflow/roots/"
  - "devflow gate list --all-roots — cross-root open-gate enumeration with per-gate age and escalation marker"
  - "registration wired into launch_stage_inner; deregistration wired into both genuine workflow-terminal paths"
affects: [23-04 (aged-gate reaper consumes registry::load_roots/prune_missing), 23-05 (devflow stop), 23-11 (acceptance run)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "One file per (project_root, phase) instead of one shared aggregate file — registration has no read-modify-write step, so concurrent registrations cannot lose an entry (cross-AI review BLOCKER 4 fix)."
    - "_in / wrapper split (register_in vs register, etc.) for env-mutation-free testability, matching config_parse.rs's existing convention."
    - "Per-call unique temp-file names (pid + atomic counter) for write-temp-then-rename, since concurrent writers to the SAME entry file cannot share gates.rs's fixed .tmp suffix without tearing each other's write."

key-files:
  created:
    - crates/devflow-core/src/registry.rs
  modified:
    - crates/devflow-core/src/lib.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/main.rs
    - OPERATIONS.md

key-decisions:
  - "Deregister is fire-and-forget (returns () not Result) per the plan's artifacts spec, contradicting the action text's literal 'let _ =' instruction — followed the artifacts spec since clippy::let_unit_value (-D warnings) rejects `let _ = <unit expr>;`."
  - "Renamed the four new deregister_in_* tests to a dereg_* prefix so `rg -n 'fn deregister_in'` still matches exactly the one function definition, satisfying that acceptance criterion literally."

requirements-completed: [23b]

coverage:
  - id: D1
    description: "devflow_core::registry module — RegisteredRoot/RegistryError, cache_dir resolution, one-file-per-(root,phase) register_in/load_roots_in, no read-modify-write step"
    requirement: "23b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/registry.rs — registry::tests (16 tests, cargo test -p devflow-core --features test-support registry)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Registration wired into pipeline_launch::launch_stage_inner immediately after monitor_pid is recorded"
    requirement: "23b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs — launch_stage_persists_monitor_pid_for_reload (pre-existing test, still green after the wiring)"
        status: pass
    human_judgment: false
  - id: D3
    description: "devflow gate list --all-roots fans out registry::load_roots() across Gates::list_open per root with a leading ROOT column, age, and escalation marker"
    requirement: "23b"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs — render_gate_age_marks_escalated_gate_urgent, render_gate_age_no_marker_for_fresh_gate, render_gate_age_unknown_for_non_numeric_timestamp, all_roots_row_includes_gate_with_non_numeric_timestamp"
        status: pass
      - kind: manual_procedural
        ref: "manual smoke: DEVFLOW_CACHE_DIR + hand-placed roots entry + gate file; `devflow gate list --all-roots` rendered a fresh gate (20m, no marker) then the same gate re-timestamped ~2h old (2h!, marker present)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Registry hardening: atomic per-entry writes, 0700 cache/roots dirs, prune_missing_in for dead roots, proven both-survive concurrency"
    requirement: "23b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/registry.rs — concurrent_registration_of_different_pairs_both_survive, concurrent_registration_of_same_pair_results_in_one_valid_entry, register_in_creates_cache_and_roots_dirs_with_mode_0700, prune_missing_in_* (3 tests)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Deregistration on both genuine workflow-terminal paths (finish_workflow_with_gate_timeout success, abort), deliberately not on the gate_timeout error path"
    requirement: "23b"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/registry.rs — dereg_removes_matching_pair_and_leaves_sibling_phase_intact, dereg_is_scoped_to_one_root_and_leaves_sibling_root_intact, dereg_on_never_registered_pair_is_a_noop, dereg_is_idempotent_when_entry_already_removed"
        status: pass
      - kind: other
        ref: "rg -c 'registry::deregister' crates/devflow-cli/src/pipeline_gate.rs == 2; rg -n 'registry::' between gate_timeout emission and its Err == no match"
        status: pass
    human_judgment: false

# Metrics
duration: 16min
completed: 2026-07-26
status: complete
---

# Phase 23 Plan 03: Machine-Global Registry + Cross-Root Gate Listing Summary

**New `devflow_core::registry` module (one file per `(project_root, phase)` under `~/.cache/devflow/roots/`, zero new dependencies) wired into the launch and terminal-workflow paths, powering `devflow gate list --all-roots` — the enumeration half of bound gate lifetime (23b) that answers "what is gated on this machine?" without `ps`/`find` archaeology.**

## Performance

- **Duration:** 16 min
- **Started:** 2026-07-25T21:19:02-04:00 (Task 1 commit)
- **Completed:** 2026-07-25T21:34:58-04:00 (Task 3 commit)
- **Tasks:** 3 (Task 1 tracer + Task 2 TDD + Task 3 TDD)
- **Files modified:** 7 (1 created, 6 modified)

## Accomplishments

- `devflow_core::registry`: `RegisteredRoot`, `RegistryError`, `cache_dir()` (env resolution `DEVFLOW_CACHE_DIR` > `XDG_CACHE_HOME/devflow` > `HOME/.cache/devflow`), stable FNV-1a filename digest, `register_in`/`register`, `load_roots_in`/`load_roots` — one file per registration, no read-modify-write step, so two concurrent registrations of different pairs cannot lose either one (cross-AI review BLOCKER 4's required fix, proven live with a `std::thread::scope` test).
- Registration wired into `pipeline_launch::launch_stage_inner` immediately after `state.monitor_pid` is persisted — a launched phase can never be unregistered-but-running.
- Hardened: atomic per-entry writes (unique temp name + rename, so two concurrent writers to the SAME entry never tear each other), `0o700` cache/roots directories (T-23-33), `prune_missing_in`/`prune_missing` for dead roots and unparsable entries.
- Deregistration on both genuine terminal paths (`finish_workflow_with_gate_timeout`'s success path, `abort`) — deliberately NOT on the `gate_timeout` error path, since that phase remains a legitimate enumeration subject.
- `devflow gate list --all-roots`: fans `registry::load_roots()` out across `Gates::list_open` per root, rendering a leading ROOT column, compact age, and a trailing `!` escalation marker past `GATE_ESCALATION_THRESHOLD_SECS`; a non-numeric `timestamp` renders `?` rather than dropping the row. Single-root `gate list` output stays byte-identical.

## Task Commits

1. **Task 1: End-to-end "what is gated on this machine" — one root, one gate, one command** - `0796585` (feat)
   - **Deviation fix:** `a68c992` (docs) — `DEVFLOW_CACHE_DIR` was undocumented, failing `doc_check::source_devflow_env_vars_and_subcommands_are_documented`
2. **Task 2: Harden the registry — atomic writes, 0700 permissions, stale-root pruning, concurrency proof** (TDD)
   - RED: `3321410` (test) — 6 new tests referencing `prune_missing_in`, which did not exist yet; compile failure confirmed
   - GREEN: `6755b8a` (feat) — implemented atomic writes, `0o700` dirs, `prune_missing_in`/`prune_missing`; 12/12 registry tests pass
3. **Task 3: Deregister on terminal paths, gate age on the cross-root listing** (TDD)
   - RED: `938af8b` (test) — 8 new tests (4 registry + 4 commands.rs) referencing `deregister_in`/`render_gate_age`/`render_all_roots_gate_row`, none of which existed yet; compile failure confirmed
   - GREEN: `1efb9f1` (feat) — implemented `deregister_in`/`deregister`, wired into both terminal paths, added age/escalation rendering; 16/16 registry tests pass, clippy clean after fixing a `let_unit_value` lint

**Plan metadata:** (this SUMMARY's commit, made by the parallel-worktree orchestrator after merge — not created by this executor per the worktree protocol)

## Files Created/Modified

- `crates/devflow-core/src/registry.rs` - New module: registry storage, atomic writes, permissions, pruning, deregistration (16 unit tests)
- `crates/devflow-core/src/lib.rs` - `pub mod registry;` declared alphabetically between `recover` and `ship`
- `crates/devflow-cli/src/pipeline_launch.rs` - `registry::register` called in `launch_stage_inner` right after `monitor_pid` persists
- `crates/devflow-cli/src/pipeline_gate.rs` - `registry::deregister` called on both genuine terminal paths (`finish_workflow_with_gate_timeout`, `abort`); NOT on `gate_timeout`
- `crates/devflow-cli/src/commands.rs` - `gate_list(project_root, all_roots)`, `gate_list_all_roots`, `render_all_roots_gate_row`, `render_gate_age` (8 new unit tests)
- `crates/devflow-cli/src/main.rs` - `--all-roots` flag on `GateCmd::List`, threaded through dispatch
- `OPERATIONS.md` - `DEVFLOW_CACHE_DIR` documented in the environment-variables table (Rule 2 auto-fix)

## Decisions Made

- **Storage shape (inherited from the plan, re-verified):** one file per `(project_root, phase)`, not one shared `roots.json` — registration has no read-modify-write step, so it structurally cannot lose a concurrent registration, per cross-AI review BLOCKER 4.
- **`deregister` returns `()`, not `Result`:** the plan's artifacts section lists `fn deregister(project_root: &Path, phase: u32)` with no return type, while the action text separately said to call it "discarded with `let _ =`." These conflict when the function is unit-returning — `clippy::let_unit_value` (active under `-D warnings`) rejects `let _ = <unit expr>;`. Followed the artifacts spec (the more precise, machine-checkable source of truth) and omitted the `let` binding at both call sites; `rg -c 'registry::deregister' pipeline_gate.rs` still returns exactly 2 as required.
- **Test naming to satisfy a literal grep assertion:** the plan requires `rg -n 'fn deregister_in'` to match exactly one line (the function definition). Test names like `deregister_in_removes_matching_pair_...` would also match as a substring of `fn deregister_in`, so the four new tests were named with a `dereg_` prefix instead.
- **Verify commands needed `--features test-support`:** the plan's literal `cargo test -p devflow-core registry` fails to compile in this workspace — `devflow-core`'s own integration tests (`tests/monitor_e2e.rs`, `tests/devflow_dir_gitignore.rs`) call `devflow_core::test_support::git_command`, which is only compiled when the `test-support` feature is enabled. That feature is only turned on today via `devflow-cli`'s dev-dependency declaration (`devflow-core = { workspace = true, features = ["test-support"] }`), which Cargo only unifies into the build when the whole workspace (or at least both packages) is being tested — a bare `-p devflow-core` invocation doesn't see it. `cargo test --workspace` (matching CI's `ci.yml`) is unaffected and was used to confirm the full suite; all scoped verify commands in this SUMMARY were run with `--features test-support` added.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Documented `DEVFLOW_CACHE_DIR` in OPERATIONS.md**
- **Found during:** Task 1, running the full workspace test suite as the tracer feedback gate
- **Issue:** `crates/devflow-core/src/doc_check.rs`'s `source_devflow_env_vars_and_subcommands_are_documented` test asserts every source-read `DEVFLOW_*` env var is documented in scoped operator docs. `registry::cache_dir()` reads `DEVFLOW_CACHE_DIR` but the plan's action text never mentioned updating docs, so the workspace suite failed.
- **Fix:** Added a row to `OPERATIONS.md`'s "Environment variables" table.
- **Files modified:** OPERATIONS.md
- **Verification:** `cargo test -p devflow-core --features test-support doc_check::source_devflow_env_vars_and_subcommands_are_documented` — 1 passed
- **Committed in:** `a68c992`

**2. [Rule 3 - Blocking] Fixed `clippy::let_unit_value` from the plan's literal `let _ =` guidance**
- **Found during:** Task 3, running `cargo clippy --workspace --all-targets -- -D warnings`
- **Issue:** The plan's action text said to discard `registry::deregister`'s result with `let _ =`, but the artifacts spec defines `deregister` as unit-returning (no `Result`). `let _ = registry::deregister(...);` triggered `clippy::let_unit_value`, which is a hard error under `-D warnings`.
- **Fix:** Omitted the `let` binding at both call sites in `pipeline_gate.rs`; `deregister` already swallows its own internal error.
- **Files modified:** crates/devflow-cli/src/pipeline_gate.rs
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` exits 0
- **Committed in:** `1efb9f1`

**3. [Rule 3 - Blocking] Renamed test functions colliding with a literal grep acceptance criterion**
- **Found during:** Task 3, verifying acceptance criteria before commit
- **Issue:** The plan requires `rg -n 'fn deregister_in' crates/devflow-core/src/registry.rs` to match exactly one line. My initial test names (`deregister_in_removes_matching_pair_...` etc.) also matched as a substring of `fn deregister_in`.
- **Fix:** Renamed the four new tests to a `dereg_*` prefix.
- **Files modified:** crates/devflow-core/src/registry.rs
- **Verification:** `rg -n 'fn deregister_in' crates/devflow-core/src/registry.rs` returns exactly one line
- **Committed in:** `1efb9f1`

---

**Total deviations:** 3 auto-fixed (1 missing critical doc, 2 blocking — a lint conflict and a grep-assertion naming collision, both inherited from imprecision between the plan's action text and its own acceptance criteria/artifacts spec)
**Impact on plan:** All three fixes were necessary to make the plan's own verification gates pass; none changed the plan's design or added out-of-scope functionality.

## Issues Encountered

- `cargo test -p devflow-core registry` (the plan's literal Task 1/2/3 `<verify>` command) fails to compile without `--features test-support`, for reasons unrelated to this plan (a pre-existing workspace feature-unification quirk — see Decisions Made). Every scoped verify command in this plan's execution used `--features test-support`; the full-workspace chain (`cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`, matching CI) was also run clean at the end of every task as the authoritative gate.
- The `build_provenance.rs` integration test (`build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild`) failed once, transiently, between writing `registry.rs` and its Task 1 commit — that test snapshots only git-tracked files into a scratch clone, and `registry.rs` was still untracked. Resolved by committing; re-ran clean afterward. Not a real defect, no fix needed.

## Test Names Matched by Filters (Hermes LOW finding — recorded per protocol)

- **Task 1** `cargo test -p devflow-core --features test-support registry`: `registry::tests::{load_roots_in_on_absent_directory_returns_empty_without_panicking, path_digest_is_stable_and_distinguishes_different_paths, register_in_same_pair_twice_results_in_exactly_one_entry, register_in_same_root_two_phases_survive_as_distinct_files, register_in_two_different_pairs_both_survive_and_load_sorted, load_roots_in_skips_one_corrupt_entry_and_keeps_its_sibling}` (6 passed)
- **Task 1** `cargo test -p devflow gate_list`: `commands::tests::gate_show_errors_naming_gate_list_when_no_open_gate` (1 passed)
- **Task 2** `cargo test -p devflow-core --features test-support registry`: the above 6 plus `{concurrent_registration_of_different_pairs_both_survive, concurrent_registration_of_same_pair_results_in_one_valid_entry, register_in_creates_cache_and_roots_dirs_with_mode_0700, prune_missing_in_removes_entry_for_deleted_root_and_reports_count, prune_missing_in_keeps_entry_for_existing_root, prune_missing_in_removes_and_counts_unparsable_entry}` (12 passed)
- **Task 3** `cargo test -p devflow-core --features test-support registry`: the above 12 plus `{dereg_removes_matching_pair_and_leaves_sibling_phase_intact, dereg_is_scoped_to_one_root_and_leaves_sibling_root_intact, dereg_on_never_registered_pair_is_a_noop, dereg_is_idempotent_when_entry_already_removed}` (16 passed — matches the required "the both-survive concurrency test" naming requirement: `concurrent_registration_of_different_pairs_both_survive`)
- **Task 3** `cargo test -p devflow gate` (43 passed, listing every match since this filter is broad by design): `commands::tests::{all_roots_row_includes_gate_with_non_numeric_timestamp, render_gate_age_marks_escalated_gate_urgent, render_gate_age_no_marker_for_fresh_gate, render_gate_age_unknown_for_non_numeric_timestamp, gate_respond_auto_resolves_single_open_gate, gate_respond_requires_stage_when_ambiguous_and_errors_when_none_open, gate_show_auto_resolves_single_open_gate, gate_show_errors_asking_for_stage_with_several_open_gates, gate_show_errors_naming_gate_list_when_no_open_gate, gate_show_renders_full_untruncated_sanitized_context, gate_show_arg_parsing_accepts_phase_and_optional_stage, gate_approve_arg_parsing_accepts_positional_stage, status_shows_pending_gate_prominently, recovery_hints_includes_advance_when_stuck_and_gate_pending, doctor_reconciliation::{reconcile_phase_flags_gate_pending_without_open_gate, reconcile_phase_flags_orphan_open_gate, doctor_reports_gate_pending_without_gate_file}}`, `config_parse::tests::{parse_foreground_gate_timeout_env_override, parse_gate_timeout_env_override}`, `pipeline_gate::tests::{advance_ship_success_runs_finish_workflow, concurrent_ship_advances_finish_both_phases_independently, consecutive_failures_are_independent_across_phases, ship_override_abort_routes_through_abort, repeated_code_to_validate_transition_is_idempotent_on_the_counter, ship_override_refuses_when_lock_contended, ship_override_refuses_when_no_response_written, ship_override_refuses_when_not_at_ship_stage, ship_override_refuses_when_response_already_acked, ship_override_advances_via_written_response, abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response, terminal_merge_failure_reopens_actionable_gate_and_never_reports_finished, ship_override_bounds_foreground_wait_on_terminal_hook_failure, transition_resets_infra_failures}`, `pipeline_outcomes::tests::{external_verify_disagreement_gates_immediately, external_verify_no_verdict_gates_immediately, gate_context_rendering_neutralizes_all_controls_and_obeys_limit, rate_limited_with_unparseable_retry_hint_gates_instead_of_stalling_silently, ship_agent_failed_fires_gate, validate_failure_threshold_forces_gate_then_aborts, non_validate_failure_fires_gate_and_hook}`, `preflight::tests::{run_preflight_failing_check_gates_and_never_reaches_spawn_monitor, run_preflight_advance_gate_launches_agent_exactly_once, run_preflight_loopback_gate_launches_agent_exactly_once}`

## Known Stubs

None.

## Threat Flags

None — every new surface (the cache dir, the roots dir, the registered `project_root`s DevFlow will `read_dir` on behalf of the operator) is already covered by this plan's own `<threat_model>` (T-23-30 through T-23-34, T-23-SC), all `mitigate`d and verified: `rg -c '0o700' registry.rs` ≥ 1, `register_in`'s body contains no `load_roots_in` call, `prune_missing_in` uses `remove_file` with no whole-registry rewrite, `rg -c 'dirs' devflow-core/Cargo.toml` returns 0.

## Manual Smoke Verification

Per the plan's `<verification>` section: with `DEVFLOW_CACHE_DIR` pointed at a scratch dir containing a hand-placed roots entry (`{"project_root": "<scratch project>", "phase": 1, "registered_at": "<ts>"}`) and a matching `.devflow/gates/01-ship.json` gate file, `devflow gate list --all-roots`:
- Rendered `1  ship  20m  <scratch project>` with no urgency marker for a 20-minute-old gate.
- Re-run after re-timestamping the same gate to ~2 hours old rendered `1  ship  2h!  <scratch project>` — the escalation marker appeared.
- `devflow gate list --help` prints a line containing `--all-roots`.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `registry::load_roots`/`registry::prune_missing` are ready for plan 23-04's aged-gate reaper to consume directly.
- `devflow gate list --all-roots` is the operator-facing tool `23-ORPHAN-FORENSICS.md` had to answer by hand with `ps`/`find`; future forensics on this machine can use it instead.
- No blockers. Full workspace suite green (354 devflow-core tests, 161 devflow-cli lib tests, 20 CLI integration tests, all `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check` clean) at every task boundary.

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-26*
