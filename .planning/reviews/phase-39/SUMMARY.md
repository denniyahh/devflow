# Adversarial Code Review — Phase 39 execution (Stage 1 + Stage 2, e4f0bb6)

**Target:** `189e020..e4f0bb6` on `feature/phase-39` (10 files, +455/−76). Provider fix, capability detection, Legacy test, docs, e2e smoke.
**Reviewers:** claude (opus, high) · codex (gpt-5.6-terra, high) · antigravity (gemini-3.7-flash-high, --print-timeout 15m). MCP linear/vercel disabled during run, restored after.
**Date:** 2026-08-17 · **Root:** `.worktrees/phase-39`

## Verdict: FIX-FIRST (all three, independently)

The headline is a **BLOCKER regression in the provider fix** — it breaks standard Pi installs and can
false-green on the wrong provider. Below that, the capability detection and the e2e evidence both
over-claim what they establish.

## Convergent findings (all three, or claude+codex+antigravity in agreement)

1. **BLOCKER — the `litellm` provider fix breaks standard Pi installs and false-greens.**
   `configured_pi_providers()` (`pi.rs:127-148`) reads **only `models.json`**, which Pi uses solely
   for *custom* endpoints (LiteLLM/vLLM). A standard Pi install (built-in provider via
   `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`/OAuth in `auth.json`) has no `models.json` → the function
   returns `[]` → `health()` refuses `"no provider configured in Pi's models.json"`. The old
   hardcoded `--provider google` would have passed such an install. The remediation text is also
   wrong (`pi auth check` without `--provider`/`--model` errors). Separately, the loop returns
   `Ok(())` on the **first ready** provider in `models.json`, even if the *active* provider
   (`~/.pi/agent/settings.json` → `defaultProvider`/`defaultModel`) is a different one with broken
   credentials — preflight passes, the run fails mid-flight. (claude, codex, antigravity.)

2. **HIGH — the capability-detection name-match over-claims.** `contains("subagent")`
   (`pi.rs:160-171`) matches `@mystilleef` (the one the phase **excluded** as unsafe: child
   `--approve`, default scope `"both"`) and `@dreki-gg`/`@smoose` (deferred pending an unshipped
   `--no-approve` patch). `devflow doctor` therefore prints `pi subagent dispatch  available ✓` for
   an extension the phase itself ruled unsafe. The predicate cannot distinguish the one vetted
   package (`@bacnh85`) from the excluded ones. (claude; the name-based limit was flagged by all.)

3. **HIGH/MEDIUM — the Legacy regression test hardcodes `false`.** `pi_resolves_to_legacy_launch`
   (`pipeline_launch.rs:3283`) passes a literal `stream_launch: false` to `resolve_launch_shape`
   instead of calling `claude_stream_launch_enabled(AgentKind::Pi, …)`. It only asserts the
   fallback branch given `false`; it would still pass if the production predicate were broken to
   return `true` for Pi and route it to `PipeOwning`. The repo already has the right idiom 30 lines
   up (a `claude_stream_launch_enabled` precondition assert). (claude, codex, antigravity.)

4. **MEDIUM — the e2e smoke evidence is a proxy, not proof.** `39-E2E-SMOKE.md` records a bash
   side-effect file as the dispatch proof — but the parent's own `bash` tool could write that file
   without ever calling `subagent`. The *discriminating* evidence exists (the session transcript in
   `/tmp/p39-e2e-profile/sessions/…` shows exactly one `toolCall` named `subagent`, with the bash
   calls nested inside the subagent's result — claude verified it), but it is **not captured in the
   repo**. Worse, claude found the smoke actually ran on `deepseek`/`openrouter`, not the `litellm`
   provider the doc claims — so the smoke never exercised the `litellm` path the provider fix
   targets. (claude, codex, antigravity.)

5. **MEDIUM/LOW — "dual-stage routing" is an over-claim.** `DriverCapabilities::subagent_dispatch`
   is consumed **only** by the `devflow doctor` line. Nothing in `start`/`pipeline_launch`/`prompt`/
   `advance` branches on it. `ARCHITECTURE.md:104-106` and `docs/guides/pi-subagent-dispatch.md:39-43`
   describe "two arms" and a routing decision that do not exist — the honest statement is
   "reported only, no consumer yet." (all three.)

## Single-reviewer findings

- **claude LOW:** the doctor check spawns `pi list` on *every* `devflow doctor` (~1.28s), even when
  the project's agent isn't Pi; when `pi` is absent it prints a misleading "not installed + install
  hint" directly under the separate `pi ✗ missing` line; no test covers the new doctor check.
- **claude LOW:** `39-PLAN.md:58-59` acceptance ("one live run proves … **or** is recorded as
  blocked on credentials") is non-falsifiable — it passes either way.

## Reviewer status

- **claude** — success (opus, high; independently re-ran `cargo test -p devflow-core --lib agents::pi`
  → 12 passed, and `clippy -D warnings` clean; inspected the live `/tmp` session transcript).
- **codex** — success (gpt-5.6-terra, high; emitted twice, deduplicated).
- **antigravity** — success (gemini-3.7-flash-high; MCP disabled during run, restored after).

## Recommended fix path (for Dennis; not yet executed)

1. **Provider fix (BLOCKER):** probe the *active* provider from `settings.json`
   (`defaultProvider`/`defaultModel`), not "any ready provider in `models.json`" — and do not refuse
   when `models.json` is absent (fall back to a bare `pi auth check --provider <default>` or the
   old single-provider behavior). The current code both false-rejects and false-greens.
2. **Detection (HIGH):** match the specific vetted package (`@bacnh85/pi-subagent`), not
   `*subagent*` — so an unsafe/deferred extension isn't reported "available".
3. **Legacy test (HIGH/MEDIUM):** assert `!claude_stream_launch_enabled(AgentKind::Pi, Stage::Code, false)`
   as the precondition, so the test discriminates.
4. **E2E evidence (MEDIUM):** copy the session transcript (the `toolCall: subagent` line) into the
   repo, and either re-run against `litellm` or correct the doc's provider claim.
5. **Docs (MEDIUM/LOW):** reword "routing/two arms" → "reported only, no consumer yet"; add a
   doctor-check test; make the plan acceptance falsifiable.
