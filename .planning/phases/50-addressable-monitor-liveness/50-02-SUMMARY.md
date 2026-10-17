---
phase: 50-addressable-monitor-liveness
plan: 02
subsystem: testing
tags: [rust, e2e, test-isolation, hermetic-home, registry, source-scan, nextest]

requires:
  - phase: 23
    provides: "registry::cache_dir() resolution order (DEVFLOW_CACHE_DIR, XDG_CACHE_HOME/devflow, HOME/.cache/devflow)"
provides:
  - "tests/common/mod.rs: devflow_bin, HermeticHome (/tmp, prefix dfh), hermetic_devflow, FakeBin, fake_bin_dir, fake_bin_dir_for, wait_for, kill_and_reap, LaunchedRunReaper, entries_under"
  - "tests/test_home_isolation.rs: V-26 a_child_writing_outside_its_tempdir_home_fails (with plain-Command negative control) and the permanent scan suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment"
  - "start_lock_e2e.rs and stop_e2e.rs spawn devflow only through common::hermetic_devflow"
affects: [50-03, 50-04, 50-11, 50-21, socket tests, gate sweep tests]

actuals:
  tokens: 16200
  tasks: 2
  commits: 3
plan_head_before: f1bb4aa861a39ded9f18b79fbaddd64069dc1c73
plan_head_after: 8d24b3e73c7095bc638918b9aa33cbef5f5de04f

tech-stack:
  added: []
  patterns:
    - "Every e2e devflow child is built by common::hermetic_devflow(&HermeticHome); the home is bound before the spawn and outlives any LaunchedRunReaper"
    - "A suite is covered by the hermetic source scan as soon as it declares `mod common;`"
    - "Scan tokens are spelled through concat! split inside the token so the scanning file holds a raw spawn token exactly once"

key-files:
  created:
    - crates/devflow-cli/tests/common/mod.rs
    - crates/devflow-cli/tests/test_home_isolation.rs
  modified:
    - crates/devflow-cli/tests/start_lock_e2e.rs
    - crates/devflow-cli/tests/stop_e2e.rs

key-decisions:
  - "The hermetic test checks the builder's get_envs() before spawning, so a builder that sets nothing fails without writing to the inherited HOME"
  - "The R-13 control spawn also strips the GIT_* variables: its subject is HOME, and git hermeticity is a separate concern"
  - "The four start_lock_e2e tests that launch a run now bind a LaunchedRunReaper (threat T-50-04); no assertion changed"

patterns-established:
  - "Hermetic e2e child: let home = common::HermeticHome::new(); before the spawn, then common::hermetic_devflow(&home)"

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "Shared hermetic builder keeps an e2e devflow child's registry writes inside its own tempdir HOME, proven in both directions (hermetic run populates H's roots, plain-Command control populates R2's roots)"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/test_home_isolation.rs#a_child_writing_outside_its_tempdir_home_fails"
        status: pass
    human_judgment: false
  - id: D2
    description: "Permanent source scan: every tests/*.rs declaring `mod common;` has no raw devflow spawn (exactly one, the R-13 control, in test_home_isolation.rs) and no set_var/remove_var, nor does common/mod.rs; planted controls flag and pass as specified"
    requirement: SUPV-01
    verification:
      - kind: unit
        ref: "crates/devflow-cli/tests/test_home_isolation.rs#suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment"
        status: pass
    human_judgment: false
  - id: D3
    description: "start_lock_e2e (12 tests) and stop_e2e (22 tests) migrated onto the builder with unchanged counts and assertions; under a sentinel HOME the three suites leave no devflow file"
    requirement: SUPV-01
    verification:
      - kind: e2e
        ref: "Task 2 <automated> gate (rg mod common/hermetic_devflow, scan test, nextest list counts 12/22, sentinel-HOME run + find leak check)"
        status: pass
    human_judgment: false

duration: 10min
completed: 2026-10-17
status: complete
---

# Phase 50 Plan 02: Hermetic e2e HOME (999.141) Summary

**Shared `common::hermetic_devflow` builder that gives every e2e `devflow` child a `/tmp` tempdir HOME, XDG cache/config and DEVFLOW_CACHE_DIR with GIT_* stripped, a negative-controlled R-13 test proving it, a permanent source scan over every suite that adopts it, and `start_lock_e2e`/`stop_e2e` migrated onto it.**

## Performance

- **Duration:** about 10 min
- **Started:** 2026-10-17T11:25:46Z
- **Completed:** 2026-10-17T11:36Z
- **Tasks:** 2
- **Files modified:** 4 (2 created, 2 modified)

## Accomplishments

- `crates/devflow-cli/tests/common/mod.rs`: helpers only, no `#[test]`, opens with `#![allow(dead_code, reason = ...)]`. `HermeticHome::new()` uses `tempfile::Builder::new().prefix("dfh").tempdir_in("/tmp")`. `hermetic_devflow` sets `HOME`, `XDG_CACHE_HOME` (`home/.cache`), `XDG_CONFIG_HOME` (`home/.config`) and `DEVFLOW_CACHE_DIR` (`home/.cache/devflow`), and `env_remove`s every name in `REPO_LOCAL_GIT_VARS` and `ALSO_REDIRECTING_GIT_VARS`. `FakeBin`, `fake_bin_dir`, `wait_for`, `kill_and_reap` and `LaunchedRunReaper` moved from `start_lock_e2e.rs` (same behaviour); `fake_bin_dir_for(program, body)` and `entries_under(dir)` added.
- `test_home_isolation.rs` V-26: the hermetic `devflow start` (fake `claude` running `exec sleep 60`, `--legacy-claude-launch`, `--no-worktree`, supervise) registers under `H/.cache/devflow/roots/`. The same start built as a plain `Command::new(common::devflow_bin())` with `HOME=R2` (`/tmp/dfr*`) and `DEVFLOW_CACHE_DIR`/`XDG_CACHE_HOME` removed registers under `R2/.cache/devflow/roots/`. Each arm waits for the agent pid file and reaps monitor and agent before returning.
- The permanent scan: `code_only` (drops `//` comments outside string, raw-string and char literals), `raw_devflow_spawns`, `env_mutations` (whole identifiers). Planted controls first, then every `tests/*.rs` whose code has a `mod common;` line.
- `start_lock_e2e.rs` and `stop_e2e.rs` declare `mod common;` and have no raw devflow spawn left. Counts before and after migration: **12** and **22** (`cargo nextest list -p devflow --test T 2>/dev/null | wc -l`, measured 2026-10-17 before any edit and again in the Task 2 gate).

## Task Commits

1. **Task 1 (tracer, tdd): hermetic builder + 999.141 test + source scan.** RED `3cb2963` test(50-02), GREEN `dc2fdc5` feat(50-02). No REFACTOR commit.
2. **Task 2: migrate start_lock_e2e.rs and stop_e2e.rs.** `8d24b3e` test(50-02)

## TDD Gate Compliance (Task 1)

| Gate | Commit | Run | Evidence check |
|------|--------|-----|----------------|
| RED | `3cb2963` test(50-02) | `cargo nextest run --config-file <scratch junit profile> --profile red -p devflow --test test_home_isolation --no-tests=fail`, exit 100, 2 tests run, 2 failed | `gsd-tools check tdd-red-evidence` on the nextest JUnit report: `RED_EVIDENCE_OK` / `target_test_failed` for each target test separately |
| GREEN | `dc2fdc5` feat(50-02) | both named-test gates exit 0, `1 test run: 1 passed, 1 skipped` each | n/a |

RED failures, read from the report: `a_child_writing_outside_its_tempdir_home_fails` panicked `the hermetic command must set HOME; it inherits it` (the builder was a stub returning a bare `Command`); `suites_using_common_never_spawn_devflow_raw_or_mutate_the_environment` failed `a planted raw spawn must be flagged, left: 0, right: 1` (the counters were stubs returning 0). Semantic assessment: both targets compiled, ran and failed on their planned assertions, not on a build, fixture or load error. The RED stub builder spawned nothing: the environment check runs before the spawn, so no RED run wrote to the real cache.

Mutation checks on the GREEN code (not in the plan, run for discrimination), each against its named test, each restored afterwards:
- `.env("HOME", …)` renamed to `HOME_X` in `hermetic_devflow`: V-26 fails `the hermetic command must set HOME; it inherits it`.
- `unsafe { std::env::set_var("X", "1") }` appended to `common/mod.rs`: scan fails `common/mod.rs has 1 process-environment mutation(s)`.
- R-13 control rewritten to use `hermetic_devflow`: scan fails `test_home_isolation.rs has 0 raw devflow spawn(s); it must have 1`.

## Verification (actual runs)

- Task 1 `<automated>`, both named tests: each `1 test run: 1 passed, 1 skipped`, exit 0 (re-run as the tracer feedback gate before Task 2: passed).
- Task 2 `<automated>`, run verbatim from a script file: exit 0. Scan test passed; list counts 12 and 22; sentinel-HOME run `36 tests run: 36 passed, 0 skipped`; `leaked=[]`.
- Negative control for the sentinel-HOME leak check: the pre-migration `start_lock_e2e.rs` (`git show HEAD:` into the working tree, then restored byte-identical) under a fresh sentinel HOME passed its 12 tests and left `leaked=[…/roots/4ce12138a2dbf615-97.json …/roots/b7e0d09ddc4fceba-91.json …/roots/1f2a5b051714ada5-98.json]`, exit 1. So the gate catches a leak, and those three tests wrote into the operator's real `~/.cache/devflow/roots/` on every run until this plan.
- Plan `<verification>`: `cargo clippy --workspace --all-targets -- -D warnings` exit 0; `cargo fmt --all --check` exit 0. A dead-code probe appended to `stop_e2e.rs` made the same clippy command exit 101, which shows the integration-test targets really are linted. Also run: `doc_check` tests in `devflow-core` (6 passed).
- Real-cache check, read-only: `find ~/.cache/devflow -newer <marker>` printed nothing after the GREEN run.

What these checks do not establish: the sentinel run covers only the three suites this plan touched. The other suites still spawn `devflow` raw and keep writing to the real cache until 50-03, 50-21 and 50-04 migrate them. Passing runs here are single runs, not a flake bound. The scan does not see `/* */` block comments as comments, and it checks only files directly under `tests/`.

## Files Created/Modified

- `crates/devflow-cli/tests/common/mod.rs`: the shared hermetic helpers.
- `crates/devflow-cli/tests/test_home_isolation.rs`: V-26 plus the permanent scan, with its own copy of the `start_lock_e2e` repo fixture (`git`, `init_repo`).
- `crates/devflow-cli/tests/start_lock_e2e.rs`: `child`, `run_start`, `resume_command` and `assert_*_refuses_live_run` take a `&common::HermeticHome`. `resume_command` loses its `DEVFLOW_CACHE_DIR` parameter, because the builder sets that variable to the home's cache dir. Six local helpers deleted.
- `crates/devflow-cli/tests/stop_e2e.rs`: `let home = common::HermeticHome::new();` opens each of its 22 tests; all 27 spawns go through the builder. The local `devflow_bin` and `kill_and_reap` are deleted. `init_repo`, `wait_for` (takes seconds, not a `Duration`) and `write_live_lock` stay local.

## Decisions Made

- The hermetic arm checks `get_envs()` before it spawns. A broken builder therefore fails without a child ever writing to the inherited HOME. This also made the RED run safe.
- Each V-26 arm waits for the agent pid file before its reaper drops, so the reaper kills the agent as well as the monitor. Without the wait, a race could leave the fake `sleep 60` agent running.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Reapers on the four start_lock_e2e tests that launch a run**
- **Found during:** Task 2
- **Issue:** `start_proceeds_when_no_process_holds_the_phase_lock`, `start_clears_leftover_gate_files_after_taking_the_lock`, `start_replaces_a_leftover_state_whose_recorded_processes_are_dead` and `start_proceeds_when_no_state_exists_even_if_the_agent_pid_file_names_a_live_process` launch a run but had no `LaunchedRunReaper`. A monitor that outlived its test could write into the deleted hermetic home after the test ended (threat T-50-04, disposition mitigate).
- **Fix:** `let _reaper = common::LaunchedRunReaper::new(root, phase, …)` added after the fake bin in each; the last one spares the test-owned `LiveProcess` pid. No assertion changed.
- **Files modified:** `crates/devflow-cli/tests/start_lock_e2e.rs`
- **Verification:** the suite passes (12/12, including under the sentinel HOME); no `devflow` or `sleep 60` process of this run was left afterwards (`pgrep`).
- **Committed in:** `8d24b3e`

**2. [Rule 2 - Missing critical] The R-13 control spawn also strips GIT_* variables**
- **Found during:** Task 1
- **Issue:** the plan's plain-Command control sets only `HOME` and removes two cache variables. If the suite runs under a hook that exports `GIT_DIR`, the control's `git` calls would retarget the real repository (999.37).
- **Fix:** the control `env_remove`s `REPO_LOCAL_GIT_VARS` and `ALSO_REDIRECTING_GIT_VARS` as well. It still writes through its inherited-style `HOME`, which is the property under test.
- **Files modified:** `crates/devflow-cli/tests/test_home_isolation.rs`
- **Committed in:** `3cb2963` / `dc2fdc5`

**3. [Judgment] stop_e2e's `kill_and_reap` replaced by the common one**
- The two copies differed only in the panic message of `try_wait().expect(...)`: "poll holder" in `stop_e2e`, "poll start child" in `start_lock_e2e`. The common copy says "poll child". Kill-then-wait behaviour is identical, so it was merged rather than kept as a variant.

**4. [Minor] Task 2 commit subject is 73 characters**
- The commit-msg hook warned (warning only, 72-character guideline). It was not amended, per the no-amend rule.

---

**Total deviations:** 2 auto-fixed (Rule 2), 1 judgment merge, 1 cosmetic.
**Impact on plan:** No assertion or test count changed. The additions close the reaping and git-retargeting gaps the threat model names.

## Issues Encountered

None.

## User Setup Required

None. No external service configuration required.

## Next Phase Readiness

- 50-03, 50-21 and 50-04 can migrate the remaining suites by adding `mod common;` and replacing spawns. The scan covers each one from that commit on, without anyone editing `test_home_isolation.rs`.
- 50-11 can rewire `common::LaunchedRunReaper` to the socket. Its pid-file semantics are unchanged for now.
- The suites not yet migrated still write registry entries into the operator's real `~/.cache/devflow` on every run (shown above for the pre-migration `start_lock_e2e`).

---
*Phase: 50-addressable-monitor-liveness*
*Completed: 2026-10-17*

## Self-Check: PASSED
