---
phase: 31
slug: claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-03
---

# Phase 31 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by `/gsd-plan-phase` from `31-RESEARCH.md` § Validation Architecture.
> Per-task rows are filled by the planner; this file is `draft` until `/gsd-validate-phase` promotes it.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` — no separate framework or config file in this workspace |
| **Config file** | none — workspace `Cargo.toml` convention |
| **Quick run command** | `cargo test -p devflow-core --lib <module>::` scoped to the module under change (`agent_result::`, `monitor::`, `agents::claude::`, `outcome_policy::`) |
| **Full suite command** | `cargo test --workspace`, then the repo gate `scripts/check.sh all` (fmt + clippy `-D warnings` + test) |
| **Estimated runtime** | ~2s scoped `--lib` run; full workspace suite is the longer gate |

**Binding repo trap (`CLAUDE.md`):** `cargo test --exact <name>` exits 0 when the name matches
nothing. Assert on a real `N passed` with a non-zero `filtered out` count — never on exit code
alone. The CLI package is `devflow`, not `devflow-cli`.

---

## Sampling Rate

- **After every task commit:** the narrow `cargo test -p devflow-core --lib <module>::` for the module touched
- **After every plan wave:** `cargo test --workspace`
- **Before `/gsd-verify-work`:** `scripts/check.sh all` green, **then** the live D-16 acceptance run
- **Max feedback latency:** ~5 seconds for the scoped run

The acceptance run is **not** replaced by any automated command — review constraint H4 makes it
non-substitutable by integration tests.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 31-01 T1 | 31-01 | 1 | Constraint 1, D-09/D-10 | T-31-01, T-31-04 | Adapter always emits `--input-format stream-json --output-format stream-json`; prompt via stdin, not argv; the two pipes get independent threads | integration (real stub child, end-to-end through the cascade) | `cargo test -p devflow-core --lib monitor::tests::pipe_owning_monitor_delivers_prompt_via_stdin_and_captures_stream -- --exact` | ❌ W0 | ⬜ pending |
| 31-01 T2 | 31-01 | 1 | Constraint 1, D-13 | T-31-01 | The prompt-text invariant survives the transport change; the canary's token matcher is provenance-checked | unit | `cargo test -p devflow-core --lib agents::`; `... agent_result::tests::token_matches_only_inside_top_level_result -- --exact` | ❌ W0 | ⬜ pending |
| 31-01 T3 | 31-01 | 1 | Constraint 4, Constraint 7 | T-31-01 | Close rule fires only on marker-in-top-level-`result` **AND** drained `background_tasks_changed`; a coalesced pair is not undercounted | unit (fixture-driven, 30a/30c capture shapes) | `cargo test -p devflow-core --lib monitor::tests::coalesced_completions_do_not_undercount_children -- --exact` | ❌ W0 (fixture data exists) | ⬜ pending |
| 31-02 T1 | 31-02 | 2 | D-06, D-08 | — | `IdleTimeout` is first-class, wire-stable, gates for a human, is never auto-resumed | unit | `cargo test -p devflow-core --lib outcome_policy::` | ❌ W0 | ⬜ pending |
| 31-02 T2 | 31-02 | 2 | Constraints 5/8, D-05, D-07 | T-31-06 | The idle-timeout verdict is read before `read_capture`, so a stale stream `result` cannot shadow it | unit (fixture **must** carry a prior real `result` event, plus a negative control) | `cargo test -p devflow-core --lib agent_result::tests::idle_timeout_side_channel_wins_over_stale_stream_result -- --exact` | ❌ W0 | ⬜ pending |
| 31-02 T3 | 31-02 | 2 | D-01, D-02, D-03, D-04, D-05 | T-31-07, T-31-08, T-31-09 | Timer resets per line; 30s clamped floor logged loudly; verdict written **before** the child is signalled; no commit rolled back | unit (pure parser + injected short timeout) | `cargo test -p devflow-core --lib monitor::tests::idle_timeout_secs_clamps_below_floor_and_logs -- --exact` | ❌ W0 | ⬜ pending |
| 31-03 T1 | 31-03 | 2 | D-13 | T-31-10, T-31-11 | The declared token counts only inside a top-level `result`; a prompt echo never satisfies it | unit (injected launcher, canned captures) | `cargo test -p devflow-core --lib canary::tests::canary_absent_when_token_appears_only_as_a_prompt_echo -- --exact` | ❌ W0 | ⬜ pending |
| 31-03 T2 | 31-03 | 2 | D-15 | T-31-12, T-31-13 | Once per run; refuses on absent or unverified with distinguishable messages; outcome in provenance | unit | `cargo test -p devflow --lib pipeline_launch::tests::absent_canary_refuses_to_launch -- --exact` | ❌ W0 | ⬜ pending |
| 31-04 T1 | 31-04 | 3 | Constraint 9 (residual), D-12 | T-31-15, T-31-16 | A stream-derived `Success` cannot override a contradicting non-zero exit code; `RateLimited` and `IdleTimeout` are untouched | unit, end-to-end through `evaluate_agent_result` with a zero-exit negative control | `cargo test -p devflow-core --lib agent_result::tests::stream_success_cannot_stand_against_nonzero_exit_code -- --exact` | ❌ W0 | ⬜ pending |
| 31-04 T2 | 31-04 | 3 | D-11 | T-31-17, T-31-18 | The opt-out is explicit, off by default, loud on three channels; no automatic fallback exists | unit + `--help` output | `cargo test -p devflow --lib pipeline_launch::tests::legacy_launch_is_off_by_default -- --exact` | ❌ W0 | ⬜ pending |
| 31-05 T1-T3 | 31-05 | 4 | D-16, D-17, D-18, D-19 | T-31-19..T-31-23 | Both plans produce a `SUMMARY.md` and both merge, on the main checkout, with no orchestrator git during the run | **manual-only, by design** | N/A — review constraint H4 makes it non-substitutable by integration tests | N/A | ⬜ pending |
| — (no task) | — | — | ROADMAP §999.67 | — | Agent cannot forge `decided_by_layer` Layer-0 provenance | unit | `cargo test -p devflow-core --lib agent_result::tests::generic_marker_cannot_forge_layer0_provenance` | ✅ **exists, passes** | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**999.67 row is closed, not pending.** Verified 2026-08-03 at HEAD `0852c03`: `parse_devflow_result`
(`agent_result.rs:147-162`) applies `normalise_stream_marker_provenance` on both arms; the named
test returns `1 passed / 501 filtered out` while a deliberately bogus test name returns
`0 passed / 502 filtered out` (negative control, so the pass is not vacuous). ROADMAP scope item 6
("Fold in 999.67, XS") is stale — it was closed by `a557805` on 2026-08-02, one day before the
Phase 31 entry was written.

*What that check does not establish:* it covers the `DEVFLOW_RESULT` marker route named by 999.67.
It does not prove every route into `decided_by_layer` is normalised.

---

## Wave 0 Requirements

- [ ] `crates/devflow-core/src/monitor.rs` — no test exercises pipe-ownership, idle-timeout, or the
      close-rule `AND`; existing tests target the `sh`-script shape and need **rewriting**, not extending
- [ ] `crates/devflow-core/src/agents/mod.rs` — `claude_wraps_prompt_in_noninteractive_flags` asserts
      `--output-format json` and a positional prompt; the new contract removes both, so this assertion
      must be **replaced**
- [ ] `crates/devflow-core/src/agent_result.rs` + `outcome_policy.rs` — `AgentStatus::IdleTimeout` (D-06)
      has no representation; every exhaustive match (`as_wire_str`, `decide_action`) needs an arm and a test
- [ ] Monitor close-rule fixture reusing the real coalesced-completion capture from
      `30c-evidence-reliability/` — the data exists, the harness around it does not
- Framework install: none — `cargo test` is already fully configured

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| A DevFlow-driven multi-plan wave completes without orphaning delegated work | Phase acceptance criterion (999.64 arc goal) | Review constraint H4: **not substitutable by integration tests** — a mocked CLI validates plumbing, not the delivery premise | D-16: minimal purpose-built two-plan wave. D-17: runs on the main checkout, orchestrator touches **no git** while the executor holds the tree (`CLAUDE.md`, binding). D-18: **pass = both plans produce a `SUMMARY.md` and merge** — explicitly not "the stage reports Success" (the oracle already scored the orphaned Phase 29 stage as Success), and not "both completions observed in the stream" (constraint 7 makes an observed count the signal that can undercount). D-19: a failing run means the phase does not close. |
| Startup canary confirms `task-notification` delivery | D-13 / review M2 | The premise is undocumented CLI behaviour observed on one version; a version string is a proxy for the behaviour, not the behaviour | One throwaway task at pipeline start declares its success token; the orchestrator records the token up front and confirms it returns **inside a top-level `result` event** (`is_top_level` / `claude_stream_gate_shape`) so the prompt echo cannot satisfy it. Proves delivery, never work. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 5s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
