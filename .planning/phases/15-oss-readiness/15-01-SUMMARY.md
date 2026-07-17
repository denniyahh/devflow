---
phase: 15-oss-readiness
plan: 01
subsystem: docs
tags: [readme, security-policy, dependencies, docs-accuracy, cli-surface]

# Dependency graph
requires:
  - phase: 15a (Dogfood Enablement)
    provides: "devflow gate list/approve/reject CLI, OPERATIONS.md, --help snapshot guard, .devflow.yaml decoy removal, per-phase state-{NN}.json model"
provides:
  - "README.md command table documents gate + logs (15a commands)"
  - "README.md describes per-phase .devflow/state-NN.json instead of a single state.json"
  - "README.md Documentation section links OPERATIONS.md"
  - "SECURITY.md points incident responders at real state-NN.json + events.jsonl files"
  - "DEPENDENCIES.md doctor sample matches the shipped 1.2.0 CLI with no phantom config file"
affects: [15-02, 15-03, 15-04, 15-05]

# Tech tracking
tech-stack:
  added: []
  patterns: []

key-files:
  created: []
  modified:
    - README.md
    - SECURITY.md
    - DEPENDENCIES.md

key-decisions:
  - "SECURITY.md Supported Versions table (v1.0.0+) left unchanged — it already covers Cargo.toml's 1.2.0, no contradiction to fix"
  - "DEPENDENCIES.md 'Required for Shipping' header rewritten to name the gate-driven Ship stage (devflow gate approve --stage ship) instead of the phantom devflow ship/devflow confirm commands, since neither is a Command variant in main.rs"

patterns-established: []

requirements-completed: [15b]

coverage:
  - id: D1
    description: "README.md command table adds gate/logs rows, prose+Configuration section use per-phase state-NN.json, Documentation section links OPERATIONS.md"
    requirement: "15b"
    verification:
      - kind: unit
        ref: "cargo test -p devflow --test help_snapshot"
        status: pass
      - kind: other
        ref: "rg -n 'gate' README.md; rg -n 'logs' README.md; rg -n 'state-' README.md; rg -n 'OPERATIONS\\.md' README.md; test \"$(rg -c 'state\\.json' README.md)\" = 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "SECURITY.md Best Practices bullet replaces the nonexistent .devflow/audit.log pointer with the real .devflow/state-NN.json and .devflow/events.jsonl files"
    requirement: "15b"
    verification:
      - kind: other
        ref: "test \"$(rg -c 'audit\\.log' SECURITY.md)\" = 0; rg -q 'events\\.jsonl' SECURITY.md; rg -q 'state-' SECURITY.md"
        status: pass
    human_judgment: false
  - id: D3
    description: "DEPENDENCIES.md doctor sample bumped to devflow 1.2.0, .devflow.yaml found line removed, Required for Shipping header drops the phantom devflow confirm command"
    requirement: "15b"
    verification:
      - kind: other
        ref: "test \"$(rg -c '\\.devflow\\.yaml' DEPENDENCIES.md)\" = 0; test \"$(rg -c 'devflow confirm' DEPENDENCIES.md)\" = 0; rg -q '1\\.2\\.0' DEPENDENCIES.md; test \"$(rg -c 'v1\\.0\\.0' DEPENDENCIES.md)\" = 0"
        status: pass
    human_judgment: false

# Metrics
duration: 15min
completed: 2026-07-17
status: complete
---

# Phase 15 Plan 01: Root Docs Accuracy Pass Summary

**README/SECURITY/DEPENDENCIES corrected against real source (main.rs Command/GateCmd, workflow.rs state_path, Cargo.toml 1.2.0) — gate/logs documented, per-phase state-NN.json model, phantom audit.log/confirm/config-file references removed.**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-07-17T10:05:00-04:00
- **Completed:** 2026-07-17T10:10:02-04:00
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments
- README.md command table now documents `devflow gate list|approve|reject` and `devflow logs`, matching `main.rs`'s `enum Command` (`Gate`, `Logs`) and `enum GateCmd` (List/Approve/Reject) variants
- README.md's two "single `.devflow/state.json`" claims (pipeline prose + Configuration section) replaced with the real per-phase `.devflow/state-NN.json` model, verified against `workflow.rs::state_path()` (`STATE_FILE_PREFIX = "state-"`, `state-{phase:02}.json`)
- README.md Documentation section links `OPERATIONS.md` as the operator reference (gate protocol, env vars, `.devflow/` file inventory)
- SECURITY.md's Best Practices bullet no longer points incident responders at a nonexistent `.devflow/audit.log` — replaced with the real `.devflow/state-NN.json` and `.devflow/events.jsonl` (verified via `events.rs`'s `events.jsonl` path constant)
- DEPENDENCIES.md `doctor` sample bumped from the stale `devflow v1.0.0` to the real `1.2.0` (`Cargo.toml:9`) and the `.devflow.yaml` "found" line removed (that file was deleted as a decoy in 15a and no longer exists)
- DEPENDENCIES.md "Required for Shipping" section no longer references `devflow confirm` (not a `Command` variant in `main.rs`) or the equally phantom `devflow ship`; rewritten to name the real gate-driven Ship flow (`devflow gate approve <phase> --stage ship`)

## Task Commits

Each task was committed atomically:

1. **Task 1: README accuracy pass — gate/logs commands, per-phase state, OPERATIONS.md link** - `0da4843` (docs)
2. **Task 2: SECURITY.md — replace phantom audit-log pointer, align supported versions** - `affc030` (docs)
3. **Task 3: DEPENDENCIES.md — fix stale doctor sample and confirm command reference** - `957d95a` (docs)

**Plan metadata:** (this commit, following)

## Files Created/Modified
- `README.md` - added gate/logs command-table rows, per-phase state-NN.json wording (2 spots), OPERATIONS.md Documentation link
- `SECURITY.md` - Best Practices bullet now names real state-NN.json + events.jsonl instead of the phantom audit.log
- `DEPENDENCIES.md` - doctor sample version bumped to 1.2.0, phantom .devflow.yaml line removed, Required for Shipping header rewritten around the gate-driven Ship flow

## Decisions Made
- SECURITY.md's Supported Versions table (`v1.0.0+`) was left unchanged: `1.2.0` (the real Cargo.toml version) already falls within `v1.0.0+`, so there was no stale/contradictory claim to fix, only the audit.log line needed correction.
- DEPENDENCIES.md's "Required for Shipping" header explicitly named a second phantom command (`devflow ship`, alongside the plan-flagged `devflow confirm`) — neither is a `Command` variant in `main.rs`. Per the Docs-as-source-of-truth pattern (15-PATTERNS.md) and Rule 1 (bug fix — a false command claim is a factual bug), both were replaced in the same edit with the real `devflow gate approve <phase> --stage ship` flow rather than leaving one phantom command in place.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Corrected the verify command's cargo package name**
- **Found during:** Task 1 (README accuracy pass)
- **Issue:** The plan's `<verify>` block specifies `cargo test -p devflow-cli --test help_snapshot`, but `crates/devflow-cli/Cargo.toml` declares `name = "devflow"` (not `devflow-cli`) — `cargo test -p devflow-cli` fails with "package ID specification `devflow-cli` did not match any packages."
- **Fix:** Ran `cargo test -p devflow --test help_snapshot` instead; test passed (`help_output_matches_committed_snapshot ... ok`), confirming the README command-table edits don't drift from the real `--help` output.
- **Files modified:** None (verification command only, no source change)
- **Verification:** `cargo test -p devflow --test help_snapshot` exits 0
- **Committed in:** N/A (verification-only correction, no file change to commit)

**2. [Rule 1 - Bug] Replaced the second phantom command reference (`devflow ship`) alongside the plan-flagged `devflow confirm`**
- **Found during:** Task 3 (DEPENDENCIES.md)
- **Issue:** The plan's action for Task 3 explicitly named only `devflow confirm` as the phantom command to drop from the "Required for Shipping" header, but the same header also said "Needed for `devflow ship`" — `ship` is likewise not a `Command` variant in `main.rs` (confirmed via `rg -n "enum Command"` — no `Ship` variant; Ship is a pipeline `Stage`, driven through `devflow start` and the gate protocol, not a standalone subcommand).
- **Fix:** Rewrote the header to describe the real mechanism: "Needed for the gate-driven Ship stage (`devflow gate approve <phase> --stage ship`) — PR creation and merge," removing both phantom command references in one factually-correct sentence.
- **Files modified:** DEPENDENCIES.md
- **Verification:** `rg -c 'devflow confirm' DEPENDENCIES.md` returns 0 (plan's own acceptance criterion); manual read-through confirms no remaining `devflow ship` phantom-command claim
- **Committed in:** `957d95a` (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking verify-command fix, 1 bug — extra phantom command found via the same source-tracing the plan mandated)
**Impact on plan:** Both fixes are within the plan's own stated methodology (docs-as-source-of-truth, verified against `main.rs`) and its explicit acceptance criteria. No scope creep — no CLI/core source files were touched, no files outside the plan's `files_modified` list were changed.

## Issues Encountered
None beyond the two auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- README.md, SECURITY.md, and DEPENDENCIES.md are now consistent with the real 15a-shipped CLI surface (`gate`, `logs`), the per-phase state file model, and the current `1.2.0` version — unblocks the rest of Phase 15b's OSS-packaging work (ARCHITECTURE.md rewrite, CONTRIBUTING.md, devcontainer, crates.io publish) which references these same root docs.
- `docs/guides/quickstart.md` and `docs/guides/configuration.md` remain flagged in 15-PATTERNS.md as discretionary accuracy passes not yet scoped into any plan — worth a follow-up check before the crates.io publish plan (15-05) if they surface the same `state.json`/`.devflow.yaml` staleness class.

---
*Phase: 15-oss-readiness*
*Completed: 2026-07-17*

## Self-Check: PASSED

All modified files (README.md, SECURITY.md, DEPENDENCIES.md, this SUMMARY.md) confirmed present on disk. All three task commits (`0da4843`, `affc030`, `957d95a`) confirmed present in git log.
