# Phase 41: Antigravity Driver - Context

**Gathered:** 2026-08-19
**Revised:** 2026-08-20 (round 2) — D-02/D-03 re-opened (falsified) against live `antigravity-cli` 1.1.16; re-derived. See round-1 evidence `.planning/reviews/phase-41/SUMMARY.md`.
**Revised:** 2026-08-20 (round 3) — round-2 re-review (codex + claude BLOCK, antigravity conditional-pass) confirmed all round-1 fixes landed but BLOCKED on NEW transport-integration defects: the monitor close rule (B1), the delivery canary (B2), the PipeOwning idle-timeout policy (B3), plus F3-F7, codex-3/4/5/6 and three executor notices. All absorbed below. See `.planning/reviews/phase-41/review-2/SUMMARY.md`.
**Status:** Ready for re-planning

Review evidence: `.planning/reviews/phase-41/review-2/SUMMARY.md` (round-2 synthesis) + `claude.md`,
`codex.md`, `antigravity.md`; round-1 evidence in `.planning/reviews/phase-41/`.

## Phase Boundary

Phase 41 delivers the Antigravity driver: `devflow start --agent antigravity` launches the
Antigravity CLI headless and drives a stage to completion with honest completion detection. It also
closes two dogfood-hygiene items surfaced by the Phase 40 run — the leaked test monitors (HYG-01)
and the in-container git failures (HYG-02). Requirements: ANTG-01, ANTG-02, ANTG-03, HYG-01, HYG-02.

Round 3 adds `canary.rs` and `preflight.rs` to 41-01 scope (B2/F5) and makes the monitor's
`CloseRule` and idle-timeout policy agent-aware (B1/B3) — the read/close side of the transport that
round 1 missed and round 2 caught.

## Implementation Decisions

### Agent binary & launch
- **D-01: The driver targets the `agy` binary. [VERIFIED SOUND, rounds 1+2]** `agy` is the
  operator's single, canonical Antigravity entry point — a shell wrapper
  (`exec antigravity-cli --dangerously-skip-permissions "$@"`). The conflicting `antigravity`
  (1.1.13) and `agycli` binaries are absent from PATH. The wrapper injects
  `--dangerously-skip-permissions` itself, so the driver argv must not add it again.
  Version: live **1.1.16** at both reviews; nothing pins a version anywhere (D-04 makes that
  acceptable; T-41-03 no longer claims a lock).
- **D-02 (round-1 re-open; round-3 refinement): Stream-json launch, NOT `-p`, WITH
  `--print-timeout`, and an Antigravity-shaped first turn.** — **Reversibility:** costly.
  - **`-p` is a Go-flag STRING flag** requiring an argument; it swallows the next token and exits 0
    silently. Negative controls (round 1): bare `-p` → `flag needs an argument: -p`; `-p "<prompt>"`
    + stream-json input → mutually exclusive. **No `-p`.**
  - **RE-DERIVED argv (round 3):** `build_command` returns
    `("agy", vec!["--input-format", "stream-json", "--output-format", "stream-json", "--print-timeout", "60m"])`.
    `--print-timeout` added per round-2 F3: the CLI default is **5m**, which is below the
    documented DevFlow stage length (monitor.rs:923 cites a healthy 47-minute stage); every prior
    invocation in this repo overrides it (review runs used 15m/30m; the round-1 probe used 60s).
    `60m` is the decided floor (covers a 47m stage with margin) — see the open question on a
    6-minute negative-control probe before this argv ships. No `--dangerously-skip-permissions`
    (D-01), no prompt in argv (D-02 stdin).
  - **First-turn stdin schema is agent-specific.** DevFlow's `monitor::user_turn_line`
    (monitor.rs:726) emits `{"type":"user",...}` — the CLI rejects it (`stream input message is
    missing the "event" field`). Working shape: `{"event":"user","message":{...}}` via a new
    agent-aware `user_turn_line_for(agent, prompt)` (Claude keeps the `type`-key shape).
  - **STACK.md's argv table is wrong** on `-p` and stale on the version (see Canonical References).
- **D-10 (NEW, round 3): the legacy-launch opt-out is Claude-only.** `DEVFLOW_CLAUDE_LEGACY_LAUNCH`
  is an escape hatch for Claude's pre-31 single-document launch; Antigravity has no
  single-document format. The widened stream predicate must apply `!legacy_opt_out` ONLY when
  `agent == AgentKind::Claude`; `AgentKind::Antigravity` is stream-only and evaluates purely on
  `STREAM_JSON_STAGES` membership. (antigravity reviewer notice (b); an env `DEVFLOW_CLAUDE_LEGACY_LAUNCH=1`
  must not route Antigravity to `MonitorLaunch::Legacy` — stdin would be `/dev/null` and the child
  would silently fail.)
- **D-06 (round 1, unchanged): HYG-02 is a worktree `.git`-file mount problem**, not uid 0, not
  "3 git-env tests under root". Verified both ways in the pinned image
  (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`): worktree fails, main checkout as
  root passes. Fix lives in `check-in-container.sh` (mount the gitdir), NOT the three test files,
  NOT `skip_if_root()`. (41-02.)

### Completion detection
- **D-03 (round-1 re-open; round-3 refinement): Antigravity needs its own stream-json parser AND
  its own close rule — both sides of the transport are agent-specific.**
  - **Live stream shape:** `{"event":"init",...}` → `{"event":"step_update",...}` →
    `{"event":"result","result":{"status":"SUCCESS","response":"DEVFLOW_RESULT: ..."}}`. Events
    carry an `event` key; Claude carries `type`/`subtype`. `is_claude_event_stream`
    (agent_result.rs:862) gates false for Antigravity; `parse_marker_lines` needs the marker at
    line start, but it is JSON-escaped inside `result.response`.
  - **RE-DERIVED (parser):** `is_antigravity_event_stream` (gate on `event == "init"`) +
    `parse_antigravity_event_result` (extract the last `result` event's `response` string → feed
    `parse_marker_lines`) in `agent_result.rs`, wired into `evaluate_layer1`'s `.or_else` chain
    (agent_result.rs:1866) like `parse_codex_event_result`. Marker-less → `None` → Layer 2.
  - **RE-DERIVED (ERROR envelope, round-3 antigravity notice (c)):** when `result.status ==
    "ERROR"` (or an `error` field is present), return `Some(AgentResult { status: Failed, reason:
    Some(error), decided_by_layer: Some(1), ... })` — mirroring `claude_stream_envelope_failure`.
    Without this, Layer 1 returns `None` and loses the CLI's explicit error reason to Layer 2.
  - **RE-DERIVED (close rule, round-3 B1 — the round-1 miss):** the monitor's
    `CloseRule::should_close` (monitor.rs:705) requires `marker_seen`, which is set ONLY by
    `event_is_top_level_result_marker` (agent_result.rs:1208): `type=="result"` AND `result` as a
    **string** that parses as a marker. Antigravity emits `event:"result"` with `result` as an
    **object** → `marker_seen` never true → stdin never released → `recv_timeout` fires
    `fire_idle_timeout` (monitor.rs:963) → `evaluate_layer1`'s FIRST statement returns the
    idle-timeout side channel before `read_capture` → **`parse_antigravity_event_result` never
    runs on a real stage** (claude proved this live: the child idled on stdin 36s+ after its final
    byte; stream-json "runs a turn for each" line, it does not self-exit). Fix: an agent-aware
    close predicate — `event == "result"` + `result.response` string → `parse_marker_lines` —
    threaded through `CloseRule` at BOTH construction sites (monitor.rs:882 in
    `run_pipe_owning_monitor`, canary.rs:~360 in the canary's own close rule). The
    `background_tasks` / `open_tasks` drain arms read `type:"system"` subtypes Antigravity never
    emits — **vacuously satisfied and MUST be stated as such** in the implementation, not silently
    inherited. (B1; the write/read/close triple is now all agent-aware.)
  - **Wiring note:** the trait hook `parse_completion` has no call sites today; the driver
    overrides it to delegate (contract completeness), but the load-bearing dispatch is
    `evaluate_layer1`.
  - **ANTG-03 gate:** a marker-less run never advances a **commit-gated** stage (`evaluate_layer2`
    sets `commit_gated = matches!(stage, Stage::Plan | Stage::Code)`, agent_result.rs:2037).
    Define is NOT commit-gated and legitimately advances on exit 0. Regression gates at
    `Stage::Plan` with `wait_for_gate`.
- **D-07 (NEW, round 3 — transport routing): the stream-launch predicate is widened, and the
  delivery canary must be agent-specific.** `canary_gate(state, stream_launch, ...)`
  (pipeline_launch.rs:462) is gated on nothing but `stream_launch`; its launcher is hardcoded
  `ClaudeCanaryLauncher` (pipeline_launch.rs:145 → canary.rs:286 spawns `claude` via
  `ClaudeDriver.build_command`, canary.rs:323 writes the `type`-key turn). Widening the predicate
  (D-02/Task 2) would arm the Claude canary on every Antigravity run at Stage::Define: a Claude
  invocation spent per run, and `claude` absent/unauthenticated → `Unverified` → `refuse_launch` →
  **Antigravity never launches**. Fix (chosen): an `AntigravityCanaryLauncher` in canary.rs
  mirroring `ClaudeCanaryLauncher` — `AntigravityDriver.build_command` + `user_turn_line_for(
  AgentKind::Antigravity, ...)` + the agent-aware CloseRule; the launcher is selected by agent at
  the canary_gate call site. The canary's trust decision (`token_reported_in_capture`) is a raw
  token search, so an Antigravity-shaped capture (token JSON-escaped inside `response`) matches.
  canary.rs joins 41-01 scope.
- **D-08 (NEW, round 3): PipeOwning idle-timeout policy is agent-specific, not silently
  inherited.** The 120s floor (monitor.rs:1022-1027) was measured against Claude's fixed 30.00s
  `tool_progress` keepalive; the doc explicitly forbids applying it to an unmeasured agent, and
  the reader is `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` (monitor.rs:236-237, resolved at
  spawn_monitor_inner:372 where `state.agent` is in scope). Fix: `idle_timeout_setting_for(agent)`
  — Claude reads the existing variable unchanged; Antigravity reads
  `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS` with a **decided** default (same 120s floor as a
  documented starting point, revisit after the first real cadence measurement — the decision is
  explicit, not inherited). This is the companion to B1: B1 makes the close rule fire; B3 bounds
  how long a genuinely silent Antigravity child is tolerated.

### Health / preflight
- **D-04 (round-1 revised; round-3 refined): Presence-only health check, with a testable doctor
  seam and an explicit unattended-mode decision.**
  - `ensure_agent_binary` / `agent_program` (preflight.rs:84-97) already resolve `agy` (verified
    sound round 1). The planned `health() -> Ok(())` stays a trait default (dead code otherwise).
  - **`devflow doctor` needs a commands.rs entry** — the check list is hardcoded
    (commands.rs:2287). Round-2 F7: `cmd_check` is a NESTED fn inside `doctor()` (commands.rs:2202)
    with no assertable seam — the entry must be reachable by a unit test. Fix: extract the checks
    construction into a named function (e.g. `fn doctor_checks() -> Vec<Check>`) used by
    `doctor()`, and test it (entry exists, cmd `agy`, version arg `--version`) plus a PATH-based
    presence test with a stubbed `agy` (env-dependence made explicit: green locally with `agy` on
    PATH, "warn" status when absent — never a hard failure).
  - **Unattended mode (round-2 F5):** the widened predicate flips `unattended_launch_shape_condition`
    C2 (preflight.rs:974) — without a decision, `devflow start --agent antigravity --mode auto`
    would become permitted for an **undogfooded** driver, and the AutoChainGuard comment
    (pipeline_launch.rs:870-875, "implies a Claude + stream launch") becomes false. DECISION:
    `--mode auto` stays **refused** for Antigravity until the driver has a real dogfooded run —
    C2 holds only for stream agents that are dogfooded (Claude today); the cause branch names
    `antigravity` explicitly. AutoChainGuard DOES engage for Antigravity on the PipeOwning arm
    (the chain-flag lifetime guard is correct for any pipe-owning launch) — its comment is
    corrected, its behavior is tested. preflight.rs joins 41-01 scope (round-1 finding 6 was about
    `agent_program`, not C2 — the pre-review "do not touch preflight" instruction is rescinded).

### Prompt rendering
- **D-05: Reuse `render_claude_style`. [VERIFIED SOUND, rounds 1+2]** `contract_checks` asserts
  `prompt.contains("DEVFLOW_RESULT")` for all five stages; `ClaudeDriver.render_prompt` is
  byte-identical to `render_claude_style` (agents/mod.rs:225-228). No dedicated renderer until a
  live probe shows the Claude framing is wrong.

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Scope
- `.planning/ROADMAP.md` § "Phase 41: Antigravity Driver" — goal, success criteria. Criteria 2, 3
  and 6 corrected across rounds 2+3 (version; "commit-gated stage without a marker"; worktree-mount
  HYG-02).
- `.planning/REQUIREMENTS.md` — ANTG-01..03, HYG-01, HYG-02. **NOTE: ANTG-02's "Claude-style `-p`"
  and HYG-02's "3 git-env tests under root" wordings are superseded** (D-02/D-06).

### Driver contract & patterns to mirror
- `crates/devflow-core/src/agents/claude.rs` — `ClaudeDriver` (stream-json `build_command`, minus
  the `-p` boolean; add `--print-timeout`).
- `crates/devflow-core/src/agents/mod.rs` — `AgentDriver` trait (`parse_completion` default →
  `None`), `driver_for`, `contract_checks` (7 results), and the hardcoded
  `every_driver_passes_the_conformance_suite` array (agents/mod.rs:274-279, 4 drivers → 5).
- `crates/devflow-core/src/agent_result.rs` — `event_is_top_level_result_marker` (agent_result.rs:1208,
  the Claude close predicate), `parse_claude_event_result`/`is_claude_event_stream` (parser model),
  `claude_stream_envelope_failure` (ERROR-envelope model), `evaluate_layer1` chain (1866),
  `evaluate_layer2` (2037, commit_gated).
- `crates/devflow-core/src/monitor.rs` — `user_turn_line` (726), `idle_timeout_setting`
  (236-237, resolved at 372 where `state.agent` is in scope), `CloseRule` (582-715, constructed at
  882 inside `run_pipe_owning_monitor`), `fire_idle_timeout` (963), the PipeOwning writer (820).
- `crates/devflow-core/src/canary.rs` — `ClaudeCanaryLauncher` (build_command at ~286,
  `type`-key turn at ~323, CloseRule at ~360), `run_delivery_canary` / `token_reported_in_capture`
  (raw token search — Antigravity-shaped captures match).
- `crates/devflow-cli/src/pipeline_launch.rs` — `claude_stream_launch_enabled` (707),
  `resolve_launch_shape` (192), the canary_gate call site (141-148) and `canary_gate` (462),
  `AutoChainGuard` (870-875).
- `crates/devflow-cli/src/preflight.rs` — `unattended_launch_shape_condition` C2 (974; the widened
  predicate's second consumer).
- `crates/devflow-cli/src/commands.rs` — doctor's hardcoded check list (2287); `cmd_check` nested
  fn (2202) — extract a seam.
- `crates/devflow-core/src/state.rs` — `AgentKind` + FromStr/Display/AgentParseError.
- `ARCHITECTURE.md` § "Extension points — adding an agent".

### Research
- `.planning/research/STACK.md` — argv/flags table NOT accurate (`-p` row wrong, version stale);
  binary-name section superseded by D-01.
- `41-RESEARCH.md` / `41-PATTERNS.md` — carry pre-round-2 claims that are superseded; treat their
  `[VERIFIED: CONTEXT.md:…]` citations as stale.

### Test pattern
- `crates/devflow-cli/tests/phase7_cli.rs` — `pi_marker_less_run_does_not_advance` (1218-1254),
  `wait_for_gate` (1206), the stubbed-`claude` canary fixture (249-263, `*DEVFLOW_DELIVERY_CANARY_*`
  echo), and the canary-refusal caveat (111-125: `run_devflow_legacy_launch` is NOT the escape
  hatch for stream-path tests).
- `crates/devflow-cli/src/test_support.rs` — `ReapMonitorOnDrop` (573) / `reap_monitor_pid` (509):
  the per-PID reap model for HYG-01.

## Existing Code Insights

### Reusable Assets
- `ClaudeDriver` / `ClaudeCanaryLauncher` / `CloseRule` / `event_is_top_level_result_marker` —
  the exact shapes to make agent-aware (B1/B2/D-07/D-08).
- `wait_for_gate`, the canary fixture, `ReapMonitorOnDrop` — test assets (F4, HYG-01).
- `devflow_core::agent::{terminate_and_verify, agent_running, discover_stray_devflow_processes}` —
  per-PID reaping primitives (HYG-01 suite audit).

### Established Patterns
- Marker-less never advances a commit-gated stage; `evaluate_layer1` side-channel priority
  (idle-timeout first — why B1 is load-bearing).
- Canary = once-per-run foreground delivery proof; `Absent`/`Unverified` refuses launch.
- Stream-launch predicate is a per-agent routing choice with TWO extra consumers (preflight C2,
  AutoChainGuard) that must stay consistent with it.

### Integration Points (round-3 full list)
- `AgentKind` + FromStr/Display (state.rs); `driver_for` + conformance enrollment (agents/mod.rs).
- `evaluate_layer1` chain + close-rule predicate (agent_result.rs).
- `user_turn_line_for` + `CloseRule` + `idle_timeout_setting_for` (monitor.rs).
- `AntigravityCanaryLauncher` + agent dispatch at canary_gate (canary.rs, pipeline_launch.rs).
- Stream predicate widening + AutoChainGuard comment (pipeline_launch.rs).
- `unattended_launch_shape_condition` C2 decision + test (preflight.rs).
- `devflow doctor` checks seam (commands.rs).
- `agent_program` resolution (preflight.rs:84-97, unchanged).

## Specific Ideas

No specific requirements — open to standard approaches.

## Deferred Ideas

- **Version floor / capability probe on `agy`** — deferred; presence-only (D-04).
- **Update `research/STACK.md` + refresh RESEARCH/PATTERNS** — deferred to a docs-cleanup pass.
- **6-minute `--print-timeout` negative control (F3):** the default 5m vs long-stage behaviour was
  NOT directly measured by any reviewer; a 6-minute live probe is required before this argv ships
  (listed in 41-VALIDATION as an execution-time control, not a unit test).
- **Antigravity cadence measurement (D-08/B3):** revisit the idle-timeout default after the first
  real multi-stage run.
- **Open questions (do not block):** (a) `step_update.text_delta` cumulative vs delta-only — the
  parser keys off `result.response`, so this does not block; (b) `{"event":"user",...}` canonical
  vs one accepted shape — probe `--json-schema`; (c) multi-turn stream memory/behaviour — the
  canary and close rule cover the single-turn contract; multi-turn is out of scope for this phase.

---

*Phase: 41-Antigravity Driver*
*Context gathered: 2026-08-19 · revised: 2026-08-20 (round 3 — adversarial re-review rework)*
