---
phase: 21-operator-usability-release-execution
verified: 2026-07-23T00:00:00Z
status: passed
score: 20/20 must-haves verified
behavior_unverified: 0
overrides_applied: 0
---

# Phase 21: Operator Legibility & Observability Verification Report

**Phase Goal:** Make DevFlow's operator surface **legible** and its self-reported state **trustworthy** — every unit single-writer, operator-facing, reversible or detection-only, and testable without any irreversible side effect. Four units: 21a (operator discoverability), 21b (doctor planning-doc staleness reconciliation), 21c (sequentagent second-agent tracking), 21d (content-aware dogfood staleness ancestry arm).

**Verified:** 2026-07-23
**Status:** passed
**Re-verification:** No — initial verification

**Note on scope:** This project carries no REQUIREMENTS.md / REQ-IDs. Phase 21's contract lives in `21-CONTEXT.md`'s decisions D-01..D-08 and each plan's `must_haves` frontmatter (21-01=21d/D-07, 21-02=21a/D-03, 21-03=21b/D-04/D-05, 21-04=21c/D-06). All must-haves below are the union of the four plans' `must_haves.truths` blocks, cross-checked against ROADMAP.md's Phase 21 unit descriptions.

**Note on pre-existing artifact:** `.planning/phases/21-operator-usability-release-execution/21-VALIDATION.md` exists (a `status: draft` per-phase validation-strategy scaffold, not owned by this verification pass) and was left untouched — not treated as a gap.

## Goal Achievement

### Observable Truths

#### 21d — Content-aware dogfood staleness ancestry arm (21-01)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Docs-only strict-ancestor range → `Fresh` (no hard block) | ✓ VERIFIED | `ancestry_range_affects_build` (staleness.rs:94) filters `git diff --name-only` through `affects_compiled_binary`; `staleness::tests::docs_only_range_is_fresh` passes (`cargo test --workspace staleness::` → 35 passed, 0 failed) |
| 2 | Mixed docs+`.rs` range still `Stale`, still hard-blocks (Phase 16 false-evidence protection preserved) | ✓ VERIFIED | `staleness::tests::mixed_range_docs_and_source_is_stale`, `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`, `embedded_commit_is_stale_maps_ancestry_exit_codes` all pass; fixtures retargeted from `.txt` to `.rs`/build files per plan |
| 3 | `affects_compiled_binary` reused verbatim, not forked | ✓ VERIFIED | `rg -n "fn ancestry_range_affects_build"` → exactly one hit (staleness.rs:94); helper body calls `affects_compiled_binary` (line 99); no second `BUILD_AFFECTING_FILES` list exists |
| 4 | A git failure diffing the range fails toward `Stale`, never a false `Fresh` | ✓ VERIFIED | `ancestry_range_affects_build` returns `true` via `.unwrap_or(true)` (staleness.rs:100); `staleness::tests::git_error_range_fails_toward_stale` passes |
| 5 | Block message no longer misstates ancestry for the common ancestor-but-behind case | ✓ VERIFIED | `enforce_build_staleness`'s message (staleness.rs:322-337) now reads "a build-relevant file … changed … since this devflow binary was built, or its embedded commit is not an ancestor … at all" — describes the build-relevant-change condition, not a blanket non-ancestor claim |
| 6 | `Ok(Some(1))` reverse-probe arm and `Indeterminate` fallbacks behaviorally untouched | ✓ VERIFIED | Direct code read (staleness.rs:69-81) confirms the reverse-probe block and `_ => Indeterminate` fallback are unchanged from the documented pre-move shape; only the `Ok(Some(0))`/`Some(_)` line was narrowed |
| 7 | `events::emit` payload stays path/username-free (WR-02) | ✓ VERIFIED | `events::emit` call (staleness.rs:350-359) emits only `{stage, reason, worktree}`; `execution_root.display()` appears only in the `message`/`fire_gate_notify`/`Err` path |

#### 21a — Operator discoverability (21-02)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 8 | `devflow gate show <phase>` prints the FULL, untruncated (sanitized) gate context | ✓ VERIFIED | `gate_show`/`render_gate_show` (commands.rs:814-870) call `render_gate_context(&gate.context, usize::MAX)`, never the `100`-char form used by `gate_list` (commands.rs:724); `gate_show_renders_full_untruncated_sanitized_context` test passes |
| 9 | `gate show` auto-resolves a single open gate; errors asking for `--stage` on several; errors naming `gate list` on none | ✓ VERIFIED | commands.rs:819-844 mirrors `gate_respond`'s resolution block; `gate_show_auto_resolves_single_open_gate`, `gate_show_errors_asking_for_stage_with_several_open_gates`, `gate_show_errors_naming_gate_list_when_no_open_gate` all pass |
| 10 | Rate-limit reset time surfaced in `status` from existing `CronInstructions.retry_after`, not re-detected | ✓ VERIFIED | `cron_hint_line` (commands.rs:1099-1115) reads `instructions.retry_after` only, sanitizes via `render_gate_context(retry_after, 100)`; `rg "detect_rate_limit\|detect_claude_rate_limit\|detect_codex_rate_limit"` → no hits; `cron_hint_line_appends_sanitized_reset_when_retry_after_present`/`_omits_reset_fragment_when_retry_after_empty` pass |
| 11 | Stuck phase surfaces recovery verbs (`resume`, `advance` when gate-pending) | ✓ VERIFIED | `recovery_hints` (commands.rs:536-545), wired into `status()`'s per-phase loop (commands.rs:655-657); `recovery_hints_includes_resume_for_stuck`, `_includes_advance_when_stuck_and_gate_pending`, `_empty_for_healthy` pass |
| 12 | In-stage progress line uses REAL stage age from the latest `stage_launched` event `ts`, never phase-level `State.started_at` | ✓ VERIFIED | `latest_stage_launched_ts` (commands.rs:553-564) scans `events.jsonl` for `stage_launched` events, never reads `state.started_at`; `render_stage_progress_line` (commands.rs:570-578) omits age when `None`; `latest_stage_launched_ts_reflects_event_age_not_phase_started_at` (proves ~90s stage age rendered, not 30m phase age) and `render_stage_progress_line_omits_age_without_stage_launched_event` both pass — closes the phase's single unanimous 3/3-review MUST-FIX |

#### 21b — Doctor planning-doc staleness reconciliation (21-03)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 13 | `doctor` (text + `--json`) flags a ROADMAP.md/STATE.md version claim whose git tag doesn't exist/isn't reachable from `main` | ✓ VERIFIED | `reconcile_planning_docs` + `tag_exists_and_reachable` (commands.rs:2203-2262); **live-run confirmed**: `./target/debug/devflow doctor --json .` against this repo's real docs returns `planning_doc_staleness` with exactly 4 `Warn` findings (Phase 6/7's pre-cutoff `v1.0.0`/`v0.5.1` claims), matching the SUMMARY's stated live-verification result exactly |
| 14 | Version-cell parsing normalizes bare cells to `v`-prefixed tags | ✓ VERIFIED | `reconcile_planning_docs` (commands.rs:2238-2242) prefixes `v` when absent; `reconcile_planning_docs_normalizes_bare_cell_to_v_prefixed_tag` test passes |
| 15 | `doctor --json` stays a SINGLE JSON object with `planning_doc_staleness` as a THIRD key, never a second concatenated array (WR-01) | ✓ VERIFIED | Live run: `Object.keys(doctor --json output)` = `[environment, planning_doc_staleness, reconciliation]` — one JSON object, three keys; `doctor_json_body_carries_planning_doc_staleness_as_a_third_key` test passes |
| 16 | Detection-only: no write path to ROADMAP.md/STATE.md, `repair: None` always (D-04) | ✓ VERIFIED | `rg "OpenOptions\|fs::write\|File::create"` over commands.rs shows only test-fixture writes (inside `#[cfg(test)]` blocks); `PlanningDocFinding.repair` is hardcoded `None` at every construction site (commands.rs:2258) |
| 17 | v1.5.0 cutoff uses NUMERIC `(major, minor, patch)` tuple comparison, not lexicographic | ✓ VERIFIED | `PLANNING_DOC_STALENESS_CUTOFF: (u32, u32, u32) = (1, 5, 0)` (commands.rs:2108); comparison is `parsed >= PLANNING_DOC_STALENESS_CUTOFF` on the parsed tuple (commands.rs:2246); `reconcile_planning_docs_numeric_cutoff_is_not_lexicographic` test passes (`1.10.0` unreachable → Problem, `1.4.0` unreachable → Warn) |
| 18 | Missing ROADMAP.md/STATE.md is best-effort (no error, no fabricated Problem) | ✓ VERIFIED | `collect_planning_doc_findings` (commands.rs:2276-2287) uses `.unwrap_or_default()` on both file reads; `collect_planning_doc_findings_missing_files_yield_no_findings_not_error` test passes |

#### 21c — sequentagent second-agent tracking (21-04)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 19 | sequentagent writes a path-free per-phase record naming the running slot (A/B) + `AgentKind`, NOT routed through `State`/`save_state` | ✓ VERIFIED | `SequentagentSlotKind`/`SequentagentSlot`/`write_sequentagent_slot` (agent_result.rs:897-963); `run_agent_blocking` calls it (parallel.rs:206); `rg "save_state" crates/devflow-cli/src/parallel.rs` shows only a pre-existing D-14 doc-comment reference, no new call; `sequentagent_slot_round_trips`/`sequentagent_slot_is_path_free`/`sequentagent_slot_write_creates_devflow_dir_and_gitignore` tests pass |
| 20 | `status` surfaces a distinct line identifying the running sequentagent slot, cross-referencing pid liveness | ✓ VERIFIED | `render_sequentagent_status` (commands.rs:1045-1085), called from `status()` (commands.rs:675-677); distinguishes `running`/`starting`/`not running`; `sequentagent_status_renders_running_slot`/`_renders_dead_pid_as_not_running`/`_renders_starting_when_pid_file_missing`/`_none_when_no_records` tests pass |
| 21 | Record cleared on EVERY sequentagent exit path (success + all five error-exits) via a `Drop` guard | ✓ VERIFIED | `SequentagentSlotGuard`/`impl Drop` (parallel.rs:290-299), bound once before agent A runs (parallel.rs:366-369) so it fires on every `return`/`?` in the function body; `slot_guard_clears_record_on_early_return`/`slot_guard_clears_record_on_success_path` tests pass — resolves the phase's second unanimous 3/3-review MUST-FIX |

**Score:** 21/21 truths verified (0 present-but-behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/staleness.rs — ancestry_range_affects_build` | 21-01 | ✓ VERIFIED | Exists, substantive, wired into `embedded_commit_is_stale`'s `Ok(Some(0))` arm, reuses `affects_compiled_binary` |
| `crates/devflow-cli/src/staleness.rs — docs_only_range_is_fresh, mixed_range_docs_and_source_is_stale, git_error_range_fails_toward_stale` | 21-01 | ✓ VERIFIED | All three tests exist and pass |
| `crates/devflow-cli/src/main.rs — GateCmd::Show` + dispatch | 21-02 | ✓ VERIFIED | Variant at main.rs:331-341, dispatch arm at main.rs:457-461 calling `gate_show` |
| `crates/devflow-cli/src/commands.rs — gate_show, render_gate_show` | 21-02 | ✓ VERIFIED | commands.rs:814-870, both wired and tested |
| `crates/devflow-cli/src/commands.rs — cron_hint_line, recovery_hints, latest_stage_launched_ts, render_stage_progress_line` | 21-02 | ✓ VERIFIED | All present, wired into `status()`, unit-tested |
| `crates/devflow-cli/src/commands.rs — PlanningDocFinding, parse_semver, parse_planning_doc_versions, tag_exists_and_reachable, reconcile_planning_docs` | 21-03 | ✓ VERIFIED | commands.rs:2108-2262, wired into `doctor()`/`doctor_json_body()`; live-run data-flow confirmed (Level 4) |
| `crates/devflow-core/src/agent_result.rs — SequentagentSlotKind, SequentagentSlot, sequentagent_slot_path, write_/read_/clear_sequentagent_slot` | 21-04 | ✓ VERIFIED | agent_result.rs:897-988, all `pub`, doc-commented |
| `crates/devflow-cli/src/parallel.rs — SequentagentSlotGuard, run_agent_blocking(slot)` | 21-04 | ✓ VERIFIED | parallel.rs:155-160 (typed param), 290-299 (guard), both call sites pass `SequentagentSlotKind::A`/`::B` |
| `crates/devflow-cli/src/commands.rs — render_sequentagent_status` | 21-04 | ✓ VERIFIED | commands.rs:1045-1085, wired into `status()` |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `embedded_commit_is_stale`'s `Ok(Some(0))` strict-ancestor arm | `ancestry_range_affects_build` → `affects_compiled_binary` | Direct call chain | ✓ WIRED | staleness.rs:57-61 calls the new helper; helper calls `affects_compiled_binary` at line 99 |
| `GateCmd::Show` | `gate_show` | main.rs dispatch arm | ✓ WIRED | main.rs:457-461 |
| `cron_instruction_hints` | `CronInstructions.retry_after` (ship.rs, already populated) | `cron_hint_line` | ✓ WIRED | commands.rs:1087-1115; no new detection logic |
| `doctor()`/`doctor_json_body()` | `collect_planning_doc_findings` → `reconcile_planning_docs` → `tag_exists_and_reachable` | third JSON key + text section | ✓ WIRED | commands.rs:2091 (`"planning_doc_staleness": render_planning_doc_findings_json(doc_findings)`); live-run confirmed |
| `sequentagent` (parallel.rs) | `run_agent_blocking(slot)` | `write_sequentagent_slot` after monitor spawn | ✓ WIRED | parallel.rs:200-208 |
| `status()` | `render_sequentagent_status` → `read_sequentagent_slot` + `agent_pid_from_file`/`agent_running` | direct call | ✓ WIRED | commands.rs:675-677, 1045-1085 |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|---------------------|--------|
| `doctor --json`'s `planning_doc_staleness` | `PlanningDocFinding` array | `collect_planning_doc_findings` reading real `.planning/ROADMAP.md`/`STATE.md` + live `git` tag lookups | Yes — live run against this repo's actual docs produced 4 real Warn findings (Phase 6/7 legacy version claims), matching the SUMMARY's independently-stated live-verification result exactly | ✓ FLOWING |
| `status()`'s stage-progress line | `latest_stage_launched_ts` | Real scan of `.devflow/events.jsonl` for `stage_launched` events | Yes — sourced from the actual event log, not a static/hardcoded value; unit-tested against a fixture proving it diverges correctly from `started_at` | ✓ FLOWING |
| `status()`'s sequentagent section | `render_sequentagent_status` | Real `.devflow/phase-*-sequentagent` file enumeration + `agent_pid_from_file`/`agent_running` OS process probe | Yes — reads actual filesystem records and OS pid liveness, not a static value | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| `doctor --json` produces real findings against this repo's live planning docs | `./target/debug/devflow doctor --json .` | Single JSON object, 3 top-level keys (`environment`, `planning_doc_staleness`, `reconciliation`); 4 Warn findings, 0 Problem findings | ✓ PASS |
| Named test: docs-only ancestry range is Fresh | `cargo test --workspace staleness::tests::docs_only_range_is_fresh` | 1 passed | ✓ PASS |
| Named test: mixed range still Stale | `cargo test --workspace staleness::tests::mixed_range_docs_and_source_is_stale` | 1 passed | ✓ PASS |
| Named test: git error fails toward Stale | `cargo test --workspace staleness::tests::git_error_range_fails_toward_stale` | 1 passed | ✓ PASS |
| Named test: slot guard clears on early return | `cargo test --workspace parallel::` | 9 passed (includes both guard tests) | ✓ PASS |
| Full workspace suite | `cargo test --workspace` | 535 passed, 0 failed (156+3+4+1+1+3+20+8+1+334+2+2 across all targets — matches the task's stated 535/0) | ✓ PASS |
| Lint/format gates | `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --check` | Both clean | ✓ PASS |

### Requirements Coverage

No REQUIREMENTS.md / REQ-IDs exist for this project. Coverage is mapped against `21-CONTEXT.md`'s decisions instead:

| "Requirement" (CONTEXT decision) | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| D-03 (21a) | 21-02 | Purely additive UX surfacing, no pipeline behavior change | ✓ SATISFIED | All 21a changes are read-only presentation; no `save_state`/`transition`/`Gates::respond` introduced into display paths (grep-confirmed) |
| D-04 (21b) | 21-03 | Detection-only; never auto-corrects prose | ✓ SATISFIED | No write path to ROADMAP.md/STATE.md in any new function |
| D-05 (21b) | 21-03 | Integrated into existing `doctor` path, single JSON object | ✓ SATISFIED | Third top-level key, live-verified |
| D-06 (21c) | 21-04 | Re-scoped to sequentagent's second agent only, not routed through State | ✓ SATISFIED | Standalone sibling record, no `save_state` call added |
| D-07 (21d) | 21-01 | Content-aware ancestry arm, reuses `affects_compiled_binary`, sequenced first | ✓ SATISFIED | Verified above; Wave 1, all downstream waves depend on it |
| D-01/D-02/D-08 (scope) | CONTEXT | 999.25/999.28 removed from phase; theme fixed; 21e stays excluded stretch | ✓ SATISFIED | No release-executor or `--base` code present in this phase's diffs; no `ChangelogAppend` content changes found |

No orphaned units — ROADMAP.md's Phase 21 unit list (21a-21d, plus optional 21e) maps 1:1 to the four executed plans; 21e was correctly left as excluded stretch (not attempted, not claimed).

### Anti-Patterns Found

None. Scanned all five modified files (`staleness.rs`, `main.rs`, `commands.rs`, `agent_result.rs`, `parallel.rs`) for `TBD|FIXME|XXX|TODO|HACK|PLACEHOLDER` (case-insensitive) — zero debt markers. The single incidental match ("em-dash placeholders" in a doc comment describing ROADMAP.md table syntax) is not a stub marker. No empty-return stubs, no hardcoded-empty stand-ins for the new functions — every new function's body was read directly and matches its documented behavior.

### Human Verification Required

None. Every must-have truth is either exercised by a passing named unit test (state-transition/cleanup-invariant truths: `docs_only_range_is_fresh`, `mixed_range_docs_and_source_is_stale`, `git_error_range_fails_toward_stale`, the `SequentagentSlotGuard` early-return test) or is a pure presentation/detection change confirmed both by unit test and a live run against this repo's real data (`doctor --json`). No visual, real-time-multi-process, or external-service behavior in this phase requires human judgment beyond what the live `doctor --json` run and the full green test suite already confirm.

### Gaps Summary

No gaps. All 21 observable truths across the four units (21a/21b/21c/21d) are verified against the actual codebase — not just SUMMARY claims. Full workspace suite is 535/0 (matches the task-stated baseline exactly), clippy and fmt are clean, and the doctor reconciliation check was independently run live against this repo's real `ROADMAP.md`/`STATE.md`/git tags, producing output that matches the SUMMARY's stated result byte-for-byte (4 Warn, 0 Problem). Both of the phase's unanimous 3/3 cross-AI-review MUST-FIXES (21-02's event-derived stage age, 21-04's Drop-guard cleanup on all exit paths) are confirmed implemented and test-covered, not just claimed. The phase goal — an operator surface that is legible and self-reported state that is trustworthy, with every unit single-writer/reversible-or-detection-only/testable-without-irreversible-side-effects — is achieved.

---

*Verified: 2026-07-23*
*Verifier: Claude (gsd-verifier)*
