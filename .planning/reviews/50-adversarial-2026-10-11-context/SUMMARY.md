# Phase 50 CONTEXT — adversarial review, 2026-10-11

**Target:** `.planning/phases/50-addressable-monitor-liveness/50-CONTEXT.md` at `5698426` (a `git archive` snapshot).
**Lens:** adversarial. Prompt: `prompt.txt`.

| Lane | Model | Result | Bytes | file:line citations |
|---|---|---|---|---|
| agy | gemini-3.8-flash-high, high effort | delivered, rc 0 | 15,361 | 27 |
| codex | gpt-5.6-terra, high effort, read-only sandbox | delivered, rc 0 | 11,150 | 14 |
| pi | deepseek-flash | delivered, rc 0 | 15,860 | 17 |

No lane dropped. Claude re-verified every finding below against the source before recording it. "Verified" here
means checked this session at HEAD `5698426`.

## Needs an operator decision

- **O-1. The 2.x break wedges a live 2.x run** (all three lanes). D-14 makes `stop` refuse on a 2.x state. D-15's
  upgrade verb refuses while a recorded 2.x pid is alive. A live 2.x run therefore has no way out: it cannot be stopped
  or upgraded. A recycled 2.x `monitor_pid` wedges the phase permanently, because 2.x records no start time for it.
  Verified: `state.rs:288` (a bare `monitor_pid`), and `agent_running` is a liveness check with no identity.

## Confirmed — corrections within Claude's remit (applied to CONTEXT as R-1…R-14)

1. **Stage handoff** (agy, pi). The in-process advance spawns the next stage's monitor while the current monitor
   process is still alive and bound. Verified: `monitor.rs:480-482` ("spawns the next monitor itself") and
   `pipeline_launch.rs:979-992` (`run_pipe_owning_monitor`, then `advance_for_stage` in the same process). With
   per-phase socket naming, the next monitor either refuses under the takeover rule or unlinks a live socket.
   → R-1.
2. **Shutdown or TERM during the in-process advance** (agy, codex, pi). `advance_for_stage` takes the per-phase lock
   (`pipeline_launch.rs:1629`) and can wait on a gate (`:1830`) inside the monitor process. The spike's
   `process::exit` on shutdown would leave the lock held and the state half-written. The spike also checks
   `stopping` before it observes the child's exit and never rechecks it (`main.rs:108-124`). → R-2.
3. **The `Legacy` agent has no process group of its own** (pi). The `sh` monitor is spawned with no
   `process_group`/`setsid` (`monitor.rs:571-588`, `hermetic_command`), so `killpg` on a recorded pgid could signal
   the operator's group. → R-3.
4. **The backstop fails when the group leader is dead** (all three). `process_start_time` returns `None` for a reaped
   pid (`agent.rs:214-220`), so "validate the start time" refuses in exactly the case the backstop exists for:
   leader dead, members alive. → R-4.
5. **`sun_path` boundary is off by one** (codex, pi). The spike measured 107 bytes as the longest bindable path,
   with the first failure at 108 (audit `:104`). → R-5.
6. **The probe is not total** (codex, pi). The spike has a fourth `UNKNOWN` outcome (`main.rs:173`, and `:199` in the sweep). → R-6.
7. **The sweep cannot map a socket back to a phase without state** (all three). `registry::cache_dir()` is the
   existing resolver and honours `DEVFLOW_CACHE_DIR` (`registry.rs:53-64`). → R-7.
8. **A boot-id mismatch has no cleanup outcome** (all three). → R-8.
9. **Semver is unaddressed** (agy, pi). `pub monitor_pid` (`state.rs:288`) and
   `pub fn spawn_monitor(...) -> Result<u32, _>` (`monitor.rs:308`) are public items on a published crate. → R-9.
10. **The fault arm skips advance boundaries and does not name the launch shape** (codex). → R-10.
11. **The live run cannot exercise non-Claude parity** (pi). → R-11.
12. **D-09 should target the shared guard** (agy, partly correct). `start` and `resume` already share
    `refuse_launch_over_a_live_run` (`commands.rs:2432-2470`), so fixing the shared helper covers `resume`. Agy's
    claim that `resume` was "omitted" is right about the CONTEXT's wording but wrong about the code. → R-12.
13. **Folded items lost their acceptance tests; 2.x detection has no negative control** (pi). → R-13.
14. **Minor:** "three backlog items" while five are listed; citation drift (`spawn_agent_and_record` is at `:1007`,
    not `:992`; `spawn_monitor` is at `:308`). The CONTEXT also called it unverified that Phase 49 reached the
    `Legacy` relaunch, but `checkpoint_auto_decided` is at
    `49-evidence/attempt-3/collected/devflow/events.jsonl:21` (codex, pi). → R-14.

## Suspected / residual (recorded, not acted on)

- Signal handlers must stay async-signal-safe (pi, labelled speculation). This is folded into R-2 as a constraint.
- Withholding the monitor's own advance does not stop a concurrent foreground `devflow advance` (pi, speculation).
  Residual risk; research checks whether that path is reachable.
- Other 2.x artifacts (gate files, `cron-instructions*.json`) are not versioned (pi). → R-13 states the scope
  explicitly.
- macOS `TMPDIR` length versus the 104-byte limit in tests (agy, speculation). Moot while macOS is unclaimed.

## Disputed

- agy: "remove `monitor_pid` and D-15 can't read it". Partly valid. Nothing in the CONTEXT decided to remove the
  field. R-9 makes keeping it readable explicit.
