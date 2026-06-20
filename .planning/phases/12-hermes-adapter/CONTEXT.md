# Phase 12: Hermes Adapter

## Summary

Add **Hermes Agent** as a first-class coding agent backend in DevFlow. Hermes is
the orchestration agent used for planning, review, and debugging — this adapter
enables `devflow start --agent hermes --monitor` so Hermes can directly execute
phases when a dedicated coding agent (Claude/Codex) is not available or preferred.

## Tasks

### 1. Hermes Agent Adapter

Follow the established agent adapter checklist from ARCHITECTURE.md:

- [ ] Create `crates/devflow-core/src/agents/hermes.rs`
  - Implement `Agent` trait: `name()`, `exec_command()`, `completion_signal_detected()`
  - `exec_command`: use `hermes exec --non-interactive --json "prompt"` (or equivalent headless mode)
  - Map the shared `phase_prompt()` into Hermes' CLI flags
- [ ] Add `Hermes` variant to `Agent`/`AgentKind` enum in `state.rs`
- [ ] Add `"hermes"` → `Agent::Hermes` in `FromStr` parser
- [ ] Update `Display` impl for `Agent::Hermes` → `"hermes"`
- [ ] Update `AgentParseError` text to include `hermes`
- [ ] Add `Agent::Hermes` arm in `agents::adapter_for()`
- [ ] Add `pub mod hermes` and `pub use` in `agents/mod.rs`
- [ ] Add tests: parser aliases, shared-prompt, adapter name
- [ ] Update `phase_prompt()` if Hermes needs different prompt structure

### 2. Completion Protocol

- [ ] Detect `DEVFLOW_RESULT` marker in Hermes output
- [ ] Hermes' JSON-envelope output format (if different from Claude/Codex)
- [ ] Exit code handling: Hermes non-zero → `AgentStatus::Failed`

### 3. CLI Integration

- [ ] `devflow start --agent hermes` parses and validates
- [ ] `devflow start --agent hermes --monitor` spawns + monitors
- [ ] `devflow status` shows "hermes" as the active agent
- [ ] Hermes appears in `--help` output for agent selection

### 4. Documentation

- [ ] Add "Hermes Agent" to README agent support table
- [ ] Update ARCHITECTURE.md "Extension points" to reference the hermes adapter as an example
- [ ] Update CONTRIBUTING.md "Adding a New Agent" checklist
- [ ] Update DEPENDENCIES.md if Hermes CLI has install requirements
- [ ] Update `.devflow.yaml` docs — `hermes` is a valid agent value

## Verification

```bash
cargo test
cargo clippy -- -D warnings
cargo run -- status  # should not break with new Agent variant
echo "hermes" | cargo run -- start --phase 12 --agent hermes --dry-run  # if dry-run exists
```

## Deliverables

- `hermes.rs` adapter with full `Agent` trait implementation
- Enum variant + parser + display for `Agent::Hermes`
- Shared prompt test verifying Hermes receives identical prompt text
- Doc updates across README, ARCHITECTURE, CONTRIBUTING
