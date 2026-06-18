# Phase 9 — Open-Source Polish

**Status:** Final requirements (post-Codex review) | **Priority:** HIGH | **Target:** v1.2.0

## Goal

A stranger can discover, understand, build, and contribute to DevFlow — no Dennis-specific assumptions, clean architecture docs, CI gates every PR, OMX fully removed.

**Deferred to Phase 11:** Audit log subsystem, multi-platform release workflow, Dockerfile.

---

## 9a — Open-Source Readiness Audit

### 9a.1 — Remove Dennis-specific assumptions

| What | Where | Fix |
|---|---|---|
| `distrobox.ini` | Repo root | Delete or gitignore |
| GPG signing in tests | `git.rs`, `monitor.rs` tests | Already handled. Document required test setup. |
| `.planning/` tracked vs gitignored | `.gitignore`, prompt code | Resolve: either remove `.planning` refs from `phase_prompt()`, or document it as devflow convention. Cannot claim "gitignored" while tracked files exist. |
| Stale `.omx/` directory | Repo root | Delete — OMX session artifacts, not devflow source |
| `~/.local/bin/devflow` path | Devflow skill | Hermes-specific. No code change needed. |

### 9a.2 — Architecture documentation (`ARCHITECTURE.md`)

- [ ] Crate structure: `devflow-core` (library) + `devflow-cli` (binary)
- [ ] State machine: Step enum, transitions, persistence
- [ ] Agent trait: `Agent`, `exec_command`, `adapter_for`
- [ ] Three-layer evaluation: DEVFLOW_RESULT → exit code → existence
- [ ] Monitor daemon: direct process spawning, stdout capture, PID tracking
- [ ] Worktree model: paths, branches, sequentagent rebase flow
- [ ] Configuration: `.devflow.yaml` schema, all fields documented
- [ ] Extension points: agent adapter checklist (not "3 changes max" — actual checklist)

### 9a.3 — Agent-agnosticism verification

- [ ] No agent-specific logic in core (state machine, git, version, config)
- [ ] Agent adapters isolated in `agents/` with shared trait
- [ ] Prompt generation shared (`phase_prompt()` in `mod.rs`)
- [ ] Document actual adapter checklist: new adapter file, AgentKind variant, adapter_for() entry, module export, parser/display update, test fixture — no false "3 changes max" claim
- [ ] No Claude-specific or Codex-specific code outside their adapter files

### 9a.4 — Remove OMX agent support (officially dropped)

- [ ] Delete `crates/devflow-core/src/agents/omx.rs`
- [ ] Remove `Omx` from `AgentKind` enum in `state.rs`
- [ ] Remove `Omx` from `agents::adapter_for()` and module exports
- [ ] Strip OMX from `phase_prompt()` (agents/mod.rs)
- [ ] Remove OMX from `skills/hermes/devflow/SKILL.md` (all 4 references)
- [ ] Delete stale `.omx/` directory (OMX session artifacts)
- [ ] Grep for `(?i)omx|oh.my.codex` — ensure zero remaining references in source

### 9a.5 — Document correctness

- [ ] README matches current CLI (no `tmux`, `devflow finish`, `omx`, stale command tables)
- [ ] CONTRIBUTING.md accurate (fork PR workflow, agent adapter checklist)
- [ ] `.devflow.yaml` schema in README matches actual `Config` parser (no phantom `enabled` field)
- [ ] Deprecated commands removed from all public docs

---

## 9b — Contribution Infrastructure

### 9b.1 — Dev container

`.devcontainer/devcontainer.json`:
```json
{
  "image": "mcr.microsoft.com/devcontainers/rust:1",
  "features": {
    "ghcr.io/devcontainers/features/git:1": {},
    "ghcr.io/devcontainers/features/github-cli:1": {}
  },
  "postCreateCommand": "cargo build",
  "customizations": {
    "vscode": {
      "extensions": ["rust-lang.rust-analyzer", "tamasfe.even-better-toml"]
    }
  }
}
```

- [ ] One-click dev environment — clone, open in VS Code, accept container
- [ ] `cargo build` passes on container creation
- [ ] `gh` is convenience-only — build/test work unauthenticated

### 9b.2 — Distrobox optional

- [ ] Remove `distrobox.ini` from tracked repo (or gitignore)
- [ ] Document distrobox as optional in CONTRIBUTING.md
- [ ] Dev container is recommended setup for new contributors

---

## 9c — CI Polish

- [ ] CI runs on PRs from forks (no secrets, no `pull_request_target`)
- [ ] `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` on every PR
- [ ] Status badge in README
- [ ] Toolchain policy: document "stable Rust" (add `rust-toolchain.toml` or remove pinned 1.91 claims)

---

## Success Criteria

1. No Dennis-specific paths, tools, or assumptions in codebase
2. `ARCHITECTURE.md` documents actual design (crates, state machine, agents, monitor, worktree, config)
3. Agent-agnosticism verified — actual adapter checklist documented, no false claims
4. OMX fully removed — zero references in source, `.omx/` directory deleted
5. Dev container: clone → open → `cargo build` works in <2 minutes
6. CI has status badge, runs fork-safe PR checks (no secrets)
7. All public docs (README, CONTRIBUTING, CHANGELOG) match current CLI and agents
8. `distrobox.ini` removed or gitignored
