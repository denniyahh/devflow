# Phase 37 Research — Modular Agent Driver Architecture + Pi Driver

**Gathered:** 2026-08-15 (against `feature/phase-37` @ `f2ac40f`, the phase-36 merge base)

## What was verified

### The scattered agent logic (what the migration consolidates)

| Where | What lives there | Target under `AgentDriver` |
|---|---|---|
| `crates/devflow-core/src/stage.rs` `Stage::gsd_command()` | returns `/gsd-{discuss,plan,execute,validate,ship}-phase {N}` strings | replaced by `StageIntent` enum (no agent syntax) |
| `crates/devflow-core/src/prompt.rs` `stage_prompt()` / `gsd_command_for()` / `fix_prompt()` | interpolates the slash command + `DEVFLOW_RESULT` contract into one shared prompt for every agent | split into per-driver `render_prompt` |
| `crates/devflow-core/src/agents/mod.rs` | `AgentAdapter` trait (`name`, `exec_command`, `extra_env`, `completion_signal_detected`, `preflight`) + `adapter_for()` + `every_adapter_receives_identical_prompt_text` (the shared-prompt invariant test) | replaced by `AgentDriver` + driver selection; the invariant test retires |
| `agents/claude.rs` (288L) | Claude `stream-json` stdin transport, `exec_command`, `exec_command_single_document` | `ClaudeDriver` |
| `agents/codex.rs` (69L) | `codex exec --sandbox workspace-write --json -c sandbox_workspace_write.writable_roots=[…] <prompt>` + `GIT_CONFIG_*` signing-disable env | `CodexDriver` |
| `agents/opencode.rs` (27L) | `opencode run <prompt>` | `OpenCodeDriver` |
| `agents/pi.rs` (246L) | `pi -p --no-approve <prompt>` + `preflight` via `pi auth check --json --provider google` | `PiDriver` |
| `agent_result.rs` | `parse_codex_event_result()`, `is_codex_event_stream()`, `detect_codex_rate_limit()`, `claude_stream_session_id()`, `claude_session_id()` | per-driver `parse_completion` |
| `pipeline_launch.rs` | `MonitorLaunch::PipeOwning` (Claude stream-json) vs `MonitorLaunch::Legacy` (non-Claude); `claude_stream_launch_enabled()`; `launch_stage_inner()` | driver-owned `sandbox_requirements` / launch shape |
| `preflight.rs` | `generic_preflight_checks()` framework; `ensure_agent_binary()`, `preflight_gh_auth_check()`, `preflight_major_bump_check()`, `preflight_interactivity_check()`, `preflight_unattended_launch_check()` | per-driver `health` / `InteractivityMode` |

### The bug this phase fixes (999.31 root cause)

`Stage::gsd_command()` bakes a raw slash-command string into core, and `prompt.rs` renders it
identically for every adapter. That shared-prompt assumption holds for Claude/OpenCode and is
false for Codex — a Codex run receives `/gsd-execute-phase {N}` as a literal shell instruction.
`every_adapter_receives_identical_prompt_text` (`agents/mod.rs:131`) *enforces* the false
invariant, so it must be retired as part of 31a, replaced with a `StageIntent`-level
semantic-equivalence test.

### StageIntent migration shape (31a)

- `StageIntent` variants: `Define { phase }`, `Plan { phase }`, `Code { phase, fix: Option<FixType> }`,
  `Validate { phase }`, `Ship { phase, review_angles }` — carries **no agent-specific syntax**.
- `prompt.rs` rendering moves into per-driver `render_prompt`. Claude/OpenCode render byte-identical
  text (behavior-preserving — D-01's zero-regression bar); Codex renders a Codex-native instruction.
- `fix_prompt()` (`prompt.rs:351`, `/gsd-audit-fix` + `/gsd-execute-phase --gaps-only`) is a second
  slash-command site that must migrate too — it is how the Code→Validate loop-back renders.

### Per-agent specifics

- **Claude (zero-regression, top priority).** `stream-json` bidirectional transport
  (`--input-format stream-json --output-format stream-json --verbose`), stdin turn shape, routed via
  `MonitorLaunch::PipeOwning` with the `CloseRule` drain gate. `ClaudeDriver` must preserve the exact
  prompt text, argv, monitor routing, and completion parsing (`claude_stream_session_id` last-`init`-wins,
  torn-JSON fail-closed). This is the highest-risk piece — the shared-prompt invariant test is what
  currently keeps it pinned.
- **Codex (fixed by the migration).** `--json` JSONL completion parsing already exists
  (`parse_codex_event_result` + `is_codex_event_stream` in `agent_result.rs`) and relocates to
  `CodexDriver::parse_completion`. 31b hardening: parse `codex features list` for `multi_agent_v2`,
  explicit `--ask-for-approval never`, prefer `--add-dir` over the hand-escaped `writable_roots` TOML
  (verify equivalence for linked-worktree metadata first — 13-06 finding).
- **OpenCode.** Thin positional `opencode run <prompt>`; migrate with the same behavior.
- **Pi.** Phase 36 registered `PiAgent` on `-p` print mode (`--no-approve`, positional prompt,
  `pi auth check` preflight). Phase 37 migrates Pi onto `AgentDriver` but keeps `-p`; the JSON-mode
  unwrapper + monitor/`CloseRule` integration (end-to-end parity) is **deferred to 37.1/38** (D-04).

### Conformance suite (31c, in-scope if capacity allows)

`AgentDriver` trait surface: `discover`, `health`, `capabilities` (`#[non_exhaustive]` + `Default`,
as-needed per D-12), `render_prompt`, `build_command`, `parse_completion`, `sandbox_requirements`,
`environment`, `test_contract`. `DriverHealth` distinguishes binary-installed from
headless-execution-usable. `InteractivityMode` (`HeadlessSafe`, `RequiresExistingArtifact`,
`RequiresTypedSubagents`, `InteractiveOnly`) replaces the hardcoded Codex-Define-only check in
`preflight.rs`. `test_contract()` is the artifact future drivers (Antigravity, Hermes) implement
against.

## Validation Architecture

**Framework:** Rust `#[test]` (cargo) — `devflow-core` unit tests + `devflow` integration tests;
snapshot/byte-equality tests for the zero-regression Claude prompt.

**Per-unit verification strategy:**

- **31a StageIntent:** snapshot tests assert Claude/OpenCode `render_prompt` output is byte-identical
  to today's `stage_prompt()` text; a `StageIntent`-level acceptance test checks semantic equivalence
  (replacing `every_adapter_receives_identical_prompt_text`); Codex output contains no raw `/gsd-*`
  execution instruction (negative control).
- **31b Codex:** golden fixtures from the installed Codex CLI for `parse_completion`; `codex features
  list` parse test; argv shape tests (`--enable multi_agent_v2`, `--ask-for-approval never`).
- **31c contract:** `test_contract()` conformance suite exercised by every driver; `DriverHealth` unit
  tests (binary-present vs headless-capable); `InteractivityMode` per-stage mapping tests.
- **Zero-regression Claude:** `ClaudeDriver` render/argv/monitor-routing tests pin the stream-json
  launch + `CloseRule` behavior; existing `devflow-core --lib` + `devflow` suites stay green.

**Negative controls / fail-first:** a driver rendering a raw `/gsd-*` string fails the Codex check;
a `DriverHealth` that reports "usable" on a credentialless binary fails.

## Pitfalls (carried into the plan)

- `fix_prompt()` (`prompt.rs:351`) is a second slash-command render site — missing it leaves the
  Code→Validate loop-back still emitting `/gsd-execute-phase` to Codex.
- `every_adapter_receives_identical_prompt_text` is load-bearing today: retiring it without a
  byte-equality snapshot for Claude is how a silent Claude prompt drift happens.
- `MonitorLaunch::PipeOwning` is Claude-only; the driver `build_command` must not accidentally route
  Pi/Codex into it (Phase 36 already routes non-Claude to `Legacy`).
- The 999.31 audit's line numbers (e.g. `agent_result.rs:361-453`) have shifted — locate by symbol,
  not by line.
- `AgentAdapter` removal (D-11) is conditional; do not rip it out until all four agents run on
  `AgentDriver`, or a compat shim keeps `adapter_for` working.
