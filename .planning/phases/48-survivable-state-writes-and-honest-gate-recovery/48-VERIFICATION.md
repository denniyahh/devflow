---
phase: 48-survivable-state-writes-and-honest-gate-recovery
verified: 2026-09-23T17:00:00Z
status: gaps_found
score: 6/7 must-haves verified
covered_files: [".planning/REQUIREMENTS.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-01-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-01-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-02-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-02-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-03-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-03-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-04-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-04-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-05-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-05-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-06-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-06-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-07-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-07-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-08-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-08-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-09-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-09-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-10-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-10-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-11-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-11-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-12-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-12-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-13-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-13-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-14-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-14-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-15-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-15-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-16-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-16-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-17-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-17-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-18-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-18-SUMMARY.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-19-PLAN.md",".planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-19-SUMMARY.md","clippy.toml","crates/devflow-cli/src/commands.rs","crates/devflow-cli/src/main.rs","crates/devflow-cli/src/pipeline_gate.rs","crates/devflow-cli/src/pipeline_launch.rs","crates/devflow-cli/src/pipeline_outcomes.rs","crates/devflow-cli/src/preflight.rs","crates/devflow-cli/src/staleness.rs","crates/devflow-cli/src/test_support.rs","crates/devflow-cli/tests/gate_sweep_e2e.rs","crates/devflow-cli/tests/gate_wedge_e2e.rs","crates/devflow-cli/tests/gitignore_coverage.rs","crates/devflow-cli/tests/plan_bashism_scanner.rs","crates/devflow-cli/tests/recover_clean_e2e.rs","crates/devflow-cli/tests/start_lock_e2e.rs","crates/devflow-cli/tests/stop_e2e.rs","crates/devflow-core/src/agent.rs","crates/devflow-core/src/agent_result.rs","crates/devflow-core/src/agents/opencode.rs","crates/devflow-core/src/agents/pi.rs","crates/devflow-core/src/config.rs","crates/devflow-core/src/doc_check.rs","crates/devflow-core/src/gates.rs","crates/devflow-core/src/lock.rs","crates/devflow-core/src/monitor.rs","crates/devflow-core/src/recover.rs","crates/devflow-core/src/ship.rs","crates/devflow-core/src/state.rs","crates/devflow-core/src/test_support.rs","crates/devflow-core/src/verify.rs","crates/devflow-core/src/workflow.rs","scripts/lint-plan-bashisms.sh"]
covered_digest: "v1:sha256:407a3085884ddf3f615ce16931a41ec4917a5744944be941d7d785b297c02ac1"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 6/7
  gaps_closed:
    - "Criterion 4: #200 now pinned on the foreground `start` shape in both directions (48-18); verifier re-ran both arms and an independent lock-drop mutant"
    - "Human verification item 1 (criterion 2 scope): resolved by the operator rulings recorded in 48-19-SUMMARY (Gap B ruled a gap and closed for `start`; 999.136 deferred). The ruling text was written by the planner and copied by the executor; no operator-authored record was found"
  gaps_remaining: []
  regressions: []
  new_gaps:
    - "Criterion 2: `devflow resume` over a live lock-free run is neither excluded nor refused (backlog 999.140, WR-04). Reproduced by the verifier at HEAD d5376fa. Same class as Gap B, which the operator ruled a criterion-2 gap"
gaps:
  - truth: "Criterion 2: after the fix, a second writer for the same phase is excluded or refused loudly and never silently overwrites; the single-writer control exercising the same path still round-trips"
    status: partial
    reason: >-
      48-19 closed this for `start`. The sibling launch verb `resume` has no live-run check.
      A verifier probe against the HEAD binary made a real Legacy launch: `start --mode supervise`
      recorded monitor pid M1 and agent pid A1, and no lock was held while the agent ran.
      A second `start --force` over that run was refused, and the state bytes did not change.
      That is the control. `resume --phase N` over the same live run exited 0.
      It rewrote the state (monitor_pid M1→M2) and overwrote the agent pid file (A1→A2), while M1 and A1 were still alive.
      Two fake agents were then running for one phase.
      The operator ruled this exact class a criterion-2 gap for `start` (Gap B). 48-SECURITY.md round 7 records
      999.140 as "deferred, not accepted", with no operator ruling. The same guard also fails open when the state
      file exists but will not load (999.139). That is confirmed by reading `refuse_start_over_a_live_run`
      (`loaded.ok()` → no monitor pid) but not reproduced.
    artifacts:
      - path: "crates/devflow-cli/src/pipeline_launch.rs"
        issue: "`resume` (line 1408) takes the lock, loads state and calls `launch_stage` with no `agent::agent_running` check on the recorded monitor pid or agent pid"
      - path: "crates/devflow-cli/src/commands.rs"
        issue: "`refuse_start_over_a_live_run` (line 2426) reads monitor_pid only from a state that loads, so an unloadable state hides a live monitor (999.139, reasoned only)"
    missing:
      - "Either (a) the same live-run refusal in `resume` under its lock, with a failing-first e2e test: `resume` against a live recorded monitor/agent, plus the dead-leftover control; or (b) an explicit operator override scoping criterion 2 to `start` and deferring `resume` to 999.140 (template in the Gaps Summary)"
      - "Operator decision on 999.139 (fail closed on an unloadable state, or accept it as a limit)"
deferred: []
advisory:
  - finding: "The e2e proceed arms write to the operator's real ~/.cache/devflow/roots and do not reap the monitors they launch (999.141). The verifier's own targeted runs in this session added about 11 entries there (phases 91, 92, 97, 98 and others)"
    category: other
    reason: "Test hermeticity. It does not fall under TEST-01 as written, which covers process-global PATH mutation and the lint. It would be resolved by a per-child DEVFLOW_CACHE_DIR or HOME in the e2e helpers"
    evidence_status: "cache entry count observed (14,816 total; 11 newer than 12:40 local today). No leaked /tmp/.tmp* devflow process was found after the runs"
human_verification: []
---

# Phase 48: Survivable State Writes and Honest Gate Recovery: Verification Report

**Phase Goal:** A second writer cannot silently erase another's state update. A gate whose consumer is gone tells the operator the repair that actually works, instead of asserting a waiter that does not exist. Preflight and resume agree on where blocking-human gates exist. Checkpoints added after preflight are re-scanned. The test-suite PATH race is isolated.
**Verified:** 2026-09-23 at HEAD `d5376fa` (worktree `.worktrees/phase-48`, branch `feature/phase-48`)
**Status:** gaps_found. The criterion 4 gap is closed. A new criterion 2 gap was found: `resume`, reproduced.
**Re-verification:** Yes. Previous: gaps_found 6/7 at `a692468`.

## Goal Achievement

### Observable Truths (ROADMAP Phase 48 success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A second-writer test fails pre-fix; a single-writer control round-trips | ✓ VERIFIED (regression check) | RED was reproduced by the previous verifier against pre-fix `167363f`. Since `a692468`, production changed only in `commands.rs` (new guard and refusal text) and in two `#[non_exhaustive]` attributes (`recover.rs`, `workflow.rs`); `git diff a692468..HEAD`. This session at HEAD: `stop_e2e` 22 passed / 0 failed; `start_lock_e2e` 7 passed / 0 failed, including `start_refuses_while_another_process_holds_the_phase_lock` and the control `start_proceeds_when_no_process_holds_the_phase_lock`. |
| 2 | After the fix, a second writer for the same phase is excluded or refused loudly and never silently overwrites | ✗ FAILED (partial) | **`start`: fixed and verified.** The guard at `commands.rs:370` sits between the `_phase_lock` binding (`:360`) and `Gates::cleanup` (`:378`). The four 48-19 tests pass. Verifier mutant M1 (guard call deleted) turned exactly the two refusal arms red with their named reasons ("start must refuse a phase whose recorded monitor/agent is alive"). The dead-leftover and no-state controls stayed green. A real-launch probe also confirmed the refusal: `start --force` over a live Legacy run gave rc=1 and left the state unchanged. **`resume`: not fixed.** The same probe then ran `resume --phase 77`: rc=0, monitor_pid and agent pid file overwritten, old monitor and agent still alive, two agents running. See Gaps. |
| 3 | approve/reject/stop/sweep check for a live waiter; with none they say so, write no non-Ship answer, and name the repair; no `rm -f`; resume never reads a pending answer | ✓ VERIFIED (regression check) | The code on these paths did not change since `a692468`: the only `commands.rs` additions are the guard and refusal functions (`git diff`, comment-stripped). `stop_e2e` passed 22/22 at HEAD. `dry_run_sweep_reports_no_waiter_gates_as_left_alone` and both start wedge arms passed; they exercise the "no confirmed waiter … `devflow recover --clean --phase N`" refusal with no response file. The previous report's D-05 caveat (Unconfirmable holders still get an answer from approve/reject) and AR-48-04 still stand. |
| 4 | #200 wedge reproducible on demand; shape pinned both ways (interrupted `start` wedges, left-alone self-resolves) | ✓ VERIFIED | `gate_wedge_e2e.rs` now drives a real `devflow start --mode auto`, parked at its Define preflight gate. Both start arms assert that `lock::holder_identity` equals the start child's pid. **Interrupted** (`start_wedge_arm_…`): after the kill, reject exits non-zero with "no confirmed waiter" and names `devflow recover --clean --phase 93`. No response file is written. `recover --clean` removes the state, gate files, the planted state and gate temps, and lock-93, and each of those is checked to exist first (WR-06 fix `a33060f`). **Left alone** (`start_self_resolving_arm_…`): reject_wording=live, start exits 0, and the state and gate are cleared. `pickup_ms=993` this run. The file ran 5 passed / 0 failed. **Negative control run by the verifier:** mutant M2 (`let _phase_lock` → `let _`) turned both start arms red and left both advance arms and the sweep test green. `commands.rs` was restored byte-identical (`git diff --quiet` exit 0), and a rebuilt HEAD went back to 5/5 and 7/7. |
| 5 | CHKPT-01: one shared checkpoint parser; a marker off a task-opening line neither blocks preflight nor arms resume | ✓ VERIFIED (regression check) | `verify.rs` and `preflight.rs` are unchanged since `a692468`, which the previous report verified by test. Not re-run this session. |
| 6 | CHKPT-02: checkpoints added or changed after Code's preflight are re-scanned before the resume decision | ✓ VERIFIED (regression check) | The re-scan code in `pipeline_launch.rs` is unchanged since `a692468`: the file has no diff in that range. Not re-run this session. |
| 7 | TEST-01: process-global PATH mutations isolated or converted to per-Command scoping | ✓ VERIFIED (structural) | `git grep 'set_var("PATH"\|remove_var("PATH"' -- crates` finds 1 line at HEAD, a doc comment (`test_support.rs:302`). `clippy.toml` still disallows `std::env::set_var` and `std::env::remove_var`. The new 48-18 and 48-19 helpers set PATH through `Command::env` only. The lint's discriminating control was run by the previous verifier and was not repeated. |

**Score:** 6/7 truths verified (0 present-but-behavior-unverified).

### What each piece of evidence does NOT establish

- **Criterion 2 (start):** the e2e refusal arms fabricate a live run: a test-owned `sleep` is recorded as the monitor or agent. The verifier's scratch probe adds one real Legacy launch, supervise mode, a single run. It does not cover the pipe-owning launch or a run parked mid-`advance`. Liveness is pid-only, and a recycled pid gives a false refusal (accepted, Option A). The refusal's recovery guidance (ps identity, SIGTERM order, the `recover --clean` sequence) is asserted as text only. No test and no probe here followed it end to end.
- **Criterion 2 (resume):** the probe ran once, supervise mode, Legacy launch. It shows that `resume` overwrites and relaunches. It does not show the downstream effects: which monitor's `advance` wins, and whether a state update is later lost.
- **Criterion 4:** the start arms pin one parked shape: Define preflight, auto mode, no `.planning/config.json`. `pickup_ms` is pinned by the gate poll's first 1 s backoff step and measures nothing about the wedge. The `no_holder_after_exit` wording was accepted but never observed. Each arm was run 3 times this session (full file, M2 control, post-restore). That bounds nothing about flakiness under load.
- **Criteria 5–7:** these are regression checks by diff, plus the previous session's test evidence. They were not re-executed here.
- **Full suite:** the verifier did not run it. The orchestrator reports the pinned container gate as all OK at `d5376fa` (1443 passed, 0 failed, 34 suites). 48-VALIDATION.md's audit cites the same count at `a33060f`. Neither was reproduced here.

### Human verification item 1 (previous report): disposition

The item asked whether criterion 2 covers windows in which no process holds `lock-NN` during a live run. 48-19-SUMMARY § "Operator rulings (2026-09-22)" states the ruling:
- Gap B (a second `start` over a live run) is ruled a gap, and 48-19 closes it for `start`.
- Liveness is Option A: "refuse when the recorded agent pid or monitor pid is alive".
- The no-state carve-out is approved.
- The agent-exit → `advance` window stays backlog 999.136, operator-deferred.

**Provenance:** the wording was written by the planner (48-19-PLAN context and Known limits) and copied by the executor, whose summary says "Recorded as the plan records them". 48-SECURITY.md AR-48-06 notes the same: "the operator's wording appears only in planner/executor-written files". No operator-authored record was found. The only other hits for "Gap B" and "Option A" are these files and STATE.md, which refers to an unrelated Phase 47 decision. On that basis the item is closed.

The ruling picked "open a gap" rather than "scope criterion 2 to the lock-held inventory". It did not address `resume`, because WR-04 surfaced a day later. That is why `resume` is scored as a gap and not absorbed.

### Deferred backlog items against the criteria as written

| Item | Relation to the criteria | Why |
|------|--------------------------|-----|
| **999.136**: the recover sweep deletes a live run between agent exit and `advance` | **Would undermine criterion 2 as written; excluded by the recorded operator deferral** | The sweep is a second writer that deletes a live run's state and gates. That contradicts "never silently overwrites" and the goal's first clause. The operator explicitly kept it out (48-19 rulings; 48-SECURITY AR-48-07 lists it as operator-deferred). It is reasoned from source and was not reproduced here. The 48-19 guard does not close it. |
| **999.138**: `stop` cannot end a lock-free run | **Outside the criteria** | Criterion 3 governs `stop` when it answers a gate. In this window no gate is open and no answer is written. `stop` takes the lock before it writes, so it is not a lost update (criterion 2). **Reproduced by the verifier:** `stop` exited 0 and printed "no lock held … nothing is running `advance()`". The monitor and agent were still alive, and the state was set to `stopped=True`. It is an honesty defect in the 999.133 class, next to the phase goal's "repair that actually works" wording but not covered by any criterion. The 48-19 refusal text works around it. |
| **999.139**: an unloadable state file hides a live monitor from `start`'s guard | **Undermines criterion 2 as written (narrow)** | When the state exists but does not load, `refuse_start_over_a_live_run` never sees `monitor_pid`. With the agent dead and the monitor alive, `start` proceeds over a live run. The code path is confirmed by reading `commands.rs:2426-2434`. The trigger (serde or version skew, EACCES) was not reproduced. 48-SECURITY round 7 lists it as "deferred, not accepted" with no operator ruling. It is folded into the criterion 2 gap as a secondary item. |
| **999.140**: `resume` has no live-run check | **Undermines criterion 2 as written (reproduced)** | See truth 2 and Gaps. This is the same class as Gap B, which the operator ruled a criterion-2 gap. There is no operator ruling accepting it. |
| **999.141**: e2e tests write the real `~/.cache/devflow` and leak monitors | **Outside criterion 7 as written** | TEST-01 and criterion 7 cover process-global PATH mutation, per-Command scoping and the lint. 999.141 concerns an inherited, un-isolated `HOME`/cache dir and unreaped monitors, which is test hermeticity, not the PATH race. It does undermine the operator's global test-isolation rule. This verification's own runs added about 11 entries to the real cache (Advisory). |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/tests/gate_wedge_e2e.rs` | start-driven #200 arms, advance arms renamed | ✓ VERIFIED | `fn start_self_resolving_arm_live_start_consumes_the_define_rejection()` present. 5 tests listed and all 5 passed. No `wedge_arm_killed_start` / `self_resolving_arm_live_start` names remain. |
| `crates/devflow-cli/src/commands.rs` | `refuse_start_over_a_live_run` under the lock | ✓ VERIFIED (for `start`) | Guard at `:370`; helper at `:2426`; refusal text at `:2449` |
| `crates/devflow-cli/tests/start_lock_e2e.rs` | two refusal arms, dead-leftover control, no-state carve-out | ✓ VERIFIED | 7 listed and 7 passed. The 22+ protective fragments and the 3 forbidden stale phrasings are asserted (`bb639f5`) |
| `crates/devflow-cli/src/pipeline_launch.rs` `resume` | (criterion 2) a sibling launch verb must also refuse a live run | ✗ MISSING guard | See Gaps |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| `gate_wedge_e2e.rs` start arms | `commands::start` lock | `lock::holder_identity` == start child pid | WIRED (M2 mutant breaks it) |
| `commands::start` | `agent::agent_running` | `refuse_start_over_a_live_run` on `monitor_pid` + `agent_pid_from_file` | WIRED (M1 mutant breaks it) |
| guard call | between `_phase_lock` and `Gates::cleanup` | line order 360 < 370 < 378 | WIRED |
| `resume` | live-run check | none | NOT_WIRED (gap) |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Gap-closure tests exist | `cargo test -p devflow --test gate_wedge_e2e -- --list`; same for `start_lock_e2e` | 5 and 7 tests listed | ✓ PASS |
| Criterion 4 and criterion 2 (start) green at HEAD | `cargo test -p devflow --test gate_wedge_e2e -- --nocapture`; `cargo test -p devflow --test start_lock_e2e` | 5 passed / 0 failed, `reject_wording=live`, `pickup_ms=993`; 7 passed / 0 failed | ✓ PASS |
| M1: guard removed → refusal arms fail | transient `sed` removal of the guard call, backup plus EXIT-trap restore | 5 passed / 2 failed: exactly the monitor and agent arms, each with its named reason (1 hit each); controls ok | ✓ PASS (discriminates) |
| M2: start lock dropped → start pin fails | transient `let _phase_lock` → `let _` | 3 passed / 2 failed: both start arms; "the parked start must hold its own phase lock" appears 2 times; advance arms ok | ✓ PASS (discriminates) |
| Source restored after mutants | `git diff --quiet -- commands.rs`, then rebuild and re-run both files | exit 0; 5/5 and 7/7 | ✓ PASS |
| Criterion 3 regression | `cargo test -p devflow --test stop_e2e` | 22 passed / 0 failed | ✓ PASS |
| Real Legacy launch: second `start` refused | scratch probe `/tmp/v48/probe.sh`, HEAD binary, scratch `DEVFLOW_CACHE_DIR` | first start rc=0 (monitor 2376759, agent 2376761, no lock-77 held); `start --force` rc=1 "refusing to start; nothing was written", state unchanged | ✓ PASS |
| Real Legacy launch: `resume` over the same live run | same probe | rc=0; monitor_pid 2376759→2376782, agent pid 2376761→2376784; old monitor and agent alive; 2 fake agents running | ✗ FAIL (criterion 2) |
| 999.138 shape | scratch probe `/tmp/v48/p2.sh` | `stop` rc=0, "no lock held … nothing is running `advance()`", monitor and agent alive, `stopped=True` | reproduced (outside the criteria) |
| TEST-01 structural | `git grep` for PATH set_var/remove_var | 1 hit, a doc comment | ✓ PASS |

All probe processes were confirmed gone afterwards by `ps -p` on each recorded pid and `pgrep` for the fake agent.

### Probe Execution

No `scripts/*/tests/probe-*.sh` exist, and no PLAN declares one, so Step 7c does not apply.

### Requirements Coverage

| Requirement | Source Plans | Status | Evidence |
|-------------|-------------|--------|----------|
| SURV-01 | 48-01, 48-10, 48-11, 48-12, 48-19 | ⚠️ PARTIAL | The test arm (criterion 1) is satisfied. Criterion 2 holds for `start` but not for `resume`. The field arm is owned by Phase 49 criterion 5 |
| SURV-02 | 48-01, 48-10, 48-12, 48-13, 48-14, 48-16, 48-18, 48-19 | ✓ SATISFIED | Criteria 3 and 4 are verified, including the interrupted-`start` shape REQUIREMENTS names |
| CHKPT-01 | 48-01, 48-02 | ✓ SATISFIED | Criterion 5 (regression by diff) |
| CHKPT-02 | 48-01, 48-07, 48-15 | ✓ SATISFIED | Criterion 6 (regression by diff) |
| TEST-01 | 48-01, 48-03..48-06, 48-08, 48-09, 48-17 | ✓ SATISFIED (structural) | Criterion 7 |

All five IDs are claimed by at least one plan, and none is orphaned. REQUIREMENTS.md still marks SURV-01, SURV-02, CHKPT-02 and TEST-01 as Pending. That is bookkeeping for phase completion.

### Anti-Patterns Found

Lines added in `crates/` since `a692468`: no TBD, FIXME, XXX, TODO, HACK, `todo!` or `unimplemented!`. The pattern was checked against a known-positive sample and matched it.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/devflow-cli/tests/start_lock_e2e.rs` | fragment list in `assert_start_refuses_live_run` | Role (`monitor pid N` vs `agent pid N`) never asserted (48-REVIEW IN-02, not actioned) | ℹ️ Info | Swapping the roles in the guard would pass both arms |

### Advisory (New Scope, Unevidenced)

| # | Finding | Category | Why Advisory |
|---|---------|----------|--------------|
| 1 | The e2e proceed arms write to the real `~/.cache/devflow/roots` (999.141); this verification added about 11 entries | other | Outside TEST-01 as written; hermeticity only |

### Gaps Summary

There is one gap. Criterion 2 holds for `start` and not for `resume`.

48-18 fully closes the criterion-4 gap from the previous report. The pin is on the real `start` shape, and an independent mutant shows it discriminates. 48-19 closes Gap B for `start`: an independent mutant and a real-launch probe both confirm the refusal.

But the operator's Gap B ruling established that a second launch verb over a live, lock-free run is a criterion-2 violation. `resume` is such a verb, and it has no check. The verifier reproduced it at HEAD: `resume` over a live Legacy run exited 0, rewrote the state's `monitor_pid`, overwrote the agent pid file, and started a second agent beside the live one. 48-SECURITY round 7 records 999.140 as "deferred, not accepted", and no operator ruling accepts it. 999.139 is the same class, a narrow fail-open in the new guard, and is reasoned only.

**Fix shape:** move the guard into a shared helper and call it in `resume` right after its lock. Write a failing-first e2e arm for `resume` against a live recorded monitor or agent, plus the dead-leftover control. Separately, decide whether 999.139 fails closed.

**If the operator instead accepts `start`-only scope for this phase,** add an override:

```yaml
overrides:
  - must_have: "Criterion 2: after the fix, a second writer for the same phase is excluded or refused loudly and never silently overwrites"
    reason: "Closed for `start` (48-19). `resume` over a live lock-free run (999.140) and an unloadable state hiding a live monitor (999.139) are deferred to backlog by operator decision"
    accepted_by: "{operator}"
    accepted_at: "{ISO timestamp}"
```

---

_Verified: 2026-09-23_
_Verifier: Claude (gsd-verifier)_
