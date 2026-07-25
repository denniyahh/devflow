---
phase: 23-end-to-end-dogfood
plan: 01
subsystem: infra
tags: [devflow, git, bash, dogfood, preflight]

requires: []
provides:
  - A rebuilt `devflow` binary on PATH provably matching Cargo.toml's
    workspace version (closes RESEARCH.md Pitfall 6 — stale binary)
  - "`scripts/scratch-dogfood-repo.sh`: a reproducible, isolated `devflow
    start` probe target scaffolder"
  - "`23-PROBE-SETUP.md`: the recorded scaffolding decision plus verbatim
    `devflow doctor` / `devflow start --dry-run` / `claude --version`
    evidence, scoped honestly (structural vs. partial-runtime vs.
    behavioral)"
affects: [23-02]

tech-stack:
  added: []
  patterns:
    - "Scratch-repo scaffolding for isolated CLI probes: git repo created
      outside the checkout, refused by absolute-path-prefix check,
      repo-local git identity only (no --global, no GIT_* env export)"

key-files:
  created:
    - scripts/scratch-dogfood-repo.sh
    - .planning/phases/23-end-to-end-dogfood/23-PROBE-SETUP.md
  modified: []

key-decisions:
  - "Probe target is a synthetic single-task phase in a fresh repo, not an
    imported real backlog item (RESEARCH.md Open Question 3) — the probe
    tests the pipeline mechanism, not the content of the work."
  - "No `.claude/` directory is scaffolded — `preflight.rs`'s
    `run_preflight` was read in full and checks only that the `claude`
    binary resolves on PATH for --agent claude; there is no project-local
    `.claude/` requirement in the current code (`rg '\\.claude' crates/`
    returns zero matches, test or otherwise)."
  - "`--dry-run`'s claim is deliberately bounded to 'structural only' in
    both this SUMMARY and 23-PROBE-SETUP.md, per commands.rs:119-122
    returning before `ensure_agent_binary` — the behavioral proof is left
    to plan 23-02's real probe run."

requirements-completed: [23a]

coverage:
  - id: D1
    description: "devflow binary on PATH rebuilt and proven to match Cargo.toml's workspace version (Pitfall 6 closed)"
    requirement: "23a"
    verification:
      - kind: other
        ref: "devflow --version (1.8.1) == grep -m1 '^version' Cargo.toml (1.8.1); manual re-run, both matched"
        status: pass
    human_judgment: false
  - id: D2
    description: "scripts/scratch-dogfood-repo.sh scaffolds an isolated, legal devflow start target outside this checkout"
    requirement: "23a"
    verification:
      - kind: other
        ref: "bash scripts/scratch-dogfood-repo.sh /tmp/devflow-probe-check && devflow doctor /tmp/devflow-probe-check && devflow start --phase 1 --agent claude --mode auto --dry-run /tmp/devflow-probe-check | grep -qi ship && claude --version — all four steps ran, echoed SCAFFOLD_OK"
        status: pass
      - kind: other
        ref: "in-checkout destination refusal: bash scripts/scratch-dogfood-repo.sh ./scratch-inside-check exited 1 with a refusal message"
        status: pass
      - kind: other
        ref: "git config --global --get user.email unchanged (d10475u5@outlook.com) before and after two separate script runs"
        status: pass
    human_judgment: false
  - id: D3
    description: "23-PROBE-SETUP.md records the scaffolding decision and verbatim command output, correctly scoped as structural-only for --dry-run"
    requirement: "23a"
    verification:
      - kind: other
        ref: "rg -i 'end to end|end-to-end' .planning/phases/23-end-to-end-dogfood/23-PROBE-SETUP.md — zero matches"
        status: pass
    human_judgment: false

duration: 30min
completed: 2026-07-25
status: complete
---

# Phase 23 Plan 01: Rebuild Binary + Scratch Probe Target Summary

**Closed the two preconditions the 23a probe needed to be trustworthy: rebuilt `devflow` from 1.8.0 to 1.8.1 on PATH, and scaffolded `scripts/scratch-dogfood-repo.sh`, a reproducible isolated `devflow start` target with structural evidence recorded in `23-PROBE-SETUP.md`.**

## Performance

- **Duration:** ~30 min
- **Completed:** 2026-07-25T19:15:18Z
- **Tasks:** 2/2 completed
- **Files modified:** 2 created (no source files modified by Task 1 — build-only step)

## Accomplishments
- Rebuilt the workspace (`cargo build --release --workspace`) and proved `devflow --version` (1.8.1) now matches `Cargo.toml`'s `[workspace.package].version` (1.8.1) — RESEARCH.md Pitfall 6 closed with its own independently checkable proof, before any probe runs.
- Wrote `scripts/scratch-dogfood-repo.sh`: creates a throwaway git repo (main + develop, one commit, repo-local git identity only) with a minimal `.planning/` skeleton and a pre-written trivial single-file-change plan at `.planning/phases/01-add-probe-marker/01-01-PLAN.md`, entirely outside this checkout.
- Verified the scaffolded repo is a legal `devflow start` target at exactly the strength the evidence supports: `devflow doctor` reports no blocking finding, `devflow start --phase 1 --agent claude --mode auto --dry-run` names all five stages (structural only, per `commands.rs:119-122`), and `claude --version` exits 0 with no auth prompt (partial runtime proof).
- Recorded the scaffolding decision, rationale, and verbatim command output in `23-PROBE-SETUP.md`, including the explicit `.claude/`-scaffolding-omitted finding (source-checked, not assumed) and the honest structural-vs-behavioral scope boundary the plan required.

## Task Commits

Both tasks were committed together (Task 1 modified no files, so had nothing to commit on its own):

1. **Task 1: Rebuild devflow and prove the binary on PATH is not stale** — no files modified (build/install step only); verified inline, folded into the Task 2 commit below.
2. **Task 2: Scaffold a throwaway devflow probe target and prove it is a legal one** - `bede035` (feat)

**Plan metadata:** pending (this SUMMARY's commit)

## Files Created/Modified
- `scripts/scratch-dogfood-repo.sh` - Scaffolds an isolated, throwaway `devflow start` probe target (git repo + minimal `.planning/` + pre-written trivial plan) outside this checkout; refuses in-checkout destinations; repo-local git identity only
- `.planning/phases/23-end-to-end-dogfood/23-PROBE-SETUP.md` - Records the scaffolding decision, the `.claude/`-omission finding with its source evidence, and verbatim `devflow doctor` / `devflow start --dry-run` / `claude --version` output, scoped honestly

## Decisions Made
- Rebuilt the workspace directly in the sibling main checkout (`/var/home/denniyahh/Github/devflow`, at commit `2228222`, confirmed byte-identical to this worktree's base commit via its `feature/phase-23` ref) rather than only inside this ephemeral worktree — `/home/linuxbrew/.linuxbrew/bin/devflow` already symlinks into that checkout's `target/release/devflow`, so this is the build that is actually durable on the operator's PATH after the worktree is cleaned up, matching the plan's own `key_links` intent ("the probe and the operator see the same binary"). Building only inside the worktree would have produced a binary that vanishes when the orchestrator force-removes this worktree, silently re-breaking the very thing this task exists to fix.
- No `.claude/` directory is scaffolded by the script — confirmed via direct read of `preflight.rs` that `run_preflight` has no project-local `.claude/` requirement for `--agent claude` (only that the `claude` binary itself resolves on PATH, a global/host-level fact, not a per-repo one). Documented as a source-checked finding in `23-PROBE-SETUP.md` rather than silently guessed.

## Deviations from Plan

None — plan executed exactly as written. The build-location decision above is a documented interpretive choice (Task 1 has no `<files>` to modify and no explicit "which checkout" instruction), not a deviation from any stated requirement; it satisfies Task 1's acceptance criteria as literally written (`devflow --version` matches `Cargo.toml`'s version; `command -v devflow` resolves into `target/release/devflow`; the binary is newer than `main.rs`) while additionally being durable past this worktree's lifetime.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required. `claude --version` confirmed the CLI is already installed and authenticated on this host (per this plan's own `user_setup` block).

## Next Phase Readiness
- `devflow` on PATH is confirmed fresh (1.8.1) and `scripts/scratch-dogfood-repo.sh` gives plan 23-02 a one-command, reproducible probe target.
- 23-02's probe must still supply the behavioral proof `--dry-run` cannot: a real (non-dry-run) `devflow start --phase 1 --agent claude --mode auto <dest>` launch through this scratch repo, per `23-PROBE-SETUP.md`'s explicit scope boundary.
- No blockers.

## Self-Check: PASSED

- FOUND: scripts/scratch-dogfood-repo.sh
- FOUND: .planning/phases/23-end-to-end-dogfood/23-PROBE-SETUP.md
- FOUND: .planning/phases/23-end-to-end-dogfood/23-01-SUMMARY.md
- FOUND: bede035 (Task 2 commit)
- FOUND: 83cffe5 (SUMMARY metadata commit)

---
*Phase: 23-end-to-end-dogfood*
*Completed: 2026-07-25*
