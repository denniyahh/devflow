# Phase 7 — Git Worktrees + PR Integration

**Status:** Planning | **Priority:** HIGH | **Target:** v1.0.0

## Goal

Enable parallel agent execution via git worktrees and complete the ship pipeline with automated PR creation.

---

## Design Decisions

### Q1: Should `devflow parallel` be a dedicated command?

**Decision: Yes.** `devflow parallel --phases 7,8` is syntactic sugar over two `devflow start --worktree` calls, but discoverability matters. Users shouldn't need to deduce that running start twice = parallel phases. The command name makes the capability visible.

Implementation: `devflow parallel --phases 7,8 [--agents claude,codex]` spawns one `devflow start --worktree` per phase. Each phase gets its own worktree at `.worktrees/phase-NN/`. Agents map positionally to phases (phase 7 → Claude, phase 8 → Codex).

### Q2: Multiagent on same phase — shared or separate worktrees?

**Decision: Separate worktrees, rebase at merge.**

Shared worktree multiagent was rejected after steel-man analysis. The safety model (file ownership contracts, two-layer validation, diff checking) exists only to solve a self-inflicted problem. LLMs don't respect file boundaries — a clippy fix in `state.rs` during a `worktree.rs` task silently corrupts the second agent's work. Dropping all that complexity.

**Replacement model:** `devflow sequentagent --phase N --agents claude,codex` creates separate worktrees:
- `.worktrees/phase-NN-claude/`
- `.worktrees/phase-NN-codex/`

Both start from the same commit on `feature/phase-NN`. Agent A finishes → pushes → Agent B rebases onto updated branch (now includes A's work) → resolves conflicts if any → pushes. Each agent's commits preserved in history. Conflicts surfaced as explicit rebase conflicts, not silent data loss.

No file ownership parser, no validator, no diff-checker, no abort flow. Worktree boundary IS the isolation.

### Q3: Reference worktree — auto-update or manual?

**Decision: Manual refresh.** `devflow reference` creates a one-time snapshot. `devflow reference --refresh` updates it. No git hooks, no auto-sync.

Rationale: A reference worktree that silently changes defeats its purpose — you diff against it expecting a stable baseline. If it auto-updated, `diff reference/ phase-07/` would show different results each time. Manual refresh puts the user in control.

Location: `.worktrees/reference/` within the repo.

---

## Task Breakdown

### 7a — Git Worktree Support (primary)
- [ ] `devflow start --phase N --worktree` — creates worktree at `.worktrees/phase-NN/`, prompts agent with worktree path as workdir
- [ ] `devflow parallel --phases N,M [--agents agent1,agent2]` — spawns two `devflow start --worktree` calls, positional agent matching
- [ ] `devflow sequentagent --phase N --agents agent1,agent2` — separate worktrees, sequential execution with rebase
  - [ ] Worktrees at `.worktrees/phase-NN-agentname/` (e.g. `phase-07-claude`, `phase-07-codex`)
  - [ ] Both start from same commit on `feature/phase-NN`
  - [ ] Agent A completes → pushes → Agent B rebases onto updated branch → pushes
  - [ ] No file ownership contracts, no diff checking — worktree boundary is the isolation
- [ ] `devflow reference [--branch name] [--refresh]` — creates/refreshes static worktree
- [ ] `devflow cleanup` — removes worktrees alongside branch deletion
- [ ] `devflow status` — lists active worktrees with phase/agent info
- [ ] Agent adapter: pass worktree path instead of repo root when worktree mode active
- [ ] Monitor: track worktree path so `devflow check` runs in correct directory
- [ ] Verify: `devflow parallel --phases 7,8 --agents claude,codex` runs both phases concurrently without conflict

### 7b — PR Creation via `gh` CLI
- [ ] `devflow ship` calls `gh pr create` with auto-generated body
- [ ] PR body: phase summary (from CONTEXT.md), changed files (git diff --stat), test count
- [ ] Merge detection: poll PR status, auto-advance state on merge
- [ ] Verify: `devflow ship` on devflow itself creates a valid PR against develop

### 7c — Test Hardening
- [ ] Fix `spawn_monitor` test: assert pid file appears with valid content, not just `pid > 0`
- [ ] Add Layer 2 failure tests: exit=0+commits=0 → Failed, exit≠0 → Failed
- [ ] Cover `devflow_result:` (lowercase, no space) in agent_result parser
- [ ] End-to-end monitor test: spawn real agent, verify DEVFLOW_RESULT captured, check() advances

### 7d — Prompt Rationalization
- [ ] Extract `phase_prompt()` to `agents/mod.rs`
- [ ] Delete `simple_prompt()` from `codex.rs`
- [ ] `CodexAgent::exec_command` calls `phase_prompt()` instead
- [ ] Verify: both Claude and Codex prompts contain identical instruction text

---

## Success Criteria

- `devflow start --phase N --worktree` creates functional isolated agent environment
- `devflow multiagent --phase N --agents claude,codex` runs both agents sequentially in shared worktree
- `devflow reference --refresh` updates snapshot without disturbing ongoing work
- `devflow ship` creates a real PR on GitHub with descriptive body
- 115+ tests, clippy clean, new tests cover the 3 gaps from Phase 6 review
- Claude and Codex get identical prompt contracts
