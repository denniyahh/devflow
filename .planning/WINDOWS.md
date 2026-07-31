---
schema_version: 1
open_count: 1
waived_count: 1
fixed_count: 4
total_count: 6
last_updated: 2026-07-31T02:38:49.879Z
---

# Broken Windows Ledger

> Cross-phase defect register. `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 25 | deviation | crates/devflow-cli/src/preflight.rs |  | WR-03 reap fix not wired into two additional launch-driving tests found by 25-16's enumeration (run_preflight_advance_gate_launches_agent_exactly_once, run_preflight_loopback_gate_launches_agent_exactly_once); out of scope for 25-16 (owned by 25-15 in same wave) | fixed |  | 2026-07-28T17:44:48.000Z | 2026-07-28T19:22:19.665Z |
| 2 | 25 | unmet-truth | .planning/phases/25-end-to-end-dogfood-blockers/25-16-SUMMARY.md |  | 25-16 acceptance criterion 'before-delta must be at least 1' not met: every leak measurement (isolated test x6, whole-workspace x1, unfixed and fixed tree) showed delta 0 on this machine/run; fix implemented regardless per review's defensive-teardown reasoning | waived | Mis-specified acceptance criterion, not an unmet defect. The criterion required a pre-fix leaked-wrapper delta of at least 1; every measurement returned 0 (isolated single-test x6, whole-workspace on both the unfixed and fixed tree, plus an independent orchestrator probe of the two known-unfixed preflight sites). Delta 0 is NOT evidence the leak is absent: gsd-code-reviewer and gsd-verifier each independently traced the source and confirmed the spawn is real and unwaited, with no test-mode branch anywhere in launch_stage_inner or monitor::spawn_monitor. The wrapper self-exits in under a millisecond (the stubbed claude binary returns instantly, and the wrapper's trailing devflow advance resolves current_exe to the test binary, which rejects the argument shape and exits), so an after-the-fact process count structurally cannot observe it. The criterion demanded a measurement the mechanism cannot produce. The underlying leak is real and is being closed on its own merits by WINDOWS items 1 and 3 (WR-05, WR-06) via gap-closure round 4 plans 25-17 and 25-18 — waiving this item removes a bogus measurement gate, not a defect. | 2026-07-28T17:44:55.619Z | 2026-07-28T19:09:32.583Z |
| 3 | 25 | deviation | crates/devflow-cli/src/pipeline_launch.rs |  | WR-06: reap_spawned_monitor is a plain trailing statement at both sites 25-16 fixed (pipeline_launch.rs launch_stage_persists_monitor_pid_for_reload, staleness.rs mid_run_stage_transition_does_not_readjudicate_staleness), preceded by 2-4 panicking assertions. An assertion failure unwinds past the reap and drops TempDir anyway, so 25-16's must-have truth 'on every exit path including paths on which a later assertion panics' is NOT satisfied. Confirmed independently by both gsd-code-reviewer and gsd-verifier. Fix: bind an RAII Drop guard before the assertions (sketch in 25-REVIEW.md WR-06). | fixed |  | 2026-07-28T18:27:45.689Z | 2026-07-28T19:22:22.052Z |
| 4 | 25 | deviation | crates/devflow-cli/src/preflight.rs |  | 25-18 verification-step-6 re-derivation found a THIRD live leak site beyond the plan's declared two tests: run_preflight_advance_skips_recheck_on_idempotently_failing_check (Advance arm, unconditional launch_stage_inner, working codex+sh stub on PATH) spawns a real detached monitor wrapper, empirically confirmed (pid captured, unreaped). Fixed in the same plan by binding the identical ReapMonitorOnDrop::after_launch guard; not a pre-existing open defect at time of recording. | fixed |  | 2026-07-28T19:32:10.815Z | 2026-07-28T19:32:20.863Z |
| 5 | 25 | deviation | crates/devflow-cli/src/pipeline_launch.rs |  | SIXTH monitor-wrapper leak site, found by gsd-verifier after round 4 and confirmed by the orchestrator: pipeline_launch.rs::tests::resume_clears_stop_marker_and_advances_past_stop_point stubs a claude binary on PATH and calls resume(root, phase), which reaches launch_stage at pipeline_launch.rs:230 and spawns a real detached monitor wrapper (verifier observed pid 852403 under --nocapture). It binds no ReapMonitorOnDrop guard, so the wrapper outlives the TempDir teardown. resume() is a FOURTH wrapping entry point that neither 25-16's call-site enumeration nor 25-18's three-function reachability grep could see, which falsifies 25-18-SUMMARY.md's claim that no path beyond launch_stage/launch_stage_inner/run_preflight exists. ENUMERATION NOW COMPLETE (orchestrator, transitive sweep over all eight launch-reaching entry points cross-referenced against both agent-stub helpers): exactly 7 tests both reach a launch path and stub an agent binary; 5 are guarded, this one is not, and preflight.rs::run_preflight_loopback_bounds_recursion provably cannot spawn because its recursive run_preflight hits the retry ceiling and aborts, after which launch_stage short-circuits at :190-193 without calling launch_stage_inner. Fix: bind ReapMonitorOnDrop::after_launch(&state) after the resume() call and before the assertions, matching the other five sites. | fixed |  | 2026-07-28T20:01:51.067Z | 2026-07-28T20:13:59.421Z |
| 6 | 28 | unmet-truth | crates/devflow-core/src/agent_result.rs |  | HUMAN_GATE_VALUE ('blocking-human', matched by blocking_human_checkpoint_reported) is an unconfirmed default per 28-PROBE.md DIVERGENT A1 verdict, not an empirically confirmed literal against a live headless checkpoint render | fixed | A1 CLOSED by a live end-to-end run (2026-07-31). A real devflow start drove a synthetic phase declaring a gate="blocking-human" task through DevFlow own monitor. Two results: (1) the value IS blocking-human, so the constant was correct; (2) the RENDERING was not — the executor emits it as a markdown code span, **Gate:** `blocking-human`, and text_reports_human_gate trimmed only * and space, so the leading backtick left take_while yielding an empty token and the reader returned false. Genuine checkpoints fell through to the generic gate. Fixed in b22e6cf by adding the backtick to both trim sets, with three regression tests built from the verbatim capture (confirmed RED first). Retested live: the checkpoint now routes into auto-decide, emits exactly one checkpoint_auto_decided event carrying a real session_id, relaunches via --resume, and the agent resolves it; zero generic gate fires. Root cause worth keeping: RESEARCH derived the literal by reading the EMITTING source, which gave the value but not the rendering. | 2026-07-31T02:38:49.879Z | 2026-07-31T08:30:00.000Z |

````json
[
  {
    "id": 1,
    "kind": "deviation",
    "phase": "25",
    "file": "crates/devflow-cli/src/preflight.rs",
    "line": null,
    "description": "WR-03 reap fix not wired into two additional launch-driving tests found by 25-16's enumeration (run_preflight_advance_gate_launches_agent_exactly_once, run_preflight_loopback_gate_launches_agent_exactly_once); out of scope for 25-16 (owned by 25-15 in same wave)",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-07-28T17:44:48.000Z",
    "resolved_at": "2026-07-28T19:22:19.665Z"
  },
  {
    "id": 2,
    "kind": "unmet-truth",
    "phase": "25",
    "file": ".planning/phases/25-end-to-end-dogfood-blockers/25-16-SUMMARY.md",
    "line": null,
    "description": "25-16 acceptance criterion 'before-delta must be at least 1' not met: every leak measurement (isolated test x6, whole-workspace x1, unfixed and fixed tree) showed delta 0 on this machine/run; fix implemented regardless per review's defensive-teardown reasoning",
    "status": "waived",
    "reason": "Mis-specified acceptance criterion, not an unmet defect. The criterion required a pre-fix leaked-wrapper delta of at least 1; every measurement returned 0 (isolated single-test x6, whole-workspace on both the unfixed and fixed tree, plus an independent orchestrator probe of the two known-unfixed preflight sites). Delta 0 is NOT evidence the leak is absent: gsd-code-reviewer and gsd-verifier each independently traced the source and confirmed the spawn is real and unwaited, with no test-mode branch anywhere in launch_stage_inner or monitor::spawn_monitor. The wrapper self-exits in under a millisecond (the stubbed claude binary returns instantly, and the wrapper's trailing devflow advance resolves current_exe to the test binary, which rejects the argument shape and exits), so an after-the-fact process count structurally cannot observe it. The criterion demanded a measurement the mechanism cannot produce. The underlying leak is real and is being closed on its own merits by WINDOWS items 1 and 3 (WR-05, WR-06) via gap-closure round 4 plans 25-17 and 25-18 — waiving this item removes a bogus measurement gate, not a defect.",
    "recorded_at": "2026-07-28T17:44:55.619Z",
    "resolved_at": "2026-07-28T19:09:32.583Z"
  },
  {
    "id": 3,
    "kind": "deviation",
    "phase": "25",
    "file": "crates/devflow-cli/src/pipeline_launch.rs",
    "line": null,
    "description": "WR-06: reap_spawned_monitor is a plain trailing statement at both sites 25-16 fixed (pipeline_launch.rs launch_stage_persists_monitor_pid_for_reload, staleness.rs mid_run_stage_transition_does_not_readjudicate_staleness), preceded by 2-4 panicking assertions. An assertion failure unwinds past the reap and drops TempDir anyway, so 25-16's must-have truth 'on every exit path including paths on which a later assertion panics' is NOT satisfied. Confirmed independently by both gsd-code-reviewer and gsd-verifier. Fix: bind an RAII Drop guard before the assertions (sketch in 25-REVIEW.md WR-06).",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-07-28T18:27:45.689Z",
    "resolved_at": "2026-07-28T19:22:22.052Z"
  },
  {
    "id": 4,
    "kind": "deviation",
    "phase": "25",
    "file": "crates/devflow-cli/src/preflight.rs",
    "line": null,
    "description": "25-18 verification-step-6 re-derivation found a THIRD live leak site beyond the plan's declared two tests: run_preflight_advance_skips_recheck_on_idempotently_failing_check (Advance arm, unconditional launch_stage_inner, working codex+sh stub on PATH) spawns a real detached monitor wrapper, empirically confirmed (pid captured, unreaped). Fixed in the same plan by binding the identical ReapMonitorOnDrop::after_launch guard; not a pre-existing open defect at time of recording.",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-07-28T19:32:10.815Z",
    "resolved_at": "2026-07-28T19:32:20.863Z"
  },
  {
    "id": 5,
    "kind": "deviation",
    "phase": "25",
    "file": "crates/devflow-cli/src/pipeline_launch.rs",
    "line": null,
    "description": "SIXTH monitor-wrapper leak site, found by gsd-verifier after round 4 and confirmed by the orchestrator: pipeline_launch.rs::tests::resume_clears_stop_marker_and_advances_past_stop_point stubs a claude binary on PATH and calls resume(root, phase), which reaches launch_stage at pipeline_launch.rs:230 and spawns a real detached monitor wrapper (verifier observed pid 852403 under --nocapture). It binds no ReapMonitorOnDrop guard, so the wrapper outlives the TempDir teardown. resume() is a FOURTH wrapping entry point that neither 25-16's call-site enumeration nor 25-18's three-function reachability grep could see, which falsifies 25-18-SUMMARY.md's claim that no path beyond launch_stage/launch_stage_inner/run_preflight exists. ENUMERATION NOW COMPLETE (orchestrator, transitive sweep over all eight launch-reaching entry points cross-referenced against both agent-stub helpers): exactly 7 tests both reach a launch path and stub an agent binary; 5 are guarded, this one is not, and preflight.rs::run_preflight_loopback_bounds_recursion provably cannot spawn because its recursive run_preflight hits the retry ceiling and aborts, after which launch_stage short-circuits at :190-193 without calling launch_stage_inner. Fix: bind ReapMonitorOnDrop::after_launch(&state) after the resume() call and before the assertions, matching the other five sites.",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-07-28T20:01:51.067Z",
    "resolved_at": "2026-07-28T20:13:59.421Z"
  },
  {
    "id": 6,
    "kind": "unmet-truth",
    "phase": "28",
    "file": "crates/devflow-core/src/agent_result.rs",
    "line": null,
    "description": "HUMAN_GATE_VALUE ('blocking-human', matched by blocking_human_checkpoint_reported) is an unconfirmed default per 28-PROBE.md DIVERGENT A1 verdict, not an empirically confirmed literal against a live headless checkpoint render",
    "status": "fixed",
    "reason": "A1 CLOSED by a live end-to-end run (2026-07-31). A real devflow start drove a synthetic phase declaring a gate=\"blocking-human\" task through DevFlow own monitor. Two results: (1) the value IS blocking-human, so the constant was correct; (2) the RENDERING was not — the executor emits it as a markdown code span, **Gate:** `blocking-human`, and text_reports_human_gate trimmed only * and space, so the leading backtick left take_while yielding an empty token and the reader returned false. Genuine checkpoints fell through to the generic gate. Fixed in b22e6cf by adding the backtick to both trim sets, with three regression tests built from the verbatim capture (confirmed RED first). Retested live: the checkpoint now routes into auto-decide, emits exactly one checkpoint_auto_decided event carrying a real session_id, relaunches via --resume, and the agent resolves it; zero generic gate fires. Root cause worth keeping: RESEARCH derived the literal by reading the EMITTING source, which gave the value but not the rendering.",
    "recorded_at": "2026-07-31T02:38:49.879Z",
    "resolved_at": "2026-07-31T08:30:00.000Z"
  }
]
````
