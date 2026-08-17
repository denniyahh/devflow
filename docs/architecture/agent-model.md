# Agent Model

DevFlow supports four agents today (Claude Code, Codex, OpenCode, Pi) through
the modular `AgentDriver` contract (999.31).

## Driver Contract

```rust
pub trait AgentDriver {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> DriverCapabilities { DriverCapabilities::default() }
    fn render_prompt(&self, intent: &StageIntent) -> String;
    fn build_command(&self, phase: PhaseId, prompt: &str, extra_writable_roots: &[PathBuf]) -> (&'static str, Vec<String>);
    fn parse_completion(&self, output: &str) -> Option<AgentResult> { None }
    fn health(&self, state: &State) -> Result<(), String> { Ok(()) }
    fn environment(&self) -> Vec<(String, String)> { Vec::new() }
    fn test_contract(&self) -> Vec<ContractResult> { contract_checks(self) }
    fn interactivity_mode(&self, stage: Stage) -> InteractivityMode { InteractivityMode::HeadlessSafe }
}
```

## Supported Agents

| Agent | CLI Binary | Kind Variant | Accepts |
|-------|-----------|--------------|---------|
| Claude Code | `claude` | `AgentKind::Claude` | `claude` |
| OpenAI Codex | `codex` | `AgentKind::Codex` | `codex` |
| OpenCode | `opencode` | `AgentKind::OpenCode` | `opencode`, `open-code` |

## Drivers

Each agent has a dedicated driver file under `crates/devflow-core/src/agents/`:

- `claude.rs` — Claude Code driver
- `codex.rs` — Codex driver
- `opencode.rs` — OpenCode driver
- `pi.rs` — Pi driver
- `mod.rs` — `AgentDriver` trait definition + `adapter_for()` factory + conformance suite

## Driver-owned prompts

There is no shared prompt. A `StageIntent` carries the stage's data (phase, fix kind, review
angles) with no agent-specific syntax; each driver's `render_prompt` turns it into its own
instruction. Claude/OpenCode render the legacy slash-command text byte-for-byte; Codex/Pi render a
workflow-file reference.

Every prompt asks the agent to finish with a `DEVFLOW_RESULT` JSON marker. Validate additionally
requires a `pass` or `gaps` verdict; Ship requires a review-before-ship decision.

## Completion Evaluation

See [Agent Lifecycle diagram](../diagrams/agent-lifecycle.md) for the full evaluation flow.

1. **External verification** — for Code plans that declare a reviewed probe, DevFlow runs it in the execution worktree first.
2. **Native output / marker** — reads the adapter envelope or the last `DEVFLOW_RESULT` marker.
3. **Exit code and commits** — an exit failure is final; Plan and Code also require commits when no stronger result exists.
4. **Last-resort heuristic** — an exited process with commits may be marked `Unknown`, never silently treated as success.

## Adding a New Agent

See [Adding an Agent guide](../guides/adding-agent.md) for the step-by-step checklist.
