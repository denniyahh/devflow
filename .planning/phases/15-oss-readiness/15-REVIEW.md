---
phase: 15-oss-readiness
reviewed: 2026-07-17T18:12:51Z
depth: standard
files_reviewed: 4
files_reviewed_list:
  - docs/guides/quickstart.md
  - docs/guides/configuration.md
  - .devcontainer/devcontainer.json
  - .github/workflows/devcontainer.yml
findings:
  critical: 1
  warning: 2
  info: 1
  total: 4
status: issues_found
---

# Phase 15: Code Review Report

**Reviewed:** 2026-07-17T18:12:51Z
**Depth:** standard
**Files Reviewed:** 4
**Status:** issues_found

## Summary

Reviewed `docs/guides/quickstart.md`, `docs/guides/configuration.md`,
`.devcontainer/devcontainer.json`, and `.github/workflows/devcontainer.yml`
against the actual `crates/` source. The CLI-facing claims check out:
`--phase`/`--agent`/`--mode`/`--force`/`--no-worktree`/`--dry-run` match the
`Start` variant in `main.rs`; the per-phase state filename
(`.devflow/state-{NN}.json`), the `DEVFLOW_GATE_NOTIFY_CMD` /
`DEVFLOW_GATE_TIMEOUT_SECS` / `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` /
`DEVFLOW_LOG_FORMAT` env vars, the hardcoded git-flow branch names, the
Auto/Supervise gating semantics in `mode.rs`, and the crate name (`devflow`
on crates.io) all match code. `devcontainer.json` is valid JSONC, the base
image tag is pinned (not floating), and the named volumes/postCreateCommand
are internally consistent.

While tracing the documented `.devflow/state-{NN}.json` per-phase file
convention (quickstart.md:26-27) against the actual on-disk file set for
this repo's own dogfooding, I found that `.gitignore` was never updated when
the state/runtime file scheme moved from a single shared file to per-phase
files, and that this has **already caused real runtime telemetry to be
committed and tracked** in this repository (see CR-01). That is the
highest-severity finding below. The remaining two findings are a
devcontainer-CI naming/coverage gap and a Docker named-volume collision risk
specific to this project's own worktree-heavy workflow.

## Critical Issues

### CR-01: `.gitignore` doesn't cover the per-phase runtime file scheme the reviewed docs describe — real telemetry is already leaked and tracked in this repo

**File:** `.gitignore` (evidenced against `docs/guides/quickstart.md:26-27`, which documents the `.devflow/state-{NN}.json` per-phase convention this bug stems from)
**Issue:** `quickstart.md` correctly documents that DevFlow persists
per-phase progress to `.devflow/state-{NN}.json` ("state is per-phase, not a
single shared file"). The actual runtime file set is broader still —
`crates/devflow-core/src/agent_result.rs` and `lock.rs` also write
`.devflow/lock-{NN}`, `.devflow/phase-{NN}-stdout`,
`.devflow/phase-{NN}-stderr.log`, `.devflow/phase-{NN}-exit`, and
`.devflow/phase-{NN}-agent-pid` per phase. `.gitignore`, however, still only
excludes the **pre-refactor, singular** names:
```
.devflow/state.json
.devflow/lock
.devflow/last-ship.json
```
None of the current `-{NN}` per-phase patterns are covered. This is not
theoretical — this exact working tree currently has three such files
**tracked in git** (confirmed via `git ls-files .devflow`):
```
.devflow/phase-07-agent-pid
.devflow/phase-07-exit
.devflow/phase-07-stdout
```
`.devflow/phase-07-stdout` contains a real Claude Code agent JSON result
blob with a `session_id`, `total_cost_usd`, full token-usage breakdown, and
duration/telemetry data — internal operational data that has now leaked into
this project's git history via commit `f223359`. For an OSS-readiness phase
specifically concerned with what a public repo exposes, this is a live,
provable information-disclosure defect, not a hypothetical one.
**Fix:**
```gitignore
# DevFlow own state (dogfooding) — per-phase since the 14a refactor
.devflow/state-*.json
.devflow/lock-*
.devflow/phase-*-stdout
.devflow/phase-*-stderr.log
.devflow/phase-*-exit
.devflow/phase-*-agent-pid
.devflow/last-ship*.json
.devflow/cron-instructions*.json
```
And untrack the already-leaked files:
```bash
git rm --cached .devflow/phase-07-agent-pid .devflow/phase-07-exit .devflow/phase-07-stdout
```

## Warnings

### WR-01: `.github/workflows/devcontainer.yml`'s "CI-parity checks" step omits `cargo fmt --check`

**File:** `.github/workflows/devcontainer.yml:19-25`
**Issue:** The step is named "Build devcontainer and run CI-parity checks"
and runs `cargo build --workspace`, `cargo test --workspace`, and `cargo
clippy --workspace -- -D warnings` — but `ci.yml` has three required jobs
(`test`, `clippy`, **and `fmt`** via `cargo fmt --check`). Because `stable`
resolves independently in each environment (dtolnay/rust-toolchain@stable in
`ci.yml` vs. the devcontainer's own rustup-resolved stable), a rustfmt
formatting difference specific to the devcontainer's toolchain snapshot would
never be caught here despite the step's name implying full parity with CI.
**Fix:** Either rename the step to something narrower (e.g. "devcontainer
build/test smoke check") or add the missing check for true parity:
```yaml
runCmd: |
  cargo build --workspace
  cargo test --workspace
  cargo clippy --workspace -- -D warnings
  cargo fmt --check
```

### WR-02: Devcontainer named volumes are not scoped per checkout — collide across concurrent worktrees this project's own workflow encourages

**File:** `.devcontainer/devcontainer.json:10-21`
**Issue:** The `devflow-cargo-registry` and `devflow-target` mounts are
plain, unqualified Docker named-volume names. Docker named volumes are
global to the host, not scoped to a workspace folder or repo path. This
project's entire CLI model (`devflow start --phase N`, `devflow parallel`)
is built around running multiple phases concurrently, each in its **own git
worktree** (this review itself is running from
`.worktrees/phase-15`) — a setup where a contributor plausibly has two or
more worktrees of the same repo open in VS Code at once. If each is reopened
in its devcontainer, both containers mount the identical
`devflow-cargo-registry`/`devflow-target` volumes, so a `cargo build` in one
worktree can leave a stale/incompatible `target/` (different dependency
versions, different workspace members on a divergent branch) that the other
worktree's container then reuses, producing confusing, hard-to-diagnose
build/link errors that look like source bugs.
**Fix:** Scope the volume names to the workspace folder using a devcontainer
variable, e.g.:
```json
"mounts": [
  {
    "source": "devflow-cargo-registry",
    "target": "/usr/local/cargo/registry",
    "type": "volume"
  },
  {
    "source": "devflow-target-${localWorkspaceFolderBasename}",
    "target": "${containerWorkspaceFolder}/target",
    "type": "volume"
  }
]
```
(the cargo registry cache is safe to share globally since it's
content-addressed by crate/version; only the per-workspace `target/` build
cache needs to be scoped.)

## Info

### IN-01: Devcontainer doesn't include the GitHub CLI, despite `gh` being a documented DevFlow prerequisite

**File:** `.devcontainer/devcontainer.json`
**Issue:** `quickstart.md`'s Prerequisites section lists "**gh CLI** 2.0+
(for PR creation)" as required to use DevFlow end-to-end (Ship stage), and
`README.md` repeats this. `devcontainer.json` has no `features` block and
the `mcr.microsoft.com/devcontainers/rust` base image does not bundle `gh`.
A contributor who opens the devcontainer and tries to actually exercise
`devflow start ... --mode auto` through Ship inside it will hit a missing
`gh` binary. Low severity because `CONTRIBUTING.md` scopes the devcontainer's
purpose narrowly to "reproducible build/test," not full pipeline execution,
so this may be intentional — flagging for awareness rather than as a hard
defect.
**Fix:** If the devcontainer is meant to support exercising the full
pipeline (not just `cargo build`/`cargo test`), add the GitHub CLI feature:
```json
"features": {
  "ghcr.io/devcontainers/features/github-cli:1": {}
}
```

---

_Reviewed: 2026-07-17T18:12:51Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
