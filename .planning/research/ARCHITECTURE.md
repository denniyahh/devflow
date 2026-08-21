# Architecture Patterns — Adding a Harness Driver (Milestone v2.8.0)

**Domain:** DevFlow agent-driver integration
**Researched:** 2026-08-18 (source-read of the existing four-driver architecture)

## Recommended Architecture

New harnesses plug into the **existing `AgentDriver` contract** — no architectural change. Two
launch *families* already exist; each new harness maps onto one of them.

### Registration points (every new `AgentKind` touches all of these)

| Location | Change |
|----------|--------|
| `state.rs` — `enum AgentKind` | add `Antigravity`, `Hermes` variants (serde `rename_all="lowercase"` auto-handles wire form) |
| `state.rs` — `Display` | add `"antigravity"`, `"hermes"` |
| `state.rs` — `FromStr` | add `"antigravity"`, `"hermes"` arms |
| `state.rs` — `AgentParseError` message | update the `expected claude, codex, opencode, or pi` string |
| `agents/mod.rs` — `driver_for(kind)` | add `Box::new(AntigravityDriver)` / `Box::new(HermesDriver)` arms |
| `agents/{antigravity,hermes}.rs` | new driver files (mirror the closest existing driver) |
| `agent.rs` — `agent_program(kind)` | map new kinds to their binary names (`antigravity` / `hermes`) |
| CLI `--agent` arg | already `AgentKind`-typed via clap `FromStr` — no change beyond the enum |

### Two launch families (the driver decides, not the core)

```
Family A — stream-json pipe-owning (Claude today, Antigravity can join):
  stdin  ← one NDJSON user-turn line (monitor::user_turn_line)
  stdout → one JSON event per line → parsed for Layer-1 verdict / DEVFLOW_RESULT
  argv:  <bin> -p --input-format stream-json --output-format stream-json --dangerously-skip-permissions

Family B — positional single-document (Pi `-p`, Codex `exec`, OpenCode `run`, Hermes `-z`):
  argv:  <bin> <flag> "<prompt>"   (+ auto-approve flag)
  completion = process-exit transport, contract carried IN the prompt (render_prompt embeds DEVFLOW_RESULT)
  (Codex/OpenCode refine this with --json / --format json → parse_completion reads the event stream)
```

### Component Boundaries

| Component | Responsibility | Communicates With |
|-----------|---------------|-------------------|
| `AgentDriver` trait | per-agent prompt/argv/parse/health/capabilities | `driver_for` (dispatch), `monitor` (spawn), `preflight` (health gate) |
| `render_prompt` | stage prompt in the agent's dialect (Claude-style slash text vs workflow-style) | `prompt.rs` (`render_claude_style` / `render_workflow_style`) |
| `parse_completion` | turn captured output into `AgentResult` (or `None` = process-exit) | `agent_result.rs` |
| `health` | fail-closed pre-launch credential/binary check | `preflight` |

## Patterns to Follow

### Pattern 1: Mirror the closest existing driver
**When:** every new driver. Antigravity mirrors `ClaudeDriver`; Hermes mirrors `PiDriver`; OpenCode
completion mirrors `parse_codex_event_result`. Do not invent a third launch shape.

### Pattern 2: Fail-closed capability detection
**When:** implementing `capabilities()`. Match the exact vetted package/feature, return `false` on
any probe failure (Pi's `@bacnh85/pi-subagent` pattern).

### Pattern 3: Health probes what launch actually uses
**When:** implementing `health()`. Probe the provider/credential the argv *will actually use*, not a
catalog-wide "any provider ready" (Pi's `defaultProvider` lesson).

## Anti-Patterns to Avoid

1. **New launch shape.** A third family breaks the monitor's single spawn/capture path. Map onto A or B.
2. **Silent parse downgrade.** Auto-fallback from a stream to process-exit hides degradation (Phase 31 D-11).
3. **Bare name-substring capability match.** False-greens unsafe packages (Phase 39 finding 2).

## Scalability Considerations

| Concern | Now (5 harnesses) | More harnesses |
|---------|-------------------|----------------|
| `driver_for` match | one arm per kind | grows linearly — fine; `#[non_exhaustive]` enums keep additions non-breaking |
| conformance suite | `test_contract` shared | new driver inherits it; extensibility is the 999.31 D-02 proof |

## Sources

- `crates/devflow-core/src/agents/mod.rs`, `state.rs`, `agent.rs`
- Existing drivers (`claude.rs`, `codex.rs`, `opencode.rs`, `pi.rs`)
