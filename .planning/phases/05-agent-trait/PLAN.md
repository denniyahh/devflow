# Phase 5: Agent Trait + Branch Safety

> Parent: ROADMAP.md | Status: In Progress (2026-06-18)

## Goal

Two objectives in one phase — they're small enough and both touch `git.rs` + `state.rs`:

**5A — Agent trait refactor:** Extract agent-specific logic from `state.rs` into the `agents/` module. New agents added via trait impl, no `state.rs` changes needed.

**5B — Branch safety:** Three high-impact fixes from the [post-mortem](./POSTMORTEM.md) to prevent the branch divergence that happened with `feature/phase-04`.

## Context

- **Branch:** `feature/phase-05-agent-trait`
- **Base:** `develop` (clean — post Phase 3 merge)
- **Pre-existing:** agents/ module wired up but not integrated into state.rs
- **Post-mortem:** [POSTMORTEM.md](./POSTMORTEM.md) — root cause analysis of 14-file branch conflict

---

## Tasks

### 5A — Agent Trait Refactor

#### 5A-1: Define Agent trait ✅
- [x] Create `crates/devflow-core/src/agents/mod.rs` — module root
- [x] Define `Agent` trait: `name()`, `exec_command()`, `completion_signal_detected()`
- [x] `adapter_for()` factory function
- [x] Add `pub mod agents` to `lib.rs`
- [x] Add `pub type AgentKind = Agent` to `state.rs` (avoids trait/enum name collision)
- [x] Verify: `cargo build` + `cargo test` pass

#### 5A-2: Per-agent implementations ✅
- [x] `agents/claude.rs` — ClaudeAgent with rich prompts
- [x] `agents/codex.rs` — CodexAgent
- [x] `agents/omx.rs` — OmxAgent
- [x] `agents/opencode.rs` — OpenCodeAgent
- [x] Each agent implements `Agent` trait
- [x] Verify: 85 tests pass, clippy clean

#### 5A-3: Integrate trait into state.rs
- [ ] Replace `Agent::exec_command()` body with delegation to `agents::adapter_for()`
- [ ] Update `crate::agent::launch_agent()` to accept `&dyn agents::Agent` instead of `&State`
- [ ] Update `Agent::name()` to delegate to trait
- [ ] Verify: all existing tests pass identically, no behavior change
- [ ] File: `crates/devflow-core/src/state.rs`, `crates/devflow-core/src/agent.rs`

#### 5A-4: Agent config in .devflow.yaml (optional)
- [ ] Add optional `agents:` section to `AutomationConfig`
- [ ] Per-agent overrides: model, extra_flags
- [ ] Rational default when section absent
- [ ] File: `crates/devflow-core/src/config.rs`

#### 5A-5: Disable OMX support
- [ ] Comment out `Agent::Omx` variant + all match arms in `state.rs`
- [ ] Comment out Omx arm in `agents/mod.rs` adapter_for()
- [ ] Keep `agents/omx.rs` file as-is (preserved for future re-enable)
- [ ] Verify: `Agent::from_str("omx")` returns `AgentParseError`
- [ ] Verify: `Agent::from_str("oh-my-codex")` returns `AgentParseError`

### 5B — Branch Safety Fixes

#### 5B-1: `devflow list` command
- [ ] New `List` subcommand in CLI
- [ ] `GitFlow::list_branches()` — returns all `feature/phase-*` branches with:
  - Branch name
  - Commits ahead of develop
  - Commits behind develop
  - Last commit date
- [ ] Output: table with columns: `BRANCH | AHEAD | BEHIND | LAST COMMIT`
- [ ] Verify: `devflow list` shows clean output on this repo
- [ ] Files: `crates/devflow-core/src/git.rs`, `crates/devflow-cli/src/main.rs`

#### 5B-2: `devflow start` safety
- [ ] Change `feature_start` from `checkout -B` to `checkout -b` (error if exists)
- [ ] Add `--force` flag to `devflow start` for intentional overwrite
- [ ] Error message when branch exists: "feature/phase-05 already exists. Use --force to overwrite, or merge it first."
- [ ] Verify: running `devflow start --phase 5` twice errors on second attempt
- [ ] Files: `crates/devflow-core/src/git.rs`, `crates/devflow-cli/src/main.rs`

#### 5B-3: `devflow status` enhancement
- [ ] Add "Open Branches" section to `devflow status` output
- [ ] Show branch name + divergence (ahead/behind) for each feature branch
- [ ] Only show when there are open feature branches
- [ ] Verify: `devflow status` includes branch list when branches exist
- [ ] Files: `crates/devflow-cli/src/main.rs`

---

## Verification

```bash
# Agent trait
cargo test --workspace              # All tests pass
cargo clippy -- -D warnings         # Clean
cargo fmt -- --check                # Formatted

# Branch safety
devflow list                        # Shows feature branches
devflow status                      # Includes open branches section
devflow start --phase 5             # Errors (branch exists)
devflow start --phase 5 --force     # Succeeds with --force
```

## Success

1. New agents added by implementing `Agent` trait — no `state.rs` changes
2. `devflow list` shows all feature branches with divergence
3. `devflow start` refuses to overwrite existing branches
4. `devflow status` surfaces open branches in daily workflow
5. All 85+ tests pass, clippy clean
