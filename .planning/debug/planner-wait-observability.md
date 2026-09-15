---
status: diagnosed
trigger: "GSD planner and revision agents repeatedly exceeded the manual ten-minute wait threshold during the Phase 48 review replan."
created: 2026-09-15
updated: 2026-09-15
---

# Debug: Planner Wait Observability

## Symptoms

- Expected behavior: planning-agent liveness should be distinguishable from a stall, and configured timeouts should bound an unresponsive dispatch.
- Actual behavior: multiple `gsd-planner` and revision spawns remained `running` after at least one ten-minute manual wait interval; independent plan-checker spawns returned after roughly four one-minute waits.
- Error messages: none from the agents; `collaboration.list_agents` reported only `running` while no completion marker was available.
- Timeline: observed throughout the Phase 48 review-replan on 2026-09-15.
- Reproduction: dispatch a typed `gsd-planner` role with `reasoning_effort: xhigh` over the Phase 48 plan set, then poll it through the collaboration API.

## Current Focus

- bug_class: heisenbug-mandelbug (completion latency varies with role workload; no deterministic deadlock or crash trace was captured)
- hypothesis: confirmed — the Codex collaboration adapter accepts neither GSD's configured subagent deadline nor the output-file handle required by the GSD stall watcher, so a long planner run has no automatic deadline or liveness signal.
- test: compared the positive configuration-loading path with the generated Codex dispatch mapping, the planner watcher contract, and the live collaboration capability surface.
- expecting: configuration use in the loader but no timeout/output-file bridge in the adapter; the observation matched.
- candidate_causes:
  - code: Codex Task-to-collaboration conversion maps only spawn and explicit `wait_agent(timeout_ms)` calls; it does not bind `workflow.subagent_timeout` or an output file.
  - data/workload: the live planner corpus is 17 plans / 3,007 lines and planner effort is xhigh, making a long run plausible and exposing the missing observability path.
  - environment: no direct runtime telemetry distinguishes productive model work from a blocked platform task.
- and_gate: yes — an extended planner run exposes the adapter's missing deadline/liveness bridge; the bridge defect alone explains why the configured timeout cannot bound a run, while the workload is a contributing trigger rather than proof of a hang.
- next_action: return diagnosis-only report with implementation-neutral operational recommendations; do not modify source or plans.

## Evidence

- timestamp: 2026-09-15 — `.planning/config.json` sets `workflow.subagent_timeout` to 300000 ms; typed collaboration dispatches still remained running beyond ten minutes.
- timestamp: 2026-09-15 — GSD's planner workflow requires an output-file path for its stall watcher, while the available collaboration spawn result exposes only a task name.
- timestamp: 2026-09-15 — final Phase 48 plan corpus has 17 plans and 3,007 lines; planner effort resolved to xhigh.
- timestamp: 2026-09-15 — `.planning/config.json` directly contains `workflow.subagent_timeout: 300000`, and GSD's configuration loader has four `subagent_timeout` resolution sites.
- timestamp: 2026-09-15 — `plan-phase.md` requires each planner/checker wait to call `gsd_stall_watch` with the real `{outputFile}` from the immediately preceding background-agent result; the helper calls this binding load-bearing.
- timestamp: 2026-09-15 — negative control: `runtime-artifact-conversion.cjs` contains no `subagent_timeout`/`subagentTimeout` reference, while `config-loader.cjs` does contain four; the setting is loaded but not connected to Codex dispatch.
- timestamp: 2026-09-15 — negative control: the Codex dispatch conversion contains no `outputFile`, `output_file`, or `canReadOutputFile` binding. The live `spawn_agent` API likewise exposes no output-file or timeout parameter; it exposes a separate explicit `wait_agent(timeout_ms)` and `interrupt_agent` operation.

## Eliminated

- hypothesis: every agent dispatch is universally slow — plan-checker agents completed within roughly four one-minute waits in this run.

## Resolution

root_cause: The configured 300-second timeout is configuration-only on the Codex collaboration path: the generated Task-to-`spawn_agent` adapter neither passes it to a supervisor nor arranges a deadline-triggered interrupt. The same adapter cannot bind the output-file handle required by `gsd_stall_watch`, leaving only `running` status during a long planner invocation. An xhigh planner over the verified 17-plan / 3,007-line corpus is a contributing condition that makes the gap visible, not evidence that the planner was deadlocked.
fix: Diagnose-only mode — no change applied. Operational direction: make the adapter return durable task telemetry (including an output/log handle or equivalent progress heartbeat), start an elapsed-time supervisor from dispatch using `workflow.subagent_timeout`, and record/interrupt on expiry. Until then, treat manual polling as liveness-unknown rather than a stall verdict.
verification: Confirmed by a positive/negative-control comparison: GSD's loader resolves `subagent_timeout`; its Codex adapter has no timeout binding and no output-file binding; the watcher contract requires that output file. This proves the missing enforcement/observability bridge, but does not establish whether any historical planner was productive, platform-blocked, or deadlocked.
files_changed: []
