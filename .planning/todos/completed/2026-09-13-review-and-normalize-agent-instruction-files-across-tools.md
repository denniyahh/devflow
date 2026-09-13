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

## Solution (Resolved 2026-09-13)

Implemented per Phase 47 Retrospective D1 & D2 decisions:
- **One source per scope:** Global agent rules consolidated into `~/.config/agents/AGENTS.md`. Minimal, public-safe repository rules (28 lines) retained in `CLAUDE.md`.
- **Deduplication:** Deleted redundant repo-level graphify files (`AGENTS.md`, `.agents/`, `.cursor/`, `.opencode/`). Graphify hooks moved globally to `~/.claude/settings.json` and `~/.codex/hooks.json`.
- **Enforced prose:** Added P1 mechanical trap linters (`--exact`, `rg -c`, `-p devflow --lib`) to `scripts/lint-plan-bashisms.sh`.
- **Claude Code subagent rules:** Added to `~/.claude/rules/claude-subagent-worktrees.md`.
- **Local IDE state:** Added `**/.claude/settings.local.json` to `.gitignore`.
- **Skills curation:** `skills-lock.json` pruned of stray Office tools (`docx`, `pptx`, `xlsx`, `pdf`) and curated to 9 development skills.
