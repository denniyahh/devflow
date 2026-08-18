# Adversarial Review — Phase 37.1 deliverables (spike + NOT VIABLE decision gate)

**Targets:** `37.1-RESEARCH.md` · `37.1-VALIDATION.md` · `37.1-PATTERNS.md` · `37.1-DECISION-GATE.md`
**Reviewers:** codex (gpt-5.6-terra, high) · antigravity (gemini-3.7-flash-high, --print-timeout 15m). No claude this run, per operator instruction.
**Date:** 2026-08-17
**Review root:** `.worktrees/phase-37.1` (feature/phase-37.1 @ cef4059) — both reviewers read the live Pi 0.84.1 binary + bundled package source and the repo code at the Phase-37 commit.

## Verdict: REVISE — both reviewers, independently, on the same core defect

The decision gate's `NOT VIABLE` verdict is an **overgeneralization**. The research actually
establishes a *narrow* technical fact — Pi cannot be routed through Claude's existing
`PipeOwning`/`CloseRule` machinery — and the decision gate then converts that into a broad,
milestone-level veto ("no defensible full-dispatch arm"), without any evidence excluding a
full-dispatch design that uses a *new* monitor rather than Claude's protocol. The second,
independent load-bearing flaw: the gate declares itself "all code-verified" and closed, while the
one assumption the verdict rests on (A1 — the extension-tool event shape) is recorded in the
research as "not inspected directly — inferred", and the validation strategy is still
`status: draft` / `nyquist_compliant: false` / `Approval: pending` with zero negative controls.

## Cross-review consensus (both reviewers, code-verified)

1. **BLOCKER — verdict overgeneralization.** `DECISION-GATE.md:4` ("no defensible full-dispatch
   arm") vs `RESEARCH.md:43` ("NOT VIABLE … that **reuses DevFlow's existing
   PipeOwning/CloseRule machinery**"). The code supports only the narrow claim: `PipeOwning` is
   Claude's stdin-turn protocol (`monitor.rs`), Pi waits for stdin EOF (`main.js:48-63`), and
   `pipeline_launch.rs` routes only Claude to `PipeOwning`. Nothing forbids a Pi-specific
   process-exit/tailing monitor. The broad veto is a scope/cost decision dressed as technical
   impossibility.

2. **BLOCKER — gate asserts verification the evidence does not support.** A1 is
   "not inspected directly — inferred" (`RESEARCH.md:323`), yet `DECISION-GATE.md:12` says
   "all code-verified" and treats the inferred event shape as fact. `VALIDATION.md` is
   `draft`/`nyquist_compliant: false`/`pending` with no negative control, yet the gate is marked
   closed. No smoke test, no falsifiable check.

3. **HIGH — "`--mode json` is single-shot, not a stream" is an equivocation.** Pi's own
   `print-mode.js` describes `--mode json` as a "JSON event stream" and emits NDJSON; the real
   (narrower) fact is the stdin-EOF-vs-held-open deadlock, which only kills the `PipeOwning`
   transport, not the existence of an observable event stream.

4. **HIGH — the documented parser seam is not a real dispatch path.** `RESEARCH.md:60` maps Pi
   completion to `AgentDriver::parse_completion`, but that trait method has a default impl, only
   Codex implements it, and no production caller invokes it — Layer 1 hard-codes Claude/marker/
   Codex parsers (`agent_result.rs:1834-1841`). Phase 39 needs a Layer-1 wiring decision + tests,
   not just a parser.

## Divergent / single-reviewer findings

- **antigravity BLOCKER 1 (the strongest technical attack):** the bundled `subagent` extension's
  `execute()` **synchronously awaits** child processes (`Promise.all(workers)`) before returning
  the tool result, so under `MonitorLaunch::Legacy` process-exit supervision the parent `pi` stays
  alive until subagents finish. If true, subagent dispatch needs **no new drain gate at all** —
  the verdict's phantom "DevFlow can't see the children" requirement is wrong, and the NOT VIABLE
  conclusion collapses to "we'd rather not build the monitor", which is a cost call, not viability.
- **codex HIGH 5:** the "install at user scope" safety control is unenforceable — the extension
  accepts model-supplied `agentScope` (`"project"`/`"both"`) and its confirmation only runs when
  `ctx.hasUI`, so under `--no-approve` a global extension can still execute repo-controlled
  `.pi/agents`. Needs a negative test (headless `agentScope:"both"` must refuse).
- **antigravity HIGH 4:** the "comparison doc" deliverable was bypassed — all 6 third-party
  packages were dismissed on metadata heuristics with zero source read.
- **antigravity MEDIUM 5:** `PATTERNS.md` places `parse_pi_result` in `agents/pi.rs` while
  `RESEARCH.md` places it in `agent_result.rs` (Layer 1) — conflicting implementation targets.
- **antigravity MEDIUM 6:** the "read/bash/edit/write only" claim misquotes `pi --help`, which
  lists 7 built-in tools (`grep`/`find`/`ls` additionally).
- **antigravity LOW 7:** `RESEARCH.md` says follow-on scheduling is "a roadmap call, not a
  research one" (open for Phase 39 discuss), but `DECISION-GATE.md` pre-emptively moves full
  dispatch to the backlog and declares baseline the "only" arm.

## Reviewer status

- **codex** — success (gpt-5.6-terra, high; emitted findings twice, deduplicated).
- **antigravity** — success (gemini-3.7-flash-high; MCP linear/vercel disabled during the run, restored after).

## Recommended path (for Dennis)

The review does **not** overturn the operational outcome (Phase 39 baseline-first is still the
safe default), but the *verdict as written* is not defensible and should be rewritten before it is
treated as a locked gate:

1. **Reword the verdict to the narrow claim** — "full dispatch via the *existing*
   `PipeOwning`/`CloseRule` machinery is not viable" — and either (a) re-open full dispatch as a
   Phase 39 discuss-phase roadmap decision (per `RESEARCH.md:335-338`'s own words), or (b) record
   it explicitly as a **cost/scope decision**, not a technical impossibility.
2. **Resolve A1 before the gate is treated as closed** — the 15-minute smoke test the research
   itself names, or explicitly downgrade A1 to a blocking open question (no "all code-verified"
   claim).
3. **Bring `VALIDATION.md` out of draft** with at least one negative control, or stop claiming the
   gate is closed.
4. **Fix the factual claims** — `--mode json` event-stream wording; the 7-tool surface;
   `parse_pi_result` location contradiction.
