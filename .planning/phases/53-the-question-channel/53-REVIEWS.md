---
phase: 53
reviewers: [codex, antigravity, pi]
reviewed_at: 2026-11-18T21:41:00Z
plans_reviewed: [53-01-PLAN.md, 53-02-PLAN.md, 53-03-PLAN.md, 53-04-PLAN.md, 53-05-PLAN.md, 53-06-PLAN.md, 53-07-PLAN.md, 53-08-PLAN.md, 53-09-PLAN.md, 53-10-PLAN.md, 53-11-PLAN.md, 53-12-PLAN.md, 53-13-PLAN.md]
models:
  codex: "gpt-6.1-sol (reasoning=high)"
  antigravity: "gemini-3.8-flash-high (effort=high)"
  pi: "deepseek-flash"
model_sources:
  codex: "command line"
  antigravity: "command line"
  pi: "command line"
---

# Phase 53 plan review — Tier B, one round (adversarial lens)

Lanes ran on a `git archive` snapshot of `feature/phase-53` at 063d3e0. Raw output: `reviews/review-{codex,agy,pi}.md`.
Dispositions were set by the driving agent (Claude) against the source; a finding marked REJECTED was checked and is false.
Per Tier B, CRITICAL and MAJOR findings are folded once; MINOR findings are backlog.

## Rejected (checked, false)

- **pi F-1 / agy #1 — "fabricated operator answers (1) and (2)".** Both answers are real: the operator's `devflow gate reject 53 --note`
  at 2026-11-18 19:10 UTC, recorded in `.devflow/events.jsonl` (not in the reviewed snapshot). The plans quote it correctly.
  Fold only this: cite the gate-reject note as the source in 53-06/53-08 so the provenance is checkable from the repo.
- **agy #6 — 53-10 Task 1 verify names tests that do not touch the removed variants.** The cited command is not in 53-10; its real gate
  (line 74) relies on the compiler plus 53-11's removed-surface scan.
- **agy #8 — semver.** The workspace is already a major bump (4.0.0-alpha.1); removals are allowed and are recorded in CHANGELOG.

## Operator decisions (needed before the replan)

- **OD-1 (pi F-5, agy #3; spike 53-MCP-SPIKE.md:79-82 routed it to planning or the operator):** a held codex call polls `wait` about every
  21 s (1.35 M input tokens in 15 min). With T = 3 days, one unanswered codex question can cost on the order of 10^8 tokens. 53-08 takes the long block.
- **OD-2 (pi F-2, agy #1 impact):** answer (2) refuses opencode/pi/hermes/antigravity. Consequences the plans do not state: ROADMAP criterion 1's
  "e2e test that drives the real binary with a stub agent for the other launch shapes" and ASK-01's "every supported agent" are no longer met as written,
  and `--mode auto` loses its only non-claude fallback (antigravity).

## To fold (CRITICAL/MAJOR, verified or plausible against source)

1. **Stage-wide gate close can delete the runner's replacement failure gate** (codex 1; `Gates::close` takes no gate id, gates.rs:1022). Identity-checked close; control: replacement gate survives a late server cleanup.
2. **Abandoned ask-lock recovery race allows two owners** (codex 2; 53-07:132). Serialize reclamation; test two contenders that saw the same abandoned file.
3. **Notify hook blocks with no timeout before the answer wait** (codex 3; confirmed: gates.rs:1912-1925 uses `Command::output()`). Bounded, cancel-aware notify; control with a hook that never exits.
4. **Operator abort lost when the server dies after the answer is published** (codex 4, agy #7). Runner backstop applies a bound abort answer before disposing of the question; kill-window test.
5. **`abort_requested` is never cleared** (pi F-8; no plan consumes it). Consume it as `relaunch_note` is consumed.
6. **53-10 Task 1 cannot compile** (codex 5): exhaustive matches on `GateKind` (pipeline_outcomes.rs:565-580, test_support.rs:761-775) lose their arm before the variant is removed in Task 2. Merge the removal into one task.
7. **Queued calls have no timeout path** (pi F-11, codex 6). `--timeout-secs` must cover the ask-lock wait; define whether queue time consumes T; test it.
8. **Codex launches start the operator's global MCP servers** (pi F-3, agy S-2; spike 53-MCP-SPIKE.md:36-38,110 disabled them per launch). Decide and state it in 53-08; if kept on, say why.
9. **`.devflow/ask-<NN>.mcp.json` is not gitignored or cleaned** (pi F-6, agy #5). Add to `.gitignore` and `gitignore_coverage.rs`; remove on `discard`.
10. **`devflow status` shows `gate: none` during a question; 53-04 silences `check_orphan_gate` instead** (pi F-7). Make status report an open question gate.
11. **`gate sweep` reaps an old question gate as an abort** (pi F-9; `reap_answer` has no kind guard, gates.rs:935-956). Guard by kind or define the order against the server timeout.
12. **D-13 "any stage" is false for Validate** (pi F-10; pipeline_launch.rs:2970-2976 routes Validate failures elsewhere). Narrow the truth or carry the reason; add a Validate-stage test.
13. **Idle guard bound is the 3-day gate timeout** (pi F-4); and silence after an answer is not covered (agy #2, speculation). Tie to OD-1; state the production bound.
14. **Run-binding control needs the 53-01 fixture to pass `started_at`; `tests/common/ask_client.rs` missing from 53-07 files** (pi S-1).
15. **Cancellation data structure: `AtomicBool` cannot match request ids; `mpsc` cannot remove queued items** (agy #4). Specify a set and a removable queue.
16. **Claude's `--mcp-config` splice is pinned only by a unit test, not a real argv** (pi S-2, speculation). Add a stub-claude e2e or record why not.
17. **Tool name the prompt tells the agent to call may be namespaced** (agy S-1, speculation; claude exposes `mcp__devflow__devflow_ask`). Check against the spike transcripts.

## Backlog (MINOR)

- pi F-12 (53-03 edits prompt.rs/agent_result.rs doc links outside its files_modified), pi F-13 (53-10 misses `launch_stage_inner_resets_checkpoint_resumes_counter`),
  pi S-3 (hermes test deletion justification), pi S-4 (unmeasured 300 s runtime).
