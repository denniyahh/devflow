---
phase: 50-addressable-monitor-liveness
verified: 2026-10-23T10:24:24Z
status: passed
score: 8/8 roadmap success criteria verified (0 present-but-behavior-unverified)
covered_files:
  - ".github/workflows/ci.yml"
  - ".planning/phases/50-addressable-monitor-liveness/50-01-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-01-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-02-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-02-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-03-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-03-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-04-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-04-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-05-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-05-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-06-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-06-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-07-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-07-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-08-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-08-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-09-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-09-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-10-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-10-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-11-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-11-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-12-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-12-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-13-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-13-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-14-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-14-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-15-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-15-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-16-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-16-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-17-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-17-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-18-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-18-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-19-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-19-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-20-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-20-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-21-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-21-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-22-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-22-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-23-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-23-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-24-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-24-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-25-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-25-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-26-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-26-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-27-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-27-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-28-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-28-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-29-PLAN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-29-SUMMARY.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-LIVE-RUN.md"
  - ".planning/phases/50-addressable-monitor-liveness/50-RUNBOOK.md"
  - ".planning/phases/50-addressable-monitor-liveness/tools/EVIDENCE-LAYOUT.md"
  - ".planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh"
  - ".planning/phases/50-addressable-monitor-liveness/tools/fault-arm.sh"
  - ".planning/phases/50-addressable-monitor-liveness/tools/semver-gate.sh"
  - ".planning/phases/50-addressable-monitor-liveness/tools/wr03-revert-check.sh"
  - "ARCHITECTURE.md"
  - "OPERATIONS.md"
  - "crates/devflow-cli/Cargo.toml"
  - "crates/devflow-cli/src/commands.rs"
  - "crates/devflow-cli/src/main.rs"
  - "crates/devflow-cli/src/pipeline_gate.rs"
  - "crates/devflow-cli/src/pipeline_launch.rs"
  - "crates/devflow-cli/src/pipeline_outcomes.rs"
  - "crates/devflow-cli/src/preflight.rs"
  - "crates/devflow-cli/src/staleness.rs"
  - "crates/devflow-cli/src/test_support.rs"
  - "crates/devflow-cli/tests/auto_chain_flag_e2e.rs"
  - "crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs"
  - "crates/devflow-cli/tests/common/mod.rs"
  - "crates/devflow-cli/tests/doctor_antigravity.rs"
  - "crates/devflow-cli/tests/gate_notify_persist_e2e.rs"
  - "crates/devflow-cli/tests/gate_sweep_e2e.rs"
  - "crates/devflow-cli/tests/gate_wedge_e2e.rs"
  - "crates/devflow-cli/tests/help_snapshot.rs"
  - "crates/devflow-cli/tests/hermetic_spawn_sites.rs"
  - "crates/devflow-cli/tests/log_format_env.rs"
  - "crates/devflow-cli/tests/phase7_cli.rs"
  - "crates/devflow-cli/tests/plan_bashism_scanner.rs"
  - "crates/devflow-cli/tests/reap_strays_e2e.rs"
  - "crates/devflow-cli/tests/recover_clean_e2e.rs"
  - "crates/devflow-cli/tests/release_check.rs"
  - "crates/devflow-cli/tests/snapshots/devflow-help.txt"
  - "crates/devflow-cli/tests/start_lock_e2e.rs"
  - "crates/devflow-cli/tests/start_reachability_e2e.rs"
  - "crates/devflow-cli/tests/stop_e2e.rs"
  - "crates/devflow-cli/tests/supervisor_e2e.rs"
  - "crates/devflow-cli/tests/test_home_isolation.rs"
  - "crates/devflow-cli/tests/yes_ship_config.rs"
  - "crates/devflow-core/src/agent.rs"
  - "crates/devflow-core/src/doc_check.rs"
  - "crates/devflow-core/src/gates.rs"
  - "crates/devflow-core/src/lib.rs"
  - "crates/devflow-core/src/lock.rs"
  - "crates/devflow-core/src/monitor.rs"
  - "crates/devflow-core/src/recover.rs"
  - "crates/devflow-core/src/registry.rs"
  - "crates/devflow-core/src/ship.rs"
  - "crates/devflow-core/src/state.rs"
  - "crates/devflow-core/src/supervisor.rs"
  - "crates/devflow-core/src/supervisor/backstop.rs"
  - "crates/devflow-core/src/supervisor/protocol.rs"
  - "crates/devflow-core/src/supervisor/runtime.rs"
  - "crates/devflow-core/src/test_support.rs"
  - "crates/devflow-core/tests/devflow_dir_gitignore.rs"
  - "crates/devflow-core/tests/monitor_e2e.rs"
  - "doc-check-allowlist.toml"
  - "docs/diagrams/agent-lifecycle.md"
  - "docs/guides/unattended-mode.md"
covered_digest: "v3:sha256:e9abee80aafea90ddc4cebf0818fa79ec68639e22a296121d07d1a5395cb3d8f"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: passed
  previous_score: 8/8
  reason: "stale: the phase close-out (f6e34f2, be92c83; merged to develop in PR #225) changed 8 covered files after the 2026-10-21 verification"
  gaps_closed: []
  gaps_remaining: []
  regressions: []
---

# Phase 50: Addressable Monitor Liveness Verification Report

**Phase Goal:** "Is this run's monitor alive?" is answerable from disk state alone without a PID, in three
distinguishable states (GONE, STALE, ALIVE), so a stalled unattended run is recoverable by design rather than by
diagnosis. This is the milestone's breaking change.
**Verified:** 2026-10-23T10:24:24Z
**Status:** passed
**Re-verification:** Yes. The 2026-10-21 report (`cc50411`, passed 8/8) went stale when the close-out changed covered
files. That report was used as a lead only; every criterion was re-checked against the current tree.

## What changed since the previous verification

`git diff cc50411 HEAD` over `crates/`, `Cargo.*`, the docs, the allowlist, `.github/` and the phase directory
shows exactly two commits' worth of change (`cc50411` is an ancestor of HEAD):

- `f6e34f2` (8 files). Production: `monitor.rs` `group_empties_within` now asks the backstop's census
  (`supervisor::backstop::group_members`, made `pub(crate)`) instead of `kill(-pgid, 0)`. Tests: the new
  `test_support::live_group_members` / `process_is_live` replace every test-side `kill(-pgid, 0)` group probe in
  `runtime.rs`, `monitor.rs`, `pipeline_launch.rs`, `stop_e2e.rs`, `supervisor_e2e.rs`, `start_lock_e2e.rs`, plus a
  new test `test_support::tests::a_zombie_is_not_a_live_group_member`.
- `be92c83`: `options: --init` on the CI Test and Sequential 2-CPU jobs (`ci.yml` lines 46-53, 107-108).

Commits after `78178f6` change only `.planning/`, `graphify-out/` and the plan-linter test, as the brief said
(`git diff 673e9fb 78178f6 -- crates/` touches only `plan_bashism_scanner.rs` and its fixture).

## Lead: what argues against the result

1. **The zombie half of the production change has never run end to end.** The census excludes zombies by the
   same `state == 'Z'` rule the new unit test proves on a real unreaped zombie, but no test drives
   `group_empties_within` with a group that holds only zombies. On this host PID 1 reaps, and CI on PR #225's head
   (`673e9fb`) ran with `--init`, so it reaped too: no CI run exists for the fix commit alone (`gh run list
   --commit a265527` is empty). "18 failures under a non-reaping PID 1 are fixed by the census" is therefore not
   shown by any run; `--init` removed the condition rather than exercising the fix. The phase's eight criteria do not
   depend on it, because both a reaping host and CI now reap.
2. **The census narrows "occupied" to this uid.** Before, `kill(-pgid, 0)` answering `EPERM` (a member of another
   uid, such as a setuid program the agent ran) counted as "not empty". The census skips other uids, so such a
   member now reads as empty and `end_recorded_agent_group` removes the sidecar. This matches the backstop's
   existing, deliberate rule T-50-25 (`gathering_skips_processes_of_another_uid` asserts another uid's group reads
   empty, and it passed here), and SIGKILL from this uid could not have ended that member anyway. It is a behaviour
   change in a rare case, not a broken guarantee.
3. **Carried from the previous report, unchanged:** the Ship finalization retry loop checks for a stop only once,
   before the loop (backlog 999.166 item 7, deferred by operator decision of 2026-10-21); neither the WR-03 nor the
   CR-01 fix path ran live. The close-out did not touch `pipeline_gate.rs` or `gates.rs`.

## The three guarantees the brief asked about, for the production change

| Guarantee | Status | Evidence |
|-----------|--------|----------|
| A zombie is not a live member | ✓ VERIFIED (rule), not exercised on the production path | `backstop::group_members` skips `stat.state == 'Z'` (backstop.rs line 432). The same `process_stat_fields` parse and `!= 'Z'` filter in `test_support::live_group_members` passed `a_zombie_is_not_a_live_group_member`, which makes a real unreaped zombie, shows `kill(-pgid, 0)` still returns 0 for it (the negative control), and shows the census excludes it; its running-process control counts the same pid. See Lead 1 for the limit |
| An empty group is still reported empty | ✓ VERIFIED | `monitor::tests::a_supervisor_killed_before_publishing_has_its_agent_group_ended_through_its_sidecar` passed. It exercises the production `end_recorded_agent_group` → `group_empties_within` path and asserts the sidecar is gone; the sidecar is kept exactly when `group_empties_within` returns false (`keep_sidecar: true`, monitor.rs lines 2081-2088 and 1987), so this assertion discriminates. Its runs 2 and 3 are the controls (no sidecar, wrong start time: nothing signalled). Note the text assertion alone would not discriminate: the "still had members" text also contains "ended the agent group … its sidecar records" |
| An unreadable `/proc` does not read as empty | ✓ VERIFIED (code read) | `group_members` returns `None` when `read_dir("/proc")` fails (`.ok()?`, backstop.rs line 412), and `group_empties_within` returns true only on `Some(empty)` (`is_some_and`, monitor.rs lines 2148-2150). A pgid ≤ 1 or one that does not fit `pid_t` still returns false before any read. Not exercised by a test (it needs an unmountable `/proc`). Off Linux the path is unreachable: `assess` answers `RefuseIdentityUnavailable` there, so `LeaderPresentSignal` (the only caller) never fires |

One test-side asymmetry is worth knowing: `test_support::live_group_members` returns an empty list (not "cannot
tell") when `/proc` cannot be listed, so a test's "group is gone" check would pass vacuously off Linux. These tests
are Linux-only in practice, most "gone" assertions follow a "still running" control on the same helper, and the new
zombie test's running control proves the helper sees a live process on this host.

## Goal Achievement

### Observable Truths (roadmap contract)

Every named test below was run alone in this verification with `cargo nextest run … --no-tests=fail -E
'test(=<full path>)'`, each reporting `1 test run: 1 passed`. Control: a nonexistent name under the same command
exits 4 with `0 tests run`.

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Liveness resolves to exactly one of GONE/STALE/ALIVE from `state.json` plus the socket, no PID read; each fixture produces only its state | ✓ VERIFIED | `supervisor::tests::each_fixture_produces_exactly_one_liveness_state` passed. `supervisor.rs` (`probe`, `probe_phase`) is not in the close-out diff |
| 2 | `PipeOwning` and `Legacy` answer liveness identically | ✓ VERIFIED | `monitor::tests::both_arms_answer_liveness_identically_for_each_condition` passed; its group probe now goes through `live_group_members` |
| 3 | Socket path persisted in `state.json` under `registry::cache_dir()`; over-long path refused by name | ✓ VERIFIED | `supervisor::tests::socket_path_of_107_bytes_binds_and_108_is_refused_by_name` passed; code unchanged |
| 4 | `stop` on an ALIVE supervisor: shutdown, group killed (grace, SIGKILL), 143, no advance, socket removed, exits; lock-free run; a stop racing natural exit never launches the next stage | ✓ VERIFIED (residual in Lead 3) | Passed: `stop_e2e::stop_ends_a_lock_free_live_run_through_the_socket` (asserts the group is non-empty before and empty after), `stop_e2e::stop_with_a_launch_id_stops_a_live_socket_whose_sidecar_will_not_parse` (the probe rewritten in `f6e34f2`), `supervisor::runtime::tests::a_stop_racing_natural_completion_never_advances`, `supervisor::runtime::tests::shutdown_over_the_socket_kills_the_agent_group_and_reports_stopped`, `supervisor::runtime::tests::a_term_ignoring_group_member_is_killed_after_the_leader_exits`, `pipeline_launch::tests::a_stop_between_the_next_spawn_and_its_publish_shuts_the_next_supervisor_down` (the `group_alive` probe rewritten), `pipeline_launch::tests::a_stop_before_finalization_is_honoured_before_the_merge`, and the WR-03 pair `monitor::tests::a_completed_stream_run_returns_while_a_leftover_process_holds_the_agents_stdout` and `supervisor_e2e::a_stop_sent_while_a_completed_run_waits_on_a_held_stdout_ends_it_at_safe_point_1` |
| 5 | SIGTERM/SIGINT to either shape behaves like a socket `shutdown` | ✓ VERIFIED | `monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance` and `supervisor_e2e::sigterm_to_a_live_supervisor_ends_the_run_without_advancing` (asserts `group_members(pgid)` is empty after, both shapes) passed |
| 6 | STALE backstop signals only a validated group; previous boot removed; leader-gone refused | ✓ VERIFIED | `supervisor::backstop::tests::classify_covers_every_rule_in_order` and `supervisor::backstop::tests::gathering_skips_processes_of_another_uid` passed. The only backstop change is `group_members` becoming `pub(crate)`; the backstop's own `group_empties_within` already used the census before the close-out |
| 7 | Sockets/sidecars listed and probed with no state; `doctor` reports orphans; read-only commands never kill or remove; `start`/`resume` refuse over a live sidecar on unloadable state | ✓ VERIFIED | `start_lock_e2e::start_refuses_over_a_live_supervisor_when_the_state_will_not_load` (its `group_is_gone` probe rewritten) and `supervisor_e2e::two_concurrent_starts_yield_exactly_one_live_supervisor` passed. The other criterion-7 code is not in the close-out diff, and the full suite passed in the pinned container on `78178f6` (brief; not re-run here by instruction) |
| 8 | Recycled-pid lock reclaim; every e2e test isolates HOME, XDG_CACHE_HOME, DEVFLOW_CACHE_DIR and reaps its supervisors | ✓ VERIFIED | `lock::tests::acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time` and the negative control `test_home_isolation::a_child_writing_outside_its_tempdir_home_fails` passed; `lock.rs` and the hermetic helpers are not in the close-out diff |

**Score:** 8/8 roadmap success criteria verified, 0 present-but-behavior-unverified.

The plan-level truths of 50-01 to 50-29 were checked in the previous two reports. Their files are unchanged except
for the eight close-out files above, and the phase directory itself is unchanged since `cc50411`.

### Required Artifacts (close-out)

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/monitor.rs` | `group_empties_within` on the census; test probes on `live_group_members` | ✓ VERIFIED | called from `end_recorded_agent_group` (line 2079), the only caller |
| `crates/devflow-core/src/supervisor/backstop.rs` | `pub(crate) fn group_members` | ✓ VERIFIED | used by `gather`, its own `group_empties_within` and monitor.rs |
| `crates/devflow-core/src/test_support.rs` | `live_group_members`, `process_is_live`, zombie test | ✓ VERIFIED | behind `#[cfg(any(test, feature = "test-support"))]` (lib.rs line 82); the CLI enables `test-support` via its dev-dependency (devflow-cli Cargo.toml line 29). Additive `pub` items behind a feature, not a published-API break |
| `.github/workflows/ci.yml` | `--init` on Test and Sequential 2-CPU | ✓ VERIFIED | lines 53 and 108 |

`rg 'kill\(-'` over `crates/` finds no `kill(-pgid, 0)` existence probe left outside the zombie test's own
control. One single-pid probe remains: `monitor::tests::a_supervisor_that_fails_to_publish_ends_its_agent_group`
polls `kill(agent, 0)` for ESRCH (monitor.rs line 6235), which a non-reaping PID 1 would keep failing on an orphaned
zombie agent. With `--init` in CI it does not matter today (Info, below).

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `end_recorded_agent_group` | `backstop::group_members` | `group_empties_within` polls it until `Some(empty)` or the deadline | WIRED (test above) |
| `group_empties_within` result | sidecar removal | `keep_sidecar` → `remove_launch_files` vs socket-only removal | WIRED (the test asserts the sidecar is gone) |
| CLI tests | `devflow_core::test_support::live_group_members` | `test-support` feature on the dev-dependency | WIRED (the CLI tests compiled and passed) |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| 13 core lib tests named above, one per command | `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=…)'` | each `1 passed` | ✓ PASS |
| 8 CLI integration tests (`stop_e2e`, `supervisor_e2e`, `start_lock_e2e`, `test_home_isolation`) | `cargo nextest run -p devflow --test <name> --no-tests=fail -E 'test(=…)'` | each `1 passed` | ✓ PASS |
| 2 CLI bin tests (`pipeline_launch::tests::…`) | `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=…)'` | each `1 passed` | ✓ PASS |
| Name-gate control | same command, nonexistent test name | rc 4, `0 tests run` | ✓ discriminates |
| macOS compile | `cargo check --target aarch64-apple-darwin -p devflow-core -p devflow --all-targets` | finished, rc 0 | ✓ PASS (compile only) |
| PR #225 CI | `gh pr checks 225` (head `673e9fb`) | 12 of 12 pass, including Test and Sequential 2-CPU | ✓ PASS, but with `--init` (Lead 1) |
| Full suite | not re-run (instruction: another verifier runs concurrently) | brief reports `scripts/check.sh all` green in the pinned container on `78178f6` | not checked here |

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|--------------|-------------|--------|----------|
| SUPV-01 | all 29 plans (`requirements: [SUPV-01]`; 29 of 29 matched, control ID `SUPV-99` matched 0) | Monitor liveness answerable from disk state alone without a PID, in three distinguishable states; both shapes identical; `sun_path` bound; path in `state.json` under the cache dir | ✓ SATISFIED | criteria 1-8 verified above. REQUIREMENTS.md now shows `[x]` and `Complete` (lines 122, 205) |

No orphaned requirement: REQUIREMENTS.md maps only SUPV-01 to Phase 50.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| lines added since `cc50411` (crates, ci.yml) | — | `TBD`/`FIXME`/`XXX`/`TODO`/`HACK` | none | zero hits; positive control: the same scan finds 16 added lines containing `zombie` |
| `crates/devflow-core/src/monitor.rs` | 6235 | zombie-blind `kill(pid, 0)` in `a_supervisor_that_fails_to_publish_ends_its_agent_group` | ℹ️ Info | would fail under a non-reaping PID 1; masked by `--init` |
| `crates/devflow-core/src/monitor.rs` | 2126 | `group_member_count` (refusal text only) still counts zombies | ℹ️ Info | wording in a refusal message, no decision rests on it |
| `crates/devflow-cli/src/pipeline_gate.rs` | finalization loop | stop checked once before a retry loop | ⚠️ Warning (carried; 999.166 item 7, operator-deferred) | narrow window, unreproduced |

### Human Verification Required

None. No truth needs a human test step. The judgment-tier prohibitions of plans 50-26 to 50-29 were settled from
evidence in the previous report, nothing was found broken, and the close-out does not touch the files they concern.

### What this verification does not establish

- **The zombie-only production path, and any non-reaping-PID-1 run** (Lead 1). Nothing on record runs the census
  fix without a reaping PID 1.
- **The unreadable-`/proc` guarantee rests on code reading**, not a test.
- **The full suite was not re-run here.** That rests on the brief's report of the pinned-container run on
  `78178f6` and on PR #225's CI; I confirmed the CI result (12 of 12 pass on `673e9fb`) but not the container run.
  Each named test passed once on one machine, which says nothing about timing flake rates.
- **Live evidence is unchanged**: attempt 2 ran on a binary built before the close-out, with no stop sent. The
  close-out's production change lies on the RV-03 path (a supervisor killed before it published), which attempt 2
  did not exercise.
- **macOS is compile-only.**

### Gaps Summary

No gaps. The close-out's production change keeps the phase's guarantees: an empty group is still reported empty
(tested on the production path, with a discriminating assertion), an unreadable `/proc` still never reads as empty
(code), and a zombie no longer counts as a member (rule tested on a real zombie; the production path is not
exercised with one). It also stops counting another uid's member as occupying the group, which matches the
backstop's existing rule. All eight roadmap criteria hold on the current tree by named tests that passed in this
run. The carried residual (999.166 item 7) stays deferred by operator decision.

---

_Verified: 2026-10-23T10:24:24Z_
_Verifier: Claude (gsd-verifier)_
