---
created: 2026-09-13T18:37:15.901Z
title: Review and normalize agent instruction files across tools
area: tooling
severity: minor
files:
  - CLAUDE.md
  - AGENTS.md
  - .agents/rules/graphify.md
  - .agents/workflows/graphify.md
  - .claude/settings.json
  - .claude/settings.local.json
  - .codex/hooks.json
  - .cursor/rules/graphify.mdc
  - .opencode/opencode.json
  - skills-lock.json
  - ~/.config/agents/AGENTS.md
  - ~/.claude/rules/
  - ~/.gemini/GEMINI.md
  - ~/.config/opencode/AGENTS.md
  - ~/.claude/projects/-var-home-denniyahh-Github-devflow/memory/MEMORY.md
---

## Problem

Each agent tool reads a different instruction file, and the files say different things. Measured 2026-09-13:

- **Global rules.**
  - `~/.config/agents/AGENTS.md` (20,347 bytes) is the source. `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md` are symlinks to it.
  - `~/.gemini/GEMINI.md` and `~/.config/opencode/AGENTS.md` are identical 2,319-byte **copies**, not symlinks. They start with the context7 block, so gemini and opencode get a much smaller rule set than Claude and codex.
  - `~/.claude/rules/` (`code-style.md`, `context7.md`, `effort-routing.md`, `git-workflow.md`) loads only in Claude Code.
- **Project rules.**
  - `CLAUDE.md` (18,672 bytes) holds the worktree, sync and verification-habit rules.
  - `AGENTS.md` (1,474 bytes) contains only a graphify section. Codex loads `AGENTS.md` and never sees `CLAUDE.md`'s worktree rule. That is how stray phase 47 planning work landed in the main checkout (`b102898`; see memory `project-codex-main-checkout-stray-phase-work`).
  - The graphify instructions are repeated in `AGENTS.md`, `.agents/rules/graphify.md`, `.agents/workflows/graphify.md`, `.cursor/rules/graphify.mdc` and the `.opencode` graphify plugin.
- **Standing context cost.** Every Claude Code session loads about 52 KB of rules and memory:
  - the global file (20,347 bytes);
  - the project `CLAUDE.md` (18,672 bytes, including 11 "verification habits" bullets);
  - `MEMORY.md` (13,313 bytes, 43 entries).

  The memory file `project-gsd-execute-devflow-quirks.md` alone is 20,363 bytes. The phase 47 retrospective found that rules only written as prose kept recurring, while lint-enforced rules held.
- **Installed but probably irrelevant to this project** (confirm with the operator before removing):
  - the `azure-*`, `microsoft-foundry` and `entra-app-registration` skills;
  - the enabled `vercel-plugin`;
  - the claude.ai Vercel and Linear connectors. Linear was retired 2026-09-02 (memory `reference-github-issues-devflow`).
- **`.opencode/`** has 2,515 files on disk but 2 tracked; the other 2,510 are `node_modules`.
- **Workspace-only divergence.** All the project files above exist only on `workspace/denniyahh`, not on `develop`. See the retrospective's workspace assessment.

## Solution

TBD. Starting points:

- **One source per scope.** Pick one canonical file each for global and project rules, and generate or symlink every tool's file from it.
- **Close the gaps.** Give codex, gemini and opencode the project rules they currently miss, starting with the worktree rules.
- **Deduplicate** the graphify instruction blocks.
- **Delete enforced prose.** Once a rule is enforced by a lint or hook, replace its prose with a one-line pointer. This is retrospective proposal 7, which is **not yet decided**.
- **Prune** stale or phase-specific memory entries.
- **Remove unused tooling** (skills, plugins, connectors) after operator confirmation. Coordinate with the todo "Evaluate skills, plugins, MCP and ACP servers to add".
- **Decide D1 first.** The retrospective's open decision D1 (workspace branch structure) may move or upstream several of these files: `.planning/audits/2026-09-13-phase-47-retrospective.md`.
