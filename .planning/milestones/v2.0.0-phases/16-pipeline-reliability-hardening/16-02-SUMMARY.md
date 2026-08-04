---
phase: 16-pipeline-reliability-hardening
plan: 02
subsystem: configuration
tags: [rust, toml, serde, configuration, precedence]

requires:
  - phase: 16-pipeline-reliability-hardening
    provides: D-03 decision to reopen configuration only for Phase 16 reliability knobs
provides:
  - Minimal typed devflow.toml loader for capture retention, review angles, and external verification
  - Environment-over-file-over-default resolver functions for every Phase 16 knob
  - Fail-soft behavior for missing, unreadable, malformed, and invalid configuration values
affects: [16-03-external-verification-and-capture-history, 16-04-review-depth, 16-05-doc-checks]

tech-stack:
  added: [toml 1.1.2]
  patterns: [serde-default partial config, env-file-default precedence, fail-soft project configuration]

key-files:
  created: []
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/devflow-core/Cargo.toml
    - crates/devflow-core/src/config.rs

key-decisions:
  - "Use DEVFLOW_CAPTURE_RETENTION, DEVFLOW_REVIEW_ANGLES, and DEVFLOW_EXTERNAL_VERIFY_ENABLED as the env-first override surface."
  - "Parse DEVFLOW_REVIEW_ANGLES as a comma-separated list, trimming and dropping empty entries."
  - "Treat invalid environment overrides like malformed devflow.toml: warn and fall back to the next precedence layer rather than aborting."
  - "Keep GitFlowConfig and its main/develop/feature constants outside devflow.toml."

patterns-established:
  - "Phase 16 config resolvers own precedence; downstream modules call a resolver rather than re-reading env vars or TOML."
  - "Project configuration is additive and fail-soft, while the branch model remains fixed."

requirements-completed: [16a, 16b, 16d, 16e]

coverage:
  - id: D1
    description: "A missing or partial devflow.toml loads a typed DevflowConfig with stable built-in defaults."
    requirement: "16a, 16b, 16d, 16e"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#missing_file_uses_devflow_defaults"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#file_overrides_capture_retention_default"
        status: pass
    human_judgment: false
  - id: D2
    description: "Environment values override devflow.toml for capture retention, Ship review angles, and external verification."
    requirement: "16a, 16b, 16d, 16e"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#env_overrides_file_capture_retention"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#env_overrides_file_review_angles"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#env_overrides_file_external_verification"
        status: pass
    human_judgment: false
  - id: D3
    description: "Malformed devflow.toml content degrades to defaults without aborting the workflow."
    requirement: "16a, 16b, 16d, 16e"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#malformed_file_falls_back_to_defaults"
        status: pass
      - kind: other
        ref: "cargo clippy --workspace -- -D warnings"
        status: pass
    human_judgment: false

duration: 23min
completed: 2026-07-17
status: complete
---

# Phase 16 Plan 02: Minimal Configuration Foundation Summary

**A typed, fail-soft `devflow.toml` surface now supplies Phase 16 reliability knobs through tested env-over-file-over-default resolvers while preserving the fixed git-flow model.**

## Performance

- **Duration:** 23 min
- **Started:** 2026-07-18T00:00:21Z
- **Completed:** 2026-07-18T00:23:27Z
- **Tasks:** 3
- **Files modified:** 5

## Accomplishments

- Added the approved official `toml = "1"` dependency, resolved to `1.1.2+spec-1.1.0`, and a serde-default `DevflowConfig` loader.
- Added env-first resolver functions for `capture_retention`, `review_angles`, and `external_verify_enabled`, with invalid values falling back safely.
- Added seven focused config tests proving defaults, file parsing, all three environment overrides, and malformed-file recovery.

## Task Commits

1. **Task 0: Record operator approval of the official toml-rs/toml dependency** - `158a828` (docs)
2. **Task 1: Add the dependency and typed fail-soft loader** - `dae5770` (feat)
3. **Task 2 RED: Define the missing precedence contract** - `9bc5ab3` (test)
4. **Task 2 GREEN: Add env-first resolver functions** - `8c41c94` (feat)
5. **Task 2 coverage: Exercise every environment override** - `9f2fb4f` (test)

## Files Created/Modified

- `Cargo.toml` - Adds the workspace `toml = "1"` dependency constraint.
- `Cargo.lock` - Locks `toml` and its parser/serializer dependency graph at the resolved versions.
- `crates/devflow-core/Cargo.toml` - Enables the workspace TOML dependency for the core library.
- `crates/devflow-core/src/config.rs` - Defines `DevflowConfig`, the fail-soft loader, env-first resolvers, and focused unit tests.
- `.planning/phases/16-pipeline-reliability-hardening/16-02-PLAN.md` - Records the completed blocking-human dependency approval.

## Resolver Contract

- `load_config(project_root: &Path) -> DevflowConfig`
- `capture_retention(project_root: &Path) -> usize` via `DEVFLOW_CAPTURE_RETENTION`
- `review_angles(project_root: &Path) -> Option<Vec<String>>` via comma-separated `DEVFLOW_REVIEW_ANGLES`
- `external_verify_enabled(project_root: &Path) -> bool` via `DEVFLOW_EXTERNAL_VERIFY_ENABLED`

## Decisions Made

- Downstream plans consume the public resolver functions so precedence logic stays centralized.
- Empty or invalid environment values never abort execution; they warn when appropriate and defer to `devflow.toml` or defaults.
- The minimal file carries only Phase 16 knobs. `GitFlowConfig` remains hardcoded and unchanged.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Closed an interrupted partial implementation before summary generation**
- **Found during:** Safe-resume inspection after validation loop-back
- **Issue:** Production commit `dae5770` added the TOML struct and loader but returned without the required per-knob env resolvers, precedence tests, or `16-02-SUMMARY.md`.
- **Fix:** Added the missing RED/GREEN precedence cycle, covered every knob, reran CI parity, and completed the atomic plan close-out.
- **Files modified:** `crates/devflow-core/src/config.rs`
- **Verification:** Scoped config tests, workspace tests, clippy, formatting, schema-drift gate, and UI-safety gate all pass.
- **Committed in:** `9bc5ab3`, `8c41c94`, `9f2fb4f`

---

**Total deviations:** 1 auto-fixed bug. **Impact:** Restored the plan's specified behavior and legal close-out state without expanding configuration beyond D-03.

## Issues Encountered

- The original Code run correctly stopped at the non-auto-approvable package-legitimacy gate. Operator approval was recorded before the dependency edit.
- The validation loop could not collect an interactive gap disposition, leaving committed production work without a summary. Safe-resume inspection recovered the partial plan instead of duplicating it.
- The codebase-drift capability emitted an existing repository-wide advisory; it was non-blocking and requested no mapper dispatch. Schema drift and UI safety both reported `block: false`.

## User Setup Required

None - `devflow.toml` is optional and absent-file behavior is unchanged.

## Verification

- `cargo test -p devflow-core config::` - pass (7 tests)
- `cargo build --workspace` - pass
- `cargo test --workspace` - pass (282 tests)
- `cargo clippy --workspace -- -D warnings` - pass
- `cargo fmt --all --check` - pass
- `rg 'no config file' crates/devflow-core/src/config.rs` - no match
- `verify.schema-drift` capability gate - pass (`block: false`)
- `ui.safety-gate` capability gate - pass (`block: false`)

## Next Phase Readiness

- Plan 16-03 can use `capture_retention()` and `external_verify_enabled()` without implementing configuration or precedence again.
- Plan 16-04 can use `review_angles()` for its Ship prompt override.
- Plan 16-05 can reuse the same TOML dependency for its checked-in doc-claim allowlist.
- No blockers remain from Plan 16-02.

## Self-Check: PASSED

- All key modified files exist.
- Task commits `158a828`, `dae5770`, `9bc5ab3`, `8c41c94`, and `9f2fb4f` exist in repository history.
- Every task acceptance criterion and plan-level verification passes at current HEAD.

---
*Phase: 16-pipeline-reliability-hardening*
*Completed: 2026-07-17*
