# Adversarial Code Review — Phase 37 implementation

**Targets:** the Phase 37 diff (`agents/*`, `prompt.rs`, `agent_result.rs`, `pipeline_launch.rs`,
`pipeline_gate.rs`, `test_support.rs`) vs `develop`.
**Reviewers:** claude (opus, high) · codex (gpt-5.6-luna, high) · antigravity (gemini-3.1-pro-high)
**Date:** 2026-08-16
**Review root:** `.worktrees/phase-37` @ `feature/phase-37` (both reviewers read the live tree + ran the suite)

## Verdict: REQUEST CHANGES — BLOCK

The DriverShim is sound and the slash-command elimination is complete, but the new Codex/Pi renderer
(`render_workflow_style`) is a **regression of four per-stage contracts** the legacy renderer
enforced. Do not ship until the renderer is fixed.

## Findings (deduplicated, cross-review)

### BLOCKERS — one root cause: `render_workflow_style` dropped the per-stage contracts

`render_workflow_style` (`prompt.rs:380-404`) replaced every stage's dedicated prompt with one
generic template. claude + codex independently found the same four consequences:

1. **HIGH — Codex/Pi can never pass Validate.** The generic template's only completion contract is
   `COMPLETION_PROTOCOL` (status + reason), with **no `verdict` field**. The legacy
   `validate_stage_prompt` demanded `"verdict": "pass"|"gaps"` because
   `pipeline_outcomes.rs:235-245` refuses to advance on a bare `status: success`. A perfectly
   compliant Codex/Pi Validate emits `{"status":"success"}` → `Failed`/`Ambiguous` → loops back to
   Code, every time, until the consecutive-failure counter gates. (claude #1, codex #1)
2. **HIGH — Codex/Pi Ship bypasses the review gate.** `StageIntent::Ship { phase, .. }` discards
   `review_angles`; the renderer points straight at `ship.md` with no code-review-first, no
   `REVIEW.md` Critical gate, and no `review:` failure prefix (the `is_review_rejection` loop-back
   contract). A Codex/Pi Ship runs the merge/version-bump/tag/publish hooks with no review in front.
   Also: `for_stage_in_project` still reads `config::review_angles` and throws it away for these
   agents. (claude #4, codex #2)
3. **HIGH — Native Define/Plan reintroduce interactive workflows.** Define maps to `discuss-phase.md`
   — the exact interview `define_stage_prompt` (D-14) forbids; Plan loses `idempotent_stage_prompt`'s
   guard, the 13-06 Codex-dogfood fix ("headless Codex can never answer the interactive
   Overwrite/Append/Cancel"). (claude #2/#3, codex #3)
4. **HIGH — Pi is pointed at Codex's install directory.** `prompt.rs:399` hardcodes
   `$HOME/.codex/gsd-core/workflows/…`, and Pi uses it. Pi's workflows are at `~/.pi/agent/gsd-core/…`.
   It only "works" here because Codex happens to be installed; on a Pi-only host every Pi stage names
   a nonexistent path. (claude #5)

### MEDIUM — the decoupling is nominal, not wired

5. **InteractivityMode / DriverHealth / capabilities / discover are dead metadata.** No caller
   outside `agents/mod.rs` + its tests. The hardcoded `state.agent == AgentKind::Codex` checks are
   untouched at `preflight.rs:613` and `commands.rs:289`. 37-04-PLAN listed "preflight consumes
   `InteractivityMode` generically" as a completion criterion; 37-04-SUMMARY deferred it — an unmet
   criterion, not a deferral. `PiDriver` declares no `interactivity_mode` at all. (claude #6, codex #4)
6. **`parse_completion` is not wired into evaluation.** `CodexDriver::parse_completion` exists but the
   production cascade still calls the free `parse_codex_event_result` at `agent_result.rs:1840`.
   `is_codex_event_stream` was widened to `pub(crate)` with zero cross-module callers. (claude #6,
   codex #5)
7. **`test_contract()` is largely unfalsifiable.** "name non-empty" and "build_command names a
   program" cannot fail (static str literal / discarded `_args`); the five stage checks share one
   name (can't localize a failure); the only real predicate is `contains("DEVFLOW_RESULT")`, which a
   driver rendering `"do nothing"` + the protocol passes. No negative control. The `--auto` assertion
   at `mod.rs:450` is vacuous (the boilerplate contains `--auto` for every stage). (claude #7, codex #7)
8. **`PiDriver::health` can block preflight indefinitely, and probes a hardcoded provider.**
   `pi.rs:52-55` runs `pi auth check` with no timeout; Pi refreshes expired OAuth by default
   (no `--no-refresh` passed), so a stalled refresh hangs preflight. `--provider google` is hardcoded
   while Pi accepts `--provider`/`--model` — a Pi run on anthropic/openai gets a false "no provider
   credential resolves". (claude #8, codex #8)

### Codex-only (pre-existing code, surfaced during the review)

9. **HIGH — Codex parser lets an earlier success marker beat a later terminal failure.**
   `agent_result.rs:764-781` returns the last `agent_message` marker before examining `turn.failed`
   (`:784-812`); a stream of `success` then `turn.failed` is read as Success. Pre-existing (not in the
   Phase 37 diff), but a real defect — the existing test covers success+`turn.completed`, not
   success+`turn.failed`.
10. **MEDIUM — Codex writable-root serialization mishandles hostile paths.** `codex.rs:47-60` uses
    `root.display().to_string()` + escapes only `\` and `"`; non-UTF-8 → `�`, newlines → invalid TOML.
    Pre-existing logic (relocated verbatim from `CodexAgent`).

### antigravity — reviewed, but weak (1 false positive, 2 out-of-scope)

- **#1 "Claude `-p` argument shadowing" — FALSE POSITIVE.** `-p`/`--print` is a boolean flag
  (verified against `claude --help`); the `["-p", "--input-format", "stream-json", …]` argv is the
  shipped, byte-identical behavior (claude's own review confirms the DriverShim matches `develop`).
- **#2 "auto_chain_guard race" (pipeline_launch.rs:876-903) — OUT OF SCOPE.** Zero diff vs `develop`
  in Phase 37; pre-existing code.
- **#3 "Codex rate-limit JSON suppression" (agent_result.rs:228-234) — OUT OF SCOPE.** Zero diff vs
  `develop`; pre-existing code.

## Verified sound (not padding)

- **DriverShim zero-regression for Claude/OpenCode: holds.** All four delegations match `develop`'s
  adapters byte-for-byte (claude verified each). The regressions are in the Codex/Pi *renderer*, not
  the shim.
- **Slash-command elimination: complete.** All three call sites rewired; no path emits `/gsd-*` to
  Codex/Pi; the only remaining `prompt_override` producer is Claude-resume-only.
- **Flag construction: correct** (codex `-a never exec`, pi `-p --no-approve`, `pi auth check` shape
  all verified against the installed binaries).
- **No shell injection** — all spawns use `Command::args`.

## Recommended fix

The four BLOCKERs are one function. Port the per-stage contracts into the workflow-style renderer:
Validate must demand `verdict`, Ship must keep the review gate + `review_angles` + `review:` prefix,
Define must be the D-14 no-op, Plan must keep the idempotency guard — and the workflow path must be
per-driver (not `$HOME/.codex/…` hardcoded), since Pi and Codex install to different roots. A
negative control for `test_contract()` (a deliberately-broken driver that must fail) would then catch
this class of regression. Items 5-8 and 10 are already the substance of the deferred `999.106`.
