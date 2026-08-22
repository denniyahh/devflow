# Codex Compatibility and Driver Architecture Review

Date: 2026-07-24
Repository: DevFlow
Baseline inspected: local `codex-cli 0.145.0`

## Executive Summary

DevFlow has real Codex dogfooding fixes already in place: Codex launches through `codex exec --json --sandbox workspace-write`, linked-worktree `.git` writable roots are granted, signing is disabled in Codex's process tree, Codex JSONL `agent_message` completion markers are parsed, marker-less `turn.completed` defers instead of passing, and fresh Codex Define runs without a pre-existing `CONTEXT.md` fail fast.

The remaining compatibility problem is architectural: the pipeline is advertised as agent-neutral, but core stage and prompt code still carries raw GSD slash-command strings and tests enforce identical prompt text for Claude, Codex, and OpenCode. That is incompatible with Codex-specific execution semantics and with the confirmed dogfood failure where generic `/gsd-*` labels reached Codex as literal shell commands. Codex also needs capability and health discovery that DevFlow does not currently model: `multi_agent_v2` is stable but disabled by default in a clean `CODEX_HOME`, the project `.codex/config.toml` does not enable it, and the Codex adapter does not pass `--enable multi_agent_v2`.

Recommended direction: keep the pipeline and state machine stage-neutral, but replace the current thin `AgentAdapter` with a modular driver contract. Drivers should own capability discovery, command building, prompt rendering, lifecycle and health probes, completion parsing, environment and sandbox requirements, and a shared conformance test contract. Migration can preserve Claude/OpenCode behavior by wrapping the current prompt and command behavior in legacy drivers first, then moving Codex to a richer driver implementation.

Independent delegated review lanes were requested by the OMX `code-review` skill, but they did not return before the user's follow-up instruction. This report is completed inline from direct repository and local CLI evidence only.

## Local Codex Ground Truth

Commands inspected:

- `codex --version` reported `codex-cli 0.145.0`.
- `codex exec --help` shows first-class support for `--enable <FEATURE>`, `--disable <FEATURE>`, `--sandbox read-only|workspace-write|danger-full-access`, `--cd <DIR>`, `--add-dir <DIR>`, `--json`, `--output-last-message <FILE>`, `--ignore-user-config`, and `--ask-for-approval never` at the top-level help.
- `codex features list` in this repository reported `multi_agent_v2 stable true`, because user config enables it.
- A clean temporary `CODEX_HOME` reported `multi_agent_v2 stable false`, confirming it is disabled by default.
- Project `.codex/config.toml` enables `multi_agent = true`, but not `multi_agent_v2`.
- User `~/.codex/config.toml` enables `multi_agent_v2 = true`.
- `codex doctor` reported config/auth loaded, but also provider HTTP reachability failure under this restricted environment and a stale or unreachable app-server socket.
- Minimal `codex exec --json --sandbox workspace-write --enable multi_agent_v2 ...` failed before a model turn with `failed to initialize in-process app-server client: Read-only file system`.

## Severity-Ranked Findings

### HIGH: Core Prompt Model Still Emits Raw GSD Slash Commands

Status: Confirmed defect / architecture blocker.

Evidence:

- `crates/devflow-core/src/stage.rs:51-59` stores `/gsd-discuss-phase {N}`, `/gsd-plan-phase {N}`, `/gsd-execute-phase {N}`, `/gsd-validate-phase {N}`, and `/gsd-ship {N}` directly in the core `Stage` model.
- `crates/devflow-core/src/prompt.rs:43-46` substitutes those strings generically.
- `crates/devflow-core/src/prompt.rs:200-217` renders Code and generic stage prompts as "Run the GSD workflow command..." followed by the raw command.
- `crates/devflow-core/src/prompt.rs:221-228` does the same for loop-back commands `/gsd-audit-fix` and `/gsd-execute-phase --gaps-only`.
- `crates/devflow-core/src/agents/mod.rs:99-108` tests that every adapter receives identical prompt text.

Why this is Codex-incompatible:

The user confirmed the dogfooding failure: generic `/gsd-*` labels were passed to Codex as literal shell commands. The current architecture still makes raw slash-command labels a core invariant and does not let the Codex driver render a Codex-native instruction such as "invoke the GSD skill/workflow through Codex skill routing" with fallback behavior when the skill is unavailable.

Required fix shape:

Move command semantics out of `Stage::gsd_command()` and into a driver-owned prompt renderer. The pipeline should ask for `StageIntent::Code { phase }`; each driver renders that into its own supported surface.

### HIGH: Typed Subagent Capability Is Not Explicitly Enabled or Detected by DevFlow

Status: Confirmed defect for portable Codex runs.

Evidence:

- Clean `CODEX_HOME` `codex features list` shows `multi_agent_v2 stable false`.
- Repository `.codex/config.toml:10-14` enables `multi_agent = true`, `hooks = true`, and `goals = true`, but not `multi_agent_v2`.
- User `~/.codex/config.toml:6-9` enables `multi_agent_v2 = true`; DevFlow therefore relies on global user state.
- `crates/devflow-core/src/agents/codex.rs:21-48` builds `codex exec --sandbox workspace-write --json ...` but never adds `--enable multi_agent_v2` or detects feature state.

Why this matters:

The confirmed finding says `multi_agent_v2` must be explicitly enabled or capability-detected for typed GSD subagents. Codex 0.145.0 has the feature, but disabled-by-default behavior means a clean machine or CI run will not behave like this operator's user config.

Required fix shape:

Codex driver capability discovery should parse `codex features list` and fail or degrade when `multi_agent_v2` is unavailable. For stages that require typed GSD subagents, the command builder should pass `--enable multi_agent_v2` unless an operator explicitly disables typed subagents.

### HIGH: Codex Health Is Reduced to Binary Existence

Status: Confirmed insufficiency.

Evidence:

- `crates/devflow-cli/src/commands.rs:1420-1447` implements `devflow doctor` environment checks with `cmd_check("codex", "codex", "--version", ...)`.
- `crates/devflow-cli/src/preflight.rs:55-91` only validates that the agent binary resolves.
- Local `codex doctor` reports auth/config loaded, but provider reachability failure and stale app-server state.
- Minimal `codex exec` probes failed before a turn with an app-server initialization error.

Why this is Codex-incompatible:

`codex --version` can pass while `codex exec` cannot start or cannot reach the provider. For DevFlow, that failure happens inside a detached monitor and is later reduced to exit-code or generic failure handling. Codex needs a launch health protocol that distinguishes "binary installed" from "headless execution usable".

Required fix shape:

Codex driver preflight should expose `DriverHealth` with at least binary, config parse, auth, provider reachability, app-server/runtime readiness, and feature availability. DevFlow can still gate fail-soft, but the reason should be Codex-specific and actionable.

### HIGH: Current Adapter Contract Is Too Thin for Codex

Status: Confirmed architecture defect.

Evidence:

- `crates/devflow-core/src/agents/mod.rs:10-59` exposes only `name`, `exec_command`, `extra_env`, `completion_signal_detected`, and `preflight`.
- `completion_signal_detected` is implemented as `false` for Codex, Claude, and OpenCode (`codex.rs:65-67`, `claude.rs:33-35`, `opencode.rs:23-25`).
- Codex-specific output parsing lives in `crates/devflow-core/src/agent_result.rs:361-453`, not behind the Codex adapter.
- Codex-specific sandbox roots are threaded through a generic `extra_writable_roots` parameter (`agents/mod.rs:18-31`) and command-built manually in `codex.rs:32-47`.

Why this matters:

Codex compatibility is distributed across prompt rendering, preflight, command args, global completion parsing, docs, and tests. That makes it difficult to preserve other drivers while adapting Codex to 0.145.0 behavior.

Required fix shape:

Introduce a driver API with separate extension points for capabilities, command construction, prompt rendering, lifecycle, completion parsing, sandbox/environment needs, and contract tests.

### MEDIUM: Codex Command Builder Does Not Pin Approval Policy

Status: Confirmed risk / likely defect for unattended runs.

Evidence:

- `codex --help` exposes `--ask-for-approval never`.
- `codex doctor` reported effective approval policy `OnRequest`.
- `crates/devflow-core/src/agents/codex.rs:21-48` does not pass `--ask-for-approval never`.

Why this matters:

DevFlow's monitor is unattended and runs with `stdin` null. If Codex inherits `OnRequest`, any command requiring approval can fail, stall, or produce a tool-level error that DevFlow later interprets indirectly.

Recommendation:

Codex driver should set an explicit noninteractive approval policy for DevFlow-owned runs, likely `--ask-for-approval never`, and document the security tradeoff alongside the sandbox policy.

### MEDIUM: Codex Sandbox Uses Raw TOML Override Instead of First-Class CLI Paths

Status: Recommendation.

Evidence:

- `codex exec --help` supports `--add-dir <DIR>`.
- `crates/devflow-core/src/agents/codex.rs:32-47` builds `-c sandbox_workspace_write.writable_roots=[...]` by manual TOML string escaping.
- `crates/devflow-cli/src/preflight.rs:33-53` computes the main `.git` and linked worktree admin dir as extra writable roots.

Why this matters:

The current implementation may work, but it bypasses the CLI's typed path interface and keeps string-escaping logic in DevFlow. A Codex driver command builder should prefer `--add-dir` if it grants equivalent writable access in 0.145.0, falling back to `-c` only when needed and covered by tests.

### MEDIUM: Completion Parser Is Not Driver-Owned and Lacks Live Contract Coverage

Status: Confirmed insufficiency.

Evidence:

- Codex JSONL parser is global in `agent_result.rs:361-453`.
- The parser comment at `agent_result.rs:387-391` says it was written against a documented schema and "not yet verified" against the installed Codex CLI version.
- Tests at `agent_result.rs:1483-1575` use hand-authored JSONL fixtures for `turn.failed`, `turn.completed`, and `agent_message`.
- A live Codex JSONL smoke probe could not reach a turn in this environment due app-server startup failure.

Why this matters:

The hand-authored tests are valuable, but they are not a contract against Codex 0.145.0. Completion parsing should be a driver component with golden fixtures captured from real Codex versions, plus graceful "unknown event schema" diagnostics.

### MEDIUM: Fresh Codex Define Preflight Exists, but Codex Plan/Code Interactivity Is Still Only a Warning

Status: Confirmed risk.

Evidence:

- `crates/devflow-cli/src/commands.rs:135-142` hard-fails fresh Codex start when no `CONTEXT.md` exists.
- `crates/devflow-cli/src/commands.rs:143-148` only warns when no `PLAN.md` exists: "headless codex planning is untested and may need input".
- `crates/devflow-cli/src/preflight.rs:99-125` gates only `Codex + Auto + Define + missing CONTEXT.md`.

Why this matters:

Codex headless interactivity limitations are known for Define. Plan may also invoke GSD planning flows with interviews, advisors, or typed subagents. Until capabilities and prompt rendering are driver-owned, warning-only behavior can still burn unattended runs.

Recommendation:

Codex driver should declare per-stage `InteractivityMode`: `HeadlessSafe`, `RequiresExistingArtifact`, `RequiresTypedSubagents`, or `InteractiveOnly`. Pipeline preflight should consume that instead of hardcoding Define only.

### LOW: Documentation Overstates Agent-Agnostic Prompt Sharing

Status: Confirmed documentation mismatch with target architecture.

Evidence:

- `README.md:62-67` says all agents implement the same trait and receive the same prompt text for a stage.
- `docs/architecture/agent-model.md:38-46` says all agents receive the same prompt via `stage_prompt()`.
- `docs/guides/adding-agent.md:44-46` tells new adapters not to bypass shared prompts.
- `ARCHITECTURE.md:87-119` documents the current thin adapter and shared GSD slash-command prompts.

Why this matters:

These docs encode the design that caused the Codex mismatch. They should be updated during migration to describe agent-neutral stage intents plus driver-specific rendering.

## Confirmed Good Codex Adaptations to Preserve

- `crates/devflow-core/src/agents/codex.rs:21-26` uses `codex exec --sandbox workspace-write --json`.
- `crates/devflow-core/src/agents/codex.rs:52-63` scopes unsigned git commit/tag behavior with `GIT_CONFIG_*`.
- `crates/devflow-cli/src/preflight.rs:33-53` computes linked-worktree git metadata writable roots.
- `crates/devflow-core/src/agent_result.rs:403-421` finds `DEVFLOW_RESULT` inside Codex `agent_message` items.
- `crates/devflow-core/src/agent_result.rs:424-435` does not treat bare `turn.completed` as success.
- `crates/devflow-core/src/agent_result.rs:181-227` avoids false-positive rate-limit detection from JSONL event content.
- `crates/devflow-cli/tests/phase7_cli.rs:861-901` covers fresh Codex Define preflight.

## Target Modular Driver Architecture

### Core Principle

The pipeline owns *what* should happen. Drivers own *how a specific agent surface executes it*.

Core should not store `/gsd-*`, `claude -p`, `codex exec`, JSONL schema assumptions, sandbox TOML keys, or native subagent syntax. It should store `StageIntent`, phase number, mode, worktree paths, and completion policy.

### Proposed Public/Core API

```rust
pub trait AgentDriver {
    fn id(&self) -> DriverId;
    fn display_name(&self) -> &'static str;

    fn discover(&self, ctx: &DriverContext) -> DriverDiscovery;
    fn health(&self, ctx: &DriverContext) -> DriverHealth;

    fn capabilities(&self, discovery: &DriverDiscovery) -> DriverCapabilities;
    fn render_prompt(&self, req: PromptRequest) -> Result<RenderedPrompt, DriverError>;
    fn build_command(&self, req: CommandRequest) -> Result<AgentCommand, DriverError>;
    fn parse_completion(&self, output: OutputCapture) -> CompletionParseResult;

    fn sandbox_requirements(&self, req: SandboxRequest) -> SandboxRequirements;
    fn environment(&self, req: EnvironmentRequest) -> Vec<(String, String)>;
    fn test_contract(&self) -> DriverTestContract;
}
```

### Key Types

`DriverCapabilities`:

- `headless_exec`
- `json_events`
- `last_message_output_file`
- `workspace_write_sandbox`
- `extra_writable_roots`
- `approval_policy_control`
- `typed_subagents`
- `skill_routing`
- `hooks_supported`
- `requires_network`
- `supports_live_health_probe`

`StageIntent`:

- `Define { phase }`
- `Plan { phase }`
- `Code { phase, fix: Option<FixType> }`
- `Validate { phase }`
- `Ship { phase, review_angles }`

`RenderedPrompt`:

- `body`
- `completion_contract`
- `expected_artifacts`
- `requires_interactive_input`
- `requires_typed_subagents`
- `driver_notes`

`AgentCommand`:

- `program`
- `args`
- `cwd`
- `stdin_policy`
- `stdout_policy`
- `stderr_policy`
- `env`
- `kill_policy`
- `health_probe`

`CompletionParseResult`:

- `Completed(AgentResult)`
- `Failed(AgentResult)`
- `RateLimited { retry_after }`
- `NoMarkerButNativeFailure { reason }`
- `NoDecisiveSignal { diagnostics }`
- `UnknownSchema { event_types }`

### Codex Driver Requirements

Codex driver should:

- Parse `codex --version` and require a supported semver range, initially `>=0.145.0`.
- Parse `codex features list` and require or enable `multi_agent_v2` when typed GSD subagents are needed.
- Pass `--enable multi_agent_v2` for Codex stages that require typed subagents.
- Pass `--json`.
- Pass explicit sandbox and approval policy, e.g. `--sandbox workspace-write --ask-for-approval never`.
- Prefer `--cd <worktree>` over monitor-only `cd` if it improves Codex's workspace-root handling.
- Prefer `--add-dir <path>` for writable roots when verified equivalent to `sandbox_workspace_write.writable_roots`.
- Capture and parse Codex JSONL event streams in the Codex driver, not globally.
- Use `--output-last-message` if it gives a stable final-message channel for `DEVFLOW_RESULT`.
- Run a bounded `codex doctor --json` or equivalent health probe before long unattended stages.
- Render GSD workflow prompts in Codex-native language, not raw shell-like `/gsd-*` commands.

### Pipeline / State Machine Contract

Pipeline remains driver-neutral:

- `State` stores `agent: DriverId`, `stage`, `phase`, `mode`, `worktree_path`, monitor/gate counters, and no driver-specific feature flags.
- `Stage` stores no GSD command strings.
- `launch_stage` asks the selected driver for render, sandbox requirements, environment, and command.
- `advance` asks the selected driver to parse Layer 1 completion, then applies shared outcome policy.
- Gate decisions, retry counters, worktree lifecycle, and hook dispatch stay in DevFlow core/CLI.

## Migration Plan

1. Add driver API alongside existing `AgentAdapter`.
2. Implement `LegacyDriverAdapter` wrapping the current Claude/OpenCode behavior and current Codex behavior byte-for-byte.
3. Replace `Stage::gsd_command()` with `StageIntent`; keep old strings in a legacy prompt renderer during transition.
4. Move `prompt.rs` logic into `drivers/*/prompt.rs`, keeping current output for Claude/OpenCode initially.
5. Move Codex JSONL parsing from `agent_result.rs` into `CodexDriver::parse_completion`; keep shared fallback marker parser as a utility.
6. Add `CodexDriver::discover()` and `CodexDriver::health()` with tests for clean `CODEX_HOME`, project config, and user config.
7. Update Codex command builder to enable/detect `multi_agent_v2`, set approval policy, and use first-class CLI args where verified.
8. Add driver conformance tests and captured Codex 0.145.0 JSONL fixtures.
9. Update docs to describe stage intents and driver-specific prompt rendering.
10. Remove `completion_signal_detected` once all drivers use the new parser protocol.

## Risks

- Enabling `multi_agent_v2` explicitly can change Codex tool schema shape; tests must pin the expected typed subagent behavior.
- `--add-dir` may not be exactly equivalent to `sandbox_workspace_write.writable_roots` for linked worktree git metadata; verify before replacing the TOML override.
- `codex doctor` can fail due transient network restrictions; health should distinguish hard local misconfiguration from temporary provider reachability.
- Driver-specific prompts can drift semantically across Claude, Codex, and OpenCode; use shared `StageIntent` acceptance tests to keep behavior equivalent.
- Keeping old and new adapter paths during migration risks double maintenance; put a deprecation date on `AgentAdapter`.

## Validation Matrix

| Area | Validation | Expected Evidence |
|---|---|---|
| Codex version | `codex --version` parsed by driver | supported semver or actionable failure |
| Feature defaults | clean `CODEX_HOME` `codex features list` | `multi_agent_v2` false unless explicitly enabled |
| Feature override | driver command includes `--enable multi_agent_v2` when required | command-builder unit test |
| Project config | `.codex/config.toml` without global config | driver does not rely on user config for typed subagents |
| Headless approval | Codex command sets `--ask-for-approval never` or documented equivalent | command-builder unit test |
| Sandbox roots | linked worktree can commit under Codex sandbox | integration probe with real worktree |
| Completion success | real Codex JSONL with agent-message `DEVFLOW_RESULT: success` | golden fixture + parser test |
| Completion failure | real Codex JSONL `turn.failed` and marker failure | golden fixture + parser test |
| Marker-less completion | real or fixture `turn.completed` without marker | does not auto-advance |
| Rate limit | Codex plain-text and JSONL rate-limit cases | classified `RateLimited`, no doc-content false positives |
| Health probe | `codex doctor --json` or bounded exec probe | binary/config/auth/reachability/app-server reported separately |
| Prompt rendering | Codex Code prompt contains no raw shell-like `/gsd-*` execution instruction | snapshot test |
| Legacy preservation | Claude/OpenCode prompts and command args unchanged during first migration stage | snapshot tests |
| Docs | adding-agent guide describes drivers and stage intents | doc check |

## Final Recommendation

Current Codex compatibility posture: REQUEST CHANGES before treating Codex as a reliable first-class DevFlow driver.

The highest-value next phase is not another point fix in `prompt.rs`; it is the driver API migration. Codex-specific patches should land behind `CodexDriver` so the pipeline remains stable and other drivers preserve behavior while Codex catches up to the locally installed CLI's real feature, health, sandbox, and completion contracts.
