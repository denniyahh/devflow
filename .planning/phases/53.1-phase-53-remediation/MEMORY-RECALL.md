# Memory Recall (MemPalace)

_Wing: devflow · Mode: augment · Transport: mcp_
_Hit quality: top hits similarity 0.53–0.66; nonsense-query control 0.20–0.23._

## Prior decisions
- Phase 52 D-15 (CUT-03): a rate limit with no usable fallback stops at a real gate; its context names the reset time and the exact `devflow resume --phase N …` command; `gate approve` relaunches the same stage on the same agent now, `gate abort` ends the run. 999.250 changes the approve-first wording of this gate, not the gate itself. — drawer_devflow_decisions_3d4e5494ab888e8bd19cbed0 (52-CONTEXT.md, 2026-11-08)
- Phase 52 D-14: the rejection note reaches the relaunched agent's prompt verbatim (`fix_prompt` gained a note). D-05 here relies on it. — same drawer
- Phase 21-02: `devflow gate show` prints the full, sanitized gate context (`render_gate_context(ctx, usize::MAX)`); `status` appends the rate-limit reset from `CronInstructions.retry_after`. Relevant to 999.206 (is the reason shown whole) and 999.250 (where the reset is rendered). — drawer_devflow_milestones_f9d01e51c2e9086900a106f6 (21-02-SUMMARY.md)
- Phase 53-03: a failed stage declaring `checkpoint: blocking-human` opens the never-silent stage failure gate (Validate included, no fix loop); the checkpoint relaunch is deleted. 999.249's gate text sits on that gate. — drawer_devflow_milestones_bbf79e2e3eb49b3d43960fac (53-03-SUMMARY.md)
- Phase 51-11: OPERATIONS.md has a "Rate-limit failover" section with the two answers and the `agent_handoff*` events; D-04 doc changes belong there. — drawer_devflow_milestones_acefa41a4c4b4efc7711ae28 (51-11-SUMMARY.md)

## Patterns
- Rate-limit reset and recovery verbs are presentation of already-computed fields; no new rate-limit detection in the gate/status layer. — 21-02 drawers
- Pure, unit-tested render helpers (`recovery_hints`, `render_pending_gate_banner`, `cron_hint_line`) for operator-facing text; test the helper, not the CLI output. — drawer_devflow_planning_a12eedfb4013416f551c7ddd (21-02-PLAN.md)

## Surprises / gotchas
- The post-commit hook touching `.planning/user/errors/state-stale.json` has been noted as "expected noise" in executor summaries since Phase 31 and Phase 50 (50-25-SUMMARY, 31-01-SUMMARY). That is 999.240: executors learned to ignore a dirty tracked file. — drawer_devflow_milestones_dde8718737b32763669c202c, drawer_devflow_milestones_537a9385cc42af697f845a63
- The build-staleness guard (`staleness.rs`) is content-aware on the dirty-tree arm; moving the marker out of a tracked path also removes a `dirty` source for installed builds (999.240 / build.rs). — drawer_devflow_decisions_2937a2d2df3f6ab2d8ad54bc (999.29 CONTEXT)
