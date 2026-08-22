#!/usr/bin/env bash
#
# Scaffold a throwaway git repository that is a legal `devflow start`
# target — for probing the Define->Ship pipeline (phase 23a) without
# touching this checkout's own working tree or git state (D-01
# blast-radius isolation, T-23-01/T-23-02).
#
# Scaffolding decision (Claude's Discretion item #2, recorded in
# 23-CONTEXT.md / 23-01-PLAN.md "Recorded planning choices"): the probe
# target is a SYNTHETIC single-task phase, not an imported real backlog
# item — the probe is about the supervisor/pipeline mechanism, not about
# the content of the work.
#
# Minimum scaffold, per `crates/devflow-cli/src/preflight.rs`'s
# `run_preflight` (read in full before writing this script — the only
# thing it checks for `--agent claude` is that the `claude` binary
# resolves on PATH; there is no project-local `.claude/` requirement, so
# none is scaffolded here):
#   - a git repo with `main` + `develop` and one initial commit
#   - `.planning/{PROJECT.md,ROADMAP.md,STATE.md}` (ROADMAP's phase heading must
#     be `### Phase N:` — three hashes — to satisfy the reachability guard in
#     `preflight.rs`, and the current preflight also requires a minimal
#     `.planning/config.json` so the 35.1 unattended-launch chain-flag
#     prerequisite has somewhere to hold `workflow._auto_chain_active`)
#   - one phase directory carrying a trivial, already-written plan, so
#     Define/Plan have a real target instead of inventing scope
#
# Branch names below are hardcoded to "main"/"develop" — verified against
# `crates/devflow-core/src/config.rs:15,17`
# (`pub const MAIN: &str = "main";` / `pub const DEVELOP: &str = "develop";`)
# at plan time, not re-read live by this script.
#
# Usage: scripts/scratch-dogfood-repo.sh [destination]
#   destination defaults to $TMPDIR (or /tmp)/devflow-dogfood-scratch-$$
#   if omitted. Never a sibling `.worktrees/` entry of this repo.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

DEST="${1:-${TMPDIR:-/tmp}/devflow-dogfood-scratch-$$}"

# T-23-01 (Tampering, high, mitigate): refuse any destination inside this
# checkout — resolve both to absolute paths and compare prefixes.
DEST_ABS="$(realpath -m -- "$DEST")"
if [ "$DEST_ABS" = "$REPO_ROOT" ] || case "$DEST_ABS" in "$REPO_ROOT"/*) true ;; *) false ;; esac; then
    echo "ERROR: refusing to scaffold inside this checkout ($REPO_ROOT)." >&2
    echo "       Requested destination: $DEST_ABS" >&2
    echo "       Pick a destination outside the checkout (e.g. under \$TMPDIR or ~)." >&2
    exit 1
fi

if [ -e "$DEST_ABS" ]; then
    echo "ERROR: destination already exists: $DEST_ABS" >&2
    echo "       Remove it first, or pick a different destination." >&2
    exit 1
fi

mkdir -p "$DEST_ABS"

# Force the initial branch to "main" regardless of the operator's
# init.defaultBranch config, without requiring a git version new enough
# for `git init -b`.
git init -q "$DEST_ABS"
git -C "$DEST_ABS" symbolic-ref HEAD refs/heads/main

# T-23-02 (Tampering, high, mitigate): repo-local identity only — never
# --global, and no GIT_* env export (999.37 class).
git -C "$DEST_ABS" config user.name "DevFlow Probe"
git -C "$DEST_ABS" config user.email "devflow-probe@localhost"

mkdir -p "$DEST_ABS/.planning/phases/01-add-probe-marker"

cat > "$DEST_ABS/.planning/PROJECT.md" <<'EOF'
# Devflow Probe Target

## What This Is

A throwaway, single-purpose scratch repository scaffolded by
`scripts/scratch-dogfood-repo.sh` (DevFlow phase 23a) solely to give
`devflow start --phase 1` a legal, isolated target for probing the
Define->Ship pipeline mechanism. Nothing in this repo is meant to be kept.

## Core Value

Prove the pipeline mechanism carries a trivial change end-to-end — not to
build anything real.

## Requirements

### Active

- Phase 1: add a one-line marker file (`PROBE.md`) — deliberately trivial
  so Define/Plan exercise the pipeline, not the work.
EOF

cat > "$DEST_ABS/.planning/ROADMAP.md" <<'EOF'
# Roadmap

### Phase 1: Add PROBE marker file

Goal: create `PROBE.md` at the repo root containing exactly one line,
proving the pipeline can carry a trivial change from Define through Ship.

Status: not started
EOF

cat > "$DEST_ABS/.planning/STATE.md" <<'EOF'
---
gsd_state_version: 1.0
current_phase: 1
current_phase_name: Add PROBE marker file
status: planned
---

# Devflow Probe Target — Project State

## Active Phase

Phase 1 — Add PROBE marker file. Plan 01-01 is pre-written (see
`.planning/phases/01-add-probe-marker/01-01-PLAN.md`) so Define/Plan have
a real artifact to work against rather than inventing scope during a
probe.

## Current Position

Phase: 1
Plan: 01-01
Status: Planned — ready for `devflow start`
EOF

cat > "$DEST_ABS/.planning/phases/01-add-probe-marker/01-01-PLAN.md" <<'EOF'
---
phase: 01-add-probe-marker
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - PROBE.md
autonomous: true
---

<objective>
Create `PROBE.md` containing exactly one line, proving DevFlow's
Define->Ship pipeline can carry a trivial change end-to-end. This is a
throwaway scratch repo (scripts/scratch-dogfood-repo.sh) — the change
itself has no value beyond exercising the pipeline mechanism.
</objective>

<tasks>

<task type="auto">
  <name>Task 1: Create PROBE.md</name>
  <files>PROBE.md</files>
  <action>
Create a file named `PROBE.md` at the repository root containing exactly
one line: `probe ok`.
  </action>
  <acceptance_criteria>
    - `PROBE.md` exists at the repository root
    - Its content is exactly `probe ok`
  </acceptance_criteria>
  <verify>
    <automated>test "$(cat PROBE.md)" = "probe ok" && echo PROBE_OK</automated>
  </verify>
  <done>`PROBE.md` exists with the single line `probe ok`.</done>
</task>

</tasks>

<verification>
1. `PROBE.md` exists and contains exactly `probe ok`.
</verification>

<success_criteria>
- The pipeline produced a single committed file with the exact expected
  content.
</success_criteria>

<output>
Create `.planning/phases/01-add-probe-marker/01-01-SUMMARY.md` when done.
</output>
EOF

cat > "$DEST_ABS/.planning/config.json" <<'EOF'
{
  "workflow": {}
}
EOF

git -C "$DEST_ABS" add -A
git -C "$DEST_ABS" commit -q -m "chore: scaffold devflow probe target"
git -C "$DEST_ABS" branch develop
git -C "$DEST_ABS" checkout -q develop

echo "Scaffolded devflow probe target: $DEST_ABS"
echo "Next: devflow start --phase 1 --agent claude --mode auto $DEST_ABS"
