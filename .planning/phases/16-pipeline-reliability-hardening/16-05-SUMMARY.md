---
phase: 16-pipeline-reliability-hardening
plan: 05
subsystem: deterministic-invariants
tags: [documentation, gitignore, cargo-tests, allowlist, runtime-security]

requires:
  - phase: 16-02
    provides: TOML/serde loading pattern
  - phase: 16-03
    provides: capture-history path constructor and new config environment variables
provides:
  - constructor-derived gitignore coverage for thirteen runtime paths
  - bidirectional operator-doc/source existence checks
  - reason-required checked-in exception policy
  - source-pinned RUST_LOG default assertion
affects: [ci, operator-docs, runtime-files, future-cli-changes]

tech-stack:
  added: []
  patterns:
    - production path constructors are the runtime-file inventory source of truth
    - narrow token extraction plus explicit semantic pinned claims

key-files:
  created:
    - crates/devflow-core/src/doc_check.rs
    - doc-check-allowlist.toml
  modified:
    - .gitignore
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/lock.rs
    - crates/devflow-core/src/workflow.rs
    - crates/devflow-core/src/ship.rs
    - README.md
    - ARCHITECTURE.md
    - CONTRIBUTING.md
    - OPERATIONS.md
    - docs/guides/adding-agent.md
    - docs/guides/configuration.md
    - docs/guides/quickstart.md

key-decisions:
  - "Scoped docs are README.md, ARCHITECTURE.md, CONTRIBUTING.md, OPERATIONS.md, plus every Markdown file directly under docs/guides/."
  - "Generic existence extraction is limited to DEVFLOW_* names, devflow commands, CLI flags, .devflow paths, and Rust-like inline identifiers; semantic values use explicit pinned assertions."
  - "Only flags owned by external tools are allowlisted; real DevFlow drift is fixed in documentation."

patterns-established:
  - "Every allowlist exception carries kind, token, and a non-empty reason."
  - "Source-to-doc coverage derives command names from Command and environment names from actual read call sites."

requirements-completed: [16c, 16i]

coverage:
  - id: D1
    description: "Every enumerated DevFlow runtime path is constructor-derived and covered by a checked-in gitignore pattern."
    requirement: 16i
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#gitignore_covers_all_devflow_paths"
        status: pass
    human_judgment: false
  - id: D2
    description: "Scoped operator docs and Rust source are checked bidirectionally for named commands, flags, environment variables, paths, and identifiers."
    requirement: 16c
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_referenced_identifiers_exist_in_source"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
    human_judgment: false
  - id: D3
    description: "RUST_LOG's documented info default is pinned to both EnvFilter source fallback branches and exceptions require reasons."
    requirement: 16c
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#pinned_doc_claims_match_source"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#allowlist_entries_require_reasons"
        status: pass
    human_judgment: false

duration: 8min
completed: 2026-07-18
status: complete
---

# Phase 16 Plan 05: Deterministic Documentation and Runtime-Path Invariants Summary

**Cargo tests now prevent runtime telemetry paths from escaping gitignore and keep operator documentation synchronized bidirectionally with the CLI and environment surface.**

## Performance

- **Duration:** 8 min
- **Started:** 2026-07-18T01:01:35Z
- **Completed:** 2026-07-18T01:10:01Z
- **Tasks:** 2
- **Files modified:** 14

## Accomplishments

- Added a source-derived gitignore invariant covering thirteen runtime constructors, including capture history.
- Added narrow docs→source and source→docs checks with a reason-required TOML allowlist.
- Corrected real operator-doc drift exposed by the RED tests instead of suppressing it.

## Task Commits

1. **Task 1 RED: Runtime gitignore invariant** - `f13a3bc` (test)
2. **Task 1 GREEN: Constructor-derived runtime coverage** - `a107642` (test)
3. **Task 2 RED: Bidirectional documentation contracts** - `1c2d5e4` (test)
4. **Task 2 GREEN: Documentation checks and drift fixes** - `2ae7664` (test)

## Files Created/Modified

- `crates/devflow-core/src/doc_check.rs` - Implements all five deterministic invariant tests.
- `doc-check-allowlist.toml` - Stores visible, reason-required exceptions.
- `.gitignore` - Adds `.devflow/history/` coverage found missing by Task 1.
- Operator docs - Describe Layer 0, minimal config, current env vars, capture history, gate-driven Ship, and `AgentAdapter` accurately.

## Decisions Made

- Constructor set: `events_path`; stdout/stderr/exit/PID/history; per-phase and legacy state; gates directory; phase/project locks; per-phase and legacy cron instructions.
- Exposed as `pub(crate)`: `workflow::legacy_state_path`, `lock::lock_path`, `lock::project_lock_path`, and `ship::legacy_cron_instructions_path`. `Gates::dir` was already public.
- Scoped doc const: `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `OPERATIONS.md`; `docs/guides/*.md` is discovered dynamically. CHANGELOG and `.planning` are never scanned.
- Seeded allowlist: `--image`, `--name`, `--no-deps`, `--no-ff`, `--path`, `--release`, and `--workspace-folder`, each justified as an external-tool option.
- Initial pinned table: `RUST_LOG` defaults to `info` in operator docs and both `EnvFilter::new("info")` source fallback branches.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added capture-history gitignore coverage**
- **Found during:** Task 1
- **Issue:** Plan 16-03 introduced `.devflow/history/phase-NN/`, but the existing gitignore had no matching rule.
- **Fix:** Added `.devflow/history/`; the constructor-derived invariant now guards it.
- **Files modified:** `.gitignore`
- **Verification:** `gitignore_covers_all_devflow_paths` passes for all thirteen constructors.
- **Committed in:** `a107642`

**2. [Rule 2 - Missing Critical] Corrected operator documentation exposed by the checker**
- **Found during:** Task 2
- **Issue:** Docs still claimed no config/three layers, omitted new env vars, named nonexistent Ship commands, and showed the retired agent trait API.
- **Fix:** Updated all scoped operator references to current behavior, reserving the allowlist for external-tool flags only.
- **Files modified:** README, ARCHITECTURE, CONTRIBUTING, OPERATIONS, and three guides.
- **Verification:** All five `doc_check` tests pass.
- **Committed in:** `2ae7664`

---

**Total deviations:** 2 auto-fixed (2 missing-critical accuracy/security gaps).
**Impact on plan:** Both fixes are required for the invariants to represent actual protection rather than pass over known drift.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Future CLI/config/runtime-path changes now fail locally and in CI when docs or gitignore lag.
- No blockers remain from Plan 16-05.

## Self-Check: PASSED

---
*Phase: 16-pipeline-reliability-hardening*
*Completed: 2026-07-18*
