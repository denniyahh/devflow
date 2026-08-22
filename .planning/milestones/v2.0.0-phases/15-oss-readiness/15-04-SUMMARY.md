---
phase: 15-oss-readiness
plan: 04
subsystem: infra
tags: [licensing, spdx, cargo-publish, packaging]

# Dependency graph
requires:
  - phase: 15-oss-readiness (15-01/02/03)
    provides: SECURITY.md/DEPENDENCIES.md accuracy, ARCHITECTURE.md rewrite, CONTRIBUTING.md + devcontainer
provides:
  - "LICENSE-APACHE canonical Apache-2.0 text at repo root, backing the existing dual-license SPDX claim"
  - "Proven-green cargo publish --dry-run / cargo package --workspace baseline for Plan 15-05's real publish"
affects: [15-05-oss-readiness]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Reconstructing legal license text: cross-verified against an already-vendored canonical copy in ~/.cargo/registry/src (cfg-if crate's LICENSE-APACHE) instead of relying on from-memory recall, after an initial from-memory draft was found to contain garbled clauses in sections 8/9 and the Appendix intro"

key-files:
  created: [LICENSE-APACHE]
  modified: []

key-decisions:
  - "Kept dual license (MIT OR Apache-2.0) per plan's locked resolution — added the missing LICENSE-APACHE rather than narrowing Cargo.toml to MIT-only"
  - "Sourced canonical Apache-2.0 body from an already-vendored copy on disk (cargo registry cache) rather than trusting model recall, after a first draft was caught with garbled section 8/9 text during self-review"

patterns-established: []

requirements-completed: [15b]

coverage:
  - id: D1
    description: "LICENSE-APACHE added with canonical Apache-2.0 text and copyright holder matched to existing LICENSE (Dennis Kim, 2026)"
    requirement: "15b"
    verification:
      - kind: other
        ref: "rg -q 'Apache License' LICENSE-APACHE && rg -q 'Version 2.0' LICENSE-APACHE && ! rg -q 'Permission is hereby granted, free of charge' LICENSE-APACHE"
        status: pass
      - kind: other
        ref: "diff against ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cfg-if-1.0.4/LICENSE-APACHE (byte-identical except copyright line)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Packaging dry-runs (cargo publish --dry-run -p devflow-core, cargo package --workspace) pass cleanly against the current tree"
    requirement: "15b"
    verification:
      - kind: other
        ref: "cargo publish --dry-run -p devflow-core (exit 0)"
        status: pass
      - kind: other
        ref: "cargo package --workspace (exit 0)"
        status: pass
      - kind: integration
        ref: "cargo test -p devflow --test help_snapshot"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-07-17
status: complete
---

# Phase 15 Plan 04: Dual-License Closure + Publish Baseline Summary

**Added canonical LICENSE-APACHE (byte-verified against a vendored crate's copy) so the existing `MIT OR Apache-2.0` SPDX claim is finally backed by both license texts, then re-proved `cargo publish --dry-run` and `cargo package --workspace` green for Plan 15-05.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-07-17T14:20:00Z (approx)
- **Completed:** 2026-07-17T14:32:26Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- `LICENSE-APACHE` created at repo root with the canonical, unmodified Apache License 2.0 body and a copyright line matched to the existing `LICENSE` file's holder (Dennis Kim, 2026)
- Confirmed Cargo.toml's `license = "MIT OR Apache-2.0"` and README's badge/section now agree with two real on-disk license files instead of one
- Re-ran and confirmed green: `cargo publish --dry-run -p devflow-core` (packages, verifies, compiles from the packaged form, aborts only at the dry-run upload step as expected) and `cargo package --workspace` (packages both `devflow-core` and `devflow` CLI crates cleanly)
- Additionally re-ran `cargo test -p devflow --test help_snapshot` (the plan's `<verification>` cross-check that no CLI source was touched) — passed

## Task Commits

Each task was committed atomically:

1. **Task 1: Add canonical LICENSE-APACHE matching the declared dual license** - `58ca12e` (feat)
2. **Task 2: Re-verify publish readiness (dry-run + workspace package)** - no commit (verification-only task; no files modified, per plan's `files_modified` scope)

**Plan metadata:** (recorded below)

## Files Created/Modified
- `LICENSE-APACHE` - Canonical Apache License 2.0 text, copyright line "Copyright 2026 Dennis Kim" matched to the existing MIT `LICENSE` file

## Decisions Made
- Kept the dual license (per the plan's locked resolution) rather than narrowing Cargo.toml to MIT-only — the missing file was the gap, not the SPDX claim itself.
- After an initial from-memory reconstruction of the Apache-2.0 body was self-caught with garbled/nonsensical wording in Section 8 ("Limitation of Liability"), Section 9 ("Accepting Warranty..."), and the Appendix intro paragraph, sourced the canonical text directly from an already-vendored copy in the local Cargo registry cache (`~/.cargo/registry/src/.../cfg-if-1.0.4/LICENSE-APACHE`) and diffed byte-for-byte to confirm exact match before committing, rather than trusting a second from-memory attempt. Only the `[yyyy] [name of copyright owner]` placeholder line was substituted.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Plan's Task 1 verify command has a `rg -c` zero-match quirk that would fail even on a correct file**
- **Found during:** Task 1 verification
- **Issue:** The plan's automated verify step uses `test "$(rg -c 'Permission is hereby granted, free of charge' LICENSE-APACHE)" = "0"`. Ripgrep's `-c` flag prints nothing and exits 1 on zero matches — it does not print the literal string `"0"` — so this comparison evaluates `"" = "0"` (false) even when the file correctly contains no MIT text. This is a plan-authoring quirk, not an application bug.
- **Fix:** Re-expressed the same intent with `! rg -q 'Permission is hereby granted, free of charge' LICENSE-APACHE` (checks the "no match" condition directly via exit code) and confirmed it passes. No source files changed; this only affects how the check was executed during this run.
- **Files modified:** none (verification-only)
- **Verification:** Manually ran both the literal plan command (confirmed it fails despite the file being correct) and the corrected equivalent (confirmed it passes)
- **Committed in:** N/A (verification methodology only, no code change)

---

**Total deviations:** 1 auto-fixed (1 verify-command correction, no source impact)
**Impact on plan:** Zero impact on deliverables — the underlying acceptance criterion (LICENSE-APACHE contains no MIT boilerplate) is satisfied; only the literal shell one-liner in the plan doesn't match real `rg -c` semantics.

## Issues Encountered
- First draft of `LICENSE-APACHE`, written from model recall, contained garbled/nonsensical clauses in Section 8, Section 9, and the Appendix intro. Caught via self-review before committing, then resolved by sourcing the canonical text byte-for-byte from a vendored copy already present in the local Cargo registry cache (see Decisions Made). No garbled text was ever committed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Plan 15-05 (the real `cargo publish` to crates.io, non-autonomous, requires operator's crates.io token) now has a proven-green baseline: both license files present and correct, `cargo publish --dry-run` and `cargo package --workspace` pass on the current tree.
- No CLI or core source code was touched by this plan (packaging metadata + license file only), consistent with the plan's explicit prohibition.
- This was the last Wave 1 plan for 15b; Wave 2 (15-05) can now proceed once all four Wave 1 plans are confirmed complete.

---
*Phase: 15-oss-readiness*
*Completed: 2026-07-17*

## Self-Check: PASSED

- FOUND: LICENSE-APACHE
- FOUND: 58ca12e (Task 1 commit)
- FOUND: 0d9d93e (SUMMARY commit)
