---
phase: 41-antigravity-driver
plan: 01
subsystem: agents
tags: [antigravity, stream-json, transport, canary, close-rule, idle-timeout, preflight, doctor, regression]

requires:
  - phase: 31/34/35.1
    provides: stream-json transport machinery, canary gate, AutoChainGuard, preflight C2 (all made agent-aware here)
  - phase: 40
    provides: marker/exit/liveness completion machinery (re-derived for the event-key schema)

provides:
  - AntigravityDriver end-to-end: argv (no `-p`, `--print-timeout 60m`), event-key first turn, agent-aware close rule, ERROR-envelope parse, delivery canary, per-agent idle timeout, preflight refusal, doctor entry, conformance enrollment 4→5
  - Full transport regression gates: marker-less never advances a commit-gated stage; happy path only on a real marker; schema-rejecting stub (codex-3)
  - `MonitorReapGuard` (defined here; systematic pass + suite audit in 41-02)

affects: [agent transport routing, canary trust decision, unattended-launch policy, doctor surface]

actuals:
  tokens: 85000
  raw_tokens: 43000
  tasks: 8
  commits: 2 (4e71053 wave-1; 122dedc 41-02 Task 1)

tech-stack:
  added: [agy (Antigravity CLI) driver, event-key stream parser, agent-aware canary launcher]
  patterns: [agent-aware transport triple (write/read/close), agent-dispatch canary, per-agent policy resolution, suite-level reap registry]

key-files:
  created:
    - crates/devflow-core/src/agents/antigravity.rs
    - crates/devflow-core/tests/agent_kind_antigravity.rs
    - crates/devflow-cli/tests/doctor_antigravity.rs
  modified:
    - crates/devflow-core/src/agent_result.rs
    - crates/devflow-core/src/monitor.rs
    - crates/devflow-core/src/canary.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-core/src/agents/mod.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/main.rs
    - crates/devflow-cli/tests/phase7_cli.rs
    - crates/devflow-cli/tests/auto_chain_flag_e2e.rs
    - crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs
    - OPERATIONS.md

key-decisions:
  - "The round-2 review's B1 close-rule defect was real and load-bearing: Claude's `type:\"result\"` + string-result predicate never matches Antigravity's `event:\"result\"` + object-result, so stdin would never be released and every real stage would idle-timeout before its capture was parsed. The close predicate is now agent-aware (`event_is_top_level_antigravity_result_marker`) and the `type:\"system\"` drain arms are STATED vacuously satisfied for Antigravity, not silently inherited."
  - "The canary trust decision needed to be agent-aware, one layer deeper than the plan claimed: `token_reported_in_capture` filters `type:\"result\"` + STRING result, so an Antigravity-shaped capture could never Confirmed and every launch would be refused. Added `token_reported_in_capture_for(agent, ...)` (event:result + result.response), wired through a `CanaryLauncher::agent()` trait method. Deviation from the plan's 'raw search still matches' claim — the claim was false against the actual implementation."
  - "The idle-timeout policy is per-agent (`idle_timeout_setting_for`): Antigravity reads `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` with the DECIDED 120s floor (D-08). Added `notice_for(env)` so a clamp/typo notice names the variable the operator actually set, never the Claude one."
  - "`claude_stream_launch_enabled` was renamed `stream_launch_enabled` — a predicate returning true for Antigravity under a Claude-only name would be exactly the misleading-name class this repo's reviews flag. The legacy opt-out stays Claude-only (D-10)."
  - "Unattended `--mode auto` stays REFUSED for the undogfooded Antigravity driver (F5/D-04) — the widened predicate alone must not flip preflight C2."
  - "Doctor: extracted `doctor_checks()` seam; the PATH-based presence tests live in the INTEGRATION suite (`tests/doctor_antigravity.rs`) because a unit-test harness's `CARGO_BIN_EXE_devflow` resolves to the harness binary itself (spawning it re-enters the test suite — infinite recursion). PATH is scoped per-child via `Command::env`, never process-global (a global PATH mutation poisoned parallel tests spawning `sh`)."
  - "The `--print-timeout 60m` argv was live-probed against agy 1.1.16: accepted, event-key first turn works, the stream completed with a marker in `result.response` (~4s). The ERROR envelope was also observed live (malformed input probe) and parses to Some(Failed) as designed. Full >5m negative control deferred to the first real long stage (plan's deferred-ideas note)."
  - "The `__monitor` subcommand gained a required `--agent` arg; pre-existing e2e tests constructing the invocation manually were updated to pass it."

patterns-established:
  - "Stub `agy` on tempdir PATH answering canary turns (token echo in `event:result.response`) + schema-checking every turn (exit 92 on non-event-key) — the F4/codex-3 fixture."
  - "Every `<automated>` verify names tests unique to the new work and matches 0 tests on the unmodified tree (F6): `antigravity_event`, `user_turn_line_for`, `close_rule_antigravity`, `idle_timeout_setting_for`, `canary_antigravity`, `stream_launch_includes_antigravity`, `auto_chain_guard_antigravity`, `unattended_launch_shape_condition_antigravity`, `antigravity_driver`, `agent_kind_antigravity`, `antigravity_conformance_enrollment`, `doctor_includes_antigravity`, `antigravity` (phase7_cli)."

requirements-completed:
  - ANTG-01, ANTG-02, ANTG-03

coverage:
  - id: T1
    description: "Antigravity stream parser + ERROR envelope + agent-aware close predicate"
    requirement: ANTG-03
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agent_result.rs#antigravity_event_* (7 tests)"
        status: pass
    human_judgment: false
  - id: T2
    description: "Agent-aware monitor transport — event-key turn, CloseRule, per-agent idle timeout"
    requirement: ANTG-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/monitor.rs#user_turn_line_for_* / close_rule_antigravity_* / idle_timeout_setting_for_* / pipe_owning_writer_delivers_antigravity_event_key_turn"
        status: pass
    human_judgment: false
  - id: T3
    description: "Stream predicate widening + AntigravityCanaryLauncher + canary dispatch + agent-aware trust"
    requirement: ANTG-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/canary.rs#canary_antigravity_* (3); pipeline_launch.rs#stream_launch_includes_antigravity_* / canary_launcher_for_selects_antigravity_canary / auto_chain_guard_antigravity_engages_on_auto_code"
        status: pass
    human_judgment: false
  - id: T4
    description: "Unattended C2 — Antigravity --mode auto refused until dogfooded"
    requirement: ANTG-02
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/preflight.rs#unattended_launch_shape_condition_antigravity_* (3)"
        status: pass
    human_judgment: false
  - id: T5
    description: "AntigravityDriver argv (no -p, --print-timeout 60m) + spawn smoke test"
    requirement: ANTG-02
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/agents/antigravity.rs#antigravity_driver_* (6)"
        status: pass
    human_judgment: false
  - id: T6
    description: "AgentKind variant + dispatch + conformance enrollment 4→5"
    requirement: ANTG-01
    verification:
      - kind: unit + integration
        ref: "state.rs#agent_kind_antigravity_*; agents/mod.rs#antigravity_conformance_enrollment; tests/agent_kind_antigravity.rs"
        status: pass
    human_judgment: false
  - id: T7
    description: "doctor_checks() seam + antigravity/agy entry + PATH-based presence"
    requirement: ANTG-01
    verification:
      - kind: unit + integration
        ref: "commands.rs#doctor_includes_antigravity_check_in_the_seam; tests/doctor_antigravity.rs (2)"
        status: pass
    human_judgment: false
  - id: T8
    description: "Canary-aware agy stub + marker-less/happy/discrimination regressions + MonitorReapGuard"
    requirement: ANTG-03
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/phase7_cli.rs#marker_less_antigravity_never_advances / antigravity_parses_devflow_result_from_stream / antigravity_init_without_marker_gates_at_plan"
        status: pass
    human_judgment: false

duration: 6h
completed: 2026-08-20
status: complete
---

# Phase 41 Plan 01: Antigravity Driver — Wave 1 Summary

The Antigravity driver end to end, per the round-3 reworked plan (cde987a). Two
adversarial review rounds BLOCKed on transport integration; every round-2
finding (B1-B3, F3-F7, codex-3/4/5/6, antigravity notices a-c) is absorbed into
the code, not just the docs.

## Performance

- **Duration:** ~6h (two review rounds' findings implemented + full verification)
- **Tasks:** 8/8 complete
- **Commits:** 2 (`4e71053`, plus `122dedc` for the 41-02 Task 1 reap pass)

## Accomplishments

- **Parser** (`agent_result.rs`): `is_antigravity_event_stream` (event-key
  gate), `parse_antigravity_event_result` (last `event:result` object's
  `result.response` STRING → `parse_marker_lines`; ERROR envelope →
  `Some(Failed{reason})` — the CLI's explicit reason survives Layer 1),
  `event_is_top_level_antigravity_result_marker` (the agent-aware CLOSE
  predicate, B1), wired into `evaluate_layer1` after the Claude parser.
- **Transport** (`monitor.rs`): `user_turn_line_for(agent, prompt)` — the
  Antigravity first turn is `{"event":"user",...}`, Claude stays
  byte-identical; `CloseRule::for_agent` with the drain arms stated vacuously
  satisfied; `idle_timeout_setting_for` with the decided per-agent variable;
  `--agent` threaded through the `__monitor` re-exec.
- **Canary** (`canary.rs`): `AntigravityCanaryLauncher` (agy-based, event-key
  turn, agent-aware close rule) via a shared `run_stream_canary` supervisor;
  the trust decision is agent-aware (`token_reported_in_capture_for`).
- **Routing** (`pipeline_launch.rs`): `stream_launch_enabled` (Claude |
  Antigravity; legacy opt-out Claude-only, D-10); canary dispatched by agent;
  AutoChainGuard comment corrected.
- **Preflight** (`preflight.rs`): `--mode auto` refused for the undogfooded
  driver, cause names antigravity.
- **Driver** (`antigravity.rs`): exact argv — no `-p`, no skip-permissions
  (D-01), `--print-timeout 60m` (F3) — spawn smoke test (F7),
  `parse_completion` delegate.
- **Enrollment** (`state.rs` / `agents/mod.rs`): `AgentKind::Antigravity`,
  dispatch, conformance array 4→5 with the uniquely-named
  `antigravity_conformance_enrollment` (F6).
- **Doctor** (`commands.rs`): `doctor_checks()` seam + `antigravity`/`agy
  --version` entry (F7); PATH-scoped presence tests in the integration suite.
- **Regressions** (`phase7_cli.rs`): canary-aware agy stub (F4), marker-less
  gates at Plan, marker stream advances, init-without-marker discrimination
  control, schema-rejecting stub (codex-3), `MonitorReapGuard` defined.

## Verification

- Every task verify shows a real `1 passed` (or 2-7) with non-zero
  `filtered out`; each filter matches 0 tests on the unmodified tree (F6).
- Full workspace suite green: `cargo test --workspace` all pass.
- `cargo clippy --workspace --all-targets` clean; `cargo fmt --check` clean.
- Live probe (agy 1.1.16): the shipped argv is accepted, the event-key turn
  round-trips, the stream completes with a marker in `result.response`
  (recorded in 41-VALIDATION one-time probes).

## Deviations from Plan

1. **Agent-aware canary trust (additive).** The plan said the raw
   `token_reported_in_capture` "still matches" Antigravity captures — false
   against the actual implementation (`type:"result"` + string result only).
   Added `token_reported_in_capture_for(agent, ...)` + `CanaryLauncher::agent()`.
2. **Doctor PATH tests moved to the integration suite.** Unit-test harness
   `CARGO_BIN_EXE_devflow` resolves to the harness binary; spawning it
   re-enters the suite (infinite recursion). Also: the first attempt mutated
   the process-global PATH, which poisoned parallel tests spawning `sh` —
   rewritten to per-child `Command::env` scoping.
3. **`claude_stream_launch_enabled` renamed `stream_launch_enabled`** (naming
   honesty for a widened predicate).
4. **`__monitor --agent` required arg** — pre-existing e2e tests updated.
5. **Task execution order adapted to dependencies**: the `AgentKind` variant
   (Task 6 Part A) landed before Task 2/3 because every transport function
   matches on it; task verifies still map 1:1 to the plan's names.

## Self-Check: PASSED
