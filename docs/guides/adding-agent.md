# Adding an Agent

DevFlow supports four agents today (Claude Code, Codex, OpenCode, Pi) through the modular
`AgentDriver` contract (999.31). Adding a new supported agent is a checklist, not a mystery.

## Checklist

1. **Add a driver file** under `crates/devflow-core/src/agents/` implementing the `AgentDriver` trait
2. **Add a variant** to the `AgentKind` enum in `state.rs`
3. **Update the `FromStr` parser**, `Display`, and `AgentParseError` text in `state.rs`
4. **Add a match arm** in `agents::adapter_for()`
5. **Add `pub mod` and `pub use`** exports in `agents/mod.rs`
6. **Add/extend tests** — driver name, parser aliases, and a conformance-suite run
7. **Update docs** — `README.md`, `CONTRIBUTING.md`, `ARCHITECTURE.md`, dependency matrix

## Driver contract

Each driver owns its prompt rendering, command building, completion parsing, health, and
interactivity declaration. The trait (in `crates/devflow-core/src/agents/mod.rs`):

```rust
pub trait AgentDriver {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> DriverCapabilities { DriverCapabilities::default() }
    fn render_prompt(&self, intent: &StageIntent) -> String;
    fn build_command(&self, phase: PhaseId, prompt: &str, extra_writable_roots: &[PathBuf]) -> (&'static str, Vec<String>);
    fn parse_completion(&self, output: &str) -> Option<AgentResult> { None }
    fn health(&self, state: &State) -> Result<(), String> { Ok(()) }
    fn environment(&self) -> Vec<(String, String)> { Vec::new() }
    fn sandbox_requirements(&self) -> SandboxRequirements { SandboxRequirements::default() }
    fn discover(&self) -> Result<(), String> { Ok(()) }
    fn test_contract(&self) -> Vec<ContractResult> { contract_checks(self) }
    fn interactivity_mode(&self, stage: Stage) -> InteractivityMode { InteractivityMode::HeadlessSafe }
}
```

## Example: minimal driver

```rust
pub struct MyDriver;

impl AgentDriver for MyDriver {
    fn name(&self) -> &'static str { "My Agent" }

    fn render_prompt(&self, intent: &StageIntent) -> String {
        // Render YOUR instruction from the intent data — it carries no agent
        // syntax, so you are free to write your agent's native form.
        crate::prompt::render_workflow_style(intent, "the My-Agent coding agent")
    }

    fn build_command(&self, phase: PhaseId, prompt: &str, _roots: &[PathBuf]) -> (&'static str, Vec<String>) {
        ("my-agent", vec!["--phase".into(), phase.to_string(), prompt.to_string()])
    }
}
```

## Prompt rendering is driver-owned

There is **no shared prompt**. A `StageIntent` carries the stage's *data* (phase, fix kind, review
angles) with no agent-specific syntax; each driver's `render_prompt` turns that data into its own
instruction. Claude and OpenCode render the legacy slash-command text byte-for-byte; Codex and Pi
render a workflow-file reference. Do **not** invent a new shared prompt or bypass
`render_prompt`.

## Conformance

Every driver must pass `test_contract()` (the shared conformance suite in `agents/mod.rs`). That is
the artifact a new driver implements against — the suite asserts the driver names a non-empty
program, renders the completion contract at every stage, and carries a non-empty name.
