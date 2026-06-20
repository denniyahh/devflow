# Configuration

DevFlow stores project configuration in `.devflow.yaml`.

## Default Configuration

```yaml
version:
  scheme: semver
  file: Cargo.toml
  field: workspace.package.version
  build_number: git

automation:
  auto_branch: true
  auto_verify: true
  auto_docs: true
  auto_version: patch
  auto_ship: false
  auto_cleanup: true
  verify_command: "cargo test"
  lint_command: "cargo clippy -- -D warnings"
  docs_command: "cargo doc --no-deps 2>&1"
  continue_on_error: true
  docs_auto_commit: false

git_flow:
  main: main
  develop: develop
  feature_prefix: feature/
```

## Configuration Fields

### `version`

| Field | Description | Default |
|-------|-------------|---------|
| `scheme` | Versioning scheme (`semver`) | `semver` |
| `file` | File containing version | `Cargo.toml` |
| `field` | Field path in version file | `workspace.package.version` |
| `build_number` | Build number source (`git` = commit count) | `git` |

### `automation`

| Field | Description | Default |
|-------|-------------|---------|
| `auto_branch` | Auto-create feature branches | `true` |
| `auto_verify` | Auto-run tests after agent completes | `true` |
| `auto_docs` | Auto-update documentation | `true` |
| `auto_version` | Auto-bump version (`patch`, `minor`, `major`) | `patch` |
| `auto_ship` | Auto-create PR and release branch | `false` |
| `auto_cleanup` | Auto-remove worktrees+branches after ship | `true` |
| `verify_command` | Command to run for verification | `cargo test` |
| `lint_command` | Command to run for linting | `cargo clippy -- -D warnings` |
| `docs_command` | Command to generate docs | `cargo doc --no-deps` |
| `continue_on_error` | Non-fatal errors don't halt pipeline | `true` |

### `git_flow`

| Field | Description | Default |
|-------|-------------|---------|
| `main` | Production branch name | `main` |
| `develop` | Integration branch name | `develop` |
| `feature_prefix` | Prefix for feature branches | `feature/` |

## Viewing Effective Config

```bash
devflow config
```

Shows the merged configuration from `.devflow.yaml` defaults + overrides + CLI flags.
