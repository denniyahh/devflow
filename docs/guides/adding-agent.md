# Adding an Agent

DevFlow is agent-agnostic. Adding a new agent backend is a checklist, not a mystery.

## Checklist

1. **Add an adapter file** under `crates/devflow-core/src/agents/` implementing the `AgentAdapter` trait
2. **Add a variant** to the `AgentKind` enum in `state.rs`
3. **Update the `FromStr` parser**, `Display`, and `AgentParseError` text in `state.rs`
4. **Add a match arm** in `agents::adapter_for()`
5. **Add `pub mod` and `pub use`** exports in `agents/mod.rs`
6. **Add/extend tests** — adapter name, parser aliases, prompt-sharing test
7. **Update docs** — `README.md`, `CONTRIBUTING.md`, `ARCHITECTURE.md`, dependency matrix

## Agent Trait

```rust
pub trait AgentAdapter {
    fn name(&self) -> &str;
    fn exec_command(&self, phase: u32, prompt: &str, extra_writable_roots: &[PathBuf]) -> (String, Vec<String>);
    fn extra_env(&self) -> Vec<(String, String)> { Vec::new() }
    fn completion_signal_detected(&self, output: &str) -> bool;
}
```

## Example: Minimal Adapter

```rust
pub struct MyAgent;

impl AgentAdapter for MyAgent {
    fn name(&self) -> &str { "My Agent" }

    fn exec_command(&self, phase: u32, prompt: &str, _roots: &[PathBuf]) -> (String, Vec<String>) {
        ("my-agent".into(), vec!["--phase".into(), phase.to_string(), prompt.into()])
    }

    fn completion_signal_detected(&self, output: &str) -> bool {
        output.contains("MYAGENT_COMPLETE")
    }
}
```

## Prompt Sharing

All agents receive the same prompt text via `stage_prompt()`. Adapter prompt-sharing tests verify this invariant — ensure your adapter doesn't bypass it.
