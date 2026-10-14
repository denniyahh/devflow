---
phase: "50"
slug: "addressable-monitor-liveness"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-10-14"
---

# Phase 50 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Source: `50-RESEARCH.md` § Validation Architecture (rows V-01…V-29). Task IDs are assigned by the planner.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (rustc 1.97.1), `tempfile`, `insta`; TDD mode on |
| **Config file** | `scripts/check.sh`, `rust-toolchain.toml`, workspace `Cargo.toml` |
| **Quick run command** | `cargo test -p devflow-core --lib supervisor::` then `cargo test -p devflow --bin devflow commands::` |
| **Full suite command** | `scripts/check.sh all` (then `scripts/check-in-container.sh` before push) |
| **Estimated runtime** | quick: tens of seconds; full: several minutes (container 2–9 min cold) |

**Exact-name rule:** run a single test as
`cargo test -p <pkg> <target> <name> -- --exact 2>&1 | tee /dev/stderr | rg -q '1 passed'`.
A bare `--exact` name that matches nothing still exits 0, so qualify unit-test names by module and assert on
`1 passed`. The package is `devflow`, not `devflow-cli`, and it is binary-only: use `--bin devflow`, never `--lib`.

---

## Sampling Rate

- **After every task commit:** the task's named tests (exact, `1 passed`) plus `cargo test -p devflow-core --lib` or `cargo test -p devflow --bin devflow` for the touched crate
- **After every plan wave:** `scripts/check.sh all`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo mutants -v --file` on `supervisor/backstop.rs` and `supervisor/runtime.rs` (no surviving mutants in the decision functions)
- **Before `/gsd-verify-work`:** full suite green, `scripts/check-in-container.sh`, the V-27 semver comparison, then the live rig
- **Max feedback latency:** the quick command; the full suite runs at wave boundaries

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| V-01 | TBD | TBD | SUPV-01 crit 1 (GONE) | — | N/A | unit | `cargo test -p devflow-core --lib supervisor::tests::probe_reports_gone_only_when_no_socket_file_exists -- --exact` | ❌ W0 | ⬜ pending |
| V-02 | TBD | TBD | SUPV-01 crit 1 (STALE) | — | N/A | unit | `cargo test -p devflow-core --lib supervisor::tests::probe_reports_stale_only_for_a_socket_whose_listener_is_gone -- --exact` | ❌ W0 | ⬜ pending |
| V-03 | TBD | TBD | SUPV-01 crit 1 (ALIVE) | — | N/A | unit | `cargo test -p devflow-core --lib supervisor::tests::probe_reports_alive_only_after_a_ping_reply -- --exact` | ❌ W0 | ⬜ pending |
| V-04 | TBD | TBD | SUPV-01 crit 1 (discrimination) | — | N/A | unit + mutants | `cargo test -p devflow-core --lib supervisor::tests::each_fixture_produces_exactly_one_liveness_state -- --exact`; `cargo mutants -v --file crates/devflow-core/src/supervisor.rs` | ❌ W0 | ⬜ pending |
| V-05 | TBD | TBD | R-6 | backstop must not fire on a stray file | non-socket path is a probe error, never STALE | unit | `cargo test -p devflow-core --lib supervisor::tests::a_non_socket_or_unreadable_path_is_a_probe_error_not_stale -- --exact` | ❌ W0 | ⬜ pending |
| V-06 | TBD | TBD | SUPV-01 crit 3 / R-5 | — | N/A | unit | `cargo test -p devflow-core --lib supervisor::tests::socket_path_of_107_bytes_binds_and_108_is_refused_by_name -- --exact` | ❌ W0 | ⬜ pending |
| V-07 | TBD | TBD | SUPV-01 crit 3 | — | N/A | unit | `cargo test -p devflow --bin devflow pipeline_launch::tests::launch_publishes_the_socket_path_under_the_cache_dir_in_state -- --exact` | ❌ W0 | ⬜ pending |
| V-08 | TBD | TBD | SUPV-01 crit 2 / R-11 | — | socket mode 0600 | unit (table) | `cargo test -p devflow-core --lib monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter -- --exact` | ❌ W0 | ⬜ pending |
| V-09 | TBD | TBD | SUPV-01 crit 2 | — | N/A | unit | `cargo test -p devflow-core --lib monitor::tests::both_arms_answer_liveness_identically_for_each_condition -- --exact` | ❌ W0 | ⬜ pending |
| V-10 | TBD | TBD | R-3 | group kill must not hit the launcher's group | `pgid == agent_pid` | unit | `cargo test -p devflow-core --lib monitor::tests::both_arms_put_the_agent_in_its_own_group -- --exact` | ❌ W0 | ⬜ pending |
| V-11 | TBD | TBD | R-1 (atomic publish) | live socket never replaced | publish refuses an existing name | unit | `cargo test -p devflow-core --lib supervisor::tests::publish_never_exposes_an_unlistening_socket_or_replaces_a_live_one -- --exact` | ❌ W0 | ⬜ pending |
| V-12 | TBD | TBD | R-1 (concurrency) | — | N/A | e2e | `cargo test -p devflow --test supervisor_e2e two_concurrent_starts_yield_exactly_one_live_supervisor -- --exact` | ❌ W0 | ⬜ pending |
| V-13 | TBD | TBD | R-1 (handoff) | — | N/A | unit | `cargo test -p devflow --bin devflow pipeline_launch::tests::stage_handoff_publishes_the_next_socket_before_the_old_one_is_unlinked -- --exact` | ❌ W0 | ⬜ pending |
| V-14 | TBD | TBD | R-2 | — | N/A | unit (+ control) | `cargo test -p devflow-core --lib supervisor::runtime::tests::a_stop_racing_natural_completion_never_advances -- --exact` | ❌ W0 | ⬜ pending |
| V-15 | TBD | TBD | R-2 (gate) | — | lock released on abort | unit | `cargo test -p devflow --bin devflow pipeline_launch::tests::a_signal_during_a_gate_wait_aborts_and_releases_the_lock -- --exact` | ❌ W0 | ⬜ pending |
| V-16 | TBD | TBD | D-05 / 999.134 | — | N/A | unit | `cargo test -p devflow-core --lib monitor::tests::sigterm_to_the_supervisor_kills_the_agent_group_records_143_and_does_not_advance -- --exact` | ❌ W0 | ⬜ pending |
| V-17 | TBD | TBD | D-04 / 999.138 | — | N/A | e2e | `cargo test -p devflow --test stop_e2e stop_ends_a_lock_free_live_run_through_the_socket -- --exact` | ❌ W0 | ⬜ pending |
| V-18 | TBD | TBD | R-4 (leader present) | wrong process signalled | start-time-validated pidfd group signal | unit | `cargo test -p devflow-core --lib supervisor::backstop::tests::backstop_kills_a_validated_group_whose_leader_is_present -- --exact` | ❌ W0 | ⬜ pending |
| V-19 | TBD | TBD | R-4 (leader gone) | wrong process signalled | refuse and report; nothing signalled | unit | `cargo test -p devflow-core --lib supervisor::backstop::tests::backstop_refuses_and_reports_a_group_whose_leader_is_gone -- --exact` | ❌ W0 | ⬜ pending |
| V-20 | TBD | TBD | R-4 (reuse) | wrong process signalled | reused pgid never signalled | unit | `cargo test -p devflow-core --lib supervisor::backstop::tests::a_reused_pgid_is_never_signalled -- --exact` | ❌ W0 | ⬜ pending |
| V-21 | TBD | TBD | R-8 | — | previous-boot socket: nothing signalled | unit | `cargo test -p devflow-core --lib supervisor::backstop::tests::a_previous_boot_socket_is_removed_and_nothing_is_signalled -- --exact` | ❌ W0 | ⬜ pending |
| V-22 | TBD | TBD | R-7 / D-06 | — | sweep is read-only | unit | `cargo test -p devflow-core --lib supervisor::tests::sweep_reports_without_removing_or_signalling -- --exact` | ❌ W0 | ⬜ pending |
| V-23 | TBD | TBD | 999.139 / R-12 | — | N/A | e2e | `cargo test -p devflow --test start_lock_e2e start_refuses_over_a_live_supervisor_when_the_state_will_not_load -- --exact` | ❌ W0 | ⬜ pending |
| V-24 | TBD | TBD | 999.128 | — | N/A | unit | `cargo test -p devflow-core --lib lock::tests::acquire_reclaims_a_lock_whose_live_pid_has_a_different_start_time -- --exact` | ❌ W0 | ⬜ pending |
| V-25 | TBD | TBD | 999.128 (repair) | — | a `Live` holder still refuses | unit (rewritten) | `cargo test -p devflow-core --lib recover::tests::clean_phase_returns_without_cleanup_for_a_recycled_pid_lock -- --exact` (renamed by the plan) | ✅ (flip) | ⬜ pending |
| V-26 | TBD | TBD | 999.141 / D-07 | tests touch the real `~/.cache/devflow` | hermetic HOME | e2e | `cargo test -p devflow --test test_home_isolation a_child_writing_outside_its_tempdir_home_fails -- --exact` | ❌ W0 | ⬜ pending |
| V-27 | TBD | TBD | R-9 | — | N/A | script gate | `cargo semver-checks check-release -p devflow-core 2>&1 \| rg '^--- failure' \| sort` compared to the plan's named list (includes 2 pre-existing failures) | n/a | ⬜ pending |
| V-28 | TBD | TBD | D-16 | — | N/A | manual (live rig) | runbook | manual | ⬜ pending |
| V-29 | TBD | TBD | D-17 / R-10 | — | N/A | manual + script | `tools/fault-arm.sh` record | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/devflow-cli/tests/common/mod.rs` — hermetic `HOME`/`XDG_*`/`DEVFLOW_CACHE_DIR` child builder plus reaper (999.141; lands **first**, D-07)
- [ ] an injectable cache dir for in-crate `--bin` tests (research Finding 7c)
- [ ] `crates/devflow-cli/tests/test_home_isolation.rs` — V-26
- [ ] `crates/devflow-cli/tests/supervisor_e2e.rs` — V-12 and fault-arm fixtures
- [ ] `crates/devflow-core/src/supervisor*.rs` test modules — V-01…V-06, V-11, V-14, V-18…V-22
- [ ] `.planning/phases/50-addressable-monitor-liveness/tools/fault-arm.sh` and the Phase 50 copies of the Phase 49 rig tools

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live run to Ship that crosses both monitor arms | D-16 (SUPV-01 crit 2) | needs a real agent, a disposable GitHub repo and the operator's preflight approvals | Phase 49 rig runbook, one `checkpoint:human-action` per attempt, capped at 3 attempts (49 D-10/D-11/D-18/D-19) |
| Fault arm: ALIVE→stop→GONE; kill -9→STALE→stop→GONE; SIGKILL while advancing; SIGKILL between publish and unlink; stop racing completion | D-17 / R-10 | real process faults against a live supervisor | `tools/fault-arm.sh`; each record names the launch shape, socket path, probe result, group members before and after, and whether advance ran |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < quick-command runtime
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
