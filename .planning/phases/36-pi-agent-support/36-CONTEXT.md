# Phase 36: Pi Agent Support + Release-Preflight Hardening - Context

**Gathered:** 2026-08-15
**Status:** Ready for planning

## Phase Boundary

DevFlow gains a fourth first-class agent adapter — **Pi** (the Pi coding-agent harness) — so a
`devflow start` run can drive Pi headless alongside Claude, Codex, and OpenCode. Three small
release/trust gaps close in the same phase because they sit in code this phase already touches:
999.67 (agent result parsing lets an agent plant its own Layer-0 provenance), 999.96
(`release --check` can't catch a forgotten version bump), and 999.104 (release-signing key
workflow). The full modular `AgentDriver` refactor is explicitly **Phase 37** (backlog 999.31),
not here — Phase 36 uses the existing `AgentAdapter` surface.

## Requirements (locked via SPEC.md)

**4 requirements are locked.** See `36-SPEC.md` for full requirements, boundaries, and acceptance
criteria. Downstream agents MUST read `36-SPEC.md` before planning or implementing.

**In scope (from SPEC.md):** Pi `AgentAdapter` + `AgentKind::Pi` (Code-stage first); 999.67 (XS,
`parse_devflow_result` overwrite); 999.96 (S, `release --check` version-bump row); 999.104
(one-line signing probe + preflight fingerprint check).

**Out of scope (from SPEC.md):** the `AgentDriver` contract / conformance suite (Phase 37); Pi
prompt-native rendering (behavior-preserving initially); `{N}-VERIFICATION.md` artifact changes;
any upstream GSD dependency.

## Implementation Decisions

### Pi stage coverage
- **D-01:** Code-stage vertical slice first. Prove the transport + completion parsing end-to-end
  on one stage before widening. The other four stages are the same prompt-wrapping shape, so they
  are additive once Code works. — **Reversibility:** reversible — widening stage coverage is
  purely additive.

### Release-signing key (999.104)
- **D-02:** One-line probe fix **plus** surfacing the fingerprint check at preflight (the
  decision-3 companion). The probe alone fixes *detection*; the preflight fingerprint check is
  what turns "silent until push" into "fails at preflight." — **Reversibility:** reversible —
  a later two-key-model rework would replace, not conflict with, this check.
- **D-03:** The two-key-model rework (999.104 decision 2) is **deferred** to a follow-up backlog
  entry, not attempted here. It is a workflow redesign (M), not a bug fix, and does not belong in
  a phase whose main event is the Pi adapter.

### Pi interface (established from Pi docs v0.84.1)
- **D-04:** Pi's interface is established from its own docs (see Canonical References), not
  assumed. Key facts recorded here because the doc path is environment-specific:
  - Binary: `pi`.
  - Headless modes: `--mode json` (all session events as JSON lines to stdout — first line is a
    `{"type":"session",...}` header, then `agent_start` / `turn_start` / `message_start` /
    `message_update` (deltas) / `message_end` / `turn_end` / `tool_execution_*` / `agent_end`);
    `-p` / `--print` (print final response and exit); `--mode rpc` (stdin/stdout RPC).
  - Completion signal: the `agent_end` event (JSON mode), or process exit (`-p` mode).
  - Project trust: non-interactive modes never prompt; they consult `defaultProjectTrust`
    (default `ask` → project resources ignored unless approved), overridable per-run with
    `--approve` / `--no-approve`.
  - Sessions: `--no-session` for ephemeral runs; `-c` / `-r` / `--session` for resume.
  - Exact flag selection (json vs. print; approve vs. no-approve; no-session vs. session) and
    exit-code semantics are **plan-phase** decisions — the planner verifies them against the
    installed `pi` binary and pins golden fixtures (mirroring how Codex fixtures are captured from
    the installed version).

### Bundling (999.67 + 999.96)
- **D-05:** Both stay in Phase 36. 999.67 is XS (one-line overwrite + mirror test in
  `agent_result.rs`, the file Pi's completion parsing touches); 999.96 is S (one `release --check`
  row + test, the path 999.104's probe touches). Confirmed by the operator — no wave overhead.

### the agent's Discretion
- Pi's human-readable adapter `name()` string (e.g. `"Pi"` vs `"Pi Coding Agent"`) — mirror the
  existing convention (`"Claude Code"`, `"OpenAI Codex"`, `"OpenCode"`) and pick a stable value.
- Whether the `-p` or `--mode json` transport is used for the first cut, pending the plan-phase
  verification of exit codes and drain-gate mapping.

## Canonical References

Downstream agents MUST read these before planning or implementing.

### Phase requirements
- `36-SPEC.md` — locked requirements, boundaries, acceptance criteria. MUST read first.

### Agent architecture
- `.planning/phases/999.31-agent-driver-modularization/CONTEXT.md` — the Phase 37 target; its
  D-02 ("prove the contract against a second native implementation") is why Pi lands first.
- `docs/guides/adding-agent.md` — how a new agent is added (currently encodes the shared-prompt
  assumption this phase works within).
- `docs/architecture/agent-model.md` — the agent/adapter model.

### Pi interface (source docs, environment-specific paths)
- Pi `docs/usage.md` § "CLI Reference" / "Modes" — `pi` CLI flags, `--print`, `--mode json`,
  project-trust behavior.
- Pi `docs/json.md` — JSON event stream mode, event types, `agent_end` terminal event.
- Pi `docs/environment-variables.md` — env knobs (e.g. trust) if the adapter needs them.

### Backlog items in scope
- 999.67 (`.planning/ROADMAP.md`), 999.96, 999.104 — full prose in ROADMAP.md's Backlog section.

## Existing Code Insights

### Reusable Assets
- `crates/devflow-core/src/agents/claude.rs`, `codex.rs`, `opencode.rs` — three existing adapters
  to mirror; each is a small struct implementing `AgentAdapter`.
- `crates/devflow-core/src/agent_result.rs` `parse_marker_lines` / `parse_devflow_result` — the
  completion/result parser (999.67 edits this).
- The `{COMPLETION_PROTOCOL}` prompt fragment (`prompt.rs`) — the `DEVFLOW_RESULT` marker the
  agent writes on completion; the Pi prompt reuses it.

### Established Patterns
- Adapters only *format* the shared stage prompt into their CLI's flags; the prompt text itself is
  agent-agnostic (`prompt.rs`). A Pi adapter fits this: `exec_command` returns
  `("pi", vec![...])` with `--mode json` and the prompt positional.
- Golden completion fixtures are captured from the installed agent version, not hand-authored
  (the Codex pattern) — apply the same to Pi.

### Integration Points
- `crates/devflow-core/src/state.rs:387` — `AgentKind` enum; add `AgentKind::Pi`.
- `crates/devflow-core/src/agents/mod.rs` — `adapter_for(kind)` match; add the `Pi` arm and
  `pub mod pi;`.
- `crates/devflow-core/src/agent_result.rs:166,1590` — `parse_devflow_result` /
  `parse_marker_lines` (999.67).
- `crates/devflow-cli/src/commands.rs` — `release --check` and its tag-signing probe
  (999.96 + 999.104).
- `crates/devflow-core/src/monitor.rs` — the pipe-owning monitor; its drain gate (`CloseRule`)
  reads agent-specific events, so Pi's completion detection (`agent_end`) needs a Pi-aware arm.
