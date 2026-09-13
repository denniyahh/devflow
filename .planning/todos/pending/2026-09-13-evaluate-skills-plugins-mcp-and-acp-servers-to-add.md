---
created: 2026-09-13T18:37:15.901Z
title: Evaluate skills, plugins, MCP and ACP servers to add
area: tooling
severity: minor
files:
  - ~/.claude/settings.json
  - ~/.claude.json
  - ~/.claude/skills/
  - ~/.codex/config.toml
  - ~/.config/opencode/opencode.json
---

## Problem

The agent tooling has never been reviewed systematically, either for gaps worth filling or for dead weight. Inventory taken 2026-09-13 (names only, no config values):

- **Claude Code MCP servers** (user scope): `github`, `mempalace`.
- **claude.ai connectors** available to Claude Code: Gmail, Google Calendar, Google Drive, Linear, Vercel. Linear was retired 2026-09-02.
- **Claude Code plugins enabled:** `rust-analyzer-lsp@claude-plugins-official`, `vercel-plugin@vercel-vercel-plugin`.
- **User skills:** 95 in `~/.claude/skills/`:
  - 72 `gsd-*`;
  - 7 Azure or Microsoft (`azure-*`, `microsoft-foundry`, `entra-app-registration`);
  - 16 others: `code-review-excellence`, `deployment-pipeline-design`, `docx`, `error-handling-patterns`, `external-review`, `find-docs`, `find-skills`, `git-advanced-workflows`, `github-actions-templates`, `graphify`, `pdf`, `pptx`, `rust-async-patterns`, `secrets-management`, `skill-creator`, `xlsx`.

  Every installed skill adds to the skill listing loaded into each session.
- **Codex MCP servers:** `node_repl`, `cua_repl`, `github`, `mempalace`.
- **opencode MCP servers:** `gsd`.
- **ACP clients and agents on PATH:** `zed`, `goose`, `opencode`, `pi`, `hermes`, `agy`. No Claude Code or codex ACP adapter is on PATH.
- **Documentation lookup** uses the `npx ctx7` CLI (`~/.claude/rules/context7.md`), not an MCP server.

## Solution

TBD. Criteria to apply to every candidate:

- actively maintained, from a reputable source;
- least-privilege scopes;
- local or offline where possible;
- measured context/token cost (the phase 47 retrospective found about 52 KB of standing context per session already);
- Rust or Zig implementations preferred when otherwise comparable (operator preference);
- installable without `dnf`, since the host is immutable Fedora: brew, cargo, npx or flatpak;
- a supply-chain review before install.

Areas to look at:

- ACP adapters so Zed and other ACP clients can drive Claude Code or codex.
- Rust development and testing aids beyond `rust-analyzer-lsp`.
- Anything that turns the retrospective's recurring verification traps into mechanical checks.

Removals belong with the todo "Review and normalize agent instruction files across tools".
