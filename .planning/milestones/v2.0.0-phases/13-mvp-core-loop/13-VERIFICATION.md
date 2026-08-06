---
phase: 13-mvp-core-loop
verified: 2026-07-15T00:00:00Z
status: passed
score: 24/24 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification: No — initial verification
---

# Phase 13: MVP Core Loop Verification Report

**Phase Goal:** Get the basic AI development loop (Define→Plan→Code→Validate→Ship) working
end-to-end so DevFlow is usable on real projects again — `ship.rs` GSD-native rewrite (13a),
completion-protocol correctness: verdict-vs-ran + native Claude/Codex envelope parsing (13b),
never-silent failures: WR-11 + gate notify hook + configurable timeout (13c),
worktree-by-default (13d), and a real dogfood run as the acceptance test (13e).
**Verified:** 2026-07-15
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | ship.rs no longer carries dead v1 `LastShip`/PR-body/test-summary bookkeeping | ✓ VERIFIED | `rg` for `struct LastShip\|fn build_pr_body\|fn extract_goal\|fn mark_phase_complete\|fn count_passed_tests` in `ship.rs` returns 0 hits |
| 2 | `ShipError::Missing` and cron/changelog/shell_quote survivors remain and compile | ✓ VERIFIED | `ship.rs:60` `Missing` variant present, still returned by `load_cron_instructions`; `cargo build --workspace` exits 0 |
| 3 | Ship-stage prompt sequences `/gsd-code-review {N}` before `/gsd-ship {N}` and mandates no-ship-on-Critical | ✓ VERIFIED | `prompt.rs:41-67` — doc comment + `ship_stage_prompt` body; `cargo test -p devflow-core prompt` green |
| 4 | Ship `AgentFailed` gates+notifies; `ReviewFailed` (`review:` prefix) loops back to Code with `/gsd-audit-fix` | ✓ VERIFIED | `main.rs:613-631` `handle_ship_failure`/`is_ship_review_failure` (`trim().to_ascii_lowercase().starts_with("review:")`); tests `ship_agent_failed_fires_gate`, `ship_review_failed_loops_to_code`, `ship_review_failed_uses_audit_fix` all pass individually |
| 5 | Every non-Validate stage failure (Define/Plan/Code/Ship-AgentFailed) writes a gate + fires notify — WR-11 closed | ✓ VERIFIED | `main.rs:472-482` catch-all `_ => handle_stage_failure(...)` (no more bare `CliError`); test `non_validate_failure_fires_gate_and_hook` passes |
| 6 | Notify hook is fail-soft and shell-injection-safe (context passed via env, never interpolated) | ✓ VERIFIED | `gates.rs:199-235` `fire_gate_notify`/`run_notify_command` — `.env("DEVFLOW_GATE_CONTEXT", context)`, `Command::new("sh").arg("-c").arg(cmd)` with no string interpolation of context; tests `notify_hook_runs_configured_command`, `notify_hook_failure_is_fail_soft`, `notify_hook_sets_non_silent_flag` pass |
| 7 | Gate poll timeout is configurable via `DEVFLOW_GATE_TIMEOUT_SECS`, default 7 days | ✓ VERIFIED | `main.rs:22-33` `parse_gate_timeout`/`gate_timeout_secs`; test `parse_gate_timeout_env_override` passes |
| 8 | Stale gate/response/ack cleaned up before an Advance/LoopBack retry (CR-01 closed) | ✓ VERIFIED | `main.rs:590,599` `Gates::cleanup(...)` inside `handle_stage_failure`; test `stage_failure_retry_cleans_stale_response` passes |
| 9 | A gate the active mode would not normally fire is marked unexpected (`DEVFLOW_NON_SILENT_GATE=1`) and logged | ✓ VERIFIED | `main.rs:729-736` computes `unexpected = !state.mode.should_gate(...)`, calls `fire_gate_notify(..., unexpected)` |
| 10 | Claude envelope `is_error: true` is an authoritative Layer-1 failure, overriding a same-envelope success marker | ✓ VERIFIED | `agent_result.rs:257-280` `detect_claude_envelope_failure`; tests `claude_envelope_is_error_detected`, `claude_is_error_overrides_success_marker` pass |
| 11 | Codex `--json` JSONL stream parsed: `turn.failed` decisive, `turn.completed` w/o marker defers (no unconditional Success) | ✓ VERIFIED | `agent_result.rs:298-360` `is_codex_event_stream`/`parse_codex_event_result`, reads `agent_message` items (dogfood fix B2, commit `27033bc`); test `codex_agent_message_marker_failed_wins_over_bare_turn_completed` passes |
| 12 | Layer 2's `exit=0/commits=0` gate is scoped to `Stage::Plan\|Stage::Code`; `exit≠0` remains Failed for every stage | ✓ VERIFIED | `agent_result.rs:504` `matches!(stage, Stage::Plan \| Stage::Code)`; tests `layer2_skips_commit_gate_for_define_and_validate`, `layer2_nonzero_exit_is_failed_all_stages` pass |
| 13 | A distinct `Verdict` field (`pass`/`gaps`) exists on `AgentResult`, parsed leniently (malformed → None, never a parse error) | ✓ VERIFIED | `agent_result.rs` `Verdict` enum + `deserialize_verdict_lenient`; `rg -c "verdict: None" main.rs` ≥ 1 (run_agent_blocking literal updated); `cargo build --workspace` exits 0 |
| 14 | Validate advances to Ship ONLY on explicit `verdict: pass`; `gaps` or missing verdict gates/loops (fail-safe) | ✓ VERIFIED | `main.rs:490-497` `let passed = matches!(result.verdict, Some(Verdict::Pass));`; tests `validate_gaps_does_not_advance_to_ship`, `validate_missing_verdict_does_not_advance`, `validate_pass_advances` all pass individually |
| 15 | `devflow start` with no flag creates an isolated worktree by default; `state.worktree_path: Some(_)` | ✓ VERIFIED | `main.rs:60-67,229` `no_worktree: bool` opt-out, `let worktree = !no_worktree;`; integration test `start_defaults_to_worktree` passes |
| 16 | `--no-worktree` opt-out runs in the primary checkout; `state.worktree_path: None` | ✓ VERIFIED | Integration test `start_no_worktree_uses_feature_branch` passes |
| 17 | The old `--worktree` flag survives as a hidden, deprecated no-op alias | ✓ VERIFIED | `main.rs:62` `#[arg(long, hide = true)] worktree: bool`, destructured as `worktree: _worktree` (ignored); test `reference_and_cleanup_worktree_cli_flow` still green |
| 18 | `parallel`/`sequentagent` unaffected by the default flip | ✓ VERIFIED | Source assertion (unchanged `start(..., true, false)` call site) confirmed in 13-04-SUMMARY and unchanged by later commits; full workspace suite green |
| 19 | Full workspace (`cargo test`, `clippy -D warnings`, `fmt --check`) is green after all six plans | ✓ VERIFIED | Directly re-run: `cargo test --workspace` → 23+9+175+2 = 209 tests pass, 0 failures; `cargo clippy --workspace -- -D warnings` exit 0; `cargo fmt --check` exit 0 |
| 20 | Full Define→Plan→Code→Validate→Ship loop ran end-to-end on a real external project (Claude adapter), gates answered via notify hook | ✓ VERIFIED (human) | 13-06-SUMMARY.md Task 2: real run on `denniyahh/devflow-dogfood`, worktree created, Validate advanced only on `verdict: pass`, notify hook delivered a desktop notification for the forced pre-flight failure — operator-confirmed live evidence, per task framing this is accepted as human-verified |
| 21 | The 12-12 BLOCKED Full-Ship verification is re-run and passes (real PR opened headless, no interactive stall) | ✓ VERIFIED (human) | 13-06-SUMMARY.md: PR `https://github.com/denniyahh/devflow-dogfood/pull/1` opened by `/gsd-ship`, ReviewFailed path exercised on a real Critical finding (CR-01), looped back to Code, re-shipped clean — operator-confirmed |
| 22 | Codex leg exercised through Code→Validate; envelope parser reconciled against real `--json` output | ✓ VERIFIED (human) | 13-06-SUMMARY.md Task 3: verbatim JSONL capture shows `agent_message`-embedded `DEVFLOW_RESULT` and `turn.completed`-only terminal events; the Plan 03 parser's initial gap was found live and fixed (commit `27033bc`, verified in code above) |
| 23 | At least one deliberately-forced non-Validate/Ship failure proves gate+notify fires (WR-11 dogfood evidence, mandatory) | ✓ VERIFIED (human) | 13-06-SUMMARY.md Task 1: fake failing `claude` on PATH → gate `01-define.json` written with `[never-silent]` context → `fire_gate_notify` ran the operator's `notify-send -u critical` → desktop notification observed. Task 2 additionally captured an *unforced* real Ship AgentFailed (Claude session-limit kill) as stronger evidence |
| 24 | Any dogfood failure follows a defined remediation loop (capture → patch → re-test → resume) | ✓ VERIFIED | 6 dogfood-driven fixes exist as real commits with matching diffs: `09e2803` (git tag signing), `27033bc` (Codex agent_message parsing + rate-limit false-match + reason cap), `09f96ff` (idempotent Define/Plan prompts), `f3951bf` (lock reclaim), `6403c6a` (Codex sandbox worktree/signing) — each independently confirmed against the current source (see Anti-Patterns / code inspection below) |

**Score:** 24/24 truths verified (0 present-but-behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-core/src/gates.rs` | `fire_gate_notify` + `run_notify_command`, env-based, fail-soft | ✓ VERIFIED | Present, wired into `main.rs::run_gate`, 4 unit tests pass |
| `crates/devflow-cli/src/main.rs` | `handle_stage_failure`, `handle_ship_failure`, `gate_timeout_secs`, worktree flags, verdict-aware Validate arm | ✓ VERIFIED | All present and wired; 23 unit tests + 9 integration tests pass |
| `crates/devflow-core/src/ship.rs` | Dead v1 bookkeeping removed; cron/changelog survivors intact | ✓ VERIFIED | 789→446 lines per SUMMARY; grep-confirmed; `cargo test -p devflow-core ship::` green |
| `crates/devflow-core/src/prompt.rs` | `ship_stage_prompt` + `validate_stage_prompt` special cases | ✓ VERIFIED | Both present; prompt tests green |
| `crates/devflow-core/src/agent_result.rs` | `detect_claude_envelope_failure`, `parse_codex_event_result`, `Verdict` enum, stage-scoped Layer 2 | ✓ VERIFIED | All present, wired into `evaluate_layer1`/`evaluate_agent_result`; 175 devflow-core unit tests green |
| `crates/devflow-core/src/lock.rs` | Dead-holder reclaim in `acquire()` | ✓ VERIFIED | `lock.rs:26-56` liveness-checks recorded PID via `kill -0` and reclaims |
| `crates/devflow-core/src/git.rs` | Non-interactive `tag()` under `tag.gpgsign=true` | ✓ VERIFIED | `git.rs:99-122` scopes `-c tag.gpgSign=false` per invocation |
| `crates/devflow-core/src/agents/{mod,codex}.rs`, `agent.rs`, `monitor.rs` | Codex sandbox worktree-admin-dir grant + `extra_env` gpgsign scoping | ✓ VERIFIED | Commit `6403c6a` diff matches current source (`worktree_writable_roots`, `AgentAdapter::extra_env`) |
| `.planning/phases/13-mvp-core-loop/13-06-SUMMARY.md` | Dogfood evidence (commands, gate/notify round-trip, PR link, verbatim Codex JSONL, failure classification) | ✓ VERIFIED | Present, contains all required sections; six referenced fix commits (`09e2803`, `27033bc`, `09f96ff`, `f3951bf`, `6403c6a`) exist in `git log` with diffs matching the SUMMARY's description |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `advance()` failure arm | `handle_stage_failure`/`handle_ship_failure` | `main.rs:472-482` match on `stage` | ✓ WIRED | Catch-all no longer bare-errors |
| `handle_stage_failure`/`run_gate` | `gates::fire_gate_notify` | `main.rs:736` | ✓ WIRED | Called unconditionally after every gate write |
| `handle_stage_failure` Advance/LoopBack | `Gates::cleanup` | `main.rs:590,599` | ✓ WIRED | Runs before any re-launch |
| `handle_ship_failure` `review:` path | `loop_back_to_code(FixType::AuditFix)` | `main.rs:619` | ✓ WIRED | Matches `/gsd-audit-fix` prompt |
| `advance()` Validate success arm | `Verdict::Pass` gate | `main.rs:496-497` | ✓ WIRED | `matches!(result.verdict, Some(Verdict::Pass))` |
| `Command::Start` | `start(worktree: !no_worktree)` | `main.rs:229-230` | ✓ WIRED | Confirmed by both integration tests |
| Ship prompt `review:` contract | `handle_ship_failure`/`is_ship_review_failure` | shared string convention | ✓ WIRED | Both sides use identical `trim + lowercase + starts_with("review:")` semantics |
| Codex `agent_message` items | `parse_codex_event_result` | `agent_result.rs:326-360` | ✓ WIRED | Dogfood-reconciled (commit `27033bc`); regression test present |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full workspace test suite | `cargo test --workspace` | 23+9+175+2 = 209 passed, 0 failed | ✓ PASS |
| Clippy clean | `cargo clippy --workspace -- -D warnings` | exit 0 | ✓ PASS |
| Format clean | `cargo fmt --check` | exit 0 | ✓ PASS |
| Named regression tests (individually) | `cargo test -p devflow <name>` / `cargo test -p devflow-core <name>` for all 17 tests named in 13-01/03/04/05 SUMMARYs | all `ok` | ✓ PASS |
| No debt markers introduced | `rg "TODO\|HACK\|PLACEHOLDER\|TBD\|FIXME\|XXX"` across all phase-touched files | no matches | ✓ PASS |
| Six dogfood-fix commits exist with matching diffs | `git show --stat <hash>` for `09e2803,27033bc,09f96ff,f3951bf,6403c6a` | all present, diffs match SUMMARY narrative | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| 13a | 01, 02 | Ship stage GSD-native rewrite | ✓ SATISFIED | Dead bookkeeping removed, AgentFailed/ReviewFailed split, headless-safe prompt |
| 13b | 03, 05 | Completion-protocol correctness (verdict-vs-ran + native envelope parsing) | ✓ SATISFIED | Verdict enum + gating; Claude is_error; Codex JSONL (reconciled live) |
| 13c | 01 | Never-silent loop (WR-11 + notify hook + configurable timeout) | ✓ SATISFIED | Gate+notify on every non-Validate failure; fail-soft hook; env timeout |
| WR-11 | 01 | Silent-halt catch-all bug | ✓ SATISFIED | Catch-all arm routes through `handle_stage_failure` |
| 13d | 04 | Worktree-by-default | ✓ SATISFIED | Default flipped; `--no-worktree` opt-out; hidden deprecated alias |
| 13e | 06 | Dogfood acceptance run | ✓ SATISFIED (human-verified) | Real Claude full-loop + Full-Ship PR + Codex leg + forced-failure gate/notify evidence recorded in 13-06-SUMMARY.md |

No orphaned requirements — every ID in `CONTEXT.md`/`ROADMAP.md` (`13a`–`13e`, `WR-11`) is claimed by exactly one plan's `requirements:` frontmatter and cross-referenced above.

### Anti-Patterns Found

None. Scanned every file touched by the phase (`gates.rs`, `main.rs`, `ship.rs`, `prompt.rs`, `agent_result.rs`, `git.rs`, `lock.rs`, `agents/mod.rs`, `agents/codex.rs`, `agent.rs`, `monitor.rs`, `phase7_cli.rs`) for `TODO|HACK|PLACEHOLDER|TBD|FIXME|XXX` — zero matches.

### Human Verification Required

None outstanding. The phase's one inherently-manual truth set (13e dogfood acceptance) was already executed and recorded by the operator in `13-06-SUMMARY.md` as a `checkpoint:human-verify` task with explicit resume-signal approval at each of the three checkpoints (pre-flight, Claude full-loop, Codex leg). Per the verification brief, these operator-confirmed items are treated as human-verified evidence rather than re-opened as pending items. All code-level claims arising from that dogfood run (six fix commits) were independently re-verified against the current source above.

### Gaps Summary

No gaps. All 24 merged must-have truths (roadmap Success Criteria + PLAN frontmatter truths across 13-01 through 13-06) verified either directly against the codebase (18 truths: code presence + wiring + passing named tests + full green workspace) or via already-completed human-in-the-loop dogfood evidence (6 truths, cross-checked against the six real fix commits it produced). Two informational follow-ups are explicitly deferred (not gaps): (1) one unreproducible "vanished advance" anomaly attributed to pre-fix lock contention, motivating Phase 14 observability (`logs`/`events.jsonl`); (2) fresh headless Codex `discuss-phase` remains unsupported pending an upstream GSD Codex-port fix, mitigated for now by idempotent Define/Plan prompts and a pre-flight CONTEXT.md check. Both are called out by name in `13-06-SUMMARY.md`'s own "Residual gaps / follow-ups" section and do not block this phase's goal ("the loop ran").

---

*Verified: 2026-07-15*
*Verifier: Claude (gsd-verifier)*
