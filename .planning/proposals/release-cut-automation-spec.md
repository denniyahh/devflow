# Feature Specification: Native Release Lifecycle & Release-Cut Porting for DevFlow

**Status:** Proposed  
**Author:** Pair Programming Session  
**Target Components:** `crates/devflow-core/src/` (`version.rs`, `hooks.rs`, `git.rs`), `crates/devflow-cli/src/commands.rs`  
**Related Documents:** `scripts/cut-release.sh`, `scripts/sync-main-to-develop.sh`, `scripts/deploy-docs.sh`, `CONTRIBUTING.md`  

---

## 1. Executive Summary & Problem Statement

DevFlow currently manages two distinct release lifecycles that have evolved separately:
1. **Per-Phase Workflow (`devflow ship`):** Automatically completes a single phase feature branch (`feature/phase-XX` → `develop`), runs `cargo doc --no-deps`, bumps `workspace.package.version`, updates `CHANGELOG.md`, and cleans up the local feature branch.
2. **Repository Release Cut (`scripts/cut-release.sh`):** Orchestrates production releases across dual protected branches (`develop` → `main`), verifies signed tags, coordinates squash-merge ancestry syncs, publishes multi-crate workspaces to crates.io in topological order, and builds/deploys the MkDocs Material wiki via autwicky.

While `devflow release --check` and `devflow release --verify` provide read-only preflight and postflight invariant checking, the core multi-step release execution remains trapped in an external shell script (`scripts/cut-release.sh`).

### Core Failure Modes in the Current Division of Labor

- **Multi-Crate Workspace Pin Drift:** `VersionBump` in `devflow ship` only rewrites `workspace.package.version`, leaving `[workspace.dependencies]` member self-pins stale (e.g. `devflow-core = { path = "...", version = "..." }`). This builds locally but explodes at `cargo publish` on crates.io.
- **AI vs. Maintainer Key Confusion (Trap 999.104):** `devflow ship` creates tags using ambient `git tag`, which signs with `user.signingkey` (the AI agent's key). `cut-release.sh` enforces maintainer key segregation via `devflow.releaseSigningKey`, but this protection is absent from DevFlow's internal `GitFlow` driver.
- **Hardcoded & Outdated Documentation Engine:** `devflow ship` invokes `cargo doc --no-deps`, while modern DevFlow projects use living docs-as-code wikis (MkDocs Material + Mermaid via autwicky) deployed to `gh-pages`.
- **Squash-Merge Ancestry Erasure:** Direct local merges in `devflow ship` do not support GitHub squash-merged PR workflows to trunk (`main`), leaving `develop` and `main` susceptible to silent merge-base divergence without manual `-X ours` syncs.

---

## 2. Rationalization: `devflow ship` vs. `scripts/cut-release.sh`

| Capability | `devflow ship` (Current) | `cut-release.sh` (Shell Script) | Proposed DevFlow Native Behavior |
|---|---|---|---|
| **Lifecycle Level** | Phase-level (`phase-XX` → `develop`) | Repository-level (vX.Y.Z production cut) | Keep `ship` for phases; promote `cut-release.sh` to native `devflow release <step>` subcommands. |
| **Workspace Pinning** | Updates only `workspace.package.version` | Manual 2-place edits in `Cargo.toml` | **Port to DevFlow:** Extend `version::write_version` to update both `workspace.package` and `[workspace.dependencies]` member pins. |
| **Signing Key Isolation** | Ambient `git tag` (uses agent key) | Enforces `devflow.releaseSigningKey` override | **Port to DevFlow:** Integrate `releaseSigningKey` resolution into `GitFlow::tag_signed` with tilde expansion and file readability checks. |
| **Documentation Pipeline** | Hardcoded `cargo doc --no-deps` | `scripts/deploy-docs.sh` (MkDocs / autwicky) | **Port to DevFlow:** Configurable documentation command in `devflow.toml` (`mkdocs`, `mdbook`, or custom script) with post-release deployment. |
| **Squash & Sync Ancestry** | Simple `git merge --no-ff` | Sync PR with `-X ours` merge-commit verification | **Port to DevFlow:** Native `devflow release sync` command verifying `git merge-base --is-ancestor origin/main origin/develop`. |
| **Crate Publish Ordering** | None (manual) | Pinned dependency order (`devflow-core` then `devflow`) | **Port to DevFlow:** Topological crate publishing in `devflow release publish`. |

---

## 3. Target Architecture & Native CLI Surface

```text
devflow release
├── check                  # Pre-cut invariant verification (existing)
├── branch [target]        # Calculate next semver and create release/vX.Y.Z off develop
├── pr-develop             # Open version bump + changelog PR to develop
├── pr-main                # Open develop -> main squash PR
├── tag                    # Sign release tag on origin/main using maintainer key
├── sync                   # Sync origin/main back to develop (-X ours) and open merge PR
├── publish                # Topologically publish workspace crates to registry
├── docs                   # Build and deploy documentation wiki to GitHub Pages
└── verify                 # Post-cut invariant verification (existing)
```

---

## 4. Detailed Functional Requirements for Porting

### FR-01: Workspace-Wide Dependency Pin Synchronization (`version.rs`)
- **Requirement:** When `version::write_version` updates a workspace manifest, it must update:
  1. `[workspace.package] version`
  2. All internal member path-dependencies in `[workspace.dependencies]` (e.g. `devflow-core = { path = "...", version = "..." }`)
  3. Manifest files of workspace members referencing peer crates.
- **Verification:** Unit test asserting round-trip version updates against multi-crate TOML fixtures.

### FR-02: Cryptographic Maintainer Key Isolation (`git.rs`)
- **Requirement:** `GitFlow::tag_signed` must check for `devflow.releaseSigningKey`:
  - If set: expand tildes, verify file readability, and execute `git -c user.signingkey=<path> tag -s <tag> <target> -m <message>`.
  - If missing/unreadable on release operations: fail fast with an actionable diagnostic before invoking `git tag`.
- **Verification:** Structural AST and mock execution tests guarding against agent key fallback.

### FR-03: Configurable Documentation Engine (`config.rs`, `hooks.rs`)
- **Requirement:** Add `[docs]` table to `devflow.toml`:
  ```toml
  [docs]
  engine = "mkdocs" # "mkdocs" | "cargo-doc" | "custom"
  build_command = "mkdocs build -f docs/mkdocs.yml"
  deploy_command = "scripts/deploy-docs.sh"
  ```
- **Execution:** `devflow release docs` invokes the configured deployment command and verifies non-zero exit.

### FR-04: Squash-Merge Ancestry Sync Verification (`commands.rs`, `git.rs`)
- **Requirement:** `devflow release sync` shall:
  1. Fetch `origin/main` and `origin/develop`.
  2. Perform a content-preserving `-X ours` merge of `origin/main` into `develop`.
  3. Validate that the resulting worktree is byte-identical to `origin/develop`.
  4. Assert `git merge-base --is-ancestor origin/main origin/develop`.

---

## 5. Phased Implementation Roadmap

1. **Phase 1: Workspace Pinning & Key Segregation in Core**
   - Refactor `version::write_version` to update workspace dependency pins.
   - Refactor `GitFlow` signing key resolution.
2. **Phase 2: CLI Release Subcommands**
   - Implement `devflow release branch`, `tag`, `sync`, `publish`, `docs`.
   - Update `scripts/cut-release.sh` to delegate directly to `devflow release <subcommand>`.
3. **Phase 3: Configurable Docs Pipeline**
   - Introduce `devflow.toml` docs schema and autwicky integration.
