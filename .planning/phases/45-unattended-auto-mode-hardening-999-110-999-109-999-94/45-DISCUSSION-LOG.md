# Phase 45: Unattended Auto-Mode Hardening (999.110 + 999.109 + 999.94) - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-01
**Phase:** 45-unattended-auto-mode-hardening-999-110-999-109-999-94
**Areas discussed:** Worktree base branch resolution (AUTO-01 / 999.110), Staleness check workspace scoping (AUTO-02 / 999.109), Unattended decision checkpoint prompt policy (DECN-01 / 999.94)

---

## Worktree Base Branch Resolution (AUTO-01 / 999.110)

| Option | Description | Selected |
|--------|-------------|----------|
| Automatic fallback check | Probe current branch first (e.g. if .planning/ exists on current HEAD / tracking branch like workspace/*), fallback to develop if missing | |
| Configurable base branch | Add a base_branch or planning_branch field in config with develop as default | ✓ |
| Strict develop tracking requirement | Mandate that .planning/ must be tracked on develop or copied into the worktree during creation | |

**User's choice:** Configurable base branch: Add a base_branch or planning_branch field in config with develop as default
**Notes:** Allows tracking branch (e.g., `workspace/denniyahh`) to be specified cleanly as the start point for worktrees.

---

## Staleness Check Workspace Scoping (AUTO-02 / 999.109)

| Option | Description | Selected |
|--------|-------------|----------|
| Workspace member prefix filter | Only match paths starting with crates/ (or within any Cargo workspace member directory) plus root Cargo.toml/Cargo.lock/build.rs/rust-toolchain.toml, explicitly ignoring non-workspace dirs like .planning/spikes/ | ✓ |
| Dynamic cargo metadata query | Query cargo metadata to dynamically obtain all member root paths and match against those roots | |
| Explicit exclusion list | Keep suffix matching but add an explicit ignore list for known non-workspace directories like .planning/spikes/ | |

**User's choice:** (Recommended) Option 1 (Prefix / static crates/ matching): Fast, pure path check, zero subprocess overhead, robust in unit tests without cargo/network, matches DevFlow's clean monorepo structure.
**Notes:** Excludes spike crates and non-workspace `.rs`/`Cargo.toml` files from tripping the self-dogfood staleness check.

---

## Unattended Decision Checkpoint Policy (DECN-01 / 999.94)

| Option | Description | Selected |
|--------|-------------|----------|
| Policy prompt layer in Code stage | Add clear instructions to code_stage_prompt instructing the agent to evaluate checkpoint options on merit, prefer explicitly recommended options, and record reasoning, overriding blind first-option selection | ✓ |
| Extend checkpoint_auto_decide_prompt for all stages | Unify checkpoint auto-resolution across all stages with the audited reasoning contract | |

**User's choice:** (Recommended) Policy prompt layer in Code stage: Add clear instructions to code_stage_prompt instructing the agent to evaluate checkpoint options on merit, prefer explicitly recommended options, and record reasoning, overriding blind first-option selection
**Notes:** Prevents GSD unattended execution from taking the first option indiscriminately.

---

## the agent's Discretion

None — all decisions explicitly chosen with operator confirmation.

## Deferred Ideas

None — discussion stayed within phase scope.
