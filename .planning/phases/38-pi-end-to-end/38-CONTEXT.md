# Phase 38: Pi End-to-End - Context

**Gathered:** 2026-08-17
**Status:** Ready for planning

## Phase Boundary

Phase 38 makes `devflow start --agent pi` complete the full pipeline — **full Claude parity, to the
degree Pi supports identical functionality**. Concretely: switch Pi's launch from `-p` print mode to
`--mode json`, add a Pi JSON-mode event unwrapper that translates Pi's event schema into the
monitor's event vocabulary, route Pi through the pipe-owning monitor arm + the `CloseRule` drain
gate, and make the Define/Plan interactivity gate driver-driven. Anything Pi cannot natively
support is **left out** (recorded as a limitation, not worked around) unless it is already covered
elsewhere in Phase 38.

## Implementation Decisions

### Scope & success bar
- **D-01 — Full Claude parity.** The success bar is `devflow start --agent pi` completing
  Plan → Code → Validate → Ship (Define is a no-op for every agent, D-14). Full parity means Pi
  does everything Claude does today; where Pi cannot support identical functionality, that item is
  **excluded** and recorded separately, not papered over — unless it is already in Phase 38's
  scope by another route. — **Reversibility:** costly — the acceptance tests written against this
  bar are what "Pi works" means from here on.

### Transport & monitor integration
- **D-02 — Commit to `--mode json`.** Pi's launch switches from `-p` (print, process-exit) to
  `--mode json` (event stream). The unwrapper translates Pi's JSON event schema into the monitor's
  event vocabulary (`task_started` / `task_notification` / `background_tasks_changed` / …) so the
  `CloseRule` drain gate gets real coverage of Pi's concurrent work — not post-hoc output parsing.
  — **Reversibility:** one-way-ish — the drain-gate integration is built on the event stream; going
  back to `-p` would drop the concurrent-work coverage this phase exists to add.

### Interactivity
- **D-04 — Mirror the Codex treatment.** Pi declares `interactivity_mode` with Define/Plan →
  `RequiresExistingArtifact` (Pi cannot run the interactive discuss-phase interview or the
  interactive plan overwrite decision headlessly, same as Codex), and the Define/Plan gate becomes
  driver-driven instead of the hardcoded `AgentKind::Codex` check. The operator intends to revisit
  interactivity handling in a later phase — this phase makes it *generic*, not *smarter*.

## Deferred (explicitly not here)

- **999.94** — the unattended `decision` checkpoint first-option guard (D-03, deferred).
- **999.106 remainder** — `AgentAdapter`/`DriverShim` removal + call-site migration → Phase 39.
- **999.107** — Codex parser success-before-`turn.failed` ordering + writable-root serialization → Phase 39.
- **Pi capabilities that cannot match Claude** — left out per D-01, recorded as limitations.

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `crates/devflow-core/src/monitor.rs` — the `CloseRule` drain gate + the stream-json event parser;
  the vocabulary Pi's unwrapper must emit (`background_tasks_changed`, `task_started`,
  `task_notification`, the `open_tasks` per-task arm).
- `crates/devflow-cli/src/pipeline_launch.rs` — `MonitorLaunch::PipeOwning` vs `Legacy` routing +
  `claude_stream_launch_enabled()` (what must widen to include Pi).
- `crates/devflow-core/src/agents/pi.rs` — the current `PiDriver` (`-p`, `--no-approve`,
  `pi auth check` health).
- `.planning/milestones/v2.6.0-phases/37-modular-agent-driver-architecture-pi-driver-999-31-pi/37-RESEARCH.md`
  + `37-CONTEXT.md` — the AgentDriver contract + the Pi deferrals this phase picks up.
- `.planning/milestones/v2.5.0-phases/35.3-drain-gate-concurrency-measurement-999-83/35.3-evidence/`
  — the measured drain-gate event families the `CloseRule` watches.

## Existing Code Insights

### Reusable Assets
- The monitor's `CloseRule` + `ParsedCapture`/`classify` machinery is Claude-stream-json-shaped but
  event-typed — a Pi unwrapper that emits the same `type`/`subtype` JSON lines reuses the drain arm
  unchanged.
- `DriverShim` already routes all four agents through their `AgentDriver`s; only the *launch shape*
  (`MonitorLaunch::PipeOwning` vs `Legacy`) is Claude-only today.

### Integration Points
- `claude_stream_launch_enabled()` (`pipeline_launch.rs`) — the predicate to widen so Pi routes
  through `PipeOwning` when its JSON-mode launch is active.
- `interactivity_mode()` (already on `AgentDriver`; `CodexDriver` overrides Define/Plan →
  `RequiresExistingArtifact`) — `PiDriver` needs the same override, and the hardcoded
  `AgentKind::Codex` checks in `preflight.rs` + `commands.rs:289` become driver-driven.

## Specific Ideas

No specific requirements — open to standard approaches within the driver/monitor architecture.
