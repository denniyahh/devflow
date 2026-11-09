---
phase: 50-addressable-monitor-liveness
plan: 03
subsystem: testing
tags: [rust, e2e, test-isolation, hermetic-home, source-scan, nextest, gate-sweep]

requires:
  - phase: 50-02
    provides: "tests/common/mod.rs (HermeticHome, hermetic_devflow, kill_and_reap, LaunchedRunReaper) and the permanent scan suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment"
provides:
  - "phase7_cli, gate_wedge_e2e, gate_notify_persist_e2e, gate_sweep_e2e, start_reachability_e2e, log_format_env, doctor_antigravity, auto_chain_leak_repair_e2e spawn devflow only through common::hermetic_devflow"
  - "gate_sweep_e2e.rs: permanent scan no_suite_on_the_common_builder_runs_a_real_reap_strays_pass (private code_only, real_reap_strays_statements)"
affects: [50-04, 50-11, 50-21, gate sweep tests, socket tests]

actuals:
  tokens: 16694
  tasks: 2
  commits: 2
plan_head_before: 7dfd069211079f80ef8768e4b491f6d350243ce0
plan_head_after: 1601fd778c0619e15070efd9f38f35837a7c1a2f

tech-stack:
  added: []
  patterns:
    - "A test binds `let home = common::HermeticHome::new();` as its first line, so it drops after every child guard and reaper"
    - "A file-local child guard that must kill on drop calls common::kill_and_reap rather than its own kill/wait"
    - "A fixture struct that owns spawns holds its HermeticHome as its LAST field (fields drop in declaration order)"

key-files:
  created:
    - .planning/phases/50-addressable-monitor-liveness/deferred-items.md
  modified:
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/gate_wedge_e2e.rs
    - crates/devflow-cli/tests/gate_notify_persist_e2e.rs
    - crates/devflow-cli/tests/gate_sweep_e2e.rs
    - crates/devflow-cli/tests/start_reachability_e2e.rs
    - crates/devflow-cli/tests/log_format_env.rs
    - crates/devflow-cli/tests/doctor_antigravity.rs
    - crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs
    - .planning/WINDOWS.md

key-decisions:
  - "gate_notify_persist_e2e: isolated_cache + ReapOnDrop(Child, TempDir) became a test-level HermeticHome passed to every spawn helper plus a ParkedChild(Child) guard whose Drop calls common::kill_and_reap; `gate approve` and `stop` in the agent-chain arms now share that home"
  - "gate_sweep_e2e gets no reaper: its doc forbids any process-termination call in the file, and none of its spawns launches a monitor"
  - "gate_wedge_e2e gets no LaunchedRunReaper: its start arms park at the Define preflight gate and its advance arms are foreground; no monitor is launched. Its local ReapOnDrop stays (the plan removes only gate_notify_persist_e2e's)"
  - "phase7_cli's 16 run-launching spawns each bind a LaunchedRunReaper right after the spawn, in addition to the existing MonitorReapGuard, which is declared later and so still drops first"
  - "The scan recognises a suite by `line.trim() == \"mod common;\"`, the same rule 50-02's scan uses"

patterns-established:
  - "Statement-level source scan: split comment-stripped code on `;`, test literals spelled through concat! so the scanning file cannot match itself"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "Three gate/phase7 suites (phase7_cli 28, gate_wedge_e2e 5, gate_notify_persist_e2e 9 tests, unchanged) spawn devflow only through the hermetic builder; isolated_cache/ReapOnDrop gone from gate_notify_persist_e2e; under a sentinel HOME they leave no devflow file"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/test_home_isolation.rs#suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment"
        status: pass
      - kind: e2e
        ref: "sentinel HOME: cargo nextest run -p devflow --test phase7_cli --test gate_wedge_e2e --test gate_notify_persist_e2e --no-tests=fail -E 'not test(=suite_reap_audit)' (41 passed) + cargo test -p devflow --test phase7_cli (28 passed); leaked=[]"
        status: pass
    human_judgment: false
  - id: D2
    description: "Five more suites (gate_sweep_e2e 4+1, start_reachability_e2e 2, log_format_env 3, doctor_antigravity 2, auto_chain_leak_repair_e2e 2) spawn devflow only through the hermetic builder and pass under a sentinel HOME with nothing leaked"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "Task 2 <automated> gate, verbatim, exit 0 (14 tests run: 14 passed; leaked=[])"
        status: pass
    human_judgment: false
  - id: D3
    description: "Permanent scan refuses any non-dry-run --reap-strays in a suite on the builder; planted controls pass, and a real pass planted in doctor_antigravity.rs makes it fail"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/gate_sweep_e2e.rs#no_suite_on_the_common_builder_runs_a_real_reap_strays_pass"
        status: pass
    human_judgment: false

duration: 14min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 03: Hermetic e2e migration, eight suites (999.141) Summary

**Eight more e2e suites now spawn `devflow` only through `common::hermetic_devflow`, with a `LaunchedRunReaper` after every run-launching spawn. A new permanent source scan in `gate_sweep_e2e.rs` refuses any non-dry-run `--reap-strays` pass in a suite on the builder.**

## Needs awareness

- **Task 1's gate cannot pass as written, and the cause predates this plan.** The gate runs all of `phase7_cli` under nextest. Nextest runs each test in its own process, so `suite_reap_audit`'s process-local `REAPED` counter is always 0 there, and the test fails after 60 s with `alive=[], in_flight=0, registered=0`. The pre-migration file from `HEAD` fails identically (exit 100), so the migration did not cause it. The test passes under `cargo test`, which is how `scripts/check.sh` runs it. Substitute evidence is below. Logged in `deferred-items.md` and as a `deviation` in `.planning/WINDOWS.md`, which is open and so blocks `/gsd-ship` until it is fixed or waived.
- **For the five Task 2 suites, the sentinel-HOME leak check cannot tell migrated from unmigrated code.** The pre-migration versions of all five, run under a sentinel HOME, also left nothing (13 passed, `leaked=[]`). They never wrote to the cache. Their migration is preventive (for 50-11's sockets), and what pins it is the two Rust scans, not the leak check. For Task 1 the check does discriminate: the pre-migration `phase7_cli` test `start_no_worktree_uses_feature_branch` leaked `roots/349763d5000738b6-12.json` into the sentinel (exit 1).

## Performance

- **Duration:** about 14 min
- **Started:** 2026-10-17T11:41:24Z
- **Completed:** 2026-10-17T11:55Z
- **Tasks:** 2
- **Files modified:** 8 test files (+ deferred-items.md, WINDOWS.md)

## Accomplishments

- Each of the eight files declares `mod common;`, has no local `devflow_bin()`, and builds every `devflow` child with `common::hermetic_devflow(&home)`. Spawn sites migrated: phase7_cli 12 (11 direct + the `run_devflow_inner` helper, behind 21 helper call sites), gate_wedge_e2e 7, gate_notify_persist_e2e 5, gate_sweep_e2e 4, start_reachability_e2e 2, log_format_env 2, doctor_antigravity 2, auto_chain_leak_repair_e2e 2.
- `LaunchedRunReaper` is bound right after each spawn that launches a run: phase7_cli 16 sites (17 reapers, since `parallel` launches phases 7 and 8), gate_notify_persist_e2e 2 (`resume_with`, the agent-chain arms), and auto_chain_leak_repair_e2e 1 (after the `__monitor` spawn).
- `no_suite_on_the_common_builder_runs_a_real_reap_strays_pass`: planted controls first (a real pass is flagged, the same with `"--dry-run"` and the same behind `//` are not), then every `tests/*.rs` declaring `mod common;`. Both literals and all planted statements are spelled through `concat!`, so the file's source does not contain them as written.

### Test counts (`cargo nextest list -p devflow --test T | wc -l`)

| Suite | Before (measured 2026-10-17, pre-edit) | After |
|---|---|---|
| phase7_cli | 28 | 28 |
| gate_wedge_e2e | 5 | 5 |
| gate_notify_persist_e2e | 9 | 9 |
| gate_sweep_e2e | 4 | 5 (+ the new scan) |
| start_reachability_e2e | 2 | 2 |
| log_format_env | 3 | 3 |
| doctor_antigravity | 2 | 2 |
| auto_chain_leak_repair_e2e | 2 | 2 |

## Task Commits

1. **Task 1 (tracer): phase7_cli, gate_wedge_e2e, gate_notify_persist_e2e.** `6652bdd` test(50-03)
2. **Task 2: five more suites + the reap-strays scan.** `1601fd7` test(50-03)

The tracer feedback gate (interactive, end-of-phase, automated-only verify) was re-run on the committed bytes before Task 2. It passed with the `suite_reap_audit` substitution described above.

## Verification (actual runs)

- **Task 1 `<automated>`, verbatim:** exit 100. Every step before the sentinel run passed: three `mod common;` checks, `isolated_cache`/`ReapOnDrop` absent, 50-02's scan `1 passed`, list counts 28/5/9. In the sentinel run, 41 tests passed and `suite_reap_audit` failed as described above.
- **Task 1 substitute:** under one sentinel HOME, nextest over the three suites with `-E 'not test(=suite_reap_audit)'` ran 41 tests, all passed (`nextest_rc=0`). `cargo test -p devflow --test phase7_cli` ran 28, all passed, `suite_reap_audit` included (`cargo_test_rc=0`). Result `leaked=[]`, exit 0.
- **Task 2 `<automated>`, verbatim:** exit 0. Both scans `1 passed`, list counts 5/2/3/2/2, sentinel run `14 tests run: 14 passed`, `leaked=[]`.
- **Scan discrimination:** a real `"--reap-strays"` call appended to `doctor_antigravity.rs` made the scan fail (exit 100, naming that file). The file was then restored byte-identical (`cmp`).
- **Plan `<verification>`:** `cargo clippy --workspace --all-targets -- -D warnings` exit 0; `cargo fmt --all --check` exit 0; `scripts/check.sh all` exit 0 (`==> check.sh: all OK`, 37 test binaries `ok`, `suite_reap_audit ... ok`, `no_suite_on_the_common_builder_runs_a_real_reap_strays_pass ... ok`).
- **Real cache:** nothing under `~/.cache/devflow` was newer than the marker after the Task 1 gate runs. During `scripts/check.sh all`, new `roots/*.json` entries did appear there. They cannot come from these eight suites, which leave nothing under a sentinel HOME. They come from targets not yet migrated (50-21, 50-04) or from in-process unit tests; I did not attribute them per target.

What these checks do not establish: each pass is a single run, not a flake bound. The leak check covers only files under HOME at the moment the suite ends. phase7_cli's documented orphaned `devflow advance` children (HYG-01) outlive the suite by up to 60 s. Every one observed carried a hermetic `HOME=/tmp/dfh…`, and after the first batch exited no `/tmp/dfh*` directory was recreated. Writes they might make after exiting were not checked beyond that. The scan splits on `;` blindly, so a `;` inside a string literal splits a statement. That can only add flags, never hide a real pass.

## Files Created/Modified

- `phase7_cli.rs`: `run_devflow`, `run_devflow_legacy_launch` and `run_devflow_inner` take `&common::HermeticHome` first. 27 tests open with a `HermeticHome` (`suite_reap_audit` spawns nothing).
- `gate_wedge_e2e.rs`: `spawn_gated_advance`, `reject`, `start_child`, `spawn_parked_start` and `reject_define` take the home. Local `FakeBin`, `wait_for` and `ReapOnDrop` stay.
- `gate_notify_persist_e2e.rs`: `ParkedChild` replaces `ReapOnDrop`, and `isolated_cache` is deleted. `spawn_parked_start(_with)`, `spawn_parked_advance` and `agent_environment_after_define` take the home. `MarkedReaper` stays.
- `gate_sweep_e2e.rs`: four spawns migrated and the scan appended (with a local copy of 50-02's `code_only` helpers). The orphaned `std::process::Command` import is removed.
- `start_reachability_e2e.rs`, `log_format_env.rs` (`run_status` takes the home), `doctor_antigravity.rs`: spawns migrated and orphaned imports removed. `log_format_env`'s explicit `RUST_LOG`/`DEVFLOW_LOG_FORMAT` handling is untouched, because the builder sets neither.
- `auto_chain_leak_repair_e2e.rs`: `Fixture.home` is the last field, dropping after `reaper`.

## Decisions Made

See `key-decisions` in the frontmatter. None alters an assertion, env var or argument list.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Task 1 gate replaced by an equivalent run for one pre-existing nextest-incompatible test**
- **Found during:** Task 1
- **Issue:** `phase7_cli::suite_reap_audit` cannot pass under nextest. This is pre-existing: it fails identically on `HEAD`'s file.
- **Fix:** no code change, because the test is out of scope. The sentinel-HOME check ran with it excluded under nextest, and with all 28 tests under `cargo test`. Recorded in `deferred-items.md` and WINDOWS.md.
- **Verification:** see Verification above.

**2. [Rule 3 - Blocking] Orphaned `std::process::Command` imports removed** in gate_sweep_e2e, start_reachability_e2e, log_format_env and doctor_antigravity (clippy `-D unused-imports`). The migration itself made them unused.

---

**Total deviations:** 2 (both Rule 3). **Impact:** no test, assertion or count changed. Deviation 1 is in verification, not code.

## Issues Encountered

None beyond the deviations.

## User Setup Required

None.

## Next Phase Readiness

- 50-21 can follow the same pattern for its five suites. Its `hermetic_spawn_sites.rs` scan ignores `//` lines, so the doc comment in `doctor_antigravity.rs` that names the binary variable will not trip it.
- `suite_reap_audit` should be made nextest-aware, or a phase gate should run `phase7_cli` under `cargo test`, before any later plan gates on a nextest run of the whole target.

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED
