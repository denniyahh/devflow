# Phase 41: Antigravity Driver - Context

**Gathered:** 2026-08-19
**Status:** Ready for planning

## Phase Boundary

Phase 41 delivers the Antigravity driver: `devflow start --agent antigravity` launches the
Antigravity CLI headless and drives a stage to completion with honest completion detection. It
also closes two dogfood-hygiene items surfaced by the Phase 40 run — the leaked test monitors
(HYG-01) and the container git-env failures (HYG-02). Requirements: ANTG-01, ANTG-02, ANTG-03,
HYG-01, HYG-02.

## Implementation Decisions

### Agent binary & launch
- **D-01: The driver targets the `agy` binary.** `agy` is the operator's single, canonical
  Antigravity entry point — a shell wrapper (`exec antigravity-cli --dangerously-skip-permissions
  "$@"`, v1.1.15). The conflicting `antigravity` (1.1.13) and `agycli` binaries were uninstalled.
  The wrapper injects `--dangerously-skip-permissions` itself, so the driver argv must not add it again.
- **D-02: Stream-json launch, mirroring `ClaudeDriver`.** — **Reversibility:** costly — undo would
  re-derive completion parsing and relaunch wiring. `build_command` returns
  `agy -p --input-format stream-json --output-format stream-json`; the initial user turn is written
  to the child's stdin via `monitor::user_turn_line` (the Phase 31 stream-launch machinery Claude
  already exercises), and events are read back one JSON object per line on stdout.

### Completion detection
- **D-03: Parse the final stream-json `result` message for `DEVFLOW_RESULT`**, with honest
  process-exit as the fallback. A marker-less stream never advances a stage (ANTG-03) — a hard
  gate, regression-tested.

### Health / preflight
- **D-04: Presence-only health check.** `ensure_agent_binary` / `devflow doctor` report Antigravity
  as installed when `agy` is on `PATH`. No version floor, no capability probe. — **Reversibility:**
  reversible — adding a floor later is a small, additive change. Rationale (operator): "Unless there
  is a functional reason to floor the version, presence-only should be fine." The marker-less
  contract (D-03) is the functional backstop: a wrong/stale binary fails the run honestly rather
  than advancing.

### Prompt rendering
- **D-05: Reuse `render_claude_style`.** Antigravity is the Claude driver family (stream-json, same
  agentic loop); no dedicated renderer until a live probe shows the Claude framing is wrong.

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Scope
- `.planning/ROADMAP.md` § "Phase 41: Antigravity Driver" — goal, success criteria (ANTG + HYG).
- `.planning/REQUIREMENTS.md` — ANTG-01..03, HYG-01, HYG-02 definitions.

### Driver contract & patterns to mirror
- `crates/devflow-core/src/agents/claude.rs` — `ClaudeDriver` (stream-json `build_command`,
  `render_claude_style`) — the pattern D-02/D-05 mirror.
- `crates/devflow-core/src/agents/mod.rs` — `AgentDriver` trait, `driver_for`, shared conformance suite.
- `crates/devflow-core/src/state.rs` — `AgentKind` enum + `FromStr`/`Display`/`AgentParseError` (add the `Antigravity` variant).
- `ARCHITECTURE.md` § "Extension points — adding an agent" — the 7-step onboarding checklist.
- `crates/devflow-core/src/monitor.rs` — `user_turn_line` (stream-json initial turn on stdin).

### Research
- `.planning/research/STACK.md` — Antigravity CLI surface. **NOTE: the "binary-name resolution"
  section is superseded by D-01** (it still documents the old `agycli`/`antigravity`/`agy` split);
  the argv/flags table (`-p`, `--input-format stream-json`, `--output-format json|stream-json`,
  `--dangerously-skip-permissions`) is still accurate.

### Test pattern
- `crates/devflow-cli/tests/phase7_cli.rs` — stub-PATH + `ENV_MUTEX` regression-test pattern for
  stubbing an agent binary (marker-less / non-zero / hung cases).

## Existing Code Insights

### Reusable Assets
- `ClaudeDriver` — the stream-json launch to mirror (D-02).
- `render_claude_style` — prompt renderer to reuse (D-05).
- `monitor::user_turn_line` — the stdin turn writer.
- `DriverCapabilities` / `SandboxRequirements` / `DriverHealth` (`#[non_exhaustive] + Default`) —
  carry everything a new driver needs; no new crate deps (STACK.md).

### Established Patterns
- Marker-less never advances (Layer 1/2/3 completion machinery) — feeds D-03's regression test.
- `ensure_agent_binary` preflight fails loud when a configured agent binary is absent.

### Integration Points
- `AgentKind` variant + `FromStr`/`Display` (`state.rs`).
- `driver_for` match arm (`agents/mod.rs`).
- `agent_program` resolution (used by `ensure_agent_binary` + `devflow doctor`).

## Specific Ideas

No specific requirements — open to standard approaches.

## Deferred Ideas

- **Version floor / capability probe on `agy`** (GA-4 option A) — considered, not chosen. Revisit if
  `devflow doctor` accuracy matters or a stale binary regresses.
- **Update `research/STACK.md`'s binary-resolution section** to the single-`agy` reality — deferred
  to plan-phase.

---

*Phase: 41-Antigravity Driver*
*Context gathered: 2026-08-19*
