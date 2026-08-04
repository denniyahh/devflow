# Phase 6: GitHub PR Integration

## Goal
Polish devflow for v1.0.0 release — PR creation, merge detection, changelog, release workflow.

## Tasks

### 6a — PR creation on ship
- [ ] After version bump in SHIPPING step, create PR via `gh pr create`
- [ ] PR title: "Release v{version}"
- [ ] PR body: auto-generated from phase summary (commits since last release)
- [ ] Base branch: `main`, head branch: `release/v{version}`
- [ ] File: `crates/devflow-core/src/git.rs`

### 6b — Merge detection
- [ ] `devflow check` detects when PR is merged (via `gh pr view --json state`)
- [ ] Auto-advances from SHIPPING to CLEANING on merge
- [ ] File: `crates/devflow-core/src/workflow.rs`

### 6c — Project polish
- [ ] Create `CHANGELOG.md` with initial entry for v0.5.0 → v1.0.0
- [ ] Update `README.md` — installation, usage, architecture diagram
- [ ] Verify `CODE_OF_CONDUCT.md` exists (should already be present)
- [ ] File: repo root

### 6d — Release workflow
- [ ] Create `.github/workflows/release.yml`
- [ ] Triggered on tag push (`v*`)
- [ ] Build binary for Linux x86_64
- [ ] Upload binary as release asset via `gh release upload`
- [ ] File: `.github/workflows/release.yml`

## Verification
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check

# Manual: test PR creation (dry-run compatible)
gh pr create --title "test" --body "test" --base main --head feature/phase-06 --dry-run
```

## Success
`devflow ship` creates a PR. CI runs on the PR. Binary is published on release. DevFlow is v1.0.0.
