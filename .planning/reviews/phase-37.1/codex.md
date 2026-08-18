> invocation: codex exec -m gpt-5.6-terra -c model_reasoning_effort=high --cd /var/home/denniyahh/Github/devflow/.worktrees/phase-37.1 "$(cat /tmp/p37.1-review-prompt.txt)"
> review root: /var/home/denniyahh/Github/devflow/.worktrees/phase-37.1 (feature/phase-37.1 @ cef4059)
> DEDUPLICATED — codex emitted the findings twice; the detailed copy is kept.

## BLOCKER

1. **`37.1-DECISION-GATE.md:4` widens the actual finding into a roadmap veto.** "**NOT VIABLE** — there is no defensible 'full-dispatch arm' for Pi in this milestone." The research's actual recommendation is narrower — `37.1-RESEARCH.md:43`: "NOT VIABLE for a 'full-dispatch arm' that **reuses DevFlow's existing `PipeOwning`/`CloseRule` machinery**."

   The cited code supports the narrow claim: `PipeOwning` is Claude's stdin-turn protocol, and Pi waits for stdin EOF before starting (Pi `main.js:48-63`). It does not prove Pi dispatch itself is non-viable. `pipeline_launch.rs` routes only Claude to `PipeOwning` (`:707-713`); nothing requires a Pi-specific streaming/tailing monitor to reuse it. This is a scope/cost decision disguised as technical impossibility.

   **What breaks:** Phase 39 is forced onto the reduced arm despite no evidence excluding a full-dispatch design that uses a new monitor rather than Claude's protocol.

2. **The gate asserts verification where the research explicitly records an untested load-bearing assumption.** `37.1-RESEARCH.md:323` says the extension-tool event shape is "**not inspected directly — inferred**". But `37.1-DECISION-GATE.md:12` says "all code-verified", then treats that exact shape as fact at `:24-26`.

   The validation file does not close this gap: it is `status: draft`, `nyquist_compliant: false` (`37.1-VALIDATION.md:6`), and its manual check only says to rerun help/list and "spot-check" sources (`:68`). Meanwhile the gate declares the branch closed (`DECISION-GATE.md:53`). No smoke test, and no negative control, exercises extension-tool NDJSON.

   **What breaks:** A parser/drain design is rejected on an admitted inference, not on the missing experiment that would falsify it.

## HIGH

3. **`37.1-RESEARCH.md:37` calls `--mode json` "single-shot print mode, not a stream" — an equivocation presented as fact.** Pi 0.84.1's installed source labels it "single-shot", but also explicitly defines `pi --mode json "prompt"` as a "**JSON event stream**" (`print-mode.js:1-7`) and writes every subscribed event as NDJSON (`:84-88`).

   The actual limitation is narrower and verified: Pi consumes stdin until EOF before prompting (`main.js:697-706`), while `PipeOwning` deliberately holds stdin open (`monitor.rs:821-835`). That establishes a deadlock for *that* transport; it does not establish absence of an observable event stream.

   **What breaks:** The documents erase the exact output capability a non-`PipeOwning` Pi monitor would need, making the negative verdict look stronger than the source permits.

4. **`37.1-RESEARCH.md:60` maps completion parsing to "`agent_result.rs` Layer 1 | `AgentDriver::parse_completion`" — that is not the current execution path.** `AgentDriver::parse_completion` has a default implementation (`agents/mod.rs:144-148`); the only implementation is Codex, and no production caller invokes the trait method. Layer 1 hard-codes Claude, generic-marker, and Codex parsers (`agent_result.rs:1834-1841`).

   **What breaks:** "Add a Pi-specific parser" can be implemented at the documented driver seam and never run; Phase 39 needs a Layer-1 dispatch/wiring decision and tests, not merely a parser.

5. **The claimed user-scope safety control is unenforceable as documented.** `37.1-RESEARCH.md:211` says the extension "MUST install at user scope", repeated in `DECISION-GATE.md:34`. But the bundled extension still accepts model-supplied `agentScope` (`index.ts:472-476`) and searches project agents for `"project"`/`"both"`; its confirmation happens only when `ctx.hasUI` (`:505-527`). DevFlow deliberately launches Pi headlessly with `--no-approve` (`pi.rs:3-8`).

   **What breaks:** A global extension can still execute repo-controlled `.pi/agents` in a headless DevFlow run; "install it globally" is not an enforcement mechanism. The gate lacks the required negative test: headless `agentScope: "both"` must refuse before spawning anything.

VERDICT: REVISE — the evidence establishes that Claude's existing `PipeOwning`/`CloseRule` path cannot be reused for Pi, but the documents overgeneralize that result into a final non-viability verdict while relying on an untested event-shape assumption and an unimplemented parser seam.
