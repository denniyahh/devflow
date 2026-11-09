---
phase: 50-addressable-monitor-liveness
plan: 21
subsystem: testing
tags: [rust, e2e, test-isolation, hermetic-home, source-scan, nextest]

requires:
  - phase: 50-02
    provides: "tests/common/mod.rs (HermeticHome, hermetic_devflow, LaunchedRunReaper) and the scan suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment"
  - phase: 50-03
    provides: "the migration pattern for the other eight e2e suites and the no-real-reap-strays scan"
provides:
  - "auto_chain_flag_e2e, yes_ship_config, release_check, reap_strays_e2e, help_snapshot spawn devflow only through common::hermetic_devflow"
  - "tests/hermetic_spawn_sites.rs: permanent scans migrated_suites_spawn_devflow_only_through_the_hermetic_builder and only_common_names_the_devflow_binary"
affects: [50-11, socket tests, any new e2e suite]

actuals:
  tokens: 4661
  tasks: 2
  commits: 2
plan_head_before: 96abf30877abc0081d1109811e8e82c663657255
plan_head_after: 99731efd5db9ba34e68bc9d79277d3e98e290189

tech-stack:
  added: []
  patterns:
    - "A helper whose child is waited on with output() creates its HermeticHome locally; a fixture that owns spawns holds it as its last field"
    - "A source scan spells its tokens through concat! and a fn (binary_env_name), so the scanning file cannot match itself"

key-files:
  created:
    - crates/devflow-cli/tests/hermetic_spawn_sites.rs
  modified:
    - crates/devflow-cli/tests/auto_chain_flag_e2e.rs
    - crates/devflow-cli/tests/yes_ship_config.rs
    - crates/devflow-cli/tests/release_check.rs
    - crates/devflow-cli/tests/reap_strays_e2e.rs
    - crates/devflow-cli/tests/help_snapshot.rs
    - .planning/phases/50-addressable-monitor-liveness/deferred-items.md

key-decisions:
  - "release_check: the explicit .env(\"HOME\", tempdir) is gone because hermetic_devflow already sets HOME to a fresh empty HermeticHome; the child still gets an isolated empty HOME, and the SSH_* removals are unchanged"
  - "auto_chain_flag_e2e: a LaunchedRunReaper is bound after each foreground __monitor run. The run halts at stop_until=Code and should launch nothing; the reaper covers a regressed halt that would detach a next-stage monitor"
  - "BINARY_NAMERS keeps recover_clean_e2e.rs as the plan specifies, although 50-04 has already landed and migrated it"

patterns-established:
  - "Every e2e suite under crates/devflow-cli/tests/ now spawns devflow through common::hermetic_devflow; two permanent scans keep it that way"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "auto_chain_flag_e2e (3), yes_ship_config (5) and release_check (6) spawn devflow only through the hermetic builder, with unchanged test counts, and pass under a sentinel HOME leaving no devflow file"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/hermetic_spawn_sites.rs#migrated_suites_spawn_devflow_only_through_the_hermetic_builder"
        status: pass
      - kind: e2e
        ref: "Task 1 <automated> gate, verbatim, exit 0 (14 tests run: 14 passed; nothing under the sentinel)"
        status: pass
    human_judgment: false
  - id: D2
    description: "reap_strays_e2e (2) and help_snapshot (1) spawn devflow only through the hermetic builder; help snapshot unchanged; reap_strays_e2e still runs no real stray reap"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "Task 2 <automated> gate, verbatim, exit 0 (3 tests run: 3 passed; snapshot git diff --quiet; nothing under the sentinel)"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/tests/gate_sweep_e2e.rs#no_suite_on_the_common_builder_runs_a_real_reap_strays_pass"
        status: pass
    human_judgment: false
  - id: D3
    description: "Under crates/devflow-cli/tests/ only common/mod.rs (and the tolerated recover_clean_e2e.rs) names the binary's env var in code"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/hermetic_spawn_sites.rs#only_common_names_the_devflow_binary"
        status: pass
    human_judgment: false

duration: 7min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 21: Hermetic e2e migration, last five suites (999.141) Summary

**The last five e2e suites now spawn `devflow` only through `common::hermetic_devflow`. A new test target, `hermetic_spawn_sites.rs`, holds two permanent scans: one pins all five suites to the builder, and one fails if any file under `tests/` other than `common/mod.rs` names the binary's env var.**

## Needs awareness

- **The operator's real cache still grows during a full check run, and these five suites are not the cause.** Before migration, the five suites left no devflow file under a sentinel HOME (17 tests, `leaked=[]`). So the sentinel leak check in both task gates passes on unmigrated code too, and does not tell migrated from unmigrated. The two Rust scans are what pin the migration. During `scripts/check.sh all` at this plan's HEAD, 33 new `roots/*.json` files still appeared in the real `~/.cache/devflow/`. Each names a `tempfile` root (`/tmp/.tmp…`). Their phase numbers (186, 191, 113, 620–631, …) occur only in the unit-test modules of `crates/devflow-cli/src/pipeline_launch.rs` and `preflight.rs`, so the writers are in-process `devflow` unit tests, not any integration suite. This answers 50-03's open question. The attribution rests on phase-number matching only; no per-test run was made. It is logged in `deferred-items.md`. Nothing in the real cache was deleted.
- `recover_clean_e2e.rs` stays in `BINARY_NAMERS` as the plan specifies, although 50-04 has already migrated it. That costs no coverage today, because 50-02's scan also refuses the binary's env name in any suite declaring `mod common;`. A later plan could drop the entry.

## Performance

- **Duration:** about 7 min
- **Started:** 2026-10-17T17:13:58Z
- **Completed:** 2026-10-17T17:21Z
- **Tasks:** 2
- **Files modified:** 6 test files (1 new), plus deferred-items.md

## Accomplishments

- Each of the five files declares `mod common;`, has no local `devflow_bin()`, and builds its one `devflow` spawn site with `common::hermetic_devflow`. No assertion, argument list or test name changed.
- `hermetic_spawn_sites.rs`. `migrated_suites_spawn_devflow_only_through_the_hermetic_builder` runs five planted controls, then requires each of the five suites to have 0 raw spawns or binary env names and at least one `common::hermetic_devflow(` call. `only_common_names_the_devflow_binary` runs two planted controls, then walks `tests/` recursively. It requires `common/mod.rs` to be found and every file that names the binary to be in `BINARY_NAMERS`.

### Test counts (`cargo nextest list -p devflow --test T | wc -l`)

| Suite | Before (measured at 96abf30) | After |
|---|---|---|
| auto_chain_flag_e2e | 3 | 3 |
| yes_ship_config | 5 | 5 |
| release_check | 6 | 6 |
| reap_strays_e2e | 2 | 2 |
| help_snapshot | 1 | 1 |
| hermetic_spawn_sites (new) | — | 2 |

The count form was checked against a missing target, which prints `0`.

## Task Commits

1. **Task 1 (tracer): auto_chain_flag_e2e, yes_ship_config, release_check, plus the first scan.** `6e4843e` test(50-21)
2. **Task 2: reap_strays_e2e, help_snapshot, plus the binary-naming scan.** `99731ef` test(50-21)

Tracer feedback gate: the run was interactive (`auto_advance` and `_auto_chain_active` both false), the mode was end-of-phase, and the verify block was automated only. So the Task 1 gate was re-run on the committed bytes before Task 2, and it exited 0.

## Verification (actual runs)

- **Task 1 `<automated>`, verbatim:** exit 0. Scan `1 passed`, list counts 3/5/6, sentinel run `14 tests run: 14 passed`, nothing found under the sentinel. The tracer re-run on commit `6e4843e` also exited 0.
- **Task 2 `<automated>`, verbatim:** exit 0. Both scans `1 passed`, list counts 2/1, `git diff --quiet` on `devflow-help.txt` exit 0, sentinel run `3 tests run: 3 passed (1 leaky)`, nothing found under the sentinel. The LEAK mark is on `reap_strays_e2e`'s `reap_clears_a_process_whose_root_was_deleted…` and shows on the pre-migration file too.
- **Negative controls on the scans:**
  - With `HEAD`'s pre-migration `yes_ship_config.rs` swapped in, scan 1 failed (exit 100) with `yes_ship_config.rs: 2 raw devflow spawn site(s)…` and `no common::hermetic_devflow( call`.
  - With pre-migration `help_snapshot.rs` swapped in and a planted `tests/common/planted_control.rs` naming the binary's env var, both scans failed (exit 100). Scan 2 listed `common/planted_control.rs` and `help_snapshot.rs`, which shows that the walk recurses and reports `/`-joined relative paths.
  - Both files were restored byte-identical (`cmp`), and the planted file was removed.
- **Existing scans now covering these files:** `test_home_isolation::suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment` `1 passed`; `gate_sweep_e2e::no_suite_on_the_common_builder_runs_a_real_reap_strays_pass` `1 passed`.
- **Plan `<verification>`:** `cargo clippy --workspace --all-targets -- -D warnings` exit 0. `cargo fmt --all --check` exit 0. `scripts/check.sh all` exit 0 (`==> check.sh: all OK`, 38 `test result: ok` lines, both new scans `ok`).

What these checks do not establish:
- Each run happened once, so the results give no bound on flakiness.
- The sentinel check sees only files still under HOME when the suite ends.
- The scans are textual. They catch the three named raw-spawn spellings and the env name in non-`//` lines. A spawn spelled another way, such as through a `let bin = …` alias of a path that is not the env var, would not be caught. A `/* */` block comment would count as code, which can only add a flag, never hide one.
- The container parity check was not run.

## Files Created/Modified

- `hermetic_spawn_sites.rs` (new): private helpers `tests_dir`, `code_lines`, `binary_env_name`, `raw_spawn_sites`, `read_suite`, `names_binary`, `rust_files_under`; consts `HERMETIC_SUITES` (5) and `BINARY_NAMERS`. It declares no `mod common;` and spawns nothing.
- `auto_chain_flag_e2e.rs`: `Fixture.home` is the last field. `run_monitor` uses the builder. Each of the 3 tests binds a `LaunchedRunReaper` right after its monitor run. The orphaned `Command` import is removed.
- `yes_ship_config.rs`: `run_dry_run` creates its own `HermeticHome` (the `--dry-run` child is waited on and launches nothing). The orphaned `Command` import is removed.
- `release_check.rs`: `run_release`'s tempdir HOME is now a `HermeticHome` passed to the builder (see key-decisions). The orphaned `Command` import is removed.
- `reap_strays_e2e.rs`: the one `devflow stop` spawn uses the builder, with the home bound as the test's first line. `std::process::Command` stays for the `sh` fixtures. No reap path changed. The file still never runs `gate sweep --reap-strays` for real.
- `help_snapshot.rs`: the `--help` spawn uses the builder. The help text did not change.
- `deferred-items.md`: one new entry, for the in-process unit-test cache leak.

## Decisions Made

See `key-decisions` in the frontmatter.

## Deviations from Plan

None in code. The plan was executed as written. Two items are recorded as decisions rather than deviations: release_check's HOME is now set by the builder, and auto_chain_flag_e2e gained a defensive reaper.

## Issues Encountered

None.

## User Setup Required

None.

## Next Phase Readiness

- 50-11 can bind sockets under the cache dir without any integration suite reaching the operator's real cache.
- Before 50-11, the in-process unit tests in `pipeline_launch.rs` and `preflight.rs` that register roots against the real HOME should be scoped. See `deferred-items.md`.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED

- FOUND: crates/devflow-cli/tests/hermetic_spawn_sites.rs and the five modified suites
- FOUND: 6e4843e, 99731ef (ancestors of HEAD)
