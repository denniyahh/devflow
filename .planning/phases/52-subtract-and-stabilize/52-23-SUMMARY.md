---
phase: 52-subtract-and-stabilize
plan: 23
subsystem: cli
tags: [rust, clap, tracing, tracing-subscriber, doctor, doc_check, observability]

requires:
  - phase: 52-21
    provides: provenance and base-currency warnings routed through writers
  - phase: 52-22
    provides: gate kinds and run_gate_with_event_context
provides:
  - "`devflow --version` prints `devflow <version> (<commit|unknown>, <clean|dirty>)` (version_label in main.rs)"
  - "no tracing lines unless RUST_LOG is set to a non-empty value (EnvFilter::new(\"off\") in both log-format branches)"
  - "doctor reports an unset or empty RUST_LOG as an info row"
  - "doc_check::pinned_doc_claims_match_source pins the off default per RUST_LOG table row, with two planted 3.0 controls"
  - "13 foreground warn! sites that were the operator's only signal also print a plain `warning:` line, each with a named test"
affects: [52-24, 52-26, 52-27, operations-docs]

actuals:
  tokens: 10800    # chars/4 over the added and removed diff lines (43188 chars)
  tasks: 3
  commits: 7       # git rev-list --count plan_head_before..HEAD at SUMMARY write
plan_head_before: 76ca3256f524dfa06e7579be56faa0b3b4582985
plan_head_after: 0807945b150af3394a413c812104cd21521d3f94

tech-stack:
  added: []
  patterns:
    - "a `_to(…, &mut dyn Write)` / `_with` seam per warning site: the public function passes `&mut std::io::stderr()`, the test passes a Vec<u8> with no tracing subscriber installed"

key-files:
  created: []
  modified:
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/tests/build_provenance.rs
    - crates/devflow-cli/tests/log_format_env.rs
    - crates/devflow-core/src/doc_check.rs
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/events.rs
    - crates/devflow-core/src/gates.rs
    - crates/devflow-core/src/git.rs
    - crates/devflow-core/src/workflow.rs
    - OPERATIONS.md
    - ARCHITECTURE.md
    - CONTRIBUTING.md
    - docs/guides/configuration.md

key-decisions:
  - "52-23: an empty RUST_LOG is treated exactly as an unset one, in main.rs's filter and in doctor's row (both info, tracing off)"
  - "52-23: a non-empty RUST_LOG that does not parse keeps the 3.0 `info` fallback; doctor still warns on it"
  - "52-23: the never-silent audit adds two labels beside the plan's (a)/(b)/(c): (n) a line that reports no skip, refusal or failure, or a guard against a value the code cannot produce; (h) advisory housekeeping whose leftover nothing depends on and a later call sweeps. Both are left, as (a) and (b) are"
  - "52-23: gate list now also warns when a gate file cannot be read, not only when it does not parse; the unreadable case used to be skipped with no log at all"

patterns-established:
  - "Only-signal warnings: tracing::warn! for RUST_LOG readers plus a plain `warning: …` line through a writer seam, tested with the sink"

requirements-completed: [OPS-01, OPS-03]

coverage:
  - id: D1
    description: "`devflow --version` names the build commit (or `unknown`) and whether the tree was dirty"
    requirement: OPS-01
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/build_provenance.rs#version_prints_the_embedded_commit_and_dirty_flag"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/main.rs#tests::version_label_renders_an_empty_commit_as_unknown"
        status: pass
    human_judgment: false
  - id: D2
    description: "No tracing lines on stderr when RUST_LOG is unset or empty; RUST_LOG=info and RUST_LOG=debug still show them"
    requirement: OPS-03
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/log_format_env.rs#rust_log_unset_prints_no_tracing_lines"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/log_format_env.rs#rust_log_unset_prints_no_json_tracing_objects"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/log_format_env.rs#an_empty_rust_log_prints_no_tracing_lines"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/log_format_env.rs#rust_log_info_still_prints_the_tracing_line"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/log_format_env.rs#rust_log_debug_is_honored_under_json_log_format"
        status: pass
    human_judgment: false
  - id: D3
    description: "doctor reports an unset or empty RUST_LOG as information (no red cross, says how to turn tracing on); an invalid value still warns"
    requirement: OPS-03
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::doctor_reports_an_unset_rust_log_as_information"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::doctor_reports_an_empty_rust_log_as_information"
        status: pass
      - kind: unit
        ref: "crates/devflow-cli/src/commands.rs#commands::tests::doctor_still_warns_on_an_invalid_rust_log"
        status: pass
    human_judgment: false
  - id: D4
    description: "The doc guard pins `off` in main.rs and in every scoped-doc RUST_LOG row, and reports both planted 3.0 controls"
    requirement: OPS-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::pinned_doc_claims_match_source"
        status: pass
    human_judgment: false
  - id: D5
    description: "Each of the 13 class-(c) warning sites prints a plain `warning:` line without tracing (14 named tests)"
    requirement: OPS-03
    verification:
      - kind: unit
        ref: "cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=<each of the 14 tests in the class-(c) table>)'"
        status: pass
    human_judgment: false
  - id: D6
    description: "The class (a), (b), (n) and (h) dispositions of the other 65 warn! sites"
    verification: []
    human_judgment: true
    rationale: "A reading of each site's process and callers, not test-backed (review concern P5): a future warn! that becomes an only signal is not caught by any test"

duration: 32min
completed: 2026-11-11
status: complete
---

# Phase 52 Plan 23: Version label, quiet default, never-silent audit Summary

**`devflow --version` prints `devflow 3.0.0 (<commit>, <clean|dirty>)` from the values build.rs embeds. With RUST_LOG unset or empty, no tracing line reaches stderr, and doctor shows that as information. The 13 foreground warnings that were the operator's only sign of a skipped, refused or failed step now also print a plain `warning:` line, each with its own test.**

## Performance

- **Duration:** about 32 min
- **Started:** 2026-11-11T07:08:40Z
- **Completed:** 2026-11-11T07:41Z
- **Tasks:** 3 (Task 1 tracer, Tasks 2 and 3 TDD)
- **Files modified:** 14

## Accomplishments

- **OPS-01.** `version_label(version, commit, dirty)` renders `<version> (<commit|unknown>, <clean|dirty>)`. `build_version()` builds it once in a `OnceLock` from `env!("CARGO_PKG_VERSION")`, `env!("DEVFLOW_BUILD_COMMIT")` and `env!("DEVFLOW_BUILD_DIRTY")`, and clap's `#[command(version = build_version())]` prints it after the name. `build.rs` is unchanged: it already reruns on every build (CR-02). The installed-style binary from this tree printed `devflow 3.0.0 (613ba3a7b6753fe61885f9e5a4555d7ac8398ba3, dirty)`.
- **OPS-03, tracing.** In both log-format branches of `main`, an unset, empty or non-Unicode `RUST_LOG` builds `EnvFilter::new("off")`. A non-empty value goes through `try_from_default_env()` as before, and one that does not parse keeps the `info` fallback.
- **OPS-03, doctor.** `rust_log_check` (split out of `doctor_checks`) gives an unset or empty value the status `info`, detail `not set or empty — tracing off`, hint `Set RUST_LOG=info to see tracing lines`. `check_line` (split out of the render loop) prints `ℹ` and the hint for `info`. Row order is unchanged: the RUST_LOG row is still last.
- **Doc guard.** `rust_log_default_findings(main_rs, docs)` requires `EnvFilter::new("off")` at least twice in `main.rs`, at least one scoped-doc table row whose first cell is `` `RUST_LOG` ``, and every such row to name `` `off` `` with no whole cell of `` `info` ``. On the real tree it finds nothing. The planted 3.0 `main.rs` fallback gives exactly one finding, and the planted 3.0 OPERATIONS.md row gives one finding that quotes it. The four RUST_LOG rows (OPERATIONS.md, ARCHITECTURE.md, CONTRIBUTING.md, `docs/guides/configuration.md`) and ARCHITECTURE.md's doctor paragraph now say `off`.
- **Never-silent audit.** All 78 `warn!`/`error!` sites are classified below (there are no `error!` sites). 13 are class (c) and now also write a `warning:` line through a writer seam. The supervisor's tracing still goes to `/dev/null`, as before. Routing it to a log file is outside OPS-03's text and was not done.

## Task Commits

1. **Task 1 (tracer):** `613ba3a` test (RED, with a compile skeleton for `version_label`), `d904a59` feat (GREEN), `1bd2ef5` fix (spawn the binary through `common::hermetic_devflow`, see Deviations)
2. **Task 2:** `8422920` test (RED, with `rust_log_check`/`check_line` split out unchanged), `f19ad6b` feat (GREEN)
3. **Task 3:** `e5402f3` test (RED, seams that write nothing), `0807945` feat (GREEN)

The plan's suggested subjects (`feat(cli): --version shows the build commit`, `fix(cli): no tracing lines by default`) were replaced by the `52-23` scope, because the TDD gate greps `test(52-23)`/`feat(52-23)`, as in 52-15..52-22.

## TDD Gate Compliance

RED evidence came from nextest JUnit reports (a scratch `--profile red` tool config). Each record went through `gsd_run check tdd-red-evidence`.

| Test | Verdict | Semantic assessment |
|---|---|---|
| `build_provenance::version_prints_the_embedded_commit_and_dirty_flag` | `RED_EVIDENCE_OK` | stdout was `devflow 3.0.0\n`, expected the commit and `dirty`: the planned failure |
| `tests::version_label_renders_an_empty_commit_as_unknown` | `RED_EVIDENCE_OK` | the skeleton returned `4.0.0`, expected `4.0.0 (unknown, clean)` |
| `log_format_env::rust_log_unset_prints_no_tracing_lines` | `RED_EVIDENCE_OK` | stderr held `INFO devflow_core::gates: gate answer written …` with RUST_LOG unset |
| `log_format_env::rust_log_unset_prints_no_json_tracing_objects` | `RED_EVIDENCE_OK` | stderr held the JSON `"level":"INFO"` record |
| `log_format_env::an_empty_rust_log_prints_no_tracing_lines` | **`INVALID_RED / unexpected_green`** | see below |
| `commands::tests::doctor_reports_an_unset_rust_log_as_information` | `RED_EVIDENCE_OK` | status was `missing` |
| `commands::tests::doctor_reports_an_empty_rust_log_as_information` | `RED_EVIDENCE_OK` | status was `warn` |
| `doc_check::pinned_doc_claims_match_source` | `RED_EVIDENCE_OK` | the real tree gave five findings: `main.rs` lacks `off`, and four rows lack it |
| each of the 14 class-(c) tests in the table below | `RED_EVIDENCE_OK` (14/14) | each failed on its own assertion with an empty sink (`""`), after its fixture ran |

**The unexpected green.** `an_empty_rust_log_prints_no_tracing_lines` passed before the implementation. I looked into why: tracing-subscriber 0.3.23's `Builder::parse("")` returns a filter with no directives and no default directive (`builder.rs` L168-172, L334-338), which enables nothing. So an empty `RUST_LOG` already printed no tracing lines in 3.0, and doctor's old `empty (logging disabled)` text said so. The test is kept as a pin on the new explicit `off` branch, not as RED evidence. Its control, `rust_log_info_still_prints_the_tracing_line`, passes, so the detector does see a tracing line when one is printed.

Gate sequence: `test` 613ba3a → `feat` d904a59; `test` 8422920 → `feat` f19ad6b; `test` e5402f3 → `feat` 0807945. There is no REFACTOR commit. The RED commits carry behaviour-preserving seams (skeleton `version_label`; `rust_log_check`/`check_line`; the `_to`/`_with` seams that write nothing yet) so the tests compile and fail on their assertions, not on a missing symbol.

**Tracer gate.** The run was interactive (`auto_advance` false, `_auto_chain_active` false), the mode `end-of-phase`, and Task 1's verify automated only. Its two-test chain was re-run after GREEN: exit 0. Expansion continued.

## The never-silent audit

Source: `rg -n '\b(warn|error)!\(' crates/devflow-cli/src crates/devflow-core/src` at `0807945`: 78 sites, all `warn!` (the plan's "about 94" was a planning-time count). Line numbers below are at `0807945`.

**Classes.** (a) runs only in the detached `__monitor`/supervisor, whose stderr is `/dev/null` (`monitor::spawn_monitor` L654-658): left. (b) the same condition already reaches the operator through a gate, an event, a `println!`/`eprintln!`, a returned error, or a later `status`/`doctor` line: left. (c) the only operator-visible sign of a skipped, refused or failed operation in a foreground command: now also prints a `warning:` line, with a test. Two labels of my own, also left: (n) the line reports no skip, refusal or failure (it announces an operation the operator asked for), or guards against a value the code cannot produce; (h) advisory housekeeping, where a leftover file is something nothing depends on and a later call sweeps (the code's own doc says so).

**Class (c): test-backed.** Each test was run alone with `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=<path>)'`, and each exited 0 with `1 passed`.

| Site | What was silent | Test |
|---|---|---|
| `config.rs:170` `load_config_to` | `devflow.toml` exists but cannot be read: every setting in it ignored | `config::tests::an_unreadable_devflow_toml_is_reported_without_tracing` |
| `config.rs:183` `load_config_to` | `devflow.toml` does not parse: settings ignored (also reached at launch when `DEVFLOW_BASE_BRANCH` skips the file) | `config::tests::an_unparsable_devflow_toml_is_reported_without_tracing` |
| `config.rs:232` `capture_retention_with` | invalid `DEVFLOW_CAPTURE_RETENTION` ignored at the stage launch `start`/`resume` run in the foreground | `config::tests::an_invalid_env_value_is_reported_without_tracing` |
| `config.rs:385` `git_flow_from` | invalid base-branch config: `status` and `cleanup` fall back to `develop` | `config::tests::an_invalid_base_branch_fallback_is_reported_without_tracing` |
| `config.rs:489` `fallback_agent_to` | unreadable `devflow.toml` at `start`: no `fallback_agent` read | `config::tests::an_unreadable_devflow_toml_fallback_agent_is_reported_without_tracing` |
| `events.rs:74` `emit_to` | events dir cannot be created: event not recorded | `events::tests::an_event_dir_that_cannot_be_created_is_reported_without_tracing` |
| `events.rs:88` `emit_to` | events log cannot be appended: event not recorded | `events::tests::an_event_log_that_cannot_be_appended_is_reported_without_tracing` |
| `workflow.rs:718` `list_states_to` | unreadable state file: its phase missing from `status`, `cleanup`, `gate list` | `workflow::tests::a_skipped_state_file_is_reported_without_tracing` |
| `git.rs:396` `cleanup_merged_to` | `devflow cleanup`: a merged branch git refused to delete is just absent from the list | `git::tests::a_merged_branch_git_refuses_to_delete_is_reported_without_tracing` |
| `gates.rs:713` `live_waiter_holders_to` | an unreadable waiter registration: its live holder left out of doctor's registry-reachable set, so doctor can report it as a stray | `gates::tests::an_unreadable_waiter_registration_is_reported_without_tracing` |
| `gates.rs:779` `await_answer_to` | a foreground waiter (a preflight gate in `start`) over an answer it cannot read, rate-limited like the warn | `gates::tests::an_unreadable_gate_answer_is_reported_without_tracing` |
| `gates.rs:1386` `list_open_to` | a gate file that does not parse, or (new) cannot be read: the gate hidden from `gate list`/`status` | `gates::tests::an_unparsable_gate_file_is_reported_without_tracing`, `gates::tests::an_unreadable_gate_file_is_reported_without_tracing` |
| `gates.rs:1747` `run_notify_command_to` | the gate notify hook exits non-zero when the gate is written in the foreground | `gates::tests::a_failing_notify_hook_is_reported_without_tracing` |

Controls, same command: `config::tests::a_valid_config_writes_no_warning`, `events::tests::a_recorded_event_writes_no_warning`, `gates::tests::a_succeeding_notify_hook_writes_no_warning` (each writes nothing to the sink).

**Classes (a), (b), (n), (h): a reading, not test-backed.**

| Site | Class | Reason |
|---|---|---|
| `pipeline_launch.rs:804`, `:819` `AutoChainGuard` | a | engaged only in `run_monitor` (L1436) |
| `pipeline_launch.rs:1180`, `:1187` `shut_down_unpublished` | a | safe point 3 of an advance, in the supervisor; the record is kept and `status` names it |
| `pipeline_launch.rs:1441` `run_monitor` | a | monitor body |
| `pipeline_launch.rs:1573` `record_safe_point_stop_exit` | a | also emits `stop_exit_file_failed` |
| `pipeline_launch.rs:1608` `end_superseded` | a | also emits an event |
| `pipeline_launch.rs:1632` `remove_liveness_file` | a | supervisor's last act; `recover --clean` sweeps the file |
| `pipeline_launch.rs:1761` `kept_env` | a | `stage_spec` from a `MonitorInvocation` |
| `pipeline_launch.rs:1793` `record_supervisor_stop` | a | `run_monitor` |
| `pipeline_launch.rs:2396` `successor_runs_on` | b | the record stays and `status` reports the probe error (its doc) |
| `pipeline_outcomes.rs:1324` `relaunch_failed_stage` | b | `gate_relaunch_refused` event and `stop_reason`, shown by `status` |
| `canary.rs:404` `run_stream_canary` | b | the outcome becomes Absent/Unverified and `refuse_launch` returns the error |
| `canary.rs:422` `run_stream_canary` | h | forensic capture append only; the outcome is read from the line channel |
| `canary.rs:495` `reap` | n | `try_wait` fails only without a child to wait on |
| `config.rs:254` `external_verify_enabled` | a | read only by `agent_result::evaluate_layer0` in the supervisor. **Gap logged** in `deferred-items.md`: the operator never learns an invalid value was ignored |
| `config.rs:427` `git_flow_for_run` | a | `hand_off_agent`, `advance_with`, `monitor::enumerate_phase_commits` |
| `gates.rs:443` `WaiterRegistration::finish` | h | the unheld file is removed by the stage's next close (its doc) |
| `gates.rs:1049` `remove_claimed_pair` | b | `gate_claimed_answer_removed` event |
| `gates.rs:1317` `read_answer` | b | its foreground caller `stop_with_existing_response` prints "has an answer that will be set aside (…)" |
| `gates.rs:1754` `run_notify_command_to` | n | `sh` could not be spawned |
| `gates.rs:1800` `set_aside` | b | event, and doctor's set-aside findings |
| `git.rs:177` `feature_start_force` | n | announces the `--force` the operator passed |
| `git.rs:277` `delete_branch` | n | announces a force delete the caller asked for |
| `liveness.rs:166`, `:497`, `:540`, `:548`, `:556` | h | hold-temp cleanup: "advisory … nothing depends on the temp being gone" |
| `monitor.rs:487` `Reservation::discard` | h | the file left reads abandoned; `recover --clean` sweeps it |
| `monitor.rs:610` `spawn_monitor` | b | `println!` of the same notice on the next line |
| `monitor.rs:1210`, `:1246`, `:1311`, `:1346`, `:1468` | a | stream I/O threads of the supervised stage |
| `monitor.rs:1894`, `:1936`, `:1993` | a | promotion watch, `run_supervised_stage` |
| `monitor.rs:2065` `AgentGroupGuard::drop` | a | `run_supervised_stage` |
| `monitor.rs:2335` `proc_table` | n | reached from the launcher's `end_unpublished_child`; `/proc` is listable on every supported host |
| `monitor.rs:2353`, `:2357`, `:2362` `signal_guarded_group` | n / b | the launcher's own child has `process_group(0)`, so these guards cannot trip for it; for a sidecar-recorded pgid, `end_recorded_agent_group` turns the refusal into its text, which reaches the `NotReady` error |
| `monitor.rs:2513` `wait_for_publish` | h | removing a failed launch's files; the launch's own failure is the returned error |
| `monitor.rs:2719`, `:2753`, `:2785`, `:2816`, `:2846`, `:3015`, `:3019` | a | idle timeout in the supervisor; recorded in the verdict file and the monitor log |
| `supervisor.rs:223` `publish_with`, `:573` `write_sidecar` | a | supervisor publish (`monitor.rs:1744`, `runtime.rs:587`) |
| `supervisor/runtime.rs:320`, `:324`, `:329`, `:351`, `:695`, `:714`, `:772`, `:862`, `:925` | a | supervisor process body |
| `workflow.rs:406` `record_write_lock_holder` | b | `status`/`doctor` then say "held by an unknown holder" |
| `workflow.rs:910` `remove_state_files` | h | orphan state temp; callers that set `report_temp_removal_failure` get the error instead |

Tally: (a) 39, (b) 8, (c) 13, (n) 8, (h) 10, with the three `signal_guarded_group` rows counted as (n). Total 78.

## Verification

- Task 1 `<automated>` chain: exit 0 (also re-run as the tracer gate).
- Task 2 `<automated>` chain (six named tests): exit 0.
- Task 3 `<automated>`: `rust_log_unset_prints_no_tracing_lines` exit 0, then `scripts/check.sh all` exit 0 ("check.sh: all OK"; devflow bin 617 passed, 0 failed, 4 ignored; devflow-core lib 1065 passed, 0 failed, 2 ignored). The known flake 999.221 did not appear.
- `rust_log_unset_still_shows_info_level_logs_by_default` no longer exists: `rg` over `crates/` exit 1.
- `cargo clippy --workspace --all-targets -- -D warnings` exit 0 and `cargo fmt --all --check` exit 0 before each GREEN commit.
- No `pub` item was added or changed in devflow-core (every seam is private; `git diff` shows no added `pub … fn`), so `cargo semver-checks` was not needed.

**What this does not establish.**
- The class-(c) tests drive each seam with a sink in-process. They prove the line is written without any tracing subscriber; they do not run each failure through the installed binary. Only the tracing default itself (D2) is tested through the binary.
- The (a)/(b)/(n)/(h) rows are my reading of each site's callers. A future `warn!` that becomes an only signal is not caught by any test.
- `--version` was checked on this dirty dev tree and, for `unknown`, through the unit test only. No no-git (crates.io-style) build was run.
- An unreadable `devflow.toml` during `devflow start` can now print the same warning more than once, because `fallback_agent`, `base_branch` and `capture_retention` each read the file. I did not deduplicate it.

## Decisions Made

See `key-decisions` in the frontmatter. One choice inside the audit needs a note: `list_open` used to skip an unreadable gate file with no log at all, a sibling of the class-(c) parse case. It now goes through the same warning (Rule 2), with its own test.

## Deviations from Plan

**1. [Rule 1 - Bug] Task 1's test named the devflow binary outside `common/mod.rs`.** Found during the first full workspace run. `hermetic_spawn_sites::only_common_names_the_devflow_binary` failed on `build_provenance.rs`. Fix: spawn through `common::hermetic_devflow` with a `HermeticHome`. Commit `1bd2ef5`. Its subject is 74 characters; the commit-msg hook warns past 72 but does not refuse, and I did not rewrite history.

**2. [Rule 2 - Missing control] Extra tests beyond the behaviour list.** Added `rust_log_unset_prints_no_json_tracing_objects`, `rust_log_info_still_prints_the_tracing_line` (detector control), `doctor_reports_an_empty_rust_log_as_information`, `doctor_still_warns_on_an_invalid_rust_log`, and the three Task 3 controls listed above.

**3. [Rule 2] `list_open` warns on an unreadable gate file too** (see Decisions).

**4. Files outside `files_modified`.** Class-(c) sites live in `crates/devflow-core/src/events.rs`, `workflow.rs` and `git.rs`, which the plan allows ("only if a class-(c) site lives there, and list them"). `pipeline_launch.rs` and `commands.rs` had no class-(c) site, so Task 3 did not touch them. `build.rs` was not changed.

**5. Commit subjects** use the `52-23` scope instead of the plan's suggested `feat(cli)`/`fix(cli)` ones (see Task Commits).

**Total deviations:** 5 (1 bug, 3 missing-control/critical additions, 1 naming). **Impact:** no scope creep beyond one sibling warning in a function Task 3 already changed.

## Issues Encountered

- The empty-`RUST_LOG` test was green before GREEN (see TDD Gate Compliance).
- The post-commit hook warned that `CONTRIBUTING.md` changed without `DEV-SETUP-CHECKLIST.md`. The change is the RUST_LOG row's wording and does not change what a fresh setup needs, so the checklist was left as is.

## User Setup Required

None.

## Next Phase Readiness

OPS-01 and OPS-03 are delivered. One gap is logged for later: an invalid `DEVFLOW_EXTERNAL_VERIFY_ENABLED` is read only in the supervisor, so its warning reaches no one (`deferred-items.md`, found in 52-23).

---
*Phase: 52-subtract-and-stabilize*
*Completed: 2026-11-11*

## Self-Check: PASSED

Key files and all eight commits (613ba3a, d904a59, 1bd2ef5, 8422920, f19ad6b, e5402f3, 0807945, a272b64) checked present on this branch; `check evaluation-scope --plan 52-23 --commits-only` resolved them (exit 0).
