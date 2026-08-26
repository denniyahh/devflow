---
phase: 43-opencode-driver-completion
verified: 2026-08-24T01:33:31Z
status: passed
score: 4/4 must-haves verified
behavior_unverified: 0
overrides_applied: 0
---

# Phase 43: OpenCode Driver Completion Verification Report

**Phase Goal:** `devflow start --agent opencode` runs headless with `--auto` + `--format json`, and completion/verdict is parsed from the JSON events.
**Verified:** 2026-08-24T01:33:31Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (Roadmap Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | The driver launches `opencode run "<prompt>" --auto --format json` | ✓ VERIFIED | `crates/devflow-core/src/agents/opencode.rs:46-62` `build_command` returns exactly `["run", prompt, "--auto", "--format", "json"]` — five separate argv elements, no `--format=json` joining. Confirmed by reading source and by `cargo test -p devflow-core --lib agents::opencode_wraps_prompt_in_run` / `agent_result::tests::opencode_build_command_is_headless_json` (both pass). |
| 2 | Completion/verdict is parsed from `--format json` events, regression-tested against a real capture (not an assumed schema) | ✓ VERIFIED | `parse_opencode_event_result` (`agent_result.rs:904-978`) implements torn-tail → error-event → marker-scan precedence exactly per D-04/D-05/D-06. All three real `43-evidence/*.jsonl` captures are vendored under `crates/devflow-core/tests/fixtures/opencode/` and are byte-identical to the evidence sources (`diff` confirmed clean for all three). They are parsed verbatim by named regression tests (`opencode_real_success_capture_is_recognised_and_marker_less`, `opencode_real_error_capture_is_failed`, `opencode_real_tool_use_capture_defers_to_layer2`) — all pass. |
| 3 | The health check fails closed when OpenCode is not usable | ✓ VERIFIED | `OpenCodeDriver::health` (`opencode.rs:88-100`) requires BOTH `output.status.success()` AND a positive parsed credential/env-var count — not exit code alone, not model-catalog. A spawn failure also maps to `Err`. Verified by 8 passing tests including the WR-01 regression `preflight_rejects_nonzero_exit_with_credential_bearing_stdout` (a stub with well-formed credential-bearing stdout but exit 1 correctly refuses). |
| 4 | The driver passes the shared conformance suite | ✓ VERIFIED | `every_driver_passes_the_conformance_suite` (`agents/mod.rs:286-311`) includes `OpenCodeDriver` in its 6-driver array and exercises real (non-default) `health`/`capabilities`/`parse_completion` bodies. `cargo test -p devflow-core --lib agents::tests::every_driver_passes_the_conformance_suite -- --exact` → `1 passed`. |

**Score:** 4/4 truths verified

### Observable Truths (Plan-Level must_haves, condensed)

All 12 must_haves.truths from 43-01-PLAN.md and 12 from 43-02-PLAN.md were individually checked against source and re-run tests. All verified, with one documented, justified deviation:

- **Deviation (documented in 43-01-SUMMARY.md, sound):** `is_opencode_event_stream`'s literal must-have text says "returns true only when... `step_start` or `step_finish`." The as-built gate was widened to also recognize OpenCode's own nested `error.name`-bearing envelope, because the real `opencode_error.jsonl` evidence capture is a single-line, error-only stream with no `step_start` at all — under the literal gate, the plan's own required truth ("a `type:"error"` event anywhere in the stream resolves to Failed," proven against the real error capture) would be unsatisfiable. The widened gate is scoped narrowly (requires the nested `error.name` shape, not a bare `type:"error"`) and T-43-07's cross-adapter-collision concern still holds — verified no other adapter's failure envelope matches this shape (Codex: `turn.failed`; Claude: `is_error` inside `type:"result"`), and the cross-adapter isolation tests (`opencode_detector_rejects_foreign_streams`, `opencode_non_stream_input_returns_none`) still pass. This is a correction that makes the implementation match the plan's own literal acceptance criteria, not a scope reduction.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/tests/fixtures/opencode/opencode_success.jsonl` | verbatim real capture | ✓ VERIFIED | `diff` against `43-evidence/opencode_success.jsonl` exits 0 |
| `crates/devflow-core/tests/fixtures/opencode/opencode_tool_use.jsonl` | verbatim real capture | ✓ VERIFIED | `diff` clean |
| `crates/devflow-core/tests/fixtures/opencode/opencode_error.jsonl` | verbatim real capture | ✓ VERIFIED | `diff` clean |
| `crates/devflow-core/tests/fixtures/opencode/opencode_success_with_marker.jsonl` | derived fixture, labelled | ✓ VERIFIED | Contains `DEVFLOW_RESULT:` inside `part.text`; filename/doc comments mark it derived, no in-JSONL comment pollution |
| `crates/devflow-core/src/agent_result.rs` | `is_opencode_event_stream`, `parse_opencode_event_result`, `evaluate_layer1` entry, regression tests | ✓ VERIFIED | All present, wired, 19 `opencode_*` tests pass |
| `crates/devflow-core/src/agents/opencode.rs` | real `build_command`, `parse_completion`, `health`, `capabilities` | ✓ VERIFIED | All four present and substantive (569 lines, was a 28-line stub); 20 `agents::opencode::` tests pass |
| `crates/devflow-core/src/agents/mod.rs` | updated argv assertions, OpenCode carve-out in `default_preflight_is_ok_for_built_in_adapters` | ✓ VERIFIED | Both argv tests assert the 5-element argv; carve-out doc comment corrected and code updated |
| `crates/devflow-cli/src/commands.rs` | corrected opencode doctor hint | ✓ VERIFIED | `cmd_check("opencode", "opencode", "--version", "npm i -g opencode-ai")`; `rg -n 'cargo install opencode' crates/` returns no match |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `evaluate_layer1` | `parse_opencode_event_result` | `.or_else` chain entry | ✓ WIRED | `agent_result.rs:2263`, positioned after `parse_codex_event_result` and before `detect_codex_rate_limit`, exactly as specified |
| `OpenCodeDriver::parse_completion` | `agent_result::parse_opencode_event_result` | one-line delegation | ✓ WIRED | `opencode.rs:69-71`, no reimplementation |
| `parse_opencode_event_result` | `normalise_stream_marker_provenance` | marker-forgery guard | ✓ WIRED | `agent_result.rs:972`; `opencode_marker_cannot_forge_layer0_provenance` passes |
| `devflow-cli::preflight` | `OpenCodeDriver::health` | trait method (real, not default) | ✓ WIRED | `health` overridden, not the trait default `Ok(())`; conformance suite exercises the real body |
| `OpenCodeDriver::capabilities` | `opencode_subagent_dispatch_available` | probe wiring | ✓ WIRED | Returns `DriverCapabilities` (never `Result`), confirmed by `capabilities_never_refuses_a_launch` |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| OPCD-01 | 43-01 | Headless launch `--auto --format json` | ✓ SATISFIED | `build_command` argv verified above |
| OPCD-02 | 43-01 | Completion parsed from JSON events, regression-tested against real capture | ✓ SATISFIED | `parse_opencode_event_result` + 3 real fixtures verified above |
| OPCD-03 | 43-02 | Fail-closed health check + capability discovery | ✓ SATISFIED | `health`/`capabilities` verified above, including WR-01 exit-status fix |

**Note (informational, not a gap):** `.planning/REQUIREMENTS.md`'s checkbox/status table for OPCD-01/02/03 still reads `[ ]` / "Pending" as of this verification. This matches the same not-yet-updated pattern already present for the prior Phase 42 (HRMS-01/02/03) requirements — updating REQUIREMENTS.md checkboxes appears to be a separate, later step in this project's workflow (not part of execute-phase), not a phase-43-specific gap.

### Anti-Patterns Found

None. Scanned all four modified/created source files (`opencode.rs`, `agent_result.rs`, `mod.rs`, `commands.rs`) for `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER`/stub-return patterns — no matches outside of an unrelated pre-existing doc comment in `commands.rs` about changelog "em-dash placeholders" (unrelated string literal, not a debt marker).

### Behavioral Spot-Checks / Test Suite Execution (run fresh in this session, not trusted from SUMMARY.md)

| Check | Command | Result | Status |
|-------|---------|--------|--------|
| OpenCode-specific `agent_result.rs` tests | `cargo test -p devflow-core --lib opencode_` | 19 passed; 0 failed | ✓ PASS |
| `opencode.rs` driver module tests | `cargo test -p devflow-core --lib agents::opencode::` | 20 passed; 0 failed (includes all 4 WR-01..WR-04 regression tests) | ✓ PASS |
| Full `devflow-core` lib suite | `cargo test -p devflow-core --lib` | 722 passed; 0 failed | ✓ PASS |
| Conformance suite (exact name) | `cargo test -p devflow-core --lib agents::tests::every_driver_passes_the_conformance_suite -- --exact` | 1 passed | ✓ PASS |
| Full workspace suite | `cargo test --workspace --no-fail-fast` | 0 `FAILED` lines across every crate/target (confirmed by grep on saved output) | ✓ PASS |
| Repo gate script | `scripts/check.sh all` | `check.sh: all OK` | ✓ PASS |
| Clippy (lib only) | `cargo clippy -p devflow-core --lib -- -D warnings` | clean | ✓ PASS |
| Clippy (all-targets) | `cargo clippy -p devflow-core --all-targets -- -D warnings` | Fails — `devflow_core::test_support` not found in `monitor_e2e`/`devflow_dir_gitignore` integration tests | ⚠️ PRE-EXISTING, not phase-43 | This is the same defect REVIEW.md documented and reproduced independently on `develop` before this phase's code existed; the `#[cfg(any(test, feature = "test-support"))]` gate on `lib.rs`'s `test_support` module predates phase 43 (git blame: commit `35654ad`, an unrelated older commit). `scripts/check.sh all` (the repo's actual gate) passes because it does not run bare `--all-targets` clippy this way. Not counted as a phase-43 gap. |
| Fixture byte-identity to real evidence | `diff` all 3 real fixtures vs `43-evidence/` | 0 exit code, all identical | ✓ PASS |
| Fixture leak scan | `rg -c 'denniyahh\|/home/\|/var/home/\|/Users/\|sk-[A-Za-z0-9]\|AIza\|Bearer ' crates/devflow-core/tests/fixtures/opencode/` | exit 1 (no match) | ✓ PASS |
| Fixtures packaged in crate | `cargo package --list -p devflow-core --allow-dirty \| rg -c 'tests/fixtures/opencode/'` | 4 | ✓ PASS |
| No `regex` crate added | `rg -n '^name = "regex"$' -A2 Cargo.lock` | no match (only pre-existing transitive `regex-automata`/`regex-syntax`) | ✓ PASS |
| Doctor hint corrected | `rg -n 'cargo install opencode' crates/` | no match | ✓ PASS |

### Code Review Fix Verification (independent, not trusted from REVIEW-FIX.md)

The prior `43-REVIEW.md` found 4 Warning-severity fail-closed gaps (WR-01 through WR-04). `43-REVIEW-FIX.md` claims all 4 were fixed in commit `35e357c`. Verified independently by reading current source (not the commit message):

- **WR-01** (health ignored exit status): confirmed fixed — `health` now requires `output.status.success() && ... > 0` (`opencode.rs:93-95`). Regression test `preflight_rejects_nonzero_exit_with_credential_bearing_stdout` passes.
- **WR-02** (ANSI stripper only terminated on `m`): confirmed fixed — terminator widened to ECMA-48 final-byte range `0x40..=0x7E` (`opencode.rs:126-134`). Regression test `strip_ansi_escapes_terminates_on_non_sgr_csi_sequence` passes.
- **WR-03** (provider count had no positional anchor): confirmed fixed — match now requires the line start with the `└` footer glyph specifically (`opencode.rs:164`). Regression test `provider_count_ignores_unanchored_matching_substring` passes.
- **WR-04** (subagent classifier did raw substring scan): confirmed fixed — match now requires a trailing `(subagent)`/`(all)` marker on a non-`[`/`{`-prefixed line (`opencode.rs:219-224`). Regression test `agent_list_ignores_marker_text_inside_json_dump_line` passes.

All 4 fixes are real, present in source, and independently test-verified in this session (not just re-stated from the fix report).

### Human Verification Required

None. All must-haves are code-presence-verifiable, wiring-verifiable, and covered by passing automated tests; no runtime/visual/external-service behavior requires human judgment for this phase's scope. The one genuinely unverified item — the zero-credential `opencode providers list` shape on a real credential-less machine (A1/P-05) — is explicitly and correctly disclosed as unverified in both plans, both summaries, and the source doc comments themselves; it is not silently claimed as tested, so it does not need to be raised again here as a fresh gap.

### Gaps Summary

No gaps found. All four ROADMAP success criteria are observably true in the codebase: the driver launches the real headless argv, completion is parsed from real captured JSON events with all three real evidence captures vendored and regression-tested, the health check fails closed on both zero-credential and non-zero-exit cases, and the driver passes the shared 6-driver conformance suite. The four Warning-severity findings from `43-REVIEW.md` were independently re-verified as fixed in source (not merely trusted from `43-REVIEW-FIX.md`). Full workspace test suite (1082+ tests across all reported suites) and `scripts/check.sh all` are green, run fresh in this verification session.

---

_Verified: 2026-08-24T01:33:31Z_
_Verifier: Claude (gsd-verifier)_
