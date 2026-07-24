---
phase: 12-bootstrap-housekeeping
verified: 2026-07-11T01:29:49Z
status: passed
score: 35/35 must-haves verified
behavior_unverified: 0
overrides_applied: 0
---

# Phase 12: Bootstrap + Housekeeping Verification Report

**Phase Goal:** Pay down the Phase 11 code-review debt (WR-01…WR-10, IN-02…IN-05), close the untested orchestration-core paths and never-run manual verifications, harden versioning (WR-04 + version-consistency to 1.2.0), and get the crates publish-ready (metadata + dry-run, NO publish). Bootstrap (12a new-project/map-codebase) is DEFERRED to its own future phase.
**Verified:** 2026-07-11T01:29:49Z
**Status:** passed
**Re-verification:** Yes — CR-01's LoopBack path was manually verified end-to-end after initial verification flagged it as behavior-unverified (see truth #35 and Human Verification below)

## Goal Achievement

This phase has no formal REQUIREMENTS.md; must-haves were sourced from each of the 12 PLAN.md `must_haves.truths` blocks (34 truths) plus one truth added for the post-wave code-review fix (CR-01), per the verification brief. Every truth below was checked against the actual codebase (source reading, `cargo build/test/clippy/fmt`, targeted single-test runs, and `cargo publish --dry-run`/`cargo package`) — not against SUMMARY.md's narrative alone.

### Observable Truths

| # | Plan | Truth | Status | Evidence |
|---|------|-------|--------|----------|
| 1 | 12-01 | A kill mid-write of state.json can never leave an empty/partial file that breaks load_state | ✓ VERIFIED | `workflow.rs::write_state_atomic` writes to `path.with_extension("tmp")` then `std::fs::rename`s over the target; no in-place `fs::write` to `state.json` remains |
| 2 | 12-01 | save_state persists via temp-write-then-rename | ✓ VERIFIED | Same as above; test `save_state_writes_atomically_and_leaves_no_temp` passes (`cargo test --workspace`) |
| 3 | 12-02 | An unparseable rate-limit reason never produces an every-minute cron | ✓ VERIFIED | `cron_schedule_from_retry_after` returns `Option<String>` (None on unparseable); test `cron_instructions_reject_unparseable_retry_time` passes |
| 4 | 12-02 | retry_after_from_reason returns "unknown" sentinel instead of leaking raw reason | ✓ VERIFIED | `.or(reason)` fallback removed; test `retry_after_from_reason_strips_prefix` asserts `"unknown"`, passes |
| 5 | 12-02 | devflow test runs the idiomatic `cargo fmt --check` | ✓ VERIFIED | `test_cmd`'s checks array: `("cargo fmt --check", "cargo fmt --check")` |
| 6 | 12-03 | Agent program+args handed to monitor as separate argv, never shell-interpolated | ✓ VERIFIED | `Command::new("sh").arg("-c").arg(&script).arg("sh").arg(program).args(args)`; script references `"$@"`; `shell_escape` only applied to internal paths, not `program`/`args` |
| 7 | 12-03 | Monitor still launches agent, captures pid/stdout/stderr/exit, runs `devflow advance` | ✓ VERIFIED | `spawn_monitor_captures_agent_pid_and_output`, `spawn_monitor_treats_agent_args_as_literal_argv`, and `monitor_owns_fake_agent_and_records_devflow_result` all pass |
| 8 | 12-04 | PID check uses libc::pid_t with a documented truncation assumption | ✓ VERIFIED | `agent.rs:67`: `libc::kill(pid as libc::pid_t, 0)` with a `///` doc comment stating the assumption |
| 9 | 12-04 | BranchCleanup distinguishes "not merged yet" from a genuine git error | ✓ VERIFIED | `hooks.rs::branch_cleanup` checks for `"not fully merged"`/`"not yet merged"` substrings and emits a distinguishable warn message vs. the generic git-error warn |
| 10 | 12-04 | A test confirms Validate→Ship fires DocsUpdate+ChangelogAppend and CHANGELOG gets a versioned entry | ✓ VERIFIED | `validate_to_ship_hooks_append_changelog` runs the real hook set against a temp repo, asserts `CHANGELOG.md` contains `"## "`; passes |
| 11 | 12-05 | TOML section tracker handles `[[array-of-tables]]` and doesn't misread inline-table values | ✓ VERIFIED | `parse_section_header` strips double brackets for `[[...]]`; `find/replace_version_in_contents` skip lines whose value starts with `{`; test `inline_table_version_does_not_shadow_workspace_package` passes |
| 12 | 12-05 | write_version correctly targets workspace.package.version in a workspace Cargo.toml (tested) | ✓ VERIFIED | `write_version_replaces_in_workspace_cargo_toml` passes |
| 13 | 12-05 | Module docs no longer assert v2.0.0 as current; devflow doctor reports 1.2.0 | ✓ VERIFIED | No `v2.0.0`-as-current-version doc claims remain in `crates/devflow-core/src/*.rs` (only unrelated match is a git tag string `"v2.0.0"` in a version.rs test fixture); `./target/debug/devflow doctor` prints `devflow v1.2.0 ... 1.2.0 ✓` |
| 14 | 12-06 | Both crates carry complete crates.io metadata (description/license/repository/readme/keywords/categories) | ✓ VERIFIED | Root `Cargo.toml [workspace.package]` has all six fields; both crate `[package]` blocks inherit via `.workspace = true` |
| 15 | 12-06 | devflow bin's path dep on devflow-core carries a version | ✓ VERIFIED | `crates/devflow-cli/Cargo.toml`: `devflow-core = { path = "../devflow-core", version = "1.2.0" }` |
| 16 | 12-06 | `cargo publish --dry-run` for devflow-core exits 0; `cargo package` produces .crate artifacts | ✓ VERIFIED | Re-ran independently: `cargo publish --dry-run -p devflow-core` exits 0 (packages, verifies, compiles, "aborting upload due to dry run"); `cargo package --workspace` produced `target/package/devflow-core-1.2.0.crate` and `devflow-1.2.0.crate` |
| 17 | 12-06 | No cargo publish run; no publish task exists | ✓ VERIFIED | `rg -n "cargo publish"` across the repo (excluding target/) finds zero script/CI hits; only doc references to the decision |
| 18 | 12-07 | Dead v1 config test helpers gone; explicit test asserts devflow ignores stray .devflow.yaml | ✓ VERIFIED | `write_config`/`write_last_ship` absent from `phase7_cli.rs`; `devflow_ignores_stray_devflow_yaml` test present and passes |
| 19 | 12-07 | parse_marker_lines documents ASCII-marker assumption; test proves LAST marker wins | ✓ VERIFIED | Doc comment present above `parse_marker_lines`; `parse_marker_lines_returns_last_marker_in_long_output` passes |
| 20 | 12-08 | poll_response returns immediately at the real 7-day timeout when a response exists | ✓ VERIFIED | `poll_response_returns_immediately_at_full_timeout` (uses `SEVEN_DAYS` const, asserts `elapsed() < 5s`) passes |
| 21 | 12-08 | list_feature_branches ahead/behind semantics correct | ✓ VERIFIED | `list_feature_branches_reports_ahead_and_behind_semantics` asserts `ahead==2, behind==1`; `ahead = rev_count("{develop}..{name}")` (branch-only commits), `behind = rev_count("{name}..{develop}")` (develop-only commits) — matches `divergence_from_develop` convention; label swap was fixed per SUMMARY and confirmed in current code |
| 22 | 12-08 | Monitor/advance path covered for missing/corrupt state | ✓ VERIFIED | `advance_state_loading_fails_cleanly_for_missing_and_corrupt_state` in `monitor_e2e.rs` asserts `MissingState` and `Json` error variants; passes |
| 23 | 12-09 | advance() over Ship-stage success + approved gate runs terminal finish flow | ✓ VERIFIED | `advance_ship_success_runs_finish_workflow` passes |
| 24 | 12-09 | MAX_CONSECUTIVE_FAILURES forces Validate gate; "abort" response clears state, no spawn | ✓ VERIFIED | `validate_failure_threshold_forces_gate_then_aborts` passes (this test was further strengthened by the CR-01 fix to also assert gate-file cleanup) |
| 25 | 12-10 | parse_rfc3339ish second-restoration documented as timezone-safe | ✓ VERIFIED | Comment present explaining whole-minute normalization + second-invariance |
| 26 | 12-10 | shell_quote safe-unquoted set widened (reduces over-quoting), unsafe input still quoted | ✓ VERIFIED | Safe set now includes `~ : @ + = %`; `shell_quote_leaves_common_safe_chars_unquoted` and `shell_quote_quotes_unsafe_input` both pass. **Note:** the post-wave code review (12-REVIEW.md, WR-08-review) found a real latent edge case — an unquoted leading `~` triggers POSIX tilde-expansion — not exploitable at the current call site (always an absolute path) but a trap for a future caller. Not fixed as part of this phase; documented here as a known, non-blocking residual issue. |
| 27 | 12-10 | Test proves parse_rfc3339ish handles negative UTC offsets | ✓ VERIFIED | `cron_schedule_normalizes_negative_offset` passes |
| 28 | 12-11 | State struct no longer carries agent_result/agent_stdout_path | ✓ VERIFIED | `rg` for those field names in `state.rs` returns nothing |
| 29 | 12-11 | Enum is AgentKind directly (alias deleted); adapter trait is AgentAdapter | ✓ VERIFIED | `enum AgentKind` in `state.rs`; `trait AgentAdapter` in `agents/mod.rs`; zero remaining `pub type AgentKind = Agent` or bare `Agent` symbol |
| 30 | 12-11 | Whole workspace builds/tests/clippy/fmt clean after rename | ✓ VERIFIED | `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` all clean; `cargo test --workspace` = 174/174 passing |
| 31 | 12-12 | Live Hermes gate round-trip executed by a human, observed end-to-end | ✓ VERIFIED (human-attested, cross-checked) | SUMMARY's described mechanics (gate request/response/ack file paths, `Gates::poll_response` exponential backoff, state.json stage flip) match the real `gates.rs`/`workflow.rs` implementation exactly; the underlying machinery genuinely exists and behaves as described. The live-Hermes-session and real-API-call portions are inherently human-attested and cannot be independently re-run by this verifier — accepted on the checkpoint's already-recorded "approved" outcome. |
| 32 | 12-12 | Real credentialed agent CLI launched through devflow, observed | ✓ VERIFIED (human-attested, cross-checked) | `devflow doctor` reporting `1.2.0` matches independently-confirmed output (truth #13); the marker-parsing/advance mechanics described (`evaluate_agent_result`, JSON envelope unwrap, `DEVFLOW_RESULT` marker) match `agent_result.rs`'s real code. Real-API-call details ($0.1790, 1 turn) are human-attested and not independently re-verifiable. |
| 33 | 12-12 | DocsUpdate fail-soft skip observed visible to user | ✓ VERIFIED | The exact WARN text quoted in the SUMMARY (`"DocsUpdate: cargo doc reported a failure; skipping commit"`) matches `hooks.rs::docs_update`'s `Ok(_) => warn!(...)` arm verbatim |
| 34 | 12-12 | Full-Ship-workflow verification explicitly recorded as BLOCKED, not silently skipped | ✓ VERIFIED | 12-12-SUMMARY.md and 12-12-PLAN.md both explicitly record this item as BLOCKED on the out-of-scope `ship.rs` rewrite, with no verification task created and no PASS claimed |
| 35 | CR-01 (post-wave review fix) | The stale-gate-file-reuse defect is fixed and verified for both the code paths the review flagged (loop_back_to_code AND abort) | ✓ VERIFIED | Fix is real and correctly applied to **both** call sites (confirmed by reading `crates/devflow-cli/src/main.rs` against commit `5ca77d6`'s diff). The **Abort** path is proven by a passing committed regression test (`abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response`, ~1.00s elapsed — proves it didn't short-circuit). The **LoopBack** path has no *committed* automated test (deliberately — see rationale below), but was manually verified end-to-end post-verification: a scratch repo drove `devflow advance` through a forced Validate gate pre-seeded with a stale non-abort rejection (`{"approved": false, "note": "needs more work"}`), confirmed `GateAction::LoopBack` fired, `loop_back_to_code` ran, and `.devflow/gates/42-validate.{json,response.json,ack.json}` were all gone afterward with `state.json` correctly showing `stage: "code"`. Ran against the real compiled binary with a stubbed `claude` in PATH (to avoid spawning a real agent) — not simulated. See Human Verification below for why this wasn't turned into a committed test. |

**Score:** 35/35 truths verified

### Required Artifacts

All artifacts declared across the 12 plans' `must_haves.artifacts` blocks were checked at exists/substantive/wired levels; all passed (see truths table above for the corresponding evidence — each artifact's provided capability is what the truth checks). No stub, orphaned, or hollow artifacts were found. `cargo build --workspace`, `cargo test --workspace` (174 tests, 0 failures), `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` are all clean as of this verification.

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `save_state` | `.devflow/state.json` | temp-write + rename | ✓ WIRED | Confirmed in `workflow.rs` |
| `spawn_monitor` | detached monitor process | program+args as `"$@"` positional params | ✓ WIRED | Confirmed in `monitor.rs`; test proves literal argv delivery |
| `sequentagent rate-limit branch` | `ship::build_cron_instructions` | `retry_after` string flow | ✓ WIRED | `cron_schedule_from_retry_after` returns `Option`, propagated through `write_rate_limit_cron` |
| `hooks_for_transition(Validate, Ship)` | `changelog_append` side effect | hook run against temp repo | ✓ WIRED | Test asserts real `CHANGELOG.md` mutation |
| `field_for → workspace.package.version` | `replace_version_in_contents` | section tracker | ✓ WIRED | Array-of-tables/inline-table guards confirmed |
| `crates/devflow-cli/Cargo.toml` | `devflow-core` | versioned path dependency | ✓ WIRED | Confirmed; `cargo package --workspace` resolves it via temp local registry |
| `devflow (any command)` | `.devflow.yaml` | non-regression ignore test | ✓ WIRED | Test passes |
| `Gates::poll_response(deadline=7d, response present)` | immediate `Some(response)` | read-file-before-deadline-check | ✓ WIRED | Test passes, elapsed < 5s |
| `handle_validate_outcome` at MAX | `run_gate → GateAction::Abort → abort` | seeded "abort" note response | ✓ WIRED | Test passes |
| `.devflow/gates/*.json` | human response file → devflow ack | live Hermes round-trip | ✓ WIRED (human-attested) | Mechanics match code; live session portion not independently re-verifiable |
| `agents::adapter_for` | `Box<dyn AgentAdapter>` | trait renamed | ✓ WIRED | Confirmed, `cargo build --workspace` clean |
| gate response file (LoopBack case) | `loop_back_to_code`'s `Gates::cleanup` call | same pattern as `abort()`'s cleanup call | ✓ WIRED | Manually verified end-to-end against the real binary (see truth #35); not covered by a committed automated test (rationale below) |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| `devflow doctor` reports 1.2.0 and detects installed agent CLIs | `./target/debug/devflow doctor` | printed `devflow v1.2.0 ... 1.2.0 ✓`, detected claude/codex/opencode | ✓ PASS |
| Full workspace test suite | `cargo test --workspace` | 12+6+154+2 = 174 passed, 0 failed | ✓ PASS |
| `cargo publish --dry-run -p devflow-core` | exits 0 | packaged, verified, compiled, "aborting upload due to dry run" | ✓ PASS |
| `cargo package --workspace` produces both `.crate` artifacts | re-ran independently | `devflow-core-1.2.0.crate` (55.6KB) and `devflow-1.2.0.crate` (25.8KB) produced | ✓ PASS |
| CR-01 abort-path regression test | `cargo test --package devflow --bin devflow tests::abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response -- --exact` | 1 passed, elapsed ≈1.00s (proves it actually waited on `poll_response`, didn't short-circuit) | ✓ PASS |
| CR-01 LoopBack-path equivalent | manual scratch-repo run against `target/debug/devflow advance`, stubbed `claude` in PATH | forced Validate gate consumed a pre-seeded non-abort rejection, resolved to `LoopBack`, and all three gate files for that stage were gone afterward; `state.json` showed `stage: "code"` | ✓ PASS (manual, not committed as an automated test — see rationale below) |

### Anti-Patterns Found

No `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` markers found in any of the ~24 files this phase modified (grepped every file listed across all 12 plans' `files_modified` frontmatter). `git status --short` on `crates/` and `Cargo.toml` shows a clean working tree — all changes are committed.

The phase's own post-wave code review (`12-REVIEW.md`) surfaced additional **pre-existing** findings not caused by Phase 12 but living in files it touched, none of which were required to be fixed by any of the 12 plans and none of which are part of this verification's must-have scope. Listed here for visibility, not counted as gaps:
- **WR-01-review** (`main.rs::cmd_check`): reports "ok" for a command that spawned but exited non-zero — confirmed still present.
- **WR-08-review** (`ship.rs::shell_quote`): a leading `~` in the widened safe set triggers shell tilde-expansion — confirmed still present, not exploitable at the current call site.
- **WR-04-review** (`hooks.rs::docs_update`): commits via `git add .`, sweeping in unrelated dirty files — confirmed still present.
- **IN-review-1/2**: locale-dependent branch-cleanup message matching; duplicated `write_atomic` helper — both are documented non-defects/low-priority notes in the review itself.

### Requirements Coverage

This project has no `.planning/REQUIREMENTS.md`. Using CONTEXT.md's WR-/IN-/12x list as the source of truth: **all 18 listed requirement IDs** (WR-01 through WR-10, IN-02 through IN-05, 12b, 12c, 12f, 12g) are claimed by at least one of the 12 plans' `requirements` frontmatter and are backed by verified truths above. No orphaned requirements found — every ID in CONTEXT.md's debt list maps to a plan and a passing verification. 12a (new-project/map-codebase) is explicitly, correctly deferred per CONTEXT.md's "Planning-Time Decisions" and ROADMAP.md's phase goal — not a gap.

### Human Verification — Resolved

### 1. CR-01 fix coverage for the LoopBack path — RESOLVED 2026-07-11

**Original gap:** the code fix for `loop_back_to_code()`'s `Gates::cleanup` call was present and structurally identical to the tested `abort()` sibling, but no automated test in the repository exercised `GateAction::LoopBack` at all.

**Resolution:** Manually verified end-to-end against the real compiled `target/debug/devflow` binary in a scratch git repo. Seeded a Validate gate response `{"approved": false, "note": "needs more work"}` (no "abort" keyword → resolves to `LoopBack`, not `Abort`), set `consecutive_failures` to the gating threshold, and ran `devflow advance`. Confirmed: the gate fired, read the stale response, resolved to `LoopBack`, `loop_back_to_code` ran, and `.devflow/gates/42-validate.{json,response.json,ack.json}` were all gone afterward with `state.json` showing `stage: "code"`.

**Deliberately NOT added as a committed automated test:** `loop_back_to_code()` unconditionally calls `launch_stage()` afterward, which spawns the configured agent CLI (resolved via PATH) with `--dangerously-skip-permissions`. No existing test in this codebase calls `loop_back_to_code` or `launch_stage` for exactly this reason — doing so in a committed test would spawn a real, credentialed agent process as a side effect of running `cargo test` on any developer machine that has `claude`/`codex`/`opencode` on PATH. This is the same reason `abort()` (which does NOT call `launch_stage`) was the only path that got a committed test in commit `5ca77d6`. A safe committed test would require a launcher seam (making the spawned program injectable) — already flagged as out of scope by 12-09's SUMMARY.

**Decision:** accepted as verified on this manual run plus code-review confidence (identical `Gates::cleanup` call construction to the tested Abort path); the missing committed-test seam is tracked as a residual note, not a phase blocker.

### Gaps Summary

No gaps found. All 12 original PLAN.md files' must-haves, plus the post-wave CR-01 fix (both call sites), are genuinely, verifiably implemented and behaviorally confirmed — checked by reading the actual source (not trusting SUMMARY.md prose), re-running `cargo build/clippy/fmt/test` from scratch, independently re-executing `cargo publish --dry-run` and `cargo package`, running the committed CR-01 Abort-path regression test, and manually driving the CR-01 LoopBack-path fix through the real compiled binary.

---

### Re-affirmation — 2026-07-23

`gsd-tools verification.status` flags this report `stale` because `12-09` through `12-12-SUMMARY.md` carry a later git commit time (`b4f9d3f9`, "mark phase 12 complete, config hygiene, 11/11r closeout") than this file. That commit only *added* those SUMMARY.md files (121 new lines, no other changes) — belated documentation of plans already covered by the 35/35 truths above, not new implementation invalidating them. Re-affirmed as `passed`; no re-verification performed or needed.

---

_Verified: 2026-07-11T01:29:49Z_
_Verifier: Claude (gsd-verifier)_
