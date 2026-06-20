# Agent Model

DevFlow is agent-agnostic — all coding agents share the same interface.

## Agent Trait

```rust
pub trait Agent {
    fn name(&self) -> &str;
    fn kind(&self) -> AgentKind;
    fn exec_command(&self, phase: u32) -> (String, Vec<String>);
    fn completion_signal_detected(&self, output: &str) -> bool;
}
```

## Supported Agents

| Agent | CLI Binary | Kind Variant | Accepts |
|-------|-----------|--------------|---------|
| Claude Code | `claude` | `AgentKind::Claude` | `claude` |
| OpenAI Codex | `codex` | `AgentKind::Codex` | `codex` |
| OpenCode | `opencode` | `AgentKind::OpenCode` | `opencode`, `open-code` |

## Agent Adapters

Each agent has a dedicated adapter file under `crates/devflow-core/src/agents/`:

- `claude.rs` — Claude Code adapter
- `codex.rs` — Codex adapter
- `opencode.rs` — OpenCode adapter
- `mod.rs` — `Agent` trait definition + `adapter_for()` factory

## Shared Prompts

All agents receive the **same prompt text** via `phase_prompt(phase)`. The prompt directs agents to:

1. Read `CLAUDE.md`, `.planning/ROADMAP.md`, `.planning/phases/NN-*/CONTEXT.md`, and `AGENTS.md`
2. Implement, test, lint, format
3. Commit per sub-task
4. Emit a `DEVFLOW_RESULT` marker

The `claude_and_codex_share_identical_prompt_text` test enforces this invariant.

## Completion Evaluation

See [Agent Lifecycle diagram](../diagrams/agent-lifecycle.md) for the full evaluation flow.

**Layer 1 — Marker (authoritative):** Scans stdout tail for `DEVFLOW_RESULT: {"status": ...}`.

**Layer 2 — Exit code + commits (reliable fallback):** `exit == 0 && commits > 0` → success.

**Layer 3 — Commit heuristic (last resort):** Commits on branch without exit code → probable success with warning.

## Adding a New Agent

See [Adding an Agent guide](../guides/adding-agent.md) for the step-by-step checklist.
