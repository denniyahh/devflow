---
phase: 25-end-to-end-dogfood-blockers
unit: 25e
backlog: 999.47 / DEN-72
measured: 2026-07-28T13:38:45Z
head_sha: 5244fac24b329204a6d95355958d6d6ef48ce1aa
vulnerable_positive: 4
vacuous_negative: 2
not_vulnerable: 25
---

# 25e / 999.47 — site census

Fresh measurement at execution HEAD (`5244fac`), not copied from
`25-CI-OBSERVATION.md`'s five-row list, which is stale (recorded at `4f65cdb`, before
this plan's own re-measurement).

## Method

Two greps over `crates/`, run from the repo root at HEAD:

```bash
rg -n '\.spawn\(\)' crates/ --type rust
rg -n 'Command::new' crates/ --type rust
```

`.spawn()` hit count: **17**. `Command::new` hit count: **75** (includes the 17 `.spawn()`
sites — every `.spawn()` call is preceded by a `Command::new`/`CommandExt` builder chain —
plus every `.output()`/`.status()` call site, which does not go through `.spawn()`
directly).

For every `.spawn()` site, read the enclosing test/function and answer one question: does
it, after that spawn, read a `/proc`-**cmdline** census about the spawned child? The two
census entry points, found by:

```bash
rg -n 'discover_stray_devflow_processes\(\)|collect_stray_process_findings\(\)' crates/ --type rust
```

are `agent::discover_stray_devflow_processes()` and
`commands::collect_stray_process_findings()`. Reading `agent::process_start_time` (which
parses `/proc/<pid>/stat` field 22, not `cmdline`) is explicitly NOT a cmdline census —
the field is valid from `fork()` onward and is not argv-derived, so it cannot participate
in this race.

## Vulnerable sites

| id | file | spawn line | test | census read | class |
|---|---|---|---|---|---|
| V1 | `crates/devflow-core/src/agent.rs` | 634 (`Command::new`), 637 (`.spawn()`) | `discover_stray_devflow_processes_finds_a_monitor_wrapper` | `discover_stray_devflow_processes()` at :641, expects FIND | VULNERABLE-POSITIVE |
| V2 | `crates/devflow-cli/src/commands.rs` | 3670 (`Command::new`), 3673 (`.spawn()`) | `gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling` | `discover_stray_devflow_processes()` at :3679, expects FIND | VULNERABLE-POSITIVE — **the observed 2/2 failure, assertion at :3678-3682** |
| V3 | `crates/devflow-cli/src/commands.rs` | 4794 (`Command::new`), 4797 (`.spawn()`) | `doctor_finds_a_real_stray_and_never_signals_it_across_two_runs` | `collect_stray_process_findings()` at :4803 and again at :4808, both expect FIND | VULNERABLE-POSITIVE — one spawn, two census reads, both in scope |
| V4 | `crates/devflow-cli/tests/reap_strays_e2e.rs` | 90 (`Command::new`, helper `spawn_monitor_wrapper_fixture`), 94 (`.spawn()`) | `reap_clears_a_process_whose_root_was_deleted_which_devflow_stop_cannot_see` | `agent::discover_stray_devflow_processes()` at :163, expects FIND | VULNERABLE-POSITIVE — the file's own `wait_for(\|\| agent_running(pid), ...)` at :114-118 does NOT close the window; `agent_running` is `kill(pid,0)` + zombie check, both true for a forked-but-unexec'd child |
| A1 | `crates/devflow-core/src/agent.rs` | 660 (`Command::new`), 664 (`.spawn()`) | `discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape` | `discover_stray_devflow_processes()` at :668, expects NOT-FIND | VACUOUS-NEGATIVE |
| A2 | `crates/devflow-core/src/agent.rs` | 683 (`Command::new("sleep")`, `arg0("devflow")` at :684), 687 (`.spawn()`) | `discover_stray_devflow_processes_rejects_devflow_named_argv0_with_wrong_argv1` | `discover_stray_devflow_processes()` at :691, expects NOT-FIND | VACUOUS-NEGATIVE |

## Not vulnerable

Every `.spawn()` site in `crates/` not listed above, with its reason:

| file:line | test/function | reason |
|---|---|---|
| `crates/devflow-core/src/agent.rs:443` (`.spawn()` for `Command::new("true")`) | `agent_running_is_false_for_an_unreaped_zombie` | reads `/proc/<pid>/status` (`State:` line) via `is_zombie`, not a cmdline census |
| `crates/devflow-core/src/agent.rs:508` | `terminate_signals_a_live_child_and_it_exits` | no `/proc` read of any kind after spawn — only `child.wait()` |
| `crates/devflow-core/src/agent.rs:572` | `terminate_and_verify_clears_a_normal_child_before_the_wait_elapses` | polls `agent_running` (kill(0) + `/proc/status`), not a cmdline census |
| `crates/devflow-core/src/agent.rs:602` | `terminate_and_verify_escalates_to_kill_for_a_term_ignoring_child` | same as above — liveness only, no cmdline read |
| `crates/devflow-core/src/agent.rs:709` (no spawn in this test) | `discover_stray_devflow_processes_excludes_an_unrelated_process` | reads the census (`discover_stray_devflow_processes()`) but against `std::process::id()` (this test binary's own pid) — no child spawned, no fork/exec window to race |
| `crates/devflow-cli/src/commands.rs:3551` | `reap_stray_candidates_dry_run_never_signals` | reads `agent::process_start_time` (via `stray_candidate_for`) and `agent::agent_running` only — `/proc/<pid>/stat`, not `cmdline` |
| `crates/devflow-cli/src/commands.rs:3578` | `reap_stray_candidates_clears_a_real_child_with_verified_death` | same — `process_start_time` + `agent_running`, no cmdline census |
| `crates/devflow-cli/src/commands.rs:3605` | `reap_stray_candidates_escalates_to_kill_for_a_term_ignoring_child` | same — `process_start_time` + `agent_running` |
| `crates/devflow-cli/src/commands.rs:3634` | `reap_stray_candidates_refuses_on_identity_mismatch_without_signalling` | same — `process_start_time` + `agent_running` |
| `crates/devflow-cli/src/commands.rs:3706` | `gate_sweep_without_reap_strays_flag_ignores_a_live_stray` | fixture shape matches V2's, but `gate_sweep` is called with `reap_strays: false`; the `discover_stray_devflow_processes()` call at `commands.rs:1150` sits inside `if reap_strays`, so no census is read on this path — only `agent_running(pid)` is asserted |
| `crates/devflow-cli/src/commands.rs:3863` | `stop_signals_the_holder_when_the_recorded_identity_matches` | reads `process_start_time`, polling until `Some` — already polls for a different reason (999.47's own start-time granularity caveat), not a cmdline census |
| `crates/devflow-cli/src/commands.rs:1882` (`Command::new("sh")`) | `test_cmd` (production, not a test) | `.status()`, not `.spawn()` — `status()` blocks until the child exits, so there is no fork/exec window to observe |
| `crates/devflow-cli/tests/reap_strays_e2e.rs:214` | `reap_clears_a_sigterm_ignoring_stray_with_a_deleted_root` | the file's own comment (`:233-238`) records this fixture has no wrapper marker, so `discover_stray_devflow_processes` would not structurally match it; only `terminate_and_verify` is exercised (liveness/`stat`, not cmdline) |
| `crates/devflow-cli/tests/stop_e2e.rs:152` | `stop_ends_a_gated_phase_through_its_own_abort_path_with_no_signal_sent` | spawns the real `devflow` binary, then polls `lock::holder(...)` and `gate_path.exists()` — filesystem/lock state, not a `/proc` cmdline census |
| `crates/devflow-cli/tests/gate_sweep_e2e.rs:284` | `sweep_ends_a_real_advance_process_through_its_own_abort_path` | same shape as `stop_e2e.rs:152` — polls `lock::holder(...)`, no cmdline census |
| `crates/devflow-core/src/git.rs:784` (`Command::new("ssh-keygen")`) | `inline_key_fingerprint` (production) | reads the child's captured stdout via `wait_with_output()`, never `/proc/<pid>/cmdline` |
| `crates/devflow-core/src/monitor.rs:160` (`Command::new("sh")` at :148) | `spawn_monitor_inner` (production) | spawns the monitor wrapper but reads no census — this is the *source* of the production hazard analysed in `25-11-PLAN.md`'s `<scope_decision>`, which plan 25-12 handles; nothing changes here in this plan |
| `crates/devflow-core/src/hooks.rs:213` (`Command::new("sh")`, `.output()` not `.spawn()`) | `docs_update` (production) | `.output()` blocks until exit; no census, no fork/exec window |
| `crates/devflow-core/src/gates.rs:323` (`Command::new("sh")`, `.output()`) | `run_notify_command` (production) | same — `.output()`, no census |
| `crates/devflow-core/src/verify.rs:106` (`Command::new("sh")`, `.output()`) | `run_external_verification` (production) | same — `.output()`, no census |
| `crates/devflow-cli/tests/phase7_cli.rs` (11 `Command::new(devflow_bin())` sites) | various | all use `.output()` on the compiled binary and assert on stdout/exit code — no `/proc` census of any kind |
| `crates/devflow-cli/tests/release_check.rs`, `start_reachability_e2e.rs`, `log_format_env.rs`, `gitignore_coverage.rs`, `build_provenance.rs`, `git_env_hermeticity.rs`, `help_snapshot.rs` | various | `.output()`/`.status()` calls against `devflow_bin()`, `git`, `ssh-keygen`, `which` — none read a `/proc` cmdline census |
| `crates/devflow-core/src/version.rs`, `agent_result.rs`, `worktree.rs`, `preflight.rs`, `staleness.rs`, `commands.rs:90,2812,2818,1927` | various `git`/version-control helpers (production and tests) | all `git`/shell invocations whose output is parsed for version/ref/diff data — none read a `/proc` cmdline census |

A bare exclusion is not a classification — every row above carries the reason its spawn
site does not participate in the 999.47 race.

## Divergence from 25-CI-OBSERVATION.md

`25-CI-OBSERVATION.md`'s five-row table (measured at `4f65cdb`) differs from this
measurement in three ways, as this plan's `<measured_census_at_plan_time>` predicted:

1. **Line numbers shifted.** `commands.rs:3669` -> `3670`/`3673`/`3679` (spawn, `.spawn()`,
   census read, as three distinct lines rather than one); `agent.rs:631` -> `634`/`637`/`641`;
   `agent.rs:664` -> `660`/`664`/`668`; `agent.rs:687` -> `683`/`687`/`691`. Off by a small,
   consistent amount (a handful of lines), consistent with 25-08/25-09 landing additional
   code in both files after the CI-OBSERVATION measurement.
2. **One of its five entries is not vulnerable.** `commands.rs:3706`
   (`gate_sweep_without_reap_strays_flag_ignores_a_live_stray`, listed as "(same fixture
   shape)") spawns the identical wrapper-shaped fixture but never reads the census — the
   `discover_stray_devflow_processes()` call it would otherwise reach is gated behind
   `if reap_strays`, and this test calls `gate_sweep` with `reap_strays: false`. Reclassified
   NOT-VULNERABLE here.
3. **It misses two genuinely vulnerable sites** outside the two files it looked at:
   `commands.rs:4794/4797/4803/4808` (`doctor_finds_a_real_stray_and_never_signals_it_across_two_runs`,
   V3 above — reads the census twice) and `reap_strays_e2e.rs:90/94/163`
   (`reap_clears_a_process_whose_root_was_deleted_which_devflow_stop_cannot_see`, V4 above).
   Neither `doctor`'s stray-finding test nor the `reap_strays_e2e.rs` integration test
   existed in `25-CI-OBSERVATION.md`'s scope — that document only walked `agent.rs` and
   `commands.rs`'s `gate_sweep`-adjacent tests, not the full `crates/` tree this plan's
   Task 1 Step 1 requires.

This document's four `VULNERABLE-POSITIVE` and two `VACUOUS-NEGATIVE` rows are the
complete, re-measured work list for Task 2.
