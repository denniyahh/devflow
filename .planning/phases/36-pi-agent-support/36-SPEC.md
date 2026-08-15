---
phase: 36
title: Pi Agent Support + Release-Preflight Hardening
status: draft (spec-phase, pre-discuss)
items: [Pi driver, 999.67, 999.96, 999.104]
---

# 36-SPEC — Pi Agent Support + Release-Preflight Hardening

## What this phase delivers (one sentence)

DevFlow can drive **Pi** (the Pi coding-agent harness) end-to-end as a fourth first-class agent
adapter, and three small release/trust gaps that sit in the same code close in the same phase.

## Why Pi first, 999.31 second

Backlog **999.31** (Modular Agent Driver Architecture) is the L-sized refactor that replaces the
thin `AgentAdapter` trait with a full `AgentDriver` contract. Its own locked decision **D-02** says
the contract must be proven against a second native implementation before it is called stable.
Landing Pi as a concrete new adapter now gives Phase 37 a live second consumer — and delivers
operator value (Pi support) without waiting on the L refactor. Phase 36 therefore uses the existing
`AgentAdapter` surface; Phase 37 lifts the whole thing onto `AgentDriver`.

## Scope

### A. Pi agent adapter (the main deliverable)

A new adapter for the Pi harness, registered in `AgentKind` / `adapter_for` alongside Claude,
Codex, and OpenCode.

Falsifiable requirements:

1. `devflow` can launch Pi for a Code-stage run and parse its completion, mirroring what it does
   for the existing three agents.
2. Pi's invocation (headless flags, stream format, completion signal, exit-code semantics) is
   established **from Pi's own docs/CLI — not assumed** — and recorded in the phase CONTEXT.
3. A discovery/health check distinguishes "Pi binary installed" from "Pi can execute headless",
   in the same shape as the existing three adapters' checks.
4. `devflow doctor` (or the equivalent preflight) reports Pi's presence/absence like the other
   adapters.

### B. 999.67 — Layer-0 provenance trust fix (XS)

`parse_devflow_result` overwrites an agent-planted `decided_by_layer` exactly as
`parse_claude_event_result` already does on the stream path, with a mirror test.

### C. 999.96 — `release --check` version-bump row (S)

A `release --check` row compares `CHANGELOG.md`'s top `## <version>` heading against the workspace
version, reporting NOT viable (with the disagreement direction stated) when they differ. Must carry
a negative control — the current tree is itself a ready-made positive fixture (changelog `2.5.0`
vs. workspace).

### D. 999.104 — release-signing key (default: one-line probe)

Default decision: repoint `release --check`'s tag-signing probe at
`devflow.releaseSigningKey` (the maintainer's key) rather than the agent's `user.signingkey`.
**Open decision — see "Open decisions" below; the two-key-model alternative is an explicit,
non-silent branch.**

## Boundaries / non-goals

- The full `AgentDriver` contract (capability discovery, driver-owned prompt rendering, shared
  conformance suite) is **Phase 37**, not here. Phase 36 uses the existing `AgentAdapter` surface.
- Pi's prompt text may initially render the same stage prompt as the other agents
  (behavior-preserving), unless Pi's interface requires otherwise.
- No change to the `{N}-VERIFICATION.md` artifact format; no upstream GSD dependency.

## Acceptance criteria (what must be TRUE)

1. A `devflow` run driven by Pi reaches a terminal completion, with its completion parsed and its
   result classified, on a real (not mocked) Pi session.
2. 999.67: the planted-`decided_by_layer` fixture on the single-document path now returns the
   overwritten (non-planted) value, pinned by a regression test with a passing counterpart.
3. 999.96: `release --check` reports NOT viable on the current skew and viable when they agree.
4. 999.104: `release --check`'s signing probe reports viable only for the maintainer key, with a
   negative control (the wrong key reported NOT viable).

## Open decisions (to settle in discuss)

1. **999.104 approach** — one-line probe (operator's lean, this SPEC's default) vs. rethinking the
   two-key model into a single release-only identity. See operator's pros/cons discussion.
2. **Pi stage coverage** — all five stages at once, or Code-stage first as a vertical slice?
3. **Pi interface specifics** — exact invocation/stream format to be established from Pi's docs
   during discuss (I can pull them).
4. **999.67 / 999.96 bundling** — both are XS/S and in-code-touched-by-this-phase, but confirm they
   don't add wave overhead (operator leaning include).

## Deferred (explicitly not here)

- 999.31 — Phase 37.
- 999.94 — Phase 37 (pencilled).
- 999.101 — upstream Claude Code; carry as an observation into Phase 37's driver contract.
