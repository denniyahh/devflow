# DevFlow — Feature Assessment

> Generated: 2026-06-17 | Comparing ROADMAP.md aspirations against actual source code

## v0.1.0 — Core Library & CLI

| Feature | ROADMAP | Actual | Notes |
|---|---|---|---|
| `state.rs` — State machine | ✅ | ✅ 223 lines | `Step` enum, `State` struct, `Agent` enum, `advance()` all present |
| `config.rs` — YAML parsing | ✅ | ✅ 294 lines | Full serde config with defaults, `should_skip()` |
| `git.rs` — Git flow | ✅ | ✅ 119 lines | `feature_start/finish`, `release_start/finish`, `cleanup_merged` |
| `tmux.rs` — Agent launch | ✅ | ✅ 64 lines | Fixed 2026-06-17: command as main process, not sh+send-keys |
| `version.rs` — Semver bump | ✅ | ✅ 164 lines | `read_version`, `bump()`, `write_version` — pyproject.toml only |
| `workflow.rs` — State persistence | ✅ | ✅ 99 lines | `save_state()`, `load_state()`, `clear_state()`, `advance_state()` |
| `devflow start` | ✅ | ✅ | Creates branch, launches agent, spawns monitor, persists state |
| `devflow check` | ✅ | ✅ | Polls tmux, advances state machine |
| `devflow status` | ✅ | ✅ | Shows step, phase, agent, session, running status |
| `devflow ship` | ✅ | ✅ | Bumps version, creates release branch |
| `devflow init` | ✅ | ✅ | Bootstraps `.devflow.yaml` + `.devflow/` |
| `devflow config` | ✅ | ✅ | Shows effective config in YAML |
| Tests (2) | ✅ | ✅ | `bumps_semver_components`, `parses_devflow_yaml_shape` |

**v0.1.0: 100% complete.**

---

## v0.2.0 — Monitor & Hermes Integration

| Feature | ROADMAP | Actual | Notes |
|---|---|---|---|
| `monitor.rs` — spawn_monitor() | ❌ TODO | ✅ 87 lines | **DONE — undocumented in ROADMAP** |
| Child process: poll tmux | ❌ TODO | ✅ | Shell script with `while tmux has-session` loop |
| Parent returns PID | ❌ TODO | ✅ | PID written to state file |
| `devflow start --monitor` flag | ❌ TODO | ✅ | `--monitor` / `--no-monitor` flags exist in CLI |
| Recovery: `devflow recover` | ❌ TODO | ✅ 127 lines | Stale detection (>24h), lock check, re-launch |
| `skills/hermes/devflow/SKILL.md` | ❌ TODO | ❌ | **MISSING** |
| Git flow CLI backend | ❌ TODO | ❌ | Only raw git commands, no `git-flow` CLI fallback |
| Better error messages for missing develop | ❌ TODO | ❌ | Not implemented |

**v0.2.0: 57% complete (4/7).** Monitor done, Hermes skill + git-flow CLI missing.

---

## v0.3.0 — Robustness & Multi-Project

| Feature | ROADMAP | Actual | Notes |
|---|---|---|---|
| `devflow recover` | ❌ TODO | ✅ 127 lines | **DONE — undocumented in ROADMAP** |
| Stale state detection (>24h) | ❌ TODO | ✅ | In `recover.rs` |
| Lock file (concurrent check) | ❌ TODO | ✅ 83 lines | `lock.rs`: `acquire()`, `release()`, stale lock detection |
| SIGTERM handler in monitor | ❌ TODO | ✅ | `trap cleanup TERM INT` in monitor shell script |
| `devflow list` — multi-project | ❌ TODO | ❌ | **MISSING** |
| Global config `~/.config/devflow/` | ❌ TODO | ❌ | **MISSING** |
| Project name from git remote | ❌ TODO | ❌ | Uses directory name only |
| Cargo.toml version support | ❌ TODO | ❌ | **MISSING — devflow can't bump its own version** |
| package.json version support | ❌ TODO | ❌ | **MISSING** |
| Calver scheme | ❌ TODO | ❌ | **MISSING** |
| Build metadata suffix | ❌ TODO | ❌ | **MISSING** |

**v0.3.0: 36% complete (4/11).** Recovery/lock/SIGTERM done. Version bumper expansion, multi-project, and calver missing.

---

## v0.4.0 — Agent Trait & Verification

| Feature | ROADMAP | Actual | Notes |
|---|---|---|---|
| `Agent` trait | ❌ TODO | ❌ | **MISSING — still an enum** |
| Per-agent impls (claude.rs, etc.) | ❌ TODO | ❌ | **MISSING** |
| Agent-specific output parsing | ❌ TODO | ❌ | **MISSING** |
| Agent config in `.devflow.yaml` | ❌ TODO | ❌ | **MISSING** |
| `devflow verify` — run verification | ❌ TODO | ❌ | **MISSING — step exists but is a no-op** |
| Config: `verify_command` | ❌ TODO | 🟡 | Field exists in config but never read/executed |
| Config: `lint_command` | ❌ TODO | 🟡 | Field exists but never executed |
| Fail-fast / continue-on-error | ❌ TODO | ❌ | Config has `continue_on_error` but not used |
| `devflow docs` | ❌ TODO | ❌ | **MISSING — step exists but is a no-op** |
| Auto-commit docs changes | ❌ TODO | ❌ | **MISSING** |

**v0.4.0: 0% complete.** Entire layer is scaffolded (config fields, state machine steps) but nothing executes.

---

## v1.0.0 — Ship-Ready

| Feature | ROADMAP | Actual | Notes |
|---|---|---|---|
| PR creation via `gh` CLI | ❌ TODO | ❌ | **MISSING** |
| PR body from phase SUMMARY.md | ❌ TODO | ❌ | **MISSING** |
| Review request automation | ❌ TODO | ❌ | **MISSING** |
| Merge detection | ❌ TODO | ❌ | **MISSING** |
| LICENSE | ❌ TODO | ✅ | MIT |
| CONTRIBUTING.md | ❌ TODO | ✅ | Exists |
| CODE_OF_CONDUCT.md | ❌ TODO | ❌ | **MISSING** |
| CHANGELOG.md | ❌ TODO | ❌ | **MISSING** |
| GitHub CI | ❌ TODO | ❌ | **MISSING — no `.github/workflows/`** |
| Release workflow | ❌ TODO | ❌ | **MISSING** |
| `cargo install` / install script | ❌ TODO | ❌ | Brew symlink only |

**v1.0.0: 18% complete (2/11).** Docs exist, everything else missing.

---

## Summary

| Version | Complete | Key Gaps |
|---|---|---|
| v0.1.0 | 100% (13/13) | — |
| v0.2.0 | 57% (4/7) | Hermes skill, git-flow CLI |
| v0.3.0 | 36% (4/11) | Cargo.toml versioner, multi-project, calver |
| v0.4.0 | 0% (0/10) | Agent trait, verify/docs execution |
| v1.0.0 | 18% (2/11) | CI/CD, PR integration, release workflow |
| **Overall** | **44% (23/52)** | |

## Top Findings (from codebase map CONCERNS.md)

| # | Severity | Issue |
|---|---|---|
| 1 | 🔴 Critical | No tests (5% coverage, 2/30 functions) |
| 2 | 🔴 Critical | No CI pipeline |
| 3 | 🔴 Critical | `unwrap()` in library code (`lock.rs:31`) |
| 4 | 🟡 High | No Cargo.toml versioner — can't self-bump |
| 5 | 🟡 High | Agent enum not trait — brittle extension |
| 6 | 🟡 High | Verify/docs steps are no-ops |
| 7 | 🟠 Medium | Stale ROADMAP + AGENTS.md |
| 8 | 🟠 Medium | No Hermes skill |
| 9 | 🟢 Low | No clippy config, hardcoded sleep 30s, no Windows |
