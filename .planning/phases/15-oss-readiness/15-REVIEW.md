---
phase: 15-oss-readiness
reviewed: 2026-07-17T00:00:00Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - .devcontainer/devcontainer.json
  - .github/workflows/devcontainer.yml
  - .gitignore
  - ARCHITECTURE.md
  - CONTRIBUTING.md
  - DEPENDENCIES.md
  - LICENSE-APACHE
  - README.md
  - SECURITY.md
  - docs/guides/configuration.md
  - docs/guides/quickstart.md
findings:
  critical: 1
  warning: 2
  info: 2
  total: 5
status: issues_found
---

# Phase 15: Code Review Report

**Reviewed:** 2026-07-17T00:00:00Z
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

Re-reviewed the full OSS-readiness file set (README, ARCHITECTURE,
CONTRIBUTING, DEPENDENCIES, SECURITY, docs/guides/*, LICENSE-APACHE,
.gitignore, and the devcontainer setup), with specific focus on verifying the
prior review's three fixes (CR-01, WR-01, WR-02) rather than assuming they
closed the gap. Every factual claim in the docs was cross-checked against the
actual source (`crates/devflow-core`, `crates/devflow-cli`) — link targets,
CLI flag/subcommand signatures, env-var defaults, file-path patterns, and
license file presence were all independently re-verified, including
empirically (pulled the pinned devcontainer base image to check its default
container user, and ran `git check-ignore` against synthetic runtime files
to confirm actual gitignore coverage rather than trusting the pattern by eye).

**WR-01 and WR-02 are correctly and completely fixed.** `devcontainer.yml`'s
CI-parity step now runs `cargo fmt --check` alongside build/test/clippy
(`.github/workflows/devcontainer.yml:26`). `devcontainer.json`'s target mount
is now `devflow-target-${localWorkspaceFolderBasename}`
(`.devcontainer/devcontainer.json:17`) — confirmed `localWorkspaceFolderBasename`
is a real devcontainers-CLI substitution variable that resolves to
`basename(localWorkspaceFolder)`, so concurrent worktree devcontainers no
longer collide on a shared `target/` volume. Also confirmed the pinned base
image's default container user is `root`, so there is no volume-ownership
permission risk from mounting a fresh named volume over `target/` in
`postCreateCommand` — a plausible failure mode I checked and ruled out rather
than assuming away.

**CR-01 is not fully fixed — it regressed.** The `.gitignore` rewrite that
untracked the three leaked `phase-07-*` telemetry files replaced the old
exact-match patterns with per-phase globs, but in doing so *dropped*
coverage of the legacy single-slot `.devflow/state.json` path (which the
code still actively supports) and never added coverage for
`.devflow/events.jsonl` or `.devflow/gates/*` — despite this same diff
adding explicit prose in ARCHITECTURE.md, README.md, and SECURITY.md
asserting `.devflow/` runtime state is git-ignored, and SECURITY.md
specifically naming `events.jsonl` as sensitive. This is the same bug class
CR-01 was opened to close, reopened by the fix itself. See CR-01 below for
the reproduction and fix.

Two further documentation-accuracy findings (WARNING) and two minor
consistency nits (INFO) round out the review.

## Critical Issues

### CR-01: `.gitignore` doesn't cover all `.devflow/` runtime files the docs claim are git-ignored — CR-01 regression

**File:** `.gitignore:22-30`
**Issue:**

The CR-01 fix (commit `d021e3a`) replaced the old exact-match `.devflow/state.json` / `.devflow/lock` / `.devflow/last-ship.json` lines with per-phase glob patterns, but in doing so:

1. **Dropped coverage of the legacy single-slot `.devflow/state.json`.** The old `.gitignore` had an exact-match line for it; the new pattern is `.devflow/state-*.json` only, which does not match `state.json` (no hyphen — `*` requires the literal `-` before it in the pattern to be present). The code still writes/reads this exact path — `crates/devflow-core/src/workflow.rs` (`legacy_state_path()`, `migrate_legacy_state()`) explicitly handles "a legacy single-slot `.devflow/state.json` from a pre-14a binary," and this exact file was tracked in git history before (`git log --all --diff-filter=A --name-only` shows `.devflow/state.json` was committed in `d4266e8`/`0294d55`).
2. **Never covers `.devflow/events.jsonl`.** `crates/devflow-core/src/events.rs` writes this file at every workflow step. SECURITY.md (this same diff, line 37) explicitly says: *"Do not expose `.devflow/state-NN.json` or `.devflow/events.jsonl` to untrusted contexts"* — yet nothing in `.gitignore` ignores it, so it can be silently `git add`-ed and pushed to a public OSS repo (`.devflow/events.jsonl` was previously tracked in this repo's git history too, per the same `git log` check).
3. **Never covers `.devflow/gates/`.** `crates/devflow-core/src/gates.rs` writes `{phase:02}-{stage}.json` / `.response.json` / `.ack.json` under `.devflow/gates/`, which can carry human-authored free-text notes (`GateResponse.note`). OPERATIONS.md documents this directory as part of the same runtime-state inventory as the other ignored files.

Confirmed empirically — none of these three paths are ignored by the current `.gitignore`:

```
$ touch .devflow/events.jsonl .devflow/state.json
$ mkdir -p .devflow/gates && touch .devflow/gates/15-ship.json
$ git check-ignore -v .devflow/events.jsonl .devflow/state.json .devflow/gates/15-ship.json
$ echo $?
1   # no output — nothing matched, none of these are ignored
```

This directly contradicts three docs statements added in this exact same diff:
- ARCHITECTURE.md:318 — "Runtime state lives under `.devflow/` (git-ignored), keyed per-phase"
- README.md:124 — "DevFlow stores runtime state per-phase in `.devflow/state-NN.json` (git-ignored)"
- SECURITY.md:37 — "Do not expose `.devflow/state-NN.json` or `.devflow/events.jsonl` to untrusted contexts"

Given this phase's own history — leaked telemetry from a prior dogfood run had to be untracked in the very commit that introduced this gap — leaving `events.jsonl`/`gates/`/legacy `state.json` uncovered is a live risk, not a theoretical one.

**Fix:**
```diff
 # DevFlow own state (dogfooding) — per-phase since the 14a refactor
+.devflow/state.json
 .devflow/state-*.json
 .devflow/lock-*
 .devflow/phase-*-stdout
 .devflow/phase-*-stderr.log
 .devflow/phase-*-exit
 .devflow/phase-*-agent-pid
 .devflow/last-ship*.json
 .devflow/cron-instructions*.json
+.devflow/events.jsonl
+.devflow/gates/
```

## Warnings

### WR-A: README Commands table implies `devflow gate list` takes a `<phase>` argument — it doesn't

**File:** `README.md:97`
**Issue:** The Commands table collapses three subcommands into one row:

```
| `devflow gate list\|approve\|reject <phase> [--stage STAGE] [--note "..."]` | ... |
```

`GateCmd::List` (`crates/devflow-cli/src/main.rs:227-232`) takes **no** `phase` argument — it lists all open gates project-wide (`GateCmd::List { project } => gate_list(...)`, `main.rs:329`). Only `Approve`/`Reject` take a positional `phase`. A reader following this row literally (`devflow gate list 3`) gets a clap argument error. OPERATIONS.md documents `gate list` correctly and separately elsewhere in this same doc set, so this is an accuracy regression specific to the collapsed README row, not a pre-existing/consistent project convention.

**Fix:** Split the row, mirroring OPERATIONS.md's phrasing:
```markdown
| `devflow gate list` | List gates awaiting a response |
| `devflow gate approve\|reject <phase> [--stage STAGE] [--note "..."]` | Answer a human gate |
```

### WR-B: `docs/guides/configuration.md` omits `DEVFLOW_GATE_NOTIFY_CMD`'s security-relevant execution detail present elsewhere in the same doc set

**File:** `docs/guides/configuration.md:30`
**Issue:** The table documents `DEVFLOW_GATE_NOTIFY_CMD` as "Shell command fired when a gate is written," but omits that it is invoked via `sh -c` with gate metadata passed only as environment variables (never interpolated into the command string) — the exact safety property that makes this env var non-injectable. ARCHITECTURE.md (same diff, line 210) states this explicitly: *"fires the notify hook ... runs `$DEVFLOW_GATE_NOTIFY_CMD` via `sh -c` with gate metadata as environment variables, never interpolated into the command string."* Since `configuration.md` is the doc most likely to be read by someone deciding what to put in this variable, and SECURITY.md's stated scope explicitly calls out "command injection via crafted prompts or config," omitting the interpolation-safety guarantee here is a missed opportunity to document the actual security boundary — a reader of only this page has no way to know whether it's safe to route untrusted phase/context data through this hook.

**Fix:** Add a sentence to the `DEVFLOW_GATE_NOTIFY_CMD` row or a footnote: "Gate metadata (`DEVFLOW_GATE_PHASE`/`DEVFLOW_GATE_STAGE`/`DEVFLOW_GATE_CONTEXT`) is passed as environment variables to the command, never interpolated into the command string — the notify command itself is still `sh -c`-evaluated, so treat it like any other shell command you control."

## Info

### IN-01: Inconsistent `cargo fmt` check invocation across CONTRIBUTING.md

**File:** `CONTRIBUTING.md:46` vs `CONTRIBUTING.md:103`
**Issue:** The "Development" section uses `cargo fmt -- --check` (line 46) while the "Required checks" section uses `cargo fmt --check` (line 103) for the same operation. Both work on the pinned toolchain, but the inconsistency reads as if two different invocations are required.
**Fix:** Standardize on one form (prefer `cargo fmt --check`, matching `.github/workflows/ci.yml:40` and `.github/workflows/devcontainer.yml:26`).

### IN-02: `.devcontainer/devcontainer.json` top-of-file comment doesn't explain the per-worktree target volume naming

**File:** `.devcontainer/devcontainer.json:1-6`
**Issue:** The file-level comment explains the pinned base image and that caches persist across rebuilds, but doesn't mention *why* the target volume is named `devflow-target-${localWorkspaceFolderBasename}` (i.e., to avoid collisions between concurrent worktree-scoped devcontainers — the WR-02 fix). A future editor collapsing this back to a single shared `devflow-target` volume (as it was before WR-02) wouldn't have an inline warning not to.
**Fix:** Add a one-line comment near the `mounts` array, e.g. `// target is scoped per-worktree (not shared) to avoid stale-incremental-build collisions across concurrent worktrees — see WR-02`.

---

_Reviewed: 2026-07-17T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
