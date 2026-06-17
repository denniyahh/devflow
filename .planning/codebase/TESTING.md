# DevFlow — Testing

> Generated: 2026-06-17 | Mapper: gsd-map-codebase (sequential)

## Current State: Minimal

DevFlow has **2 tests total** — both unit tests in `crates/devflow-core/src/version.rs` and one in `config.rs`.

```rust
// crates/devflow-core/src/version.rs (lines 155-164)
mod tests {
    #[test]
    fn bumps_semver_components() {
        assert_eq!(bump("1.2.3", "patch").expect("patch"), "1.2.4");
        assert_eq!(bump("1.2.3", "minor").expect("minor"), "1.3.0");
        assert_eq!(bump("1.2.3", "major").expect("major"), "2.0.0");
    }
}
```

```rust
// crates/devflow-core/src/config.rs (lines 284+)
#[test]
fn parses_devflow_yaml_shape() { /* ... */ }
```

## Test Framework

| Item | Detail |
|---|---|
| **Framework** | Rust built-in `#[test]` (no external test crate) |
| **Runner** | `cargo test` |
| **Coverage** | ~5% (2 functions tested out of ~30 public functions) |
| **CI** | **None** — no `.github/workflows/` directory |

## What's NOT Tested

| Module | Test Coverage | Risk |
|---|---|---|
| `state.rs` | 0% | State machine transitions, advance logic |
| `config.rs` | ~2% (one test) | YAML parsing edge cases, defaults |
| `git.rs` | 0% | Branch creation/deletion, error handling |
| `tmux.rs` | 0% | Session launch, agent detection |
| `monitor.rs` | 0% | Child process spawning, lifecycle |
| `lock.rs` | 0% | Concurrent access prevention |
| `recover.rs` | 0% | Stale detection, re-launch logic |
| `workflow.rs` | 0% | State persistence, advancement loop |
| `version.rs` | ~5% (one test) | Cargo.toml, package.json, edge cases |
| `main.rs` (CLI) | 0% | Arg parsing, command dispatch |

## Testing Gaps by Priority

1. **State machine** — advance through all steps, skip logic, config-driven skips
2. **Lock** — concurrent acquire fails, release works, stale lock detection
3. **Config** — all fields, defaults, missing file, invalid YAML
4. **Version bumper** — Cargo.toml, edge cases (pre-release, build metadata)
5. **Git flow** — requires git repo; mock or integration test
6. **Tmux** — requires tmux; integration test only
7. **Monitor** — tricky to test (forked process); integration test

## Test Infrastructure Needed

- [ ] `.github/workflows/ci.yml` — `cargo test`, `cargo clippy`, `cargo fmt --check`
- [ ] Integration test harness (temp directories with git repos)
- [ ] Tmux availability check in CI (skip tmux tests if missing)
- [ ] Test fixtures directory (sample `.devflow.yaml`, state files)
