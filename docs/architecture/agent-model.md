# Agent Model

DevFlow is agent-agnostic — all coding agents share the same interface.

## Adapter Contract

```rust
pub trait AgentAdapter {
    fn name(&self) -> &str;
    fn exec_command(
        &self,
        phase: u32,
        prompt: &str,
        extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>);
    fn extra_env(&self) -> Vec<(String, String)>;
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
- `mod.rs` — `AgentAdapter` trait definition + `adapter_for()` factory

## Shared Prompts

All agents receive the same stage-specific prompt via `stage_prompt()`. The
adapter supplies only command-line details and narrowly scoped environment
requirements.

The prompt asks agents to use the relevant GSD command and finish with a
`DEVFLOW_RESULT` JSON marker. Validate additionally requires a `pass` or
`gaps` verdict; Ship requires a review-before-ship decision.

## Completion Evaluation

See [Agent Lifecycle diagram](../diagrams/agent-lifecycle.md) for the full evaluation flow.

1. **External verification** — for Code plans that declare a reviewed probe, DevFlow runs it in the execution worktree first.
2. **Native output / marker** — reads the adapter envelope or the last `DEVFLOW_RESULT` marker.
3. **Exit code and commits** — an exit failure is final; Plan and Code also require commits when no stronger result exists.
4. **Last-resort heuristic** — an exited process with commits may be marked `Unknown`, never silently treated as success.

## Adding a New Agent

See [Adding an Agent guide](../guides/adding-agent.md) for the step-by-step checklist.
