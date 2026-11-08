# Phase 51: External Adversarial Code Review (after the review fix round)

**Date:** 2026-11-08
**Tree reviewed:** `feature/phase-51` at `b5e7e25` (code unchanged since `c6e29e0`; `git diff 376b645 b5e7e25 -- crates docs README.md OPERATIONS.md` is empty)
**Scope:** the phase's product diff from `b980749~1` (crates/, docs/, README.md, OPERATIONS.md, devflow.toml, doc-check-allowlist.toml), given to each lane as a file in a `git archive` snapshot.
**Lens:** adversarial. Reviewers were told the findings already known (current 51-REVIEW.md, the 51-VERIFICATION.md gap, backlog 999.187-999.193) and asked for new ones.

| Lane | Model | Result |
|------|-------|--------|
| codex | gpt-6.1-sol, high | complete; ran paired parser probes for findings 1-3 and the doc_check probe |
| pi | deepseek-flash | complete; read-only, no build |
| agy | gemini-3.8-flash-high, high | **PARTIAL**: print timeout at 30 min with the turn in progress (`DEADLINE_EXCEEDED`); 3 findings returned |

The snapshot was compared with a fresh `git archive` afterwards: no tracked file changed.

Every finding below was checked against the source by the orchestrating session; the check is named.

## Confirmed

| # | Sev | Lane | Finding | Check |
|---|-----|------|---------|-------|
| X-01 | major | codex | **A quota denial followed by a torn line: the monitor and the verdict disagree.** `capture_rate_limit_denial` (agent_result.rs:2590, new in this phase) returns `Denied` before it looks at a torn tail; the stream verdict path (agent_result.rs:1902) checks the torn tail first and returns an indeterminate `Failed`. The monitor (monitor.rs:2747-2763) then logs "classified as rate-limited (resumable), not as a hang", suppresses the idle-timeout record and kills the agent, while the verdict blocks failover. | Read both orderings; codex reproduced it with a probe binary. Not run live. |
| X-02 | major | pi | **A recorded canary refusal has no retry path for the same agent.** Only a handoff clears `state.canary` (pipeline_launch.rs:3208); `resume --agent <current agent>` is a silent no-op (pipeline_launch.rs:3569-3570 has no else arm). The new `gate_relaunch_refused` remedy (OPERATIONS.md:334-349) therefore requires switching agents or hand-editing state after a transient canary failure. | `rg 'canary = '` outside tests: only 592 (records) and 3208 (clears). |
| X-03 | major (test gap) | codex, pi | **No failover test runs inside a phase worktree.** `failover_e2e.rs:277` passes `--no-worktree` and the file never sets `worktree_path`; the dirty-tree unit test calls `hand_off_agent` only. Criterion 1 ("in the same worktree on the same branch") and D-10's `tree_dirty` measurement (pipeline_launch.rs:3190) are untested on the worktree arm. | `rg -- --no-worktree`, `rg -c worktree_path` = 0 in failover_e2e.rs. |
| X-04 | major (evidence) | pi | **51-SUPV01-EVIDENCE.md's line citations are stale** (pipeline_launch.rs entries off by +26: cited :2391 is a doc comment, `if successor_runs_on` is at :2417; commands.rs off by varying amounts), and the checker validates only that the file exists and the name appears somewhere. | Read :2391 and :2417. Checker behavior per pi, not re-read. |
| X-05 | minor | codex | OPERATIONS.md:423 says the cron arm writes no file when the retry time does not parse; pipeline_outcomes.rs:106 writes `cron-instructions-NN.json` before the empty-schedule check at :117. | Read both. |
| X-06 | minor | pi | README.md:110 says a fallback that fails to relaunch "stops at a gate"; at the infra ceiling it aborts (OPERATIONS.md:327 and `gate_or_abort_infra` say so). | Read README:110. |
| X-07 | minor | pi | `pub fn capture_shows_rate_limit_denial` (agent_result.rs:2555) has no production caller after this phase (only its own tests), and still gives the two-valued answer D-16 replaced; a second notion of "is this a rate limit" now exists, against its own doc comment. | `rg` outside tests. |
| X-08 | minor | pi | `devflow gate list` renders gate context at 100 chars (commands.rs:2092, :2148), cutting the D-15 idle-timeout cause clause that was budgeted for the 300-char banner. | Read both lines. |
| X-09 | minor | codex | OPERATIONS.md:305 promises only top-level events decide a rate limit; the single-document classifier (agent_result.rs:258, `json_has_*` via recursive `json_scan`) matches a nested `status_code: 429`. Pre-existing code (2026-07/08); applies to single-document captures only, not the stream launches this run used. | Read the classifier and `json_scan` callers; blame dates. |
| X-10 | minor | agy, pi | `resume --agent X --fallback-agent Y` commits twice (hand_off_agent, then resume_loaded). A kill between them can leave `agent == fallback_agent` unconsumed; the next plain resume is refused with a message that names the recovery (`--fallback-agent <other>`). agy's "permanently unresumable" is overstated. | Read refuse_fallback_for_resume (pipeline_launch.rs:3685-3707). |
| X-11 | minor | agy | A missing fallback binary is reported by `ensure_agent_binary` (preflight.rs:1107) without the `--fallback-agent` prefix every other fallback refusal carries. | Read. |
| X-12 | minor | codex | doc_check's removed-verb guard compares undecoded string literals (doc_check.rs:1486, :1551), so `alias = "\x6e"` evades it. | Read; codex probe. |

## Suspected (not verified)

- **codex:** a hanging fallback health check can park the handoff indefinitely: the handoff calls `health()` synchronously (pipeline_launch.rs:3184) and pi's health runs `pi auth check` via `.output()` with no deadline (agents/pi.rs:90). No forced-hang experiment.
- **codex:** a stream `result` with `subtype: error_rate_limit` is classified `Failed`, not `RateLimited` (the stream path reads only `rate_limit_event`, agent_result.rs:1770). Whether current Claude CLIs emit that shape in stream mode is not established.
- **pi:** `devflow status` shows nothing about an armed or spent fallback (`fallback_agent` / `fallback_consumed` have no reader outside the failover path). Enhancement, adjacent to 999.190.
- **pi (test gaps):** no hop test with a non-Claude primary; the idle-timeout gate-context test composes its own rendering; no test of a failed relaunch at `MAX_INFRA_FAILURES - 1`.

## Dismissed

- **pi #8 (three commit ids for one tree):** the code is unchanged between the verified commit `376b645` and the reviewed `b5e7e25` (empty diff above); the later commits are docs only.
- **agy #1 (`resume --fallback-agent` accepted at Plan/Validate/Ship is inert):** the armed fallback applies when the run reaches Code (Plan advances to Code; Validate loops back to it), and the notice says "takes over the Code stage". Only a resume at Ship has no Code ahead; not worth a finding.
