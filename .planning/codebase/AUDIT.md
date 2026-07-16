# DevFlow — Feature Audit & Architecture Review

> 2026-06-18 | v0.5.0 | 85 tests | 11 CLI commands

---

## 1. Architecture Overview

DevFlow is a Rust workspace with two crates:
- **`devflow-core`** (library) — state machine, git, config, agents, monitor, recover, lock, version, verify, workflow
- **`devflow-cli`** (binary) — thin CLI wrapper around core, ~590 lines

### Module Map

```
devflow-core/src/
├── config.rs     ← .devflow.yaml parser (custom, not serde_yaml)
├── state.rs      ← 7-step state machine + Agent enum
├── agent.rs      ← process spawning (launch_agent, agent_running)
├── agents/       ← Agent trait + per-agent adapters (Phase 5)
│   ├── mod.rs    ← trait + adapter_for()
│   ├── claude.rs
│   ├── codex.rs
│   ├── omx.rs    ← disabled, preserved
│   └── opencode.rs
├── git.rs        ← git-flow operations + list_feature_branches
├── lock.rs       ← PID file lock (O_EXCL)
├── monitor.rs    ← background daemon (kill -0 polling)
├── recover.rs    ← stale state detection + cleanup
├── verify.rs     ← sh -c command runner
├── version.rs    ← semver bumper + multi-format reader
└── workflow.rs   ← state persistence + advance logic
```

---

## 2. Flow Diagram

```
                          ┌─────────────────┐
                          │   devflow init   │  Bootstrap .devflow.yaml
                          └────────┬────────┘
                                   │
                          ┌────────▼────────┐
                          │  devflow start   │  --phase N --agent claude [--monitor] [--force]
                          │  ─────────────── │
                          │  1. Load config  │
                          │  2. Create       │
                          │     feature/     │
                          │     phase-NN     │
                          │  3. Spawn agent  │
                          │  4. Spawn monitor│
                          │     (optional)   │
                          │  5. Save state   │
                          └────────┬────────┘
                                   │
                    ┌──────────────┼──────────────┐
                    │              │              │
              ┌─────▼─────┐  ┌────▼────┐  ┌─────▼─────┐
              │  MONITOR   │  │  AGENT  │  │  NO MONITOR│
              │  (daemon)  │  │ (child) │  │  (blocking)│
              │            │  │         │  │            │
              │ poll kill-0│  │ claude  │  │ wait for   │
              │ every 30s  │  │ codex   │  │ agent exit │
              │            │  │ opencode│  │            │
              │ on exit →  │  │         │  │ on exit →  │
              │ devflow    │  │ produces│  │ user runs  │
              │ check ×5   │  │ code +  │  │ devflow    │
              │            │  │ commits │  │ check      │
              └─────┬──────┘  └─────────┘  └─────┬──────┘
                    │                            │
                    └──────────┬─────────────────┘
                               │
                      ┌────────▼────────┐
                      │  devflow check   │  Poll + advance
                      │  ─────────────── │
                      │  1. Acquire lock │
                      │  2. Load state   │
                      │  3. Check agent  │
                      │     running?     │
                      │  4. Advance step │
                      │                  │
                      │  ┌───────────────┤
                      │  │ VERIFYING     │
                      │  │  auto_verify? │
                      │  │  → cargo test │
                      │  │  → cargo clippy│
                      │  │               │
                      │  │ DOCSING       │
                      │  │  auto_docs?   │
                      │  │  → cargo doc  │
                      │  │               │
                      │  │ SHIPPING      │
                      │  │  auto_ship?   │
                      │  │  → bump ver   │
                      │  │  → release br │
                      │  │               │
                      │  │ CLEANING      │
                      │  │  → del merged │
                      │  │               │
                      │  │ → IDLE        │
                      │  │  clear state  │
                      │  └───────────────┘
                      └────────┬────────┘
                               │
                    ┌──────────┼──────────┐
                    │          │          │
              ┌─────▼─────┐ ┌──▼───┐ ┌───▼──────┐
              │devflow    │ │devflow│ │devflow   │
              │ status    │ │ list  │ │ ship     │
              │           │ │       │ │          │
              │ step: X   │ │BRANCH │ │ bump ver │
              │ phase: N  │ │ AHEAD │ │ release  │
              │ agent: Y  │ │BEHIND │ │ branch   │
              │ pid: Z    │ │ DATE  │ │          │
              │           │ │       │ │          │
              │ open      │ │       │ │          │
              │ branches: │ │       │ │          │
              │  • feat/5 │ │       │ │          │
              └───────────┘ └──────┘ └──────────┘

       ┌──────────┐    ┌───────────┐    ┌──────────┐
       │ devflow  │    │ devflow   │    │ devflow  │
       │ config   │    │ recover   │    │ verify   │
       │          │    │           │    │ lint     │
       │ show     │    │ inspect   │    │ docs     │
       │ .devflow │    │ stale?    │    │          │
       │ .yaml    │    │ agent     │    │ standalone│
       │          │    │ alive?    │    │ commands │
       │          │    │ --clean   │    │          │
       └──────────┘    └───────────┘    └──────────┘
```

### State Machine (7 steps)

```
IDLE ──→ BRANCHING ──→ EXECUTING ──→ VERIFYING ──→ DOCSING ──→ SHIPPING ──→ CLEANING ──→ IDLE
  │         │             │             │           │           │            │           │
  │    auto_branch     agent runs    auto_verify  auto_docs  auto_ship   auto_cleanup  clear
  │    creates         as child      runs verify  runs docs  bumps ver   deletes      state
  │    feature/        process       + lint cmd   command    creates     merged
  │    phase-NN                                   + commit   release br  branches
  │                                                         (PR in v1.0)
  │
  └── always skippable: none (Idle, Branching, Executing never skip)
      config-skippable: Verifying (auto_verify=false)
                        Docsing (auto_docs=false)
                        Shipping (always runs)
                        Cleaning (auto_cleanup=false)
```

---

## 3. Complete Feature Catalog

### Core Workflow

| Feature | How | Config |
|---|---|---|
| **Phase start** | `devflow start --phase N --agent claude` | `auto_branch` |
| **Feature branching** | `git checkout -b feature/phase-NN` from develop | `git_flow.feature_prefix` |
| **Agent launch** | Spawns Claude/Codex/OpenCode as child process | — |
| **Rich prompts** | Generated per-phase with CONTEXT.md references | — |
| **PID monitoring** | `kill -0 <pid>` check via libc | — |
| **Background monitor** | Shell daemon polls every 30s, runs `check ×5` on exit | `--monitor` flag |
| **Blocking mode** | Without `--monitor`, waits for agent to exit | — |
| **State persistence** | `.devflow/state.json` (JSON, pretty-printed) | — |
| **State advancement** | `devflow check` — locks, checks agent, advances | — |
| **Step skipping** | Config-driven: `auto_verify`, `auto_docs`, `auto_cleanup` | AutomationConfig |
| **Error tolerance** | `continue_on_error` — don't halt on verify/lint failure | AutomationConfig |

### Git Operations

| Feature | How |
|---|---|
| **Feature branch create** | `checkout -b` from develop (errors if exists) |
| **Feature branch force** | `checkout -B` with `--force` flag |
| **Feature finish** | `merge --no-ff` into develop, delete branch |
| **Release branch** | `release_start` / `release_finish` — creates, tags, merges to main+develop |
| **Cleanup merged** | Deletes local branches already merged |
| **Commit all** | `git add . && git commit --allow-empty` |
| **List branches** | Shows `feature/phase-*` with ahead/behind/date (Phase 5) |

### Version Management

| Feature | How |
|---|---|
| **Read version** | Parse from Cargo.toml, pyproject.toml, or package.json |
| **Auto-detect format** | Checks Cargo.toml → pyproject.toml → package.json |
| **Workspace support** | Reads `workspace.package.version` in Cargo.toml |
| **Semver bump** | `major`, `minor`, `patch`, `none` |
| **Build number** | `git rev-list --count` or Unix timestamp |
| **Write version** | Replace version string in-place |

### Safety & Recovery

| Feature | How |
|---|---|
| **Concurrency lock** | `.devflow/lock` PID file, O_EXCL creation |
| **Lock inspection** | `holder()` reads PID from lock file |
| **Stale lock cleanup** | Empty lock files auto-cleaned |
| **State recovery** | `devflow recover` — shows age, agent status, lock holder |
| **Stale detection** | >24h old + no running agent = stale |
| **State cleanup** | `devflow recover --clean` — removes state + lock |

### Verification & Docs

| Feature | How |
|---|---|
| **Verify** | Runs `verify_command` via `sh -c` |
| **Lint** | Runs `lint_command` via `sh -c` |
| **Docs** | Runs `docs_command`, optional auto-commit (`docs_auto_commit`) |
| **Standalone commands** | `devflow verify`, `devflow lint`, `devflow docs` — run anytime |

### CLI Commands

| # | Command | Purpose |
|---|---|---|
| 1 | `start` | Begin phase — branch + agent + monitor |
| 2 | `check` | Poll state, advance if agent done |
| 3 | `status` | Show current state + open branches |
| 4 | `list` | Table of feature branches with divergence |
| 5 | `ship` | Bump version + create release branch |
| 6 | `init` | Bootstrap `.devflow.yaml` |
| 7 | `config` | Show effective config |
| 8 | `recover` | Inspect/clean stale state |
| 9 | `verify` | Run verify + lint commands |
| 10 | `lint` | Run lint only |
| 11 | `docs` | Run docs command |

### Test Coverage

| Module | Tests | Key areas |
|---|---|---|
| config | 11 | Parse, defaults, comments, booleans, auto-detect, round-trip |
| state | 12 | Step chain, skip logic, agent enum, serde, exec_command |
| agent | 3 | agent_running (self), dead pid, label |
| agents/mod | 1 | adapter_for names |
| git | 5 | feature start/finish, release, cleanup, missing branch |
| lock | 5 | acquire, contend, release, drop, empty cleanup |
| recover | 7 | Stale detection, age formatting, missing state, live agent |
| verify | 6 | true/false, echo capture, run_or_fail, stderr |
| version | 10 | Read/write formats, bump, build number, edge cases |
| workflow | 6 | Save/load, advance, skip, terminal, cleanup disabled |
| **Total** | **85** | |

---

## 4. Expert Review — What's Missing or Could Be Better

### 🔴 Critical Gaps

| # | Gap | Impact |
|---|---|---|
| 1 | **No remote push** | Everything is local-only. `devflow ship` creates a release branch but doesn't push to origin. User must manually `git push`. Makes devflow feel incomplete. |
| 2 | **Monitor survives agent crash but silently** | If agent exits with non-zero, monitor still advances state. No failure signal — workflow proceeds as if phase succeeded. |
| 3 | **No phase completion verification** | `devflow check` advances through VERIFYING/DOCSING but doesn't check if the agent actually produced commits. A phase can "complete" with zero work done. |
| 4 | **Custom YAML parser is fragile** | `config.rs` has a hand-written line parser instead of using `serde_yaml`. No support for nested structures, lists, or multi-line strings. Adding `agents:` config (Phase 5A-4) will be painful. |

### 🟡 Important Gaps

| # | Gap | Impact |
|---|---|---|
| 5 | **No `devflow rebase`** | Feature branches go stale. Post-mortem identified this as root cause #3 of the phase-04 divergence. |
| 6 | **No agent output capture for monitor mode** | With `--monitor`, agent stdout/stderr is discarded. If it fails, there's no log to debug. |
| 7 | **No dry-run mode** | Can't preview what `devflow start` or `devflow ship` will do without executing. |
| 8 | **No pre-start divergence check** | `devflow start` doesn't warn if develop has advanced significantly. You can start from a stale base. |
| 9 | **Monitor run-once semantics** | Monitor runs `check ×5` then exits. If state machine is stuck (e.g., verify fails, continue_on_error=false), the remaining checks are wasted. |
| 10 | **No `--phase` flag on verify/lint/docs** | Standalone commands run against current state, but there's no way to verify a specific phase. |
| 11 | **Agent trait not fully integrated** | `state.rs` delegates `exec_command()` to the trait, but `agent.rs::launch_agent()` still takes `&State` instead of `&dyn Agent`. The trait exists but isn't the primary interface. |

### 🟢 Nice-to-Have

| # | Gap | Impact |
|---|---|---|
| 12 | **No `--dry-run` on ship** | Can't preview version bump before executing. |
| 13 | **No log/history** | No record of past phases. `devflow status` only shows current state. |
| 14 | **No agent timeout** | Agents run indefinitely. If Claude hangs, monitor waits forever. |
| 15 | **No parallel phase detection** | If two people start the same phase, they create conflicting branches with no warning. |
| 16 | **No `.devflow.yaml` schema validation** | Invalid config values accepted silently. Unknown keys ignored (good for forward compat, bad for typos). |
| 17 | **No `cargo install` path tested** | ROADMAP mentions it for v1.0.0 but zero work done. |
| 18 | **No `devflow logs` command** | Agent output is lost after the process exits (especially in monitor mode). |

---

## 5. Architecture Observations

### What's Good

- **Clean module boundaries** — Each module has one responsibility, well-documented
- **Error handling** — Every module uses `thiserror`, no unwraps in library code
- **Test coverage** — 85 tests covering state machine, config parsing, git operations, edge cases
- **Lock discipline** — O_EXCL file lock prevents concurrent `devflow check` races
- **State recovery** — Stale detection (>24h) with `devflow recover` is robust
- **Config auto-detection** — Works with Cargo.toml, pyproject.toml, package.json without config
- **Monitor design** — Simple shell script, no cron dependency, SIGTERM-safe

### What's Questionable

- **Custom YAML parser** — Reinvents the wheel. `serde_yaml` would give proper deserialization, list support, and error messages. The current parser is 200+ lines of manual string munging that will break on any YAML feature beyond `key: value`.
- **Agent trait lives alongside old code** — `agents/` module is wired up but `agent.rs` still has `launch_agent(&State)` that internally calls `state.agent.exec_command()`. The trait is a pass-through, not the primary interface.
- **Monitor as shell script** — Spawns `sh -c` with inline script. Works but: no structured output, PID-based process tracking can race, and 5 hardcoded `check` calls assume state machine advances exactly 5 times.
- **Version bumper writes back via string replacement** — `write_version()` does regex-like replacement in file contents. Fragile if version appears in comments or multiple places.
