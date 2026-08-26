---
phase: 43-opencode-driver-completion
fixed_at: 2026-08-23T21:00:00Z
review_path: .planning/phases/43-opencode-driver-completion/43-REVIEW.md
iteration: 1
findings_in_scope: 4
fixed: 4
skipped: 0
status: all_fixed
---

# Phase 43: Code Review Fix Report

**Fixed at:** 2026-08-23
**Source review:** `43-REVIEW.md` (status: issues_found, 0 critical / 4 warning / 1 info)
**Iteration:** 1

## Summary

All 4 Warning findings (WR-01 through WR-04) confirmed by `43-REVIEW.md` — themselves
independently re-derived from an earlier ad hoc adversarial review (`43-ADVERSARIAL-REVIEW.md`,
Codex + Antigravity) — were fixed in a single commit, applied directly rather than via
`gsd-code-fixer` since all four fixes were small, localized, and the reviewer had already
supplied exact patches.

IN-01 (disputed error-anywhere-in-stream precedence) was left as-is per the review's own
disposition: "no action recommended; revisit only if a live capture surfaces a genuine
recovered-transient-error stream."

## Fixes Applied

### WR-01: `health()` never checked subprocess exit status
**Status:** fixed
**File:** `crates/devflow-core/src/agents/opencode.rs`
**Commit:** `35e357c`
`health()` now requires `output.status.success() && opencode_configured_provider_count(...) > 0`
— both conditions, not the parsed count alone. Regression test:
`preflight_rejects_nonzero_exit_with_credential_bearing_stdout`.

### WR-02: `strip_ansi_escapes` assumed every CSI sequence terminates in `m`
**Status:** fixed
**File:** `crates/devflow-core/src/agents/opencode.rs`
**Commit:** `35e357c`
Terminator check widened to the full ECMA-48 CSI final-byte range (`0x40..=0x7E`), not just SGR's
`m`. Regression test: `strip_ansi_escapes_terminates_on_non_sgr_csi_sequence`.

### WR-03: `opencode_configured_provider_count` had no positional anchor
**Status:** fixed
**File:** `crates/devflow-core/src/agents/opencode.rs`
**Commit:** `35e357c`
Match now anchored to a line starting with the `└` footer glyph specifically (not any of
`└`/`┌`/`│`/`●`). Regression test: `provider_count_ignores_unanchored_matching_substring`.

### WR-04: `parse_opencode_agent_list_for_subagent` did a raw substring scan
**Status:** fixed
**File:** `crates/devflow-core/src/agents/opencode.rs`
**Commit:** `35e357c`
Match now requires a trailing `(subagent)`/`(all)` marker on a non-JSON line (excludes lines
starting with `[`/`{`), not `.contains(...)` over every line. Regression test:
`agent_list_ignores_marker_text_inside_json_dump_line`.

## Verification

Independently re-verified by a fresh `gsd-code-reviewer` pass (not the same session that applied
the fixes) reading current source directly — all 4 CONFIRMED-FIXED, not assumed from the commit
message. Test suite, clippy, and fmt re-run fresh (clippy cache-bust via `touch` to rule out a
stale hit):

```
cargo test -p devflow-core --lib          → 722 passed; 0 failed
cargo test -p devflow-core --lib agents::opencode::  → 20 passed; 0 failed (16 original + 4 new regression tests)
cargo clippy -p devflow-core --lib -- -D warnings    → clean
cargo fmt --check                                     → clean
scripts/check.sh all (host, full workspace + integration tests) → OK
```

## What this does NOT establish

- WR-02's fix was verified by tracing the ECMA-48 CSI grammar and constructing the specific
  non-SGR sequence the finding named; it was not fuzz-tested against the full space of malformed
  or nested escape sequences. A bare ESC appearing before a CSI sequence's final byte (a malformed
  nested escape) would still be silently absorbed rather than treated as a cancel — not the defect
  class WR-02 named, and not observed in any live `opencode` capture, but not proven absent either.
- WR-04's fix narrows but does not eliminate every false-positive: a body/description line whose
  trimmed text happens to end in exactly `"(subagent)"` with no trailing punctuation would still
  match. This is accepted per 43-CONTEXT's D-10 (capability-probe result can never refuse a
  launch — a false `true` here only affects an advisory `DriverCapabilities` field, not `health`).
- None of the 4 fixes were exercised against a live `opencode` binary — all regression tests are
  constructed fixtures (synthetic or reasoned from the one live positive-credential capture in
  `43-RESEARCH.md`), consistent with the phase's pre-existing "Honest limit (A1, P-05)" disclosure
  that a real zero-credential machine was never observed.
