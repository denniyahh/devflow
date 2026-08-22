# Phase 8 — Documentation & Onboarding

**Status:** ✅ Completed (shipped at v1.0.1) | **Target:** v1.0.1

## Goal

A stranger clones the repo and is productive in under 5 minutes — README, install, doctor, and all standard OSS files in place.

---

## 8a — README Rewrite

Current README is ~Phase 3 vintage. Complete rewrite reflecting v1.0.1 reality.

### Content
- [ ] Accurate command listing (start --worktree, parallel, sequentagent, reference, confirm, rejectpr, cleanup, check, status, ship, init)
- [ ] State machine diagram with worktree mode
- [ ] Agent protocol summary (DEVFLOW_RESULT, three-layer evaluation)
- [ ] Quick start that works on a fresh Ubuntu/macOS machine
- [ ] Configuration reference with all `.devflow.yaml` fields
- [ ] Link to DEPENDENCIES.md, CONTRIBUTING.md, ARCHITECTURE.md

### Removals
- [ ] Remove all tmux references (we use agent-specific CLI + monitor daemon now)
- [ ] Remove `omx` / oh-my-codex (deprecated, no longer supported)
- [ ] Remove `devflow finish`, `devflow config` (don't exist)
- [ ] Remove ROADMAP.md reference (file deleted — planning is in `.planning/`)

---

## 8b — Dependencies & Installation

### 8b.1 — `DEPENDENCIES.md`

Full matrix of every binary devflow depends on, organized by required/optional:

| Dependency | Required | Version | Install |
|---|---|---|---|
| git | ✅ | 2.30+ | Built-in (Linux/macOS) |
| sh/bash | ✅ | Any POSIX | Built-in |
| cargo/rust | ✅ (source) | 1.91+ | rustup.rs |
| gh CLI | ⚠ Phase 7+ | 2.0+ | `brew install gh` / `apt install gh` |
| claude | Per agent | Latest | `npm i -g @anthropic-ai/claude-code` |
| codex | Per agent | Latest | `npm i -g @openai/codex` |
| opencode | Per agent | Latest | `cargo install opencode` |

### 8b.2 — `scripts/install.sh`

Single-command bootstrap for any POSIX system:

```bash
curl -fsSL https://raw.githubusercontent.com/denniyahh/devflow/main/scripts/install.sh | bash
```

- [ ] Detect OS (Linux, macOS)
- [ ] Check git, cargo → install via OS package manager if missing
- [ ] Install devflow: `cargo install devflow` (or download binary release)
- [ ] Check optional deps (gh, claude, codex, opencode) → warn if missing, suggest install commands
- [ ] Verify: `devflow doctor` passes

### 8b.3 — `devflow doctor` command

Audits environment and reports readiness:

```
$ devflow doctor
  git 2.49.0          ✓
  sh (POSIX shell)    ✓
  cargo 1.91.0        ✓
  gh 2.78.0           ✓
  claude              ✓ (v2.1.181)
  codex               ✗ not found — install: npm i -g @openai/codex
  opencode            ✗ not found — install: cargo install opencode
  devflow v1.0.0      ✓
  .devflow.yaml       ✓ (found)
```

- [ ] Runs at start of `devflow init`
- [ ] Available standalone: `devflow doctor [--json]`
- [ ] Non-zero exit if required deps missing
- [ ] JSON output mode for scripting

---

## 8c — Standard Open-Source Files *(pulled from Phase 9)*

Before the README goes live, these must exist so links don't 404:

- [ ] `LICENSE` — MIT OR Apache-2.0
- [ ] `CONTRIBUTING.md` — setup, testing, PR process, code style, commit conventions
- [ ] `CODE_OF_CONDUCT.md` — standard Contributor Covenant
- [ ] `SECURITY.md` — vulnerability reporting process
- [ ] `CHANGELOG.md` — manually maintained (auto-generation later)
- [ ] `.github/ISSUE_TEMPLATE/bug_report.md`
- [ ] `.github/ISSUE_TEMPLATE/feature_request.md`
- [ ] `.github/PULL_REQUEST_TEMPLATE.md`

---

## 8g — Documentation Completeness

- [ ] `devflow --help` covers all subcommands with examples
- [ ] `devflow start --help` explains `--worktree`, `--agent`, `--monitor`, `--force`
- [ ] Every `.devflow.yaml` field documented inline in `devflow init` output
- [ ] Error messages are actionable ("gh CLI not found — install: brew install gh")
- [ ] `devflow doctor` output is copy-pasteable into GitHub issues

---

## Success Criteria

1. A stranger clones the repo, reads README, runs `scripts/install.sh`, and runs `devflow doctor` — all in <5 minutes
2. README.md accurately reflects all v1.1.0 commands and features
3. No tmux, OMX, or other deprecated references in any doc
4. All standard OSS files present and populated (LICENSE, CONTRIBUTING, CoC, Security, templates)
5. `devflow --help` is self-documenting; every subcommand has examples
