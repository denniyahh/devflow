# 30a evidence — 999.64 feasibility experiments (2026-08-01)

Raw artifacts backing the Phase 30 entry's 30a section in `.planning/ROADMAP.md`. The JSONL
logs are the evidence of record; the harness scripts' own printed verdicts are NOT (v1's
verdict was wrong, and v2's harness hung after its evidence was already captured).

| File | What it is |
|---|---|
| `run_experiment.py` + `raw_output.jsonl` | **v1 — INVALID.** Turn-detection heuristic mistook subagent-forwarded narration (`parent_tool_use_id` set) for orchestrator resumption. Kept as the recorded pitfall: always discriminate on `parent_tool_use_id == null`. |
| `run_experiment_v2.py` + `raw_output_v2.jsonl` | **v2 — single background child.** Proves: `result` fires, process survives on open stdin, child completes, new top-level turn follows with `origin: {kind: "task-notification"}` (event 24). Harness hung afterward on a blocking `readline` — post-evidence, harness-only. |
| `run_experiment_v3.py` + `raw_output_v3.jsonl` | **v3 — two concurrent children (10s/22s), the Phase 29 wave-2 shape.** Both completions delivered independently (`result` events at lines 36 and 53, both `origin: task-notification`); exit 0.38s after stdin close (measured in harness stdout — **unarchived**, re-measure per review finding M1). |

Independently re-verified twice: once by the session orchestrator (raw JSON re-parse,
session_id/task_id/tool_use_id cross-reference), once by an adversarial Fable review that
attacked the interpretation and failed to break the E-claims (its findings against the
*implementation plan* are recorded as the six binding constraints in the ROADMAP entry).

Environment caveat (review finding M4): these ran inside an interactive Claude Code session's
environment (inherited env vars, merged stderr, no worktree). The deciding production test —
running the v3 harness through `spawn_monitor` itself — is assigned to 30b's monitor plan.

Observed CLI version: `claude_code_version 2.1.220` (in the `init` events). The
task-notification resume behavior is undocumented and unpinned — see review finding M2.
