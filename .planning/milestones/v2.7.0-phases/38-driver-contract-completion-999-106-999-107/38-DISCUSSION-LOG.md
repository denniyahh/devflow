# Phase 38: Driver Contract Completion (999.106 + 999.107) — Discussion Log

**Gathered:** 2026-08-17
**Mode:** headless auto-discuss (`--auto --analyze`) — the operator (Dennis) directed that
phase 38's planning be preceded by a CONTEXT.md rather than run blind off the ROADMAP, and no
operator was available to answer an interactive interview, so gray areas were resolved against
the live source and the Phase-38 review evidence rather than by questionnaire.

## Areas explored and how each was resolved

### 1. Removal scope — exactly what gets deleted (999.106)
**Question:** Does "remove AgentAdapter + DriverShim" include the four legacy `*Agent` structs,
and what replaces `adapter_for()`?

**Resolved:** Yes — `AgentAdapter`, `DriverShim`, `ClaudeAgent`/`CodexAgent`/`OpenCodeAgent`/
`PiAgent` all go; `adapter_for()` is replaced by `driver_for() -> Box<dyn AgentDriver>`. The
four `*Driver` unit structs already implement the full contract and survive. (Grounded in
`agents/mod.rs`; this is the "conditional removal" 37 D-11 deferred.)

### 2. Call-site completeness — are the five named sites the whole story?
**Question:** 999.106 names five sites (canary.rs:40, test_support.rs:205/244, preflight.rs:1266,
pipeline_launch.rs:190/204). Is that exhaustive?

**Resolved:** No — found two more live uses that must be handled before `ClaudeAgent` can be
deleted: `ClaudeAgent::exec_command_single_document` (`pipeline_launch.rs:208`, the D-11 legacy
opt-out) and `ClaudeAgent::exec_resume_command` (`pipeline_launch.rs:1048`, the checkpoint-resume
path). Both are inherent methods with no `AgentDriver` counterpart and must be relocated
byte-for-byte. This is recorded as D-02's second half and flagged as the regression-sensitive
heart of the phase. (Grounded in a grep over `crates/**/*.rs` for the four struct names.)

### 3. InteractivityMode consumption — how far does the driver-driven gate extend?
**Question:** The hardcoded `agent == AgentKind::Codex` checks gate Define only. Codex declares
Define **and** Plan → `RequiresExistingArtifact`. Extend to Plan?

**Resolved:** Extend. The gate becomes agent-agnostic (`driver.interactivity_mode(stage)`), and
honoring the driver's full per-stage declaration is the entire point of the change. 999.106's
"Define/Plan path" wording corroborates. A HeadlessSafe declaration is never refused.

### 4. Parser ordering (999.107 #1) — what is the correct precedence?
**Question:** Can an earlier `agent_message` success marker override a later `turn.failed`?

**Resolved:** No. Terminal `turn.failed` must take precedence over any earlier success marker.
The current code returns the marker before reading the terminal event; the fix reorders and adds
a `success + turn.failed` negative test (existing coverage only has `success + turn.completed`).

### 5. Writable-root serialization (999.107 #2) — what counts as hostile?
**Question:** Which paths break the current `\`/`"`-only escaping?

**Resolved:** Non-UTF-8 paths (become `�`) and newline/control-containing paths (produce invalid
TOML). The fix hardens serialization; a hostile-path fixture (both cases) is required.

## Deferred ideas (captured, not acted on)

- Pi end-to-end → Phase 39 (gated on this phase's driver-driven gate).
- Antigravity-cli → future (999.32).
- Hermes → future (999.1).
- 999.94 → later.
- 999.105 (default adversarial review gate) → separate backlog item.

## Operator decisions made outside this log

- Dennis directed: stop phase 38's planning agent, produce this CONTEXT first, then relaunch
  planning with it. (Phase 37.1 was left to proceed as-is.)
