# Phase 9 Plan — Open-Source Polish

**Status:** Ready for execution | **Target:** v1.2.0 | **Tests:** cargo test + clippy + fmt

## Execution Order

Areas are ordered by dependency. Within each area, tasks are ordered.

```
9a.4 (OMX removal) → 9a.1 (Dennis cleanup) → 9a.2 (ARCHITECTURE) → 9a.3 (agent verification)
→ 9a.5 (ship branch fix) → 9a.6 (doc correctness) → 9b.1 (dev container) → 9b.2 (distrobox) → 9c (CI polish)
```

---

## 9a.4 — Remove OMX Agent Support

**Why first:** Touches multiple files; cleanest to remove before verifying architecture.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Delete OMX adapter file | `crates/devflow-core/src/agents/omx.rs` | File gone, `cargo build` passes |
| 2 | Remove `Omx` from `AgentKind` enum | `crates/devflow-core/src/state.rs` | `Omx` variant gone, tests pass |
| 3 | Remove `Omx` from `adapter_for()` + module exports | `crates/devflow-core/src/agents/mod.rs` | No `omx` in match arms |
| 4 | Strip OMX from `phase_prompt()` | `crates/devflow-core/src/agents/mod.rs` | Grep: no `omx` in prompt text |
| 5 | Remove OMX from skill docs | `skills/hermes/devflow/SKILL.md` | No `omx` in SKILL.md |
| 6 | Delete stale `.omx/` directory | `.omx/` | Directory gone |
| 7 | Grep for remaining OMX references | Entire repo | Zero hits for `(?i)omx\|oh.my.codex` |

**Estimated:** 1 commit, ~80 lines removed.

---

## 9a.1 — Remove Dennis-Specific Assumptions

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Remove `distrobox.ini` from tracked repo | `distrobox.ini`, `.gitignore` | File gone or gitignored |
| 2 | Resolve `.planning/` tracked vs gitignored | `.gitignore`, `agents/mod.rs` | Either remove `.planning` refs from prompt OR document it as convention |
| 3 | Document GPG test setup in CONTRIBUTING.md | `CONTRIBUTING.md` | Clear instructions for `git config commit.gpgsign false` in test setup |

**Estimated:** 1 commit, ~30 lines changed.

---

## 9a.2 — Architecture Documentation

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Write crate structure section | `ARCHITECTURE.md` | Accurate to `crates/` layout |
| 2 | Write state machine section | `ARCHITECTURE.md` | Matches `Step` enum in `state.rs` |
| 3 | Write agent trait section | `ARCHITECTURE.md` | Matches `Agent` trait + `adapter_for()` |
| 4 | Write three-layer evaluation section | `ARCHITECTURE.md` | Matches `agent_result.rs` (DEVFLOW_RESULT → exit code → existence) |
| 5 | Write monitor daemon section | `ARCHITECTURE.md` | Direct process spawning, stdout capture, PID tracking |
| 6 | Write worktree model section | `ARCHITECTURE.md` | Paths, branches, sequentagent rebase flow |
| 7 | Write configuration section | `ARCHITECTURE.md` | `.devflow.yaml` schema, all fields documented |
| 8 | Write extension points section | `ARCHITECTURE.md` | Agent adapter checklist (actual steps, not "3 changes max") |

**Estimated:** 1 commit, ~200 lines.

---

## 9a.3 — Agent-Agnosticism Verification

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Verify no agent-specific logic in core state machine | `state.rs` | Review: no Claude/Codex branches |
| 2 | Verify adapters are isolated | `agents/*.rs` | Each agent in own file, shared trait |
| 3 | Verify prompt generation is shared | `agents/mod.rs` | `phase_prompt()` used by all adapters |
| 4 | Document actual adapter checklist | `ARCHITECTURE.md`, `CONTRIBUTING.md` | Checklist: new file, AgentKind variant, adapter_for entry, module export, parser/display update |
| 5 | Verify no Claude/Codex-specific code outside adapters | Full codebase | Audit complete |

**Estimated:** 1 commit, ~40 lines docs + verification notes.

---

## 9a.5 — Ship Branch Safety Fix

**Bug:** `release_start()` in `git.rs` hardcodes `checkout develop`, so if you run `devflow ship` from a feature branch with unmerged commits, it silently abandons them — branches from develop instead of the current branch. This is what caused the Phase 8 data loss.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Change `release_start()` to branch from current HEAD, not develop | `crates/devflow-core/src/git.rs` | `release_start("1.0.0")` on `feature/x` creates `release/1.0.0` from `feature/x` |
| 2 | Remove any compensating logic in `ship.rs` (if the earlier "must be on feature branch" check was a workaround for this) | `crates/devflow-cli/src/main.rs` or `crates/devflow-core/src/ship.rs` | Ship still works from feature branches |
| 3 | Add test: ship from feature branch with unmerged commits — those commits appear in release branch | `crates/devflow-core/tests/` | Test passes |

**Estimated:** 1 commit, ~20 lines changed.

---

## 9a.6 — Document Correctness

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Verify README command table matches CLI | `README.md` vs `main.rs` Command enum | All commands present, none deprecated |
| 2 | Verify no `tmux`, `devflow finish`, `omx` in README | `README.md` | Grep clean |
| 3 | Verify `.devflow.yaml` sample in README matches Config | `README.md` vs `config.rs` | No phantom fields (e.g., `enabled`) |
| 4 | Update CONTRIBUTING.md for fork PR workflow | `CONTRIBUTING.md` | Fork + PR instructions clear |
| 5 | Verify CHANGELOG.md is current through v1.0.1 | `CHANGELOG.md` | All versions listed |

**Estimated:** 1 commit, ~50 lines changed.

---

## 9b.1 — Dev Container

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Create `.devcontainer/devcontainer.json` | `.devcontainer/devcontainer.json` | Valid JSON, correct features |
| 2 | Verify `cargo build` passes in container | Manual or CI | Build succeeds |

**Estimated:** 1 commit, ~20 lines.

---

## 9b.2 — Distrobox Optional

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Remove or gitignore `distrobox.ini` | `distrobox.ini`, `.gitignore` | File not tracked |
| 2 | Document distrobox as optional in CONTRIBUTING.md | `CONTRIBUTING.md` | Optional workflow section |

**Estimated:** Part of 9a.1 commit.

---

## 9c — CI Polish

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Add status badge to README | `README.md` | Badge renders on GitHub |
| 2 | Verify CI runs on fork PRs without secrets | `.github/workflows/ci.yml` | No `pull_request_target`, no secrets |
| 3 | Add `rust-toolchain.toml` (or document stable policy) | `rust-toolchain.toml` | Toolchain pinned |
| 4 | Verify `cargo fmt --check` in CI | `.github/workflows/ci.yml` | Fmt check step present |

**Estimated:** 1 commit, ~30 lines.

---

## Commit Plan

| # | Area | Files | Lines |
|---|------|-------|-------|
| 1 | 9a.4 | OMX removal (~7 files) | -80 |
| 2 | 9a.1 | Dennis cleanup (3 files) | -30 |
| 3 | 9a.2 | ARCHITECTURE.md | +200 |
| 4 | 9a.3 + 9a.5 | Agent verification + ship branch fix | +30 |
| 5 | 9a.6 | Doc correctness | +50 |
| 6 | 9b + 9c | Dev container + CI | +70 |
| **Total** | | | ~460 changed, 6 commits |

## Verification Gates

After each commit:
- `cargo build --release`
- `cargo test`
- `cargo clippy -- -D warnings`

Before ship:
- `cargo fmt -- --check`
- Manual: open dev container and verify `cargo build`
- Grep: zero OMX/tmux references in source

## Deferred (Phase 11)

- Audit log subsystem (JSONL storage, `devflow audit`, lifecycle instrumentation, rotation)
- Multi-platform release workflow (Linux x86_64/aarch64 + macOS x86_64/aarch64 binaries)
- Dockerfile
