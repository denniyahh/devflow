---
status: backlog
source: .planning/audits/2026-07-24-codex-compatibility-review.md, refined
  in operator conversation 2026-07-24
---

# Backlog: Modular Agent Driver Architecture

> Filed from the Codex compatibility audit (`.planning/audits/2026-07-24-codex-compatibility-review.md`),
> written after a confirmed dogfood failure: generic `/gsd-*` labels reached
> Codex as literal shell commands during an attempted Phase 22 light
> dogfooding trial (see `phases/22-concurrency-governance-correctness/22-CONTEXT.md`).
> The audit's root-cause finding: `Stage::gsd_command()` bakes a raw
> slash-command string into core (`crates/devflow-core/src/stage.rs`), and
> `prompt.rs` renders it identically for every adapter — an assumption that
> happened to hold for Claude/OpenCode and is false for Codex. Confirmed
> against live source 2026-07-24 (see audit for exact evidence lines).

## Goal

Replace the thin `AgentAdapter` trait (`crates/devflow-core/src/agents/mod.rs`)
with a modular `AgentDriver` contract so each agent surface owns its own
prompt rendering, command building, completion parsing, and health/capability
discovery — instead of that logic being scattered across `prompt.rs`,
`agents/*.rs`, `agent_result.rs`, and `preflight.rs` with a shared-prompt
invariant enforced by tests (`agents/mod.rs::every_adapter_receives_identical_prompt_text`).

**Operator intent (2026-07-24):** this is not a 3-driver-forever system —
Antigravity and Hermes are both committed next agents (see 999.32 and 999.1),
with more expected after. Given that, the fuller driver-contract investment
(capability discovery, health probes, a shared conformance test suite) is
justified now rather than deferred, on the reasoning that interface changes
get more expensive the more drivers implement against them.

**Depends on:** none — unblocks 999.32 (Antigravity) and should be the
implementation vehicle for 999.1 (Hermes) once promoted.

Promote with `/gsd-review-backlog` when ready.

---

## Design Decisions (locked)

- **D-01 — Capability flags are enumerated on an as-needed basis, not
  upfront.** Do not try to guess the full `DriverCapabilities` axis set
  (e.g. `hooks_supported`, `skill_routing`) from agents not yet integrated.
  Ship the flags today's real drivers (Claude, Codex, OpenCode) actually
  need; add a flag only when a concrete new driver (Antigravity, Hermes, or
  later) demonstrates the gap. Make the capabilities struct cheaply
  extensible (`#[non_exhaustive]` + `Default`, or equivalent) so adding a
  field never breaks existing drivers — this is what makes "decide later"
  safe instead of a future breaking change.
- **D-02 — Prove the contract with a second native implementation before
  calling it stable.** Don't design the trait against Codex alone and
  legacy-wrap Claude/OpenCode indefinitely. At least one of Claude/OpenCode
  should get a full native driver implementation (not a legacy wrapper) in
  the same pass that lands Codex's, so the interface is validated against
  two real, structurally different agents, not one.
- **D-03 — Sequence the fix ahead of the framework.** The concrete bug
  (raw `/gsd-*` strings reaching Codex) ships first via `StageIntent` +
  driver-owned prompt rendering (31a below). The fuller discovery/health/
  conformance machinery (31c) is real scope, not gold-plating, given D-01's
  premise — but it should not block 31a/31b.
- **D-04 — Put a deprecation date on `AgentAdapter`.** Once the driver
  migration lands for Claude, Codex, and OpenCode, remove the old trait in
  the same phase or the next one. Do not let both paths persist across
  multiple phases (audit's own risk list flags this).

## Units

### 31a — `StageIntent` + Driver-Owned Prompt Rendering

The actual dogfood-breaking defect. Fixes:

- [ ] Remove `Stage::gsd_command()` from core; replace with a
  `StageIntent` enum (`Define { phase }`, `Plan { phase }`,
  `Code { phase, fix: Option<FixType> }`, `Validate { phase }`,
  `Ship { phase, review_angles }`) carrying no agent-specific syntax.
- [ ] Move `prompt.rs`'s rendering logic into per-driver `render_prompt`
  implementations. Claude/OpenCode initially render the same text they do
  today (behavior-preserving); Codex renders a Codex-native instruction
  instead of a raw shell-like `/gsd-*` label.
- [ ] Snapshot tests: Claude/OpenCode prompt text unchanged; Codex prompt
  contains no raw `/gsd-*` execution instruction.
- [ ] Retire `every_adapter_receives_identical_prompt_text` — replace with
  a `StageIntent`-level acceptance test that checks semantic equivalence,
  not byte-identical text.

### 31b — Codex Driver Hardening

Cheap, high-value fixes independent of the full migration — can land as
soon as a `CodexDriver` exists to own them:

- [ ] Parse `codex features list`; require or pass `--enable multi_agent_v2`
  for stages needing typed GSD subagents (confirmed disabled by default in
  a clean `CODEX_HOME`; project `.codex/config.toml` does not enable it).
- [ ] Pass an explicit non-interactive approval policy
  (`--ask-for-approval never`) — `codex doctor` reported effective policy
  `OnRequest`, and the DevFlow monitor runs unattended with `stdin` null.
- [ ] Move Codex JSONL completion parsing from the global
  `agent_result.rs:361-453` into `CodexDriver::parse_completion`; capture
  golden fixtures from the actual installed Codex version rather than only
  hand-authored ones.
- [ ] Prefer `--add-dir <path>` over the current hand-escaped
  `-c sandbox_workspace_write.writable_roots=[...]` TOML override, once
  verified equivalent for linked-worktree git metadata roots.

### 31c — Driver Contract + Conformance Suite

The part that actually makes "plug and play" true for Antigravity/Hermes
and whatever comes after:

- [ ] Define the `AgentDriver` trait: `discover`, `health`, `capabilities`,
  `render_prompt`, `build_command`, `parse_completion`,
  `sandbox_requirements`, `environment`, `test_contract`.
- [ ] `DriverHealth` distinguishing binary-installed from headless-execution-usable
  (binary / config parse / auth / provider reachability / runtime readiness /
  feature availability) — Codex's `codex --version` can pass while
  `codex exec` cannot start.
- [ ] Shared conformance test suite (`test_contract()`) every driver must
  pass — this is the artifact a future Antigravity/Hermes driver
  implements against, not prose docs.
- [ ] Per-stage `InteractivityMode` (`HeadlessSafe`, `RequiresExistingArtifact`,
  `RequiresTypedSubagents`, `InteractiveOnly`) consumed by pipeline
  preflight generically, replacing the hardcoded Codex-Define-only
  `CONTEXT.md` check in `commands.rs`.

### 31d — Docs + Cleanup

- [ ] Update `README.md`, `docs/architecture/agent-model.md`,
  `docs/guides/adding-agent.md`, `ARCHITECTURE.md` — replace "all agents
  receive the same prompt" with stage-intents + driver-specific rendering.
  These docs currently encode the design that caused the Codex mismatch.
- [ ] Remove `AgentAdapter` and `completion_signal_detected` once Claude,
  Codex, and OpenCode all run on `AgentDriver` (D-04).

## Risks (carried from audit)

- Enabling `multi_agent_v2` explicitly can change Codex tool schema shape;
  pin expected typed-subagent behavior in tests.
- `--add-dir` may not be exactly equivalent to
  `sandbox_workspace_write.writable_roots` for linked-worktree git
  metadata — verify before replacing the TOML override.
- Driver-specific prompts can drift semantically across agents over time;
  the `StageIntent`-level acceptance tests (31a) are what keeps that
  drift visible.

## Required References

- `.planning/audits/2026-07-24-codex-compatibility-review.md` — full
  evidence, severity-ranked findings, validation matrix.
- `crates/devflow-core/src/stage.rs`, `prompt.rs`, `agents/mod.rs`,
  `agents/codex.rs`, `agent_result.rs`, `crates/devflow-cli/src/preflight.rs`.
- `.planning/phases/22-concurrency-governance-correctness/22-CONTEXT.md` —
  the light dogfooding trial whose Codex run surfaced this.
