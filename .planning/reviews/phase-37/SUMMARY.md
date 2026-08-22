# Adversarial Review — Phase 37 planning docs (replan gate)

**Targets:** `37-CONTEXT.md`, `37-RESEARCH.md`, `37-VALIDATION.md`, `37-01`…`37-04-PLAN.md`
**Reviewers:** claude (opus, high) · codex (gpt-5.6-sol, high) · antigravity (Gemini 3.1 Pro — re-run after MCP fix)
**Date:** 2026-08-15
**Review root:** `.worktrees/phase-37` @ `feature/phase-37` (code-verified against live tree + installed CLIs)

Both claude and codex independently read the live source and the installed CLIs (codex 0.147.0 /
0.146.0) and ran negative controls. Their findings agree on the load-bearing issues below; the
codex-only and claude-only findings are flagged. This review **blocks planning** — the DAG is broken
and several acceptance criteria are unfalsifiable.

## Cross-review consensus (both reviewers, code-verified)

1. **BLOCKER — execution DAG is invalid.** `37-03` declares `depends_on: [01]` but its first must-have
   requires the `AgentDriver` trait, which `37-02` Task 1 creates. Both are `wave: 2` and both write
   `agents/mod.rs` + `adapter_for`. Parallel execution = build break + write collision. Fix:
   `37-03.depends_on: [01, 02]` (move 37-04 to wave 4), or move the trait definition into 37-01.

2. **HIGH — `--ask-for-approval never` is not a valid `codex exec` argv.** claude: the flag is rejected
   on `codex exec` (positive control: `codex exec --ask-for-approval never --help` → `unexpected
   argument`). codex: it's a *global* flag that must precede `exec` (`codex --ask-for-approval never exec`).
   Either way, the planned argv fails to launch, and the T-37-08 argv-shape test greens on a command
   the real CLI rejects. Fix: verify against the installed CLI and use the correct form (or drop it).

3. **HIGH — "Codex is no longer broken" (37-03) is false.** 37-01 scopes to the Code stage only; no plan
   touches `prompt.rs:147` (Validate), `:193` (Plan), `:351` (`fix_prompt` → `/gsd-execute-phase --gaps-only`),
   or `pipeline_gate.rs:591` (`print_dry_run`). Codex still receives literal `/gsd-*` at Plan, Validate,
   Ship, and every Code↔Validate loop-back. Fix: either expand the migration to all stages or weaken the
   37-03 claim to "Code-stage fixed".

4. **HIGH — `AgentAdapter` removal criterion is unfalsifiable.** The grep will find references outside
   37-04's `files_modified` (`canary.rs:40`, `test_support.rs:205/244`, `preflight.rs:1266`,
   `pipeline_launch.rs:190`), so the plan's "keep it and record why" fallback is the only reachable
   outcome and the phase calls that success. Fix: enumerate the real call sites in `files_modified`,
   or make removal a hard gate with an explicit follow-up.

5. **HIGH — the Codex negative control has no positive oracle.** "no `/gsd-*`" + "not byte-identical"
   passes on `""`, `"hello"`, or `"do nothing"`. Nothing specifies what the Codex-native instruction
   must contain (phase number, Code intent, fix mode, the `--auto` auto-chain token `prompt.rs:40/290`
   exists to preserve, the completion protocol). Fix: pin the required content + a positive test.

## ROADMAP / scope drift (consensus, sharpest from codex)

6. **BLOCKER — the plans delete the ROADMAP goal without updating it.** ROADMAP:34 says Phase 37 makes
   Pi run end-to-end (JSON unwrapper + `CloseRule`) and pencils 999.94 in; the phase slug literally
   contains `999-94`. CONTEXT defers both to 37.1/38; VALIDATION launders the missing deliverable to
   "N/A" (row 37-01-02 Manual-Only). A later `roadmap.analyze`/milestone audit reads the slug and
   reports 999.94 covered. Fix: update ROADMAP's Phase 37 goal to the deferred scope (and fix the slug),
   or bring the deferrals back in.

## Claude-only (all code-verified; verify before acting)

7. **HIGH — `multi_agent_v2` is already enabled.** `codex` 0.147.0 reports `multi_agent_v2 stable true`.
   31b's "enable it" item is a no-op; the audit's real risk (tool-schema shape change) is carried into
   no plan's threat model.
8. **HIGH — Plan 01 cannot select a driver.** `prompt.rs:249` has no agent/driver parameter; the CLI
   renders the prompt before adapter selection (`pipeline_launch.rs:90`). Plan 01 orders per-driver
   routing while forbidding touching `pipeline_launch.rs` — the renderer has no way to know the driver.
9. **HIGH — "Claude → PipeOwning, never Legacy" contradicts a shipped feature.** `--legacy-claude-launch`
   (v2.3.0/Phase 31) is a supported path (`claude_stream_launch_enabled` returns false under the
   opt-out; `exec_command_single_document` is the pre-31 builder). The routing test also can't run under
   `cargo test -p devflow-core --lib` — the code lives in `devflow-cli`.
10. **HIGH — CONTEXT.md's canonical citation is wrong and dangerous.** `37-CONTEXT.md:72/89` cite
    `agent_result.rs:361-453` as Codex parsing, but `:363` is `claude_stream_session_id` (Codex is at
    `:712`/`:740`). A "MUST read" doc pointing an executor at the single most regression-sensitive
    Claude function in a zero-regression-Claude phase.
11. **HIGH — `Stage::gsd_command()` removal has an uncounted caller.** `pipeline_gate.rs:591`
    (`print_dry_run`) — in no plan's `files_modified`; `stage.rs:51-59` documents the mapping as retained
    for dry-run preview.
12. **HIGH — Pi's prompt changes with no coverage at all.** Pi is absent from the retiring invariant test
    (`agents/mod.rs:133` iterates Claude/Codex/OpenCode); its prompt is positional (an argv element), so
    37-03's "byte-equal build_command/health" and "render_prompt changed" are mutually exclusive unless
    blind; the Phase-36 leading-dash hazard (`pi.rs:14-17`) is never addressed.
13. **MEDIUM — D-03 inverted in 37-02.** CONTEXT D-03 (Pi is the second native driver, superseding 999.31
    D-02) is contradicted by 37-02's objective ("D-02 satisfied by Claude vs OpenCode") — re-instating the
    superseded decision.
14. **MEDIUM — the hardcoded interactivity check lives in `commands.rs:289`, not `preflight.rs`.** 37-04
    modifies the wrong file (the 999.31 origin names `commands.rs`; codex verified the start-time gate).
15. **MEDIUM — `--add-dir` verify-first has no falsifiable outcome.** 37-03 P-03's fallback ("record why")
    is indistinguishable from "didn't try"; no test or criterion for validate-phase to read.
16. **MEDIUM — `ARCHITECTURE.md` doesn't carry the stale claim the plan greps for.** The "same/identical
    prompt" wording is at `README.md:89`, `docs/guides/adding-agent.md:48`, `docs/architecture/agent-model.md:42`;
    ARCHITECTURE's real stale content is the `AgentAdapter` trait description (`:92-96`) + "prompt-sharing"
    (`:409`), which the grep target won't match.
17. **Note — 999.101 dropped.** 36-SPEC:89 routes it forward as "observation for Phase 37's driver
    contract"; zero mentions in any Phase 37 artifact.
18. **Note — D-11 inverts locked 999.31 D-04** ("put a deprecation date on AgentAdapter; don't let both
    paths persist") without stating the supersession — the same silent-supersession the operator flagged
    for D-02.

## antigravity — re-run (was skipped: timeout)

First run timed out — root cause was NOT the prompt size but the "linear" + "vercel" MCP servers in
`~/.gemini/config/mcp_config.json` hanging on connect (log: "MCP: 2 server(s) still connecting" for
the full 5m, then "timed out after 1496 polls"). Re-run with MCP disabled, `--model gemini-3.1-pro-high`,
`--print-timeout 15m` — completed. NOTE for the review skill: antigravity HAS `--model`/`--effort`/
`--print-timeout` flags (the skill's "config-bound, no flag" reference is outdated).

Antigravity-only findings (claude/codex overlaps omitted):
- **HIGH — `FixType` location hallucinated.** 37-01 lists `FixType` under `stage.rs`; it is defined in
  `prompt.rs:73`. Embedding it in `StageIntent` requires a move or an upward re-export, un-architected.
- **HIGH — `fix_prompt` callers ignored.** `fix_prompt` is called by `pipeline_gate.rs:198` AND
  `pipeline_outcomes.rs:4866` — neither in 37-01's `files_modified`, so the signature change breaks
  compilation.
- **MEDIUM — `codex features list` shell-out unvalidated.** `capabilities`/`environment` doing a
  synchronous `codex features list` shell-out per check is a blocking hazard, and the VALIDATION table
  had no negative control for it.

## Verdict

**REVISE — planning must not proceed.** Findings 1 and 6 are blockers (broken DAG, silent scope deletion);
findings 2–5 and 8–12 are unfalsifiable or wrong-file findings that would let a green phase ship a broken
migration. The correct next step is a plan revision pass (fix the DAG, pin the Codex prompt content and the
real flag, update ROADMAP + slug, enumerate the `AgentAdapter` call sites, and carry the non-Code-stage
migration into an explicit task or a stated deferral) — then re-run this review.
