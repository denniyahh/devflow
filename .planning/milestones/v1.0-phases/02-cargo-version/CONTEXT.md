# Phase 2: Version Bumper Expansion

## Goal
DevFlow currently only bumps `pyproject.toml`. It needs to bump `Cargo.toml` (its own format) and `package.json`. Self-dogfooding is blocked until this is done.

## Tasks

### 2a — Cargo.toml support
- [ ] Read `workspace.package.version` from `Cargo.toml`
- [ ] Write updated version back to the same field
- [ ] Handle both root `[package]` version and workspace `[workspace.package]` version
- [ ] File: `crates/devflow-core/src/version.rs`

### 2b — package.json support
- [ ] Read `version` field from `package.json`
- [ ] Write updated version back
- [ ] File: `crates/devflow-core/src/version.rs`

### 2c — Auto-detection
- [ ] When `version.file` is not in `.devflow.yaml`, auto-detect from project root:
  - `Cargo.toml` → cargo format
  - `pyproject.toml` → toml `project.version` path
  - `package.json` → json format
- [ ] File: `crates/devflow-core/src/config.rs`

### 2d — Schema update
- [ ] Add `version.file` to `.devflow.yaml` schema (optional, for override)
- [ ] Document in CLAUDE.md config section

## Verification
```bash
# Must pass:
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check

# Manual check:
devflow ship  # should bump Cargo.toml workspace.package.version
```

## Success
`devflow ship` bumps `Cargo.toml` version. DevFlow can version itself.
