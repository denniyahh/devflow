---
phase: 48-survivable-state-writes-and-honest-gate-recovery
verified: 2026-09-23T23:26:51Z
status: passed
score: 7/7 must-haves verified
covered_files: [".planning/REQUIREMENTS.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-01-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-01-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-02-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-02-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-03-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-03-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-04-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-04-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-05-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-05-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-06-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-06-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-07-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-07-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-08-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-08-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-09-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-09-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-10-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-10-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-11-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-11-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-12-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-12-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-13-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-13-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-14-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-14-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-15-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-15-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-16-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-16-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-17-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-17-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-18-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-18-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-19-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-19-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-20-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-20-SUMMARY.md","clippy.toml","crates/devflow-cli/src/commands.rs","crates/devflow-cli/src/main.rs","crates/devflow-cli/src/pipeline_gate.rs","crates/devflow-cli/src/pipeline_launch.rs","crates/devflow-cli/src/pipeline_outcomes.rs","crates/devflow-cli/src/preflight.rs","crates/devflow-cli/src/staleness.rs","crates/devflow-cli/src/test_support.rs","crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs","crates/devflow-cli/tests/gate_sweep_e2e.rs","crates/devflow-cli/tests/gate_wedge_e2e.rs","crates/devflow-cli/tests/gitignore_coverage.rs","crates/devflow-cli/tests/plan_bashism_scanner.rs","crates/devflow-cli/tests/recover_clean_e2e.rs","crates/devflow-cli/tests/start_lock_e2e.rs","crates/devflow-cli/tests/stop_e2e.rs","crates/devflow-core/src/agent.rs","crates/devflow-core/src/agent_result.rs","crates/devflow-core/src/agents/opencode.rs","crates/devflow-core/src/agents/pi.rs","crates/devflow-core/src/config.rs","crates/devflow-core/src/doc_check.rs","crates/devflow-core/src/gates.rs","crates/devflow-core/src/lock.rs","crates/devflow-core/src/monitor.rs","crates/devflow-core/src/recover.rs","crates/devflow-core/src/ship.rs","crates/devflow-core/src/state.rs","crates/devflow-core/src/test_support.rs","crates/devflow-core/src/verify.rs","crates/devflow-core/src/workflow.rs","scripts/lint-plan-bashisms.sh"]
covered_digest: "v1:sha256:6ae6481e5f2a2b8afad782c7f093b4c1389352056ea07718d08fde01df2e5051"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 6/7
  gaps_closed:
    - "Criterion 2 (`resume` half, 999.140): `devflow resume --phase N` over a live lock-free Legacy run now exits 1 with 'refusing to resume; nothing was written'. The state file and agent pid file are byte-identical afterwards, no lock is left, no stage_launched is emitted, and the recorded monitor and agent are still alive. Reproduced by the verifier against a freshly built HEAD binary (5eb3a61). The dead-leftover control relaunches. A mutant that removes the one guard call turns exactly the two resume refusal arms red with their named reasons"
    - "Operator decision on 999.139 (the previous report's second missing item): recorded 2026-09-23 in 6d31299 as '999.139 stays in the backlog'. This is a deferral, not an acceptance, and not an explicit scope ruling (see truth 2)"
  gaps_remaining: []
  regressions: []
gaps: []
deferred: []
advisory:
  - finding: "999.141 is still open: the e2e proceed arms write to the operator's real ~/.cache/devflow/roots. 34 entries newer than 19:20 local appeared during this session. Some or all came from this verifier's start_lock_e2e runs. A concurrent reviewer was also active, so the count is not attributable to one source. 48-20's new resume helper uses a per-child cache dir; the older start arms do not"
    category: other
    reason: "Outside TEST-01 as written, which covers process-global PATH mutation. It would be resolved by per-child HOME/DEVFLOW_CACHE_DIR in the older e2e helpers"
    evidence_status: "count observed; no leaked monitor or /tmp/.tmp* devflow process found afterwards"
human_verification: []
---

# Phase 48: Survivable State Writes and Honest Gate Recovery: Verification Report

**Phase Goal:** A second writer cannot silently erase another's state update. A gate whose consumer is gone tells the operator the repair that actually works, instead of asserting a waiter that does not exist. Preflight and resume agree on where blocking-human gates exist. Checkpoints added after preflight are re-scanned. The test-suite PATH race is isolated.
**Verified:** 2026-09-23T23:26:51Z at HEAD `5eb3a61` (worktree `.worktrees/phase-48`, branch `feature/phase-48`)
**Status:** passed. The one open gap, criterion 2 for `resume`, is closed.
**Re-verification:** Yes. Previous: gaps_found 6/7 at `d5376fa`.

## What argues against this result (read first)

- **The criterion-2 hole 999.139 still exists on `start`, and the verifier reproduced it with a stand-in.** The setup had a state file that will not load, an agent pid file naming a dead pid, and a live process standing in for the monitor. `start --force` exited 0, overwrote the state and launched a second agent. The hole is excluded here on one basis only: the operator's recorded deferral ("999.139 stays in the backlog", 6d31299). That is the same basis the previous report used to exclude 999.136. **No ruling explicitly places 999.139 outside criterion 2.** 48-SECURITY.md still lists it as "deferred, not accepted". If the operator does not treat that deferral as exclusion from this phase, criterion 2 reverts to partial, for `start` only. `resume` is closed either way.
- **Provenance of the operator decision.** The deferral text was committed in 6d31299 by an agent session (Claude co-author trailer). The operator did not write it. The 48-19 rulings had the same provenance, and the previous report accepted them.
- **The residue is narrow, and the verifier observed that directly.** With the state corrupted while the agent was alive, both `start --force` and `resume` refused through the agent pid file. So 999.139 bites only when the agent is dead or its pid file is gone while the monitor lives. That is the agent-exit → `advance` window, which is 999.136's window and is also operator-deferred.

## Goal Achievement

### Observable Truths (ROADMAP Phase 48 success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A second-writer test fails pre-fix; a single-writer control round-trips | ✓ VERIFIED (regression) | RED was reproduced by an earlier verifier against pre-fix `167363f`. Since `1590100`, production changed only in `commands.rs` (the shared guard and the resume text) and in `pipeline_launch.rs` (one call and a doc comment); checked with `git diff 1590100..HEAD --stat -- crates`. At HEAD this session: `start_lock_e2e` 12 passed / 0 failed, including `start_refuses_while_another_process_holds_the_phase_lock` and its control `start_proceeds_when_no_process_holds_the_phase_lock`. |
| 2 | After the fix, a second writer for the same phase is excluded or refused loudly and never silently overwrites; the single-writer control still round-trips | ✓ VERIFIED (999.139 on `start` excluded by operator deferral; see above) | **resume (the gap):** `pipeline_launch::resume` now calls `refuse_resume_over_a_live_run`. The call is at line 16 of the function, after `lock::acquire` (line 7) and before `load_state` (line 17), the `--agent` handoff, `repair_leaked_auto_chain_flag`, `save_state` and `launch_stage`. **Live probe on a real HEAD binary:** the refusal arm is refused, the dead arm relaunches, and bytes were compared in both arms (see Behavioral Spot-Checks). **Mutant (guard call deleted, scratch worktree):** 10 passed / 2 failed. The two failures were exactly `resume_refuses_a_phase_whose_recorded_monitor_is_alive` and `resume_refuses_a_stopped_phase_whose_recorded_agent_is_alive`, and each panicked at `start_lock_e2e.rs:745` with its named reason ("resume must refuse a phase whose recorded monitor/agent is alive"). The mutant's stdout showed "stage code → launched Claude Code". The dead-leftover control, the no-state carve-out, the start golden and all start arms stayed green. **start:** `start --force` over the same live run still refused, with state unchanged. `start_live_run_refusal`'s body is textually identical to 48-19's `live_run_refusal` body at `1590100`, compared with a `diff` of the extracted function bodies. `pipeline_launch::resume` has one production caller: `main.rs:612`. Every other caller is in `#[cfg(test)]`, which starts at line 1578. |
| 3 | approve/reject/stop/sweep check for a live waiter; with none they say so, write no non-Ship answer, and name the repair; no `rm -f`; resume never reads a pending answer | ✓ VERIFIED (regression) | 48-20 did not touch these paths; the only `commands.rs` hunks are the guard and refusal functions. At HEAD: `stop_e2e` 22/0, `gate_sweep_e2e` 4/0, `recover_clean_e2e` 11/0. The guard only adds a refusal ahead of `load_state` and does not read gate files. The previous report's caveats still stand: D-05 (Unconfirmable holders still get an answer) and AR-48-04. After 48-20, the no-waiter hint "`devflow resume --phase N`" can point at a verb that refuses when the run is live and lock-free. The criterion prescribes that hint literally. The operator filed the hint question as 999.142 (option b), and the refusal text itself names the working repair. |
| 4 | #200 wedge reproducible on demand; shape pinned both ways | ✓ VERIFIED (regression) | `gate_wedge_e2e` 5/0 at HEAD. The test file has not changed since the previous report verified the M2 mutant. |
| 5 | CHKPT-01: one shared checkpoint parser | ✓ VERIFIED (regression by diff) | `verify.rs` and `preflight.rs` are unchanged since `1590100`. The orchestrator's full workspace run at HEAD reported 1448 passed / 0 failed. That count is the orchestrator's; the verifier did not reproduce it. |
| 6 | CHKPT-02: checkpoints added after Code's preflight are re-scanned | ✓ VERIFIED (regression by diff) | The only change in `pipeline_launch.rs` is the resume guard line and its doc comment. The re-scan code is untouched. |
| 7 | TEST-01: PATH mutations isolated | ✓ VERIFIED (structural) | `git grep 'set_var("PATH"\|remove_var("PATH"' -- crates` finds 1 hit, a doc comment (`test_support.rs:302`). `clippy.toml` still disallows `std::env::set_var` and `std::env::remove_var`. The 48-20 test diff adds no `set_var` or `remove_var`; the new `resume_command` scopes PATH and DEVFLOW_CACHE_DIR through `Command::env`. |

**Score:** 7/7 truths verified (0 present-but-behavior-unverified).

### 48-20 plan must-haves (merged)

| Must-have | Status | Evidence |
|-----------|--------|----------|
| Live monitor: resume exits non-zero and leaves state and agent-pid bytes identical, with no lock, no stage_launched, and the process not signalled | ✓ | Test arm, plus a real-launch probe in which both the monitor and the agent were live |
| Live agent, dead monitor, stopped=true: refuses, and the stop mark survives | ✓ | `resume_refuses_a_stopped_phase_whose_recorded_agent_is_alive` (runs the refusal twice) |
| Dead-pid control: relaunches for stopped=false and stopped=true | ✓ | `resume_relaunches_a_phase_whose_recorded_processes_are_dead`, plus the probe's dead arm |
| Refusal text: says "resume", names each pid by role, and carries no start-only text | ✓ | 27 required fragments and 4+ forbidden fragments are asserted in `assert_resume_refuses_live_run`. The probe showed "(monitor pid 3070616, agent pid 3070617 alive) — refusing to resume" |
| Start text byte-identical | ✓ | Function-body `diff` against `1590100`, plus the golden test |
| Guard under the lock, before `load_state` | ✓ | Line order 7 < 16 < 17 in `resume` |
| Shared liveness (48-19's) | ✓ | `refuse_start_over_a_live_run` and `refuse_resume_over_a_live_run` both delegate to `refuse_launch_over_a_live_run`, and the liveness body is unchanged |
| One production caller of resume | ✓ | `main.rs:612` |
| No-state carve-out | ✓ | `resume_without_phase_state_reports_the_missing_state_not_a_live_run` |
| RED against pre-fix | ✓ (verifier's own mutant) | See truth 2. The verifier did not re-run the executor's RED commit; it ran an independent guard-removal mutant instead |
| Idempotency | ✓ | The agent arm refuses twice. In the probe, a second resume refused and the bytes were unchanged |
| Prohibition: the refusal never signals a recorded pid | ✓ | The test asserts `agent_running(live_pid)` after the refusal. In the probe, M1 and A1 were still alive after both resumes |
| Prohibition: never claims `stop` ends a lock-free run | ✓ | Fragment "would only mark the state stopped: it signals nothing" is asserted |
| Prohibition (judgment): unloadable state changes no launch or write for resume | ✓ | Probe `p139.sh`: resume exited 1 with "state JSON failed: EOF while parsing…". State sha unchanged, 0 launches, no lock left. The hint texts H1–H10 were not touched: no hunk outside the guard functions in `git diff 1590100..HEAD -- crates/*/src` |

### 999.139: status for each verb

| Verb | Unloadable state + live agent pid file | Unloadable state + dead or missing agent pid + live monitor | Covered? |
|------|-----------------------------------------|-------------------------------------------------------------|----------|
| `resume` | Refused with the live-run refusal. Probe: rc=1, "agent pid 3070617 alive", state and agent-pid bytes unchanged, launches stayed at 1 | Fails closed on its own `load_state`. Probe: rc=1, "state JSON failed", state bytes unchanged, 0 launches, no lock | **Covered** (no launch, no write, in both shapes) |
| `start --force` | Refused. Probe: rc=1, "refusing to start; nothing was written", state unchanged | **Proceeds.** Probe: rc=0, "stage define → launched Claude Code (monitor pid 3072055)", state overwritten, a second agent started while the stand-in monitor was still alive | **Open**: the residue of 999.139 |

**Scope:** the operator's recorded decision (6d31299, ROADMAP 999.140 promotion note; STATE `stopped_at`) is "999.139 stays in the backlog". 48-20-PLAN repeats it as "Out of scope, by operator decision". No artifact contains an explicit ruling that 999.139 is outside criterion 2. The exclusion applied here is an inference, consistent with how the previous report treated the operator-deferred 999.136. The stand-in monitor's pid is recorded nowhere readable, which is the point of 999.139. The probe therefore shows that the guard cannot see such a monitor. It does not show a real monitor ending up in that state.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/commands.rs` | `pub(crate) fn refuse_resume_over_a_live_run`, shared `refuse_launch_over_a_live_run`, `resume_live_run_refusal` | ✓ VERIFIED | Present, substantive, called from `pipeline_launch.rs:1427` |
| `crates/devflow-cli/src/pipeline_launch.rs` | `refuse_resume_over_a_live_run(project_root, phase)` under the lock | ✓ VERIFIED | Line 1427; the mutant proves it is load-bearing |
| `crates/devflow-cli/tests/start_lock_e2e.rs` | two resume refusal arms, dead control, no-state carve-out, start golden | ✓ VERIFIED | 12 listed and 12 passed. The refusal arms discriminate (mutant) |
| `crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs` | waits for `fixture.child_pid().is_some()` before resume | ✓ VERIFIED | A 6-line test-only wait, so the SIGKILL fixture knows its orphan before resuming; 2/0 at HEAD |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `pipeline_launch::resume` | `commands::refuse_resume_over_a_live_run` | call between `lock::acquire` and `load_state` | WIRED (mutant breaks it) |
| `refuse_launch_over_a_live_run` | `agent::agent_running` | monitor_pid plus agent pid file | WIRED (probe: real live pids detected) |
| `commands::start` | `refuse_start_over_a_live_run` | between `_phase_lock` (line 55 of fn) and `Gates::cleanup` (line 73), guard at line 65 | WIRED (probe: `start --force` refused) |

### Behavioral Spot-Checks

The binary was built with `cargo build -p devflow` at HEAD `5eb3a61` (19:22:45 local, sha256 `84cda2d1…`), from a clean `crates/` tree (`git diff --quiet HEAD -- crates`). It contains the string "refusing to resume". Probe scripts are under `/home/denniyahh/.claude/jobs/d73e94fd/tmp/v48r/`. Each uses a scratch repo, a scratch `DEVFLOW_CACHE_DIR`, and a fake `claude` that runs `exec sleep 97` (or 96).

| Behavior | Result | Status |
|----------|--------|--------|
| **Setup:** real `start --phase 77 --mode supervise --no-worktree --legacy-claude-launch` | rc=0. Monitor 3070616 is an `sh -c apid=…` script; agent 3070617 (`sleep 97`) has ppid 3070616. No lock-77 held. stage_launched=1 | ✓ |
| **Live arm:** `resume --phase 77` over the live run | rc=1: "phase 77: a run is still live (monitor pid 3070616, agent pid 3070617 alive) — refusing to resume; nothing was written" (9 lines). State sha `ac6950f2…` → `ac6950f2…`; agent-pid sha `b9c7a9e3…` → `b9c7a9e3…`; monitor_pid and agent pid unchanged; no lock left; stage_launched 1 → 1; M1 and A1 alive; exactly 1 fake agent running | ✓ refusal |
| **Live arm, second time** (idempotency) | rc=1 with the same refusal; both shas unchanged; still 1 fake agent | ✓ |
| **Live arm:** `start --force` (48-19 regression) | rc=1, "refusing to start; nothing was written", state sha unchanged | ✓ |
| **999.139, live agent:** state corrupted to `{"not a state` | resume rc=1 and start --force rc=1, both naming "agent pid 3070617 alive"; corrupt state bytes unchanged; launches stayed at 1 | ✓ |
| **Dead arm:** followed the refusal's guidance, SIGTERM to the monitor first | After SIGTERM to M1, both M1 and A1 were gone (the sh monitor's cleanup took the agent down). No `advance` ran: state sha unchanged, launches stayed at 1. The pid files still recorded the dead 3070616/3070617 | ✓ (the guidance worked for this shape) |
| **Dead arm:** `resume --phase 77` over the dead leftover | rc=0, relaunched. State sha `ac6950f2…` → `2e4363ee…`; monitor_pid 3070616 → 3070842 (alive); agent 3070617 → 3070843 (alive); stage_launched 1 → 2; stopped=False; 1 fake agent | ✓ control |
| **999.139 residue:** unloadable state, dead agent pid, live stand-in monitor | resume: rc=1, "state JSON failed", state unchanged, 0 launches, no lock. start --force: rc=0, launched monitor 3072055 and agent 3072056 beside the live stand-in, state overwritten | resume ✓ / start open (operator-deferred) |
| Mutant: resume guard call deleted (temporary `git worktree` at HEAD, reflinked target dir) | `start_lock_e2e`: 10 passed / 2 failed, exactly the two resume refusal arms, each with its named reason at `:745`. The mutant launched a stage | ✓ discriminates |
| HEAD e2e | `start_lock_e2e` 12/0, `auto_chain_leak_repair_e2e` 2/0, `stop_e2e` 22/0, `gate_wedge_e2e` 5/0, `gate_sweep_e2e` 4/0, `recover_clean_e2e` 11/0 | ✓ |

Cleanup was checked after each run. `kill -0` on every recorded pid (3070616, 3070617, 3070842, 3070843, 3072055, 3072056 and the stand-in 3072001) found none alive, and `pgrep` for the fake-agent sleeps and the scratch roots found nothing. The scratch worktree was removed with `git worktree remove` and the reflinked target dir deleted; `git worktree list` no longer shows it. One harness defect: the live probe's final `pkill -f "$P"` matched the probe script's own path and killed the script, which exited 144. This happened after every probed process had already been signalled, and every result line above was printed before it.

### What this evidence does NOT establish

- **One launch shape.** Both probes use Legacy supervise with an `sh -c` monitor. The pipe-owning (`__monitor`) launch, which is the default Claude path, was not probed live. It uses the same guard and liveness function, but no real pipe-owning run was refused here.
- **One run per arm.** Each arm ran once, so this bounds nothing about races. The refusal is pid-only (Option A). A recycled pid gives a false refusal, and the costly repair for that is backlog 999.143.
- **The dead arm's guidance check covers one shape only.** SIGTERM to the sh monitor ended the agent too and ran no `advance`. The "monitor defers SIGTERM while an `advance` child runs" branch of the guidance was not exercised.
- **The refused resume's downstream effects on hints (999.142)** are reasoned from source, not exercised. `status`, doctor and the gate verbs can still name `devflow resume` in states where it now refuses.
- **The full suite count** (1448/0) is the orchestrator's; the verifier ran six e2e files, not the workspace.
- **The RED commit's own failure** was not replayed. Discrimination is shown by the verifier's independent mutant.

### Requirements Coverage

| Requirement | Source Plans | Status | Evidence |
|-------------|--------------|--------|----------|
| SURV-01 | 48-01, 48-10, 48-11, 48-12, 48-19, 48-20 | ✓ SATISFIED (test arm; field arm is Phase 49's) | Criteria 1 and 2. 999.139 on `start` is operator-deferred |
| SURV-02 | 48-01, 48-10, 48-12, 48-13, 48-14, 48-16, 48-18, 48-19 | ✓ SATISFIED | Criteria 3 and 4 |
| CHKPT-01 | 48-01, 48-02 | ✓ SATISFIED | Criterion 5 |
| CHKPT-02 | 48-01, 48-07, 48-15 | ✓ SATISFIED | Criterion 6 |
| TEST-01 | 48-01, 48-03..48-06, 48-08, 48-09, 48-17 | ✓ SATISFIED (structural) | Criterion 7 |

All 5 IDs are claimed by at least one plan, and none is orphaned. REQUIREMENTS.md maps exactly these five to Phase 48 and now marks all five Complete.

### Deferred backlog items against the criteria

| Item | Relation | Status |
|------|----------|--------|
| 999.136: sweep deletes a live run in the agent-exit → advance window | Undermines criterion 2 as written | Operator-deferred (48-19 rulings); unchanged |
| 999.138: `stop` cannot end a lock-free run | Outside the criteria | Unchanged. The resume refusal now tells the truth about it |
| 999.139: unloadable state hides a live monitor | Undermines criterion 2 as written, for `start` only | Operator-deferred ("stays in the backlog", 6d31299). Residue reproduced with a stand-in; covered for `resume` |
| 999.142: hints prescribe `resume` where it refuses | Next to criterion 3 | Operator decision option (b), 2026-09-23 |
| 999.143: a recycled pid leaves `resume` no work-preserving exit | Cost of Option A | Operator-deferred 2026-09-23 |
| 999.141: e2e tests write the real cache | Outside TEST-01 as written | Advisory |

### Probe Execution

No `scripts/*/tests/probe-*.sh` exists, and no PLAN declares one, so Step 7c does not apply. The verifier's ad-hoc probes are recorded above.

### Anti-Patterns Found

No TBD, FIXME, XXX, TODO, HACK, `todo!` or `unimplemented!` in lines added to `crates/` since `1590100`. The search was checked against a known-positive sample line, which it matched. No `set_var` or `remove_var` was added in tests.

| File | Pattern | Severity | Impact |
|------|---------|----------|--------|
| `crates/devflow-cli/src/commands.rs` `resume_live_run_refusal` | Near-duplicate of `start_live_run_refusal` (about 40 lines of parallel text) | ℹ️ Info | A deliberate choice to keep the start text byte-identical; the texts can drift apart |

### Gaps Summary

There are no open gaps. The `resume` half of criterion 2 is closed and was verified three ways: a real-launch probe with a refusal arm and a relaunch control, both with byte comparisons; an independent mutant; and a source-order check. `start` keeps its refusal and its text. One criterion-2 residue remains: 999.139 on `start`, reproduced with a stand-in monitor. It is excluded only through the operator's recorded backlog deferral, which is an inference and not an explicit scope ruling.

---

_Verified: 2026-09-23T23:26:51Z_
_Verifier: Claude (gsd-verifier)_
