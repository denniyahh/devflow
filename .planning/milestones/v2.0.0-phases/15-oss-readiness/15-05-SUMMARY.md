---
phase: 15-oss-readiness
plan: 05
subsystem: infra
tags: [cargo-publish, crates-io, packaging, release]

# Dependency graph
requires:
  - phase: 15-oss-readiness (15-01/02/03/04)
    provides: doc-accuracy passes, devcontainer/CONTRIBUTING, dual-license closure, proven-green publish dry-run baseline
provides:
  - "devflow-core 1.2.0 live and resolvable on crates.io"
  - "devflow 1.2.0 live and resolvable on crates.io, installable via `cargo install devflow`"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns: []

key-files:
  created: []
  modified: []

key-decisions:
  - "Operator performed the actual `cargo publish` invocations directly, rather than the automated agent — two automated Code-stage attempts (a Layer-2 commit-count false-positive, then an agent self-report false-positive) both claimed success without publishing anything; the operator authenticated via `cargo login` and ran the real publish commands manually after the second false positive"
  - "Published leaf-first per plan ordering: devflow-core (17:39:23 UTC) before devflow (17:40:31 UTC, ~68s later), so the CLI's path+version dependency resolved from a live registry rather than a stale local path"

patterns-established: []

requirements-completed: [15b]

coverage:
  - id: D1
    description: "devflow-core 1.2.0 published to crates.io and resolvable from a clean registry query"
    requirement: "15b"
    verification:
      - kind: other
        ref: "curl https://crates.io/api/v1/crates/devflow-core (max_version/newest_version/max_stable_version: 1.2.0, yanked: false)"
        status: pass
      - kind: other
        ref: "cargo add devflow-core@1.2.0 --dry-run in a scratch crate — resolves from crates.io index"
        status: pass
    human_judgment: false
  - id: D2
    description: "devflow (CLI) 1.2.0 published to crates.io AFTER devflow-core was live, resolving its core dependency from the registry"
    requirement: "15b"
    verification:
      - kind: other
        ref: "curl https://crates.io/api/v1/crates/devflow (max_version/newest_version/max_stable_version: 1.2.0, yanked: false, created_at 68s after devflow-core)"
        status: pass
      - kind: other
        ref: "cargo add devflow@1.2.0 --dry-run in a scratch crate — resolves from crates.io index"
        status: pass
    human_judgment: false
  - id: D3
    description: "No crates.io API token committed, echoed, or written into any repo-tracked file"
    requirement: "15b"
    verification:
      - kind: other
        ref: "git status --short on .worktrees/phase-15 — clean, no new/modified files from the publish action"
        status: pass
    human_judgment: false

duration: "~2h (spanning two false-positive automated attempts and operator investigation)"
completed: 2026-07-17
status: complete
---

# Phase 15 Plan 05: Crates.io Publish Summary

**`devflow-core` and `devflow` 1.2.0 are live on crates.io, published leaf-first by the operator directly after two automated publish attempts both false-positived without actually running `cargo publish`.**

## Performance

- **Duration:** ~2h elapsed (mostly gate wait time across two false-positive Code-stage cycles); actual publish action took under 2 minutes once run
- **Started:** 2026-07-17 (Task 1 checkpoint first fired ~10:41 local)
- **Completed:** 2026-07-17T17:40:31Z (devflow live; devflow-core landed 17:39:23Z)
- **Tasks:** 3 (1 human-action checkpoint, 2 auto — both ultimately executed by the operator)
- **Files modified:** 0 (external registry action only, per plan)

## Accomplishments
- `devflow-core@1.2.0` published to crates.io — confirmed via direct API query (`max_version`/`newest_version`/`max_stable_version`: `1.2.0`, `yanked: false`) and via `cargo add devflow-core@1.2.0 --dry-run` resolving cleanly from a scratch crate
- `devflow@1.2.0` published to crates.io 68 seconds after `devflow-core`, honoring leaf-before-dependent ordering — confirmed the same way, plus `cargo add devflow@1.2.0 --dry-run` resolving its `devflow-core` path+version dependency from the live registry rather than a local path
- No token value appears in any repo-tracked file, commit, or this summary
- `cargo install devflow` is now a real, working install path for the project

## Task Commits

No commits — this plan is an external registry action only, per its `files_modified: []` scope. Publish itself has no corresponding repo diff.

## Files Created/Modified

None (external registry action only, per plan).

## Decisions Made

- **Operator ran the real publish commands directly**, rather than approving another automated retry. Context: the Code stage claimed success twice without actually publishing —
  1. First attempt evaluated success via a Layer-2 commit-count fallback (`21 commits on feature/phase-15, exit code 0`) — that commit count was unchanged from before the retry and reflected only earlier Wave-1 work, not this plan's task.
  2. Second attempt (a `--gaps-only` retry after Validate's first `loop_back`) had the agent self-report `DEVFLOW_RESULT: success` via its own marker with no evidence of attempting `cargo publish` at all (no partial output, no `15-05-SUMMARY.md`, no new commits).
  Both were independently caught against crates.io ground truth (`curl .../api/v1/crates/...` returning `does not exist` both times) rather than trusting DevFlow's own verdict. After the second false positive, the operator authenticated with `cargo login` and ran `cargo publish -p devflow-core` then `cargo publish -p devflow` directly, which this summary now documents as the actual completion evidence.
- Published leaf-first (`devflow-core` before `devflow`) per the plan's explicit ordering requirement, confirmed by the ~68-second gap between the two crates' `created_at` timestamps on crates.io.

## Deviations from Plan

### Notable Process Deviation (not a plan defect)

**1. Task 2 and Task 3 (`cargo publish` for each crate) were executed by the operator, not the automated agent**
- **Found during:** Two consecutive Code-stage "success" evaluations that Validate/Ship preflight caught as false positives (see Decisions Made above)
- **Root cause:** Not fully diagnosed — the agent's stdout/exit capture for both attempts was already cleaned up by the next stage's launch before it could be inspected, so the exact failure mode (misinterpreting the resolved human-action gate as the whole task being done, vs. some other cause) is not confirmed. Sandbox/credential-access restriction was ruled out: this run used Claude with `--dangerously-skip-permissions`, which has full filesystem/network access, not a walled sandbox.
- **Fix:** Operator ran the actual publish commands manually outside the agent loop, then this SUMMARY.md was written to give `/gsd-verify-work` a real artifact to check against.
- **Follow-up worth tracking:** if DevFlow dogfoods another external-action-only plan (no repo diff as its success signal) in a future phase, the Code-stage evaluation for that plan type may need a stronger check than "exit 0 + self-reported marker" — e.g., an explicit post-condition probe (like this plan's own registry-resolution check) run by DevFlow itself rather than trusted from the agent's self-report.

**Total deviations:** 1 (process — human executed the external action directly after automated attempts failed silently)
**Impact on plan:** None on the deliverable — both crates are live, in the correct order, exactly as the plan specified. The deviation is in *who* ran the publish commands, not what was published.

## Issues Encountered

- Two automated Code-stage attempts both reported success without evidence of running `cargo publish` — see Deviations above. Caught both times by independently querying crates.io directly rather than trusting `devflow status`/the stage's self-reported verdict.
- The Ship stage's own preflight (checking for a phase-level `VERIFICATION.md` with `status: passed`) correctly blocked shipping even after Validate itself false-positived (`verdict: pass`) on the second Code attempt — this is the check that ultimately surfaced the gap and is why this summary is being written retroactively now.

## User Setup Required

- Operator authenticated to crates.io via `cargo login` (credential stored in `~/.cargo/credentials.toml`, never entered the repo) and ran the publish commands directly. This satisfies the plan's Task 1 human-action checkpoint and Tasks 2/3's execution.

## Next Phase Readiness

- Both crates are live at 1.2.0; `cargo install devflow` works from a clean environment.
- This was the last plan in Phase 15's Wave 2 (and the phase overall) — `/gsd-verify-work` should be re-run now that this summary exists, to produce a passing `15-VERIFICATION.md` and unblock the Ship gate that is currently pending on phase 15.

---
*Phase: 15-oss-readiness*
*Completed: 2026-07-17*

## Self-Check: PASSED

- FOUND: devflow-core@1.2.0 live on crates.io (created_at 2026-07-17T17:39:23.696452Z, yanked: false)
- FOUND: devflow@1.2.0 live on crates.io (created_at 2026-07-17T17:40:31.461601Z, yanked: false)
- FOUND: cargo add --dry-run resolves both crates from the live registry index
- CONFIRMED: publish order was leaf-first (devflow-core before devflow)
