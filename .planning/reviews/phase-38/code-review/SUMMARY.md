# Adversarial Code Review — Phase 38 execution (driver contract, 9ed0432 + docs)

**Target:** `ff0e0d6..HEAD` on `feature/phase-38` (13 files, +316/−444). Driver-contract removal, InteractivityMode gate, 999.107 fixes, test-double migration.
**Reviewers:** claude (opus, high) · codex (gpt-5.6-terra, high) · antigravity (gemini-3.7-flash-high, --print-timeout 15m). MCP linear/vercel disabled during run, restored after.
**Date:** 2026-08-17 · **Root:** `.worktrees/phase-38`

## Verdict: FIX-FIRST (all three, independently)

Two of the findings are real bugs in shipped behavior; the rest are latent/robustness.

## Convergent findings (≥2 reviewers, code-verified)

1. **HIGH — the D-03 Plan gate is half-wired and checks the wrong branch.** The pre-start leg
   (`commands.rs:302-310`) is still a `println!` **warn**; the runtime leg
   (`preflight.rs:614-628`) is a hard gate. So `devflow start --agent codex --mode auto` on a
   phase with CONTEXT.md but no PLAN.md prints a warning, burns the Define stage, then wedges on a
   never-silent preflight gate at Plan — exactly the "burned agent run + dead-end gate" the
   pre-start check exists to prevent. **Worse:** the gate probes `develop` for `-PLAN.md`
   (`phase_artifact_on_develop`), but the Plan stage *produces* PLAN.md on the **feature branch** —
   so the run can never clear the gate; an operator who Advances past it has the PLAN.md committed
   to the feature branch, and any `resume`/`LoopBack` re-reads `develop` (still empty) → re-gates →
   burns `preflight_retries` → `abort()` (claude HIGH #1/#2, antigravity HIGH #2; codex HIGH #1
   adds that `start()` rejects Define regardless of mode while the runtime gate only fires in
   `Auto`, so the two disagree in `Supervise`).
2. **HIGH/MEDIUM — 999.107 #2 non-UTF-8 leg is tested, not fixed.** `to_string_lossy()` turns a
   non-UTF-8 path byte into U+FFFD — valid TOML, but it **names a different, nonexistent path**, so
   the writable-root entry doesn't authorize the real `.git`. The new test (`codex.rs:153`)
   asserts the U+FFFD replacement, codifying the loss as correct. The honest fix is to **refuse the
   launch**, not to lossily convert (claude #4, codex HIGH #2, antigravity #4).
3. **MEDIUM — `escape_toml_basic_string` misses U+007F (DEL).** `< 0x20` doesn't cover DEL; TOML
   basic strings must escape U+007F too, so a DEL-containing path emits invalid TOML and Codex
   rejects its config. Verified against the `toml`/`tomllib` crates with a negative control (all 3).
4. **LOW — stale docs.** `docs/architecture/agent-model.md:38` still says `adapter_for()`
   (deleted); `ARCHITECTURE.md:96` still says "three AgentKinds" above the four-driver edit; the
   `agent-model.md` agent table omits Pi (all 3).

## Single-reviewer findings

- **claude MEDIUM #7** — the 999.107 #1 reorder inverts a precedence that a shipped test comment
  declares "by design" (`agent_result.rs:4151-4153`: "marker-over-`turn.failed` by design (13-06
  dogfood finding)"); the comment now contradicts the code. Also the new terminal-first path
  returns `commits: None`, discarding the agent's own self-report whenever `turn.failed` is present
  (thinner gate context than before).
- **claude MEDIUM #8** — old bare `PLAN.md` phases now hard-block: `phase_artifact_on_develop`'s
  `rest.ends_with("-PLAN.md")` never matches `.planning/phases/NN-name/PLAN.md`, a shape the repo's
  own CLAUDE.md records as live.
- **claude MEDIUM #5/#6** — the `other` arm (`RequiresTypedSubagents`/`InteractiveOnly`) refuses
  regardless of `Mode`, unlike the `Auto`-scoped `RequiresExistingArtifact` arm; and `_ =>
  "-PLAN.md"` silently maps any future stage to a plan artifact. Both latent (no driver returns
  those variants today).
- **claude LOW #9/#10** — a truncated leftover doc line at `preflight.rs:599`; `ClaudeDriver::
  build_command` lost its provenance doc comment (the "`--verbose` is load-bearing" Phase-30 note).

## Verified clean (no finding — all three or claude+antigravity confirmed)

- `completion_signal_detected` was **already dead** at `ff0e0d6` (no production caller) — its
  removal is dead-code cleanup, not drift.
- `extra_env`→`environment`, `preflight`→`health` are 1:1 at every migrated site; test doubles
  behavior-identical.
- Relocated `ClaudeDriver::exec_command_single_document` / `exec_resume_command` are
  **byte-identical** to the old `ClaudeAgent` methods.
- `driver_for` is exhaustive over all four `AgentKind`s (a fifth variant is a compile error).
- All four 999.107 #1 precedence cases hold and are test-pinned; the reorder itself is correct.
- Claude zero-regression baseline holds (claude ran `cargo test --workspace` + `clippy -D warnings`,
  both clean).

## Reviewer status

- **claude** — success (opus, high; verified the workspace tests/clippy itself).
- **codex** — success (gpt-5.6-terra, high; emitted twice, deduplicated).
- **antigravity** — success (gemini-3.7-flash-high; MCP disabled during run, restored after).

## Recommended fix order (small, mechanical — but two need a decision)

1. **The Plan gate** (HIGH #1) needs a *decision*, not just a fix: the D-03 "extend to Plan" as
   implemented can't work because it checks `develop` for an output artifact. Options: (a) revert
   the Plan extension — gate Define only (the pre-D-03 behavior); (b) gate Plan against the
   worktree/feature branch instead of `develop`; (c) keep Plan as warn-only. My recommendation is
   (b) is wrong (Plan's output isn't on `develop` by design) — so it's really (a) or (c).
2. 999.107 #2: **refuse** the launch on a non-UTF-8 root rather than lossily convert; add U+007F to
   `escape_toml_basic_string`; fix the stale docs. All mechanical, no decision needed.
