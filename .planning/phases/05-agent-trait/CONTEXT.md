# Phase 5: Agent Trait Refactor

## Goal
Adding new agents currently requires modifying `state.rs` directly. Create an `Agent` trait so new agents can be added without touching core state machine code.

## Tasks

### 5a — Define Agent trait
- [ ] Create `crates/devflow-core/src/agents/mod.rs` — module root
- [ ] Define `Agent` trait:
  - `fn launch(&self, project_root: &Path, phase: u32) -> Result<(Child, u32)>` — spawn agent
  - `fn name(&self) -> &str` — human-readable name
  - `fn binary(&self) -> &str` — CLI binary name
  - `fn exec_args(&self, prompt: &str) -> Vec<String>` — non-interactive flags
- [ ] File: `crates/devflow-core/src/agents/mod.rs`

### 5b — Per-agent implementations
- [ ] `agents/claude.rs` — Claude-specific: `-p`, `--output-format json`, `--dangerously-skip-permissions`, `--max-turns 50`
- [ ] `agents/codex.rs` — Codex-specific: `exec`, `--sandbox workspace-write`, `--json`
- [ ] `agents/omx.rs` — OMX-specific: `exec`, `--sandbox workspace-write`, `--json`
- [ ] `agents/opencode.rs` — OpenCode-specific: `run`
- [ ] Each agent file in its own module

### 5c — Migrate state.rs
- [ ] Replace `Agent` enum's `exec_command` with `Agent` trait dispatch
- [ ] `state.agent` field changes from `Agent` enum to `Box<dyn Agent>`
- [ ] Update Serde serialization — tag-based enum still used for persistence
- [ ] Backward compatibility: existing state.json files deserialize correctly
- [ ] File: `crates/devflow-core/src/state.rs`

### 5d — Agent config
- [ ] Add `agents:` section to `.devflow.yaml` schema (optional)
- [ ] Per-agent overrides: model, extra_flags, env_vars
- [ ] File: `crates/devflow-core/src/config.rs`

## Verification
```bash
# All existing agents work identically after refactor
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check

# Manual: launch each agent type
devflow start --phase 5 --agent claude --monitor
devflow start --phase 5 --agent codex --monitor
```

## Success
New agents can be added by implementing a trait. No `state.rs` modifications needed. All existing agents work identically.
