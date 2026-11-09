---
phase: 50-addressable-monitor-liveness
plan: 10
subsystem: supervisor
tags: [rust, monitor, test-isolation, registry, semver, nextest, doc-check]

requires:
  - phase: 50-08
    provides: "run_pipe_owning_monitor and the stream shape that the harness entry re-runs"
  - phase: 50-04
    provides: "hermetic e2e builder; the in-process unit tests were the remaining leak"
  - phase: 50-01
    provides: "tools/semver-gate.sh check --through 50-10"
provides:
  - "devflow_core::monitor: LaunchContext (+ new), MonitorExec { Subcommand; HarnessEntry under test-support }, MONITOR_ARGV_ENV (test-support); spawn_monitor(..., ctx: &LaunchContext)"
  - "devflow CLI: pipeline_launch::launch_context() (cfg(not(test)) cache_dir() + __monitor; cfg(test) DEVFLOW_CACHE_DIR + harness entry); supervise_monitor(..., advance) behind run_monitor; spawn_agent_and_record registers with register_in(&ctx.cache_dir, ...)"
  - "test_support: #[ignore]d monitor_entry_full and monitor_entry_supervise_only; hermetic_cache_dir and run_hermetic_test_in_child; run_test_without_git passes a hermetic cache dir"
  - "Permanent scans: cli_source_registers_roots_only_through_register_in, cli_source_sets_no_hermetic_env_var_and_adds_no_process_env_mutation, the_harness_entry_variant_is_declared_only_under_the_test_support_feature"
  - "doc-check-allowlist.toml: source_to_docs exceptions for DEVFLOW_MONITOR_ARGV and DEVFLOW_TEST_MONITOR_FULL"
affects: [50-11, 50-13, 50-17, 50-24]

actuals:
  tokens: 20900
  tasks: 2
  commits: 3
plan_head_before: 0455de7bb82d1181e530bf1ba05295edc7385b21
plan_head_after: 27e9acc511b572087246531e31514cc2d7e9a1d0

tech-stack:
  added: []
  patterns:
    - "One LaunchContext per launch decides the cache dir and the monitor executable; the test build picks the harness entry at compile time, never through an env override in production code"
    - "An in-crate test that may launch a stage runs its child through test_support::run_hermetic_test_in_child, which owns a /tmp/dfu... DEVFLOW_CACHE_DIR for the child's lifetime"
    - "Source scans walk every .rs under the crate's src/, so a planted unreferenced file is a compile-free mutant"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/tests/monitor_e2e.rs
    - crates/devflow-core/tests/devflow_dir_gitignore.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/test_support.rs
    - crates/devflow-cli/src/pipeline_outcomes.rs
    - crates/devflow-cli/src/pipeline_gate.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/staleness.rs
    - doc-check-allowlist.toml
    - .planning/phases/50-addressable-monitor-liveness/deferred-items.md

key-decisions:
  - "DEVFLOW_MONITOR_ARGV holds the argv AFTER `__monitor` as a JSON array; the harness entry prefixes `devflow __monitor` and parses it with the real Cli, so the entry and `main` share one parser"
  - "The Legacy arm's `advance` tail now runs ctx.exec's executable. Every caller passes the same current_exe() spawn_monitor resolved before, so Legacy behaviour is unchanged"
  - "Production launch_context() fails the launch when no cache dir resolves (no HOME, XDG_CACHE_HOME or DEVFLOW_CACHE_DIR), as the plan specifies. Before 50-10 the registration was a silent no-op in that case"
  - "Instead of editing the enter_agent_free_child! macro alone, every CLI call of devflow_core::test_support::run_test_in_child (71 sites across six files) now goes through one wrapper, run_hermetic_test_in_child, so no sibling wrapper or hand-written child call can miss the cache dir"
  - "The two test-only variables are read through constants (MONITOR_ARGV_ENV, TEST_MONITOR_FULL_ENV); the allowlist entries are added anyway, as the plan requires"

patterns-established:
  - "Run a launching in-crate test through run_hermetic_test_in_child, never devflow_core::test_support::run_test_in_child directly"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "An in-crate Claude stream launch really runs its fake agent through the supervise-only harness entry (marker present, exit file 0) and registers under the child's own cache dir; control: the pre-50-10 Subcommand re-exec of the test binary runs no agent and writes no exit file"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::an_in_crate_stream_launch_runs_its_monitor_through_the_harness_entry"
        status: pass
    human_judgment: false
  - id: D2
    description: "No CLI source registers a root through the env-reading registry::register( (T-50-27), with planted live, commented and register_in controls"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::cli_source_registers_roots_only_through_register_in"
        status: pass
    human_judgment: false
  - id: D3
    description: "MonitorExec::HarnessEntry is declared only under cfg(feature = \"test-support\") (T-50-26), and a release build contains neither harness entry name"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#monitor::tests::the_harness_entry_variant_is_declared_only_under_the_test_support_feature"
        status: pass
      - kind: other
        ref: "cargo build -p devflow --release --quiet && test -x target/release/devflow && ! rg -a -q -e 'monitor_entry_supervise_only|monitor_entry_full' target/release/devflow"
        status: pass
    human_judgment: false
  - id: D4
    description: "Every existing spawn_monitor caller passes a LaunchContext and keeps its behaviour: both core integration targets and the whole devflow-core lib suite pass; semver gate matches --through 50-10"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "cargo nextest run -p devflow-core --test monitor_e2e / --test devflow_dir_gitignore / --lib --no-tests=fail -> 2, 2, 929 passed (1 leaky, pre-existing)"
        status: pass
      - kind: other
        ref: "semver-gate.sh check --through 50-10 -> SEMVER verdict=match"
        status: pass
    human_judgment: false
  - id: D5
    description: "The in-crate CLI suite run with HOME at a sentinel directory leaves no devflow file there (pre-plan: 33 roots/*.json); scripts/check.sh all added no entry to the real ~/.cache/devflow/roots (50-21 saw 33)"
    requirement: SUPV-01
    verification:
      - kind: integration
        ref: "Task 2 gate 1 verbatim (mktemp sentinel HOME, cargo nextest run -p devflow --bin devflow) -> 418 passed, no devflow file"
        status: pass
      - kind: other
        ref: "real ~/.cache/devflow/roots file list before and after scripts/check.sh all: 1041 and 1041, 0 new"
        status: pass
    human_judgment: false
  - id: D6
    description: "No set_var/remove_var names a hermetic variable and the crate keeps at most the 36 guarded 48-09 env mutations; planted two-line, unrelated-variable and count controls"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::cli_source_sets_no_hermetic_env_var_and_adds_no_process_env_mutation"
        status: pass
    human_judgment: false
  - id: D7
    description: "R5-W3: reasoned source_to_docs exceptions for DEVFLOW_MONITOR_ARGV and DEVFLOW_TEST_MONITOR_FULL; both doc-check tests pass"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "doc_check::source_devflow_env_vars_and_subcommands_are_documented and doc_check::allowlist_entries_require_reasons, each 1 passed"
        status: pass
    human_judgment: false
  - id: D8
    description: "test_support::monitor_entry_full (the advance-running entry, DEVFLOW_TEST_MONITOR_FULL=1) parses and dispatches like the supervise-only entry"
    requirement: SUPV-01
    verification: []
    human_judgment: true
    rationale: "No test in this plan selects the full entry; it shares run_monitor_harness_entry with the tested supervise-only entry and differs only in advance=true. 50-11 is its first user"

duration: 20min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 10: Launch Context and In-Crate Monitor Harness Summary

**`spawn_monitor` now takes one `LaunchContext` (cache dir + monitor executable). Inside the CLI's unit-test binary a stream launch re-enters the test binary at an `#[ignore]`d harness entry that runs the real `run_monitor` body, so in-crate launches really supervise their fake agent. Every in-crate child test gets its own `/tmp/dfu…` cache dir, and registration goes only through `register_in`. A full `scripts/check.sh all` run added no entry to the operator's real `~/.cache/devflow/roots` (50-21 measured 33).**

## Needs awareness

- **Production behaviour change, as the plan specifies:** a launch whose cache dir cannot be resolved (HOME, XDG_CACHE_HOME and DEVFLOW_CACHE_DIR all unset) now fails with `cannot resolve DevFlow's cache directory for this launch`. Before, the registration was silently skipped and the launch went ahead. 50-11 needs the cache dir for the socket anyway.
- **None of the in-crate tests ran a launch in the harness process itself.** The plan expected Task 2 to find some and move them into children. With the cache dir added, the full suite passed with no `without a hermetic DEVFLOW_CACHE_DIR` failure, so no test was moved. The 33 pre-plan leaks came from child test processes that inherited HOME (see the attribution below). The negative control shows the error does fire: with the wrapper's cache variable renamed, `launch_stage_persists_monitor_pid_for_reload` failed on exactly that message.
- **A sentinel-HOME run of the CLI suite also leaves `opencode`, `gh`, `mise` and `ccache` state under HOME,** at the pre-plan commit and now. No devflow file is among them, so it is outside this plan's must-have. Logged as a new open item in `deferred-items.md`.

## Performance

- **Duration:** about 20 min
- **Started:** 2026-10-17T19:08:46Z
- **Completed:** 2026-10-17T19:29Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments

- `monitor::LaunchContext { cache_dir, exec }` and `MonitorExec` (`Subcommand { exe }`; `HarnessEntry { exe, test_name }` only with `test-support`), both `#[non_exhaustive]`. `spawn_monitor` takes `ctx: &LaunchContext`. The PipeOwning re-exec is built by `monitor_command`: `Subcommand` → `<exe> __monitor <args…>` as before; `HarnessEntry` → `<exe> <test_name> --exact --include-ignored --test-threads=1`, with the argv after `__monitor` as JSON in `DEVFLOW_MONITOR_ARGV`. `hermetic_command`, `strip_notify_env`, null stdio and the adapter-env order are unchanged.
- `pipeline_launch::launch_context()`. Released build: `registry::cache_dir()` and `Subcommand { current_exe() }`. Test build: the process's own `DEVFLOW_CACHE_DIR` (else the loud error) and `HarnessEntry` naming `test_support::monitor_entry_supervise_only`, or `monitor_entry_full` when `DEVFLOW_TEST_MONITOR_FULL=1`. `spawn_agent_and_record` calls it right before `spawn_monitor` and registers with `register_in(&ctx.cache_dir, …)`.
- `run_monitor` keeps its signature and calls `supervise_monitor(…, advance: true)`. The harness entries parse the env argv with `crate::Cli::try_parse_from`, resolve the project with `crate::project_root` as `main` does, and call `supervise_monitor` with `advance` true or false.
- Every existing `spawn_monitor` caller passes a context with `Subcommand { current_exe() }` (one helper per file): the six `monitor.rs` tests named in the plan, plus the two direct `spawn_monitor_inner` callers (`a_stale_stop_marker_does_not_stop_the_next_agent` and the advance-tail window test), `tests/monitor_e2e.rs` and case 3 of `tests/devflow_dir_gitignore.rs`.
- `test_support::run_hermetic_test_in_child` now wraps all 71 CLI calls of `devflow_core::test_support::run_test_in_child`, and `run_test_without_git` passes a cache dir too.
- Three permanent source scans with planted controls, and two allowlist exceptions.

## Task Commits

1. **Task 1 (tracer): harness entry and register_in.** RED `928cd3f` test(50-10), GREEN `485d41a` feat(50-10)
2. **Task 2: hermetic cache dir for every in-crate child test, env-mutation scan, allowlist.** `27e9acc` test(50-10)

**Tracer feedback gate:** interactive run (`auto_advance=false`, `_auto_chain_active=false`), `human_verify_mode` at its default `end-of-phase`, and Task 1's `<verify>` is automated only, so row 3 applied. All three Task 1 gates were run on the GREEN tree before `485d41a` and passed (rc 0 each). Expansion followed.

## TDD Gate Compliance

| Task | RED | RED evidence (nextest JUnit, `gsd-tools check tdd-red-evidence`) | GREEN |
|------|-----|------------------------------------------------------------------|-------|
| 1 | `928cd3f` | tracer: exit 100, 1 run / 1 failed, `RED_EVIDENCE_OK target_test_failed`; register_in scan: exit 100, 1 run / 1 failed, `RED_EVIDENCE_OK target_test_failed` | `485d41a` |
| 2 | none (`type="auto"`, no `tdd` attribute) | negative controls and mutants below | n/a |

**Semantic assessment (Task 1).** In the RED tree the types, the signature change and `launch_context()` existed, but `monitor_command`'s `HarnessEntry` arm still re-execed `__monitor`. The tracer's child ran its one test. It launched, waited the 30 s bound and failed at its first planned assertion: `the harness entry must run the agent: no marker at /tmp/.tmp…/agent-ran (exit file written: false)`. There was no compile, fixture or load fault. The register_in scan failed on the one real call, `spawn_agent_and_record`'s `registry::register(&state.project_root, state.phase)`. The HarnessEntry-gating scan passed in RED, because the RED commit already declared the variant gated (the "feature already exists" case); mutant M4 below is its discrimination evidence.

## Mutation and Discrimination Evidence

Each mutant was applied, the named test was run, and the file was restored and checked with `cmp` (planted files were deleted and `git status` checked):

| Mutant | Test | Result |
|---|---|---|
| RED tree: HarnessEntry re-execs `__monitor` | tracer | caught: no marker |
| M1 planted unreferenced `src/zz_planted_50_10.rs` with a two-line `set_var(` / `"XDG_CACHE_HOME"` | env-mutation scan | caught: hermetic list |
| M2 same file with one `set_var("DEVFLOW_OTHER", …)` | env-mutation scan | caught: `37 … more than the 36` |
| M3 same file with `devflow_core::registry::register(&root, phase);` | register_in scan | caught: names the planted file |
| M4 `#[cfg(feature = "test-support")]` removed from `HarnessEntry` | gating scan | caught |
| M5 wrapper passes `DEVFLOW_CACHE_DIR_MUTANT` instead of `DEVFLOW_CACHE_DIR` | `launch_stage_persists_monitor_pid_for_reload` | caught: `in-crate test launched a stage without a hermetic DEVFLOW_CACHE_DIR; …` |
| Release binary vs test binary | gate 2's `rg -a` | release: 0 harness names, 3 `__monitor`; debug test binary: 5 harness names |
| Allowlist presence search on `HEAD:doc-check-allowlist.toml` | gate 3's `rg -U` | rc 1 at HEAD (no entry), rc 0 after |

## Sentinel-HOME Leak, Before and After

- **Before (`0455de7`, pre-plan, built in a temporary worktree under `target/`):** `cargo nextest run -p devflow --bin devflow` with HOME at a sentinel left **33** `.cache/devflow/roots/*.json`, the same count 50-21 found in the real cache. Phases: 63-68, 74, 75, 77-80, 84-88, 91, 94 (x2), 100-102, 113, 186, 191, 620, 626, 628 (x2), 629-631.
- **Attribution (by phase number only, as 50-21's was; no per-test run):** the unambiguous phases map to `pipeline_launch` tests (100 `unchanged_recorded_checkpoint_still_auto_decides`, 101 `approving_the_rescan_gate_records_the_set_and_relaunches`, 102 `rejecting_the_rescan_gate_records_nothing_and_falls_through`, 113 `auto_rescan_rejection_parks_for_supervised_repair`, 186 and 191 `advance_with_declared_checkpoint_and_{marker_field,gate_line_only}_relaunches_and_records`) and `preflight` tests (620 `run_preflight_advance_skips_recheck_on_idempotently_failing_check`, 626 `approving_the_preflight_refusal_gate_records_the_set`, 628 `approving_a_non_code_preflight_refusal_records_nothing` / `code_preflight_does_not_record_an_unrecorded_set`, 629 `approving_a_supervise_code_preflight_refusal_records_nothing`, 630 `approving_an_auto_code_preflight_refusal_never_widens_a_recorded_set`, 631 `a_supervise_code_refusal_approval_records_the_pre_agent_set_once`). Phases 63-94 each occur in several test modules (pipeline_launch, preflight, pipeline_outcomes, pipeline_gate, staleness, commands), so they cannot be pinned to one test this way.
- **After (HEAD `27e9acc`):** the same run, both with my scratch sentinel and the plan's verbatim gate, leaves no devflow file. `scripts/check.sh all` added **0** files to the real `~/.cache/devflow/roots` (1041 before, 1041 after; the listing was read-only and nothing there was modified).

**Tests moved into a child process: none.** All launching tests already ran in children (`enter_agent_free_child!` in pipeline_launch, the preflight and pipeline_gate macros, and hand-written `run_test_in_child` calls). Every child-test call in the CLI now gets a hermetic cache dir through `run_hermetic_test_in_child`: pipeline_launch 26 sites (including its macro and the tracer), pipeline_outcomes 41, preflight 1 (its macro), pipeline_gate 1 (its macro), staleness 1; and `run_test_without_git`'s callers in test_support, pipeline_launch and pipeline_outcomes.

## Semver Gate (Task 1, before `928cd3f` and again before `485d41a`)

```
SEMVER lint=constructible_struct_adds_field item=field AgentResult.checkpoint
SEMVER lint=enum_marked_non_exhaustive item=enum MonitorError
SEMVER lint=enum_marked_non_exhaustive item=enum RecoverError
SEMVER lint=enum_marked_non_exhaustive item=enum WorkflowError
SEMVER lint=function_parameter_count_changed item=devflow_core::monitor::spawn_monitor now takes 6 parameters instead of 5,
SEMVER verdict=match
```

Both runs exited 0. Negative control: the saved log compared `--through 50-08` gives `SEMVER verdict=mismatch unexpected=function_parameter_count_changed:devflow_core::monitor::spawn_monitor now takes 6 parameters instead of 5, missing=-` and exits 1. The gate was run with `--log` to a fixed scratch file; it was not edited.

## Verification (commands run, actual exit codes)

| Gate | Result |
|---|---|
| Task 1 `<automated>` 1 (three named tests), verbatim | rc 0; each `1 test run: 1 passed` |
| Task 1 `<automated>` 2 (release build, no harness names), verbatim | rc 0 |
| Task 1 `<automated>` 3 (two core integration targets, core lib, semver gate) | rc 0; `2 passed`, `2 passed`, `929 passed (1 leaky)`, `verdict=match`. Semver gate run with `--log` |
| Task 2 `<automated>` 1 (sentinel-HOME suite), verbatim incl. `mktemp` and the `rm -rf` trap | rc 0; `418 tests run: 418 passed (2 leaky), 2 skipped`; no `/tmp/p50home.*` left |
| Task 2 `<automated>` 2 (env scan, fmt, clippy workspace), verbatim | rc 0 |
| Task 2 `<automated>` 3 (both doc-check tests, allowlist presence), verbatim | rc 0; each doc-check test `1 passed` |
| Plan `<verification>`: `scripts/check.sh all` | rc 0, `==> check.sh: all OK`, 38 `test result: ok` lines |
| Extra: `cargo clippy -p devflow-core --lib -- -D warnings` (no test-support feature) | rc 0 |

The 2 skipped are the two `#[ignore]`d harness entries. The leaky tests are the pre-existing `commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling`, `commands::tests::reachable_pids_are_excluded_from_both_the_findings_and_the_reap_candidates` (CLI) and `agent::tests::discover_stray_devflow_processes_finds_a_monitor_wrapper` (core), already in `deferred-items.md`. After the runs, no process carrying this worktree's `CARGO_MANIFEST_DIR` was alive, and no `/tmp/dfu*`, `/tmp/dfs*`, `/tmp/dfh*` or `/tmp/p50home*` directory remained.

**What these checks do not establish:**
- `monitor_entry_full` (the advance-running entry) is never selected by a test in this plan. It shares its parsing and dispatch with the tested supervise-only entry, but its advance tail inside a test binary is unproven until 50-11 uses it.
- The tracer proves a Claude stream launch, once. Each gate ran once, so there is no flakiness bound. The Antigravity stream shape and the Legacy arm under a harness context are not exercised in-crate. Legacy is unchanged by design and still runs `<test binary> advance`, which libtest rejects.
- The sentinel check sees only files still under HOME when the suite ends, and only under that one HOME. The real-cache comparison covers one `check.sh all` run.
- The release-binary search proves the two entry names are absent. It does not prove that no other test-only code path is present; that rests on `cfg(test)` and the feature gate, which the gating scan pins for `HarnessEntry` only.
- The pre-plan leak attribution is by phase number. 13 of the 31 phases map to single tests; the rest are ambiguous.

## Files Created/Modified

- `crates/devflow-core/src/monitor.rs`: `MONITOR_ARGV_ENV`, `MonitorExec`, `LaunchContext`, the `spawn_monitor`/`spawn_monitor_inner` context parameter, `monitor_command`, the Legacy tail's exe from the context, `test_launch_context`, and the gating scan with its helper
- `crates/devflow-core/tests/monitor_e2e.rs`, `crates/devflow-core/tests/devflow_dir_gitignore.rs`: a `launch_context` helper each, passed to `spawn_monitor`
- `crates/devflow-cli/src/pipeline_launch.rs`: `launch_context` (both cfgs), `TEST_MONITOR_FULL_ENV`, `supervise_monitor`, `register_in`, the tracer and its helpers, the two CLI scans
- `crates/devflow-cli/src/test_support.rs`: the harness entries, `hermetic_cache_dir`, `run_hermetic_test_in_child`, and the cache dir in `run_test_without_git`
- `crates/devflow-cli/src/{pipeline_outcomes,pipeline_gate,preflight,staleness}.rs`: child-test calls renamed to the wrapper, nothing else
- `doc-check-allowlist.toml`: the two exceptions, with the plan's exact reasons
- `deferred-items.md`: the 50-21 cache-leak entry marked resolved, and one new open entry (opencode/gh/mise/ccache HOME writes)

## Decisions Made

See `key-decisions` in the frontmatter. All are inside the plan's remit.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Every child-test call, not only the macro, gets the cache dir**
- **Found during:** Task 2
- **Issue:** the plan names `enter_agent_free_child!` "and any sibling child wrapper". The CLI also has about 50 hand-written `run_test_in_child` calls, and `run_test_without_git`. A launch from any of them would have kept registering under HOME.
- **Fix:** one wrapper, `run_hermetic_test_in_child`, which every call now goes through, plus the cache dir in `run_test_without_git`. Same signature, so no assertion changed.
- **Files modified:** `pipeline_outcomes.rs`, `pipeline_gate.rs` and `staleness.rs` are outside the plan's `files_modified`. `commands.rs`, which the plan lists, needed no change because it has no child-test call.
- **Verification:** the sentinel-HOME gate, the real-cache comparison and mutant M5.

**2. [Rule 1 - Test correctness] The register_in scan flagged its own assertion message**
- **Found during:** Task 1 RED
- **Issue:** the planted-control message text contained the scan's needle, so the real scan listed it.
- **Fix:** reworded the message ("a live unscoped register call"). The RED evidence was taken after the fix and names only the real call.

**3. [Rule 3 - Blocking] A shared target dir served a stale `devflow-core`**
- **Found during:** Task 2, after the pre-plan measurement
- **Issue:** I built the pre-plan commit in a temporary worktree with `CARGO_TARGET_DIR` set to this worktree's `target/`. Cargo hashes path packages by their workspace-relative path, so both checkouts wrote the same artifact names. The next build here linked the old `devflow-core` (`spawn_monitor takes 5 arguments`).
- **Fix:** `cargo clean -p devflow-core -p devflow` and a rebuild. Every test result reported here comes after that rebuild. The temporary worktree was removed (`git worktree remove`). Lesson: never share `CARGO_TARGET_DIR` between two checkouts of one workspace.

**4. [Orchestrator-directed] Tracking, guard and gate form**
- STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched, and `.planning/user/errors/state-stale.json` was left alone. `pin-guard.sh` passed before the first edit and before every commit. The `agent-*` branch allow-list does not apply to this sequential executor on `feature/phase-50`.
- The semver gate ran with `--log` to a fixed scratch file instead of a `mktemp`/`rm -rf` wrapper. Task 2's gate 1, which does use `mktemp` and an `rm -rf` trap, ran verbatim without being refused.

---

**Total deviations:** 3 auto-fixed (1 missing-critical, 1 test correctness, 1 blocking), plus 1 process note
**Impact on plan:** no change to the planned public API. The wider wrapper adoption touches three more files than listed, by renaming the call only.

## Issues Encountered

- The tracer's RED took its full 30 s bound, because the monitor never ran. GREEN completes in about 0.14 s.

## Known Stubs

None.

## User Setup Required

None.

## Next Phase Readiness

- 50-11 can route both shapes through `__monitor`. In-crate launches already supervise through the harness entry, and every in-crate child has its own short cache dir under `/tmp` for sockets.
- 50-11 should exercise `monitor_entry_full` (with `DEVFLOW_TEST_MONITOR_FULL=1`) wherever a test needs the advance tail. The Legacy-arm tests 50-08 maps for deletion are untouched here.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

- Files: `crates/devflow-core/src/monitor.rs`, `crates/devflow-cli/src/pipeline_launch.rs`, `crates/devflow-cli/src/test_support.rs`, `doc-check-allowlist.toml` and this SUMMARY exist.
- Commits `928cd3f`, `485d41a` and `27e9acc` are ancestors of HEAD. The RED `test(50-10)` commit `928cd3f` precedes the GREEN `feat(50-10)` commit `485d41a`.
