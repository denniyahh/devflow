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

## Solution (Resolved 2026-09-13)

1. **Standing Context Pruning (Option A adopted):**
   - Removed 11 unused skills from `~/.claude/skills/` to lower standing prompt token overhead:
     - 7 Azure/Microsoft skills: `azure-deploy`, `azure-hosted-copilot-sdk`, `azure-prepare`, `azure-resource-lookup`, `azure-validate`, `entra-app-registration`, `microsoft-foundry`.
     - 4 Office skills: `docx`, `pdf`, `pptx`, `xlsx`.
   - Remaining skills inventory: 84 skills (72 `gsd-*` + 12 core development skills).
2. **Plugins & Connectors:**
   - Confirmed `vercel-plugin` is disabled (`false`) in `~/.claude/settings.json`.
   - Linear connector confirmed retired.
3. **Rust Development & Verification Tooling:**
   - Verified `rust-analyzer-lsp` is active in Claude Code (`settings.json`) and Codex (`config.toml`).
   - Verified host Rust verification toolchain (`cargo-deny`, `cargo-machete`, `cargo-nextest`, `cargo-mutants`, `cargo-llvm-cov`, `cargo-semver-checks`, `miri`) is available on PATH.
   - P1 verification trap linters implemented in `scripts/lint-plan-bashisms.sh` and enforced via `pre-commit`.
4. **Documentation & ACP Ecosystem:**
   - Documentation lookup retained on `npx ctx7` CLI (zero background daemon / token overhead).
   - ACP clients noted on PATH (`zed`, `goose`, `opencode`, `pi`, `hermes`, `agy`); no additional adapter needed for current devflow workflows.
