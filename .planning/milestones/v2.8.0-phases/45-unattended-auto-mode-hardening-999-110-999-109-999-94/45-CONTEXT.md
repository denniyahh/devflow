# Phase 45: Unattended Auto-Mode Hardening (999.110 + 999.109 + 999.94) - Context

**Gathered:** 2026-09-01
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase hardens `--mode auto` execution so that unattended phase runs can launch cleanly out of the box and run safely. Specifically:
1. Resolving the worktree base branch / start point from configuration (e.g. `base_branch` or `planning_branch` with fallback to `develop`) so that `.planning/` and `.planning/config.json` are present in freshly created phase worktrees (AUTO-01 / 999.110).
2. Scoping the self-dogfood staleness check's `affects_compiled_binary` predicate to Cargo workspace members (`crates/*`) plus root build files, ignoring `.planning/spikes/` and other non-workspace code (AUTO-02 / 999.109).
3. Establishing a prompt policy layer in the Code stage instructing the agent to evaluate `decision` checkpoints on merit and record its reasoning rather than blindly taking the first option (DECN-01 / 999.94).

</domain>

<decisions>
## Implementation Decisions

### Worktree Base Branch Resolution (AUTO-01 / 999.110)
- **D-01:** Add configurable base branch resolution in DevFlow config (`git.base_branch` or `git.planning_branch`) with `develop` as the default. When creating a phase worktree or checking base ref reachability/currency, use the configured base branch so personal tracking branches like `workspace/denniyahh` carrying `.planning/` can be targeted cleanly. — **Reversibility:** costly — changes configuration fields and worktree creation call sites across CLI commands and preflight.

### Workspace Member Scoping for Staleness Check (AUTO-02 / 999.109)
- **D-02:** Scope `affects_compiled_binary` to Cargo workspace member paths (`crates/*`) and root build files (`Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml`). Any path outside `crates/` (e.g. `.planning/spikes/*`) must return `false` so non-workspace spikes never trip the D-18 self-dogfood stale build block. — **Reversibility:** reversible — pure path predicate update in `staleness.rs`.

### Unattended Decision Checkpoint Policy (DECN-01 / 999.94)
- **D-03:** Add a dedicated policy instruction layer to `code_stage_prompt` in `prompt.rs`. When unattended, the agent is instructed to resolve any `decision` checkpoint by evaluating all presented options on their merits, respecting explicit recommended markings where present, and recording the rationale in the final response. — **Reversibility:** reversible — prompt template enhancement in `prompt.rs`.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Worktree & Preflight
- `crates/devflow-core/src/worktree.rs` — Worktree management and `add` start-point execution
- `crates/devflow-cli/src/commands.rs` — Pre-start checks, worktree creation wiring, and `ensure_phase_worktree`
- `crates/devflow-cli/src/parallel.rs` — `ensure_phase_worktree` implementation for parallel/standard phase worktree setup
- `crates/devflow-cli/src/preflight.rs` — Base ref currency, reachability, and `unattended_config_condition`

### Staleness Check
- `crates/devflow-cli/src/staleness.rs` — Build staleness detection and `affects_compiled_binary` definition

### Stage Prompts & Checkpoint Resolution
- `crates/devflow-core/src/prompt.rs` — `code_stage_prompt` and `checkpoint_auto_decide_prompt` contracts

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `devflow_core::worktree::add`: Takes `start_point` parameter for `git worktree add -b <branch> <path> <start_point>`.
- `devflow_core::config`: Configuration structures for git and workflow settings.
- `staleness::porcelain_tracked_path`: Extracts repository-relative tracked path from git status output.

### Established Patterns
- All preflight failure messages avoid embedding absolute host filesystem paths or usernames (WR-02 / 999.10).
- Staleness checks fail toward `Stale` on unexpected git errors to maintain safety.
- Stage prompts append `{COMPLETION_PROTOCOL}` to enforce structured outcome reporting.

### Integration Points
- `crates/devflow-cli/src/commands.rs`: `start` entry point where base branch reachability and currency are checked.
- `crates/devflow-cli/src/parallel.rs`: `ensure_phase_worktree` where `worktree::add` is called.
- `crates/devflow-cli/src/staleness.rs`: `affects_compiled_binary` called by `ancestry_range_affects_build` and `tree_has_modified_build_inputs`.
- `crates/devflow-core/src/prompt.rs`: `code_stage_prompt` used by `stage_prompt`.

</code_context>

<specifics>
## Specific Ideas

- Ensure unit tests cover `.planning/spikes/foo/Cargo.toml` and `.planning/spikes/foo/src/main.rs` confirming `affects_compiled_binary` evaluates to `false`.
- Ensure tests verify that worktree creation honors configured base branch instead of hardcoding `develop`.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 45-Unattended Auto-Mode Hardening (999.110 + 999.109 + 999.94)*
*Context gathered: 2026-09-01*
