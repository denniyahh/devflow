# Phase 51: Rate-Limit Agent Failover - Context

**Gathered:** 2026-11-05
**Status:** Ready for planning
**Mode:** `/gsd-discuss-phase 51 --auto` (operator convention 2026-11-04/05). Every decision below
was auto-selected as the recommended option and has NOT been individually confirmed by the operator.
The adversarial external review after Plan (codex + agy) is the check on them.

<domain>
## Phase Boundary

When an agent's stage ends `AgentStatus::RateLimited` and a fallback agent is configured, DevFlow
hands the run to the fallback and relaunches the **same stage** in the **same worktree on the same
branch**, with no manual conflict resolution. With no fallback configured, today's path is
unchanged: cron resume instructions are written. Both arms are asserted.

The phase also settles, by observation, whether failover needs Phase 50's supervisor liveness
answer (SUPV-01), and keeps the removed `sequentagent` handoff machinery (`n` verb,
`git.rebase_in`, the rebase handoff) dead.

Three backlog items are folded in by operator decision (2026-11-05): 999.102, 999.71 and 999.154.

Requirement: **SUPV-02**. Release: minor (3.1.0).

</domain>

<decisions>
## Implementation Decisions

### Fallback configuration surface
- **D-01:** One fallback agent, given as `--fallback-agent <AGENT>` on `devflow start` and
  `devflow resume`, persisted in `state-NN.json` (the same persisted-flag pattern as
  `legacy_claude_launch`). No `devflow.toml` key and no environment variable: issue #180 and
  999.41 argue against another config × env pair. Not an ordered list; one hop covers the stated
  need. `resume --fallback-agent` sets or replaces it for a run already in flight.
  — **Reversibility:** costly — the persisted field and the CLI flag become part of the state
  format and operator surface; widening to a list later is additive, removing it is not.
- **D-02:** A fallback equal to the primary agent is refused at `start`/`resume` with a message
  naming both.
- **D-03:** The fallback is validated **at launch**, not at 3 a.m.: `start` (and `resume` when it
  sets one) runs the same checks the handoff will run — `generic_preflight_checks` on a candidate
  state carrying the fallback agent, `ensure_agent_binary`, and the driver's `health` — and refuses
  the launch if the fallback would be refused at handoff. Consequence the planner must carry:
  under `--mode auto`, `unattended_launch_shape_condition` (`crates/devflow-cli/src/preflight.rs`)
  holds only for `claude` and `antigravity`, so a codex/pi/opencode/hermes fallback is refused at
  launch in auto mode. That guard is **not** widened in this phase.

### Handoff mechanism
- **D-04:** On `RateLimited`, if a fallback is configured and unused, perform the same substitution
  `resume_loaded` does today (`crates/devflow-cli/src/pipeline_launch.rs`): full preflight bundle,
  binary check, health, `commit` the agent change, emit `agent_handoff` — with
  `"reason": "rate_limited"` instead of `"resume --agent"` — then relaunch the same stage at once.
  Extract one shared function used by both `resume --agent` and failover; do not duplicate it.
- **D-05:** One hop per run. The handoff marks the fallback consumed in state. A `RateLimited`
  from the fallback takes the cron arm. The run stays on the fallback after the hop; there is no
  automatic switch back to the primary. A later cron resume relaunches with the saved (fallback)
  agent.
- **D-06:** A failover still counts toward the shared `infra_failures` ceiling
  (`MAX_INFRA_FAILURES` = 5, `crates/devflow-core/src/mode.rs`), as `handle_rate_limited_outcome`
  does today, so failover cannot become an unbounded loop.
- **D-07:** If the handoff is refused at runtime (fallback health or preflight fails despite
  passing at launch), fall back to the cron arm and emit an event naming the refusal reason.
  Never a silent skip.
- **D-08:** Zero-commit case (issue #180's open question): continue with the fallback. Same
  worktree, same branch, so nothing is lost by continuing, and pausing ends the night, which is
  what the phase exists to stop.

### Mid-stage continuation
- **D-09:** No new "continue this stage" prompt. The fallback gets the stage's normal prompt and
  relies on the stages' existing re-run safety (GSD execute-phase skips plans that already have a
  SUMMARY; Plan's `idempotent_stage_prompt`). The planner must verify per stage that a re-run on
  top of partial work is safe, and add a test for the Code stage at minimum.
- **D-10:** The worktree is handed over as-is: no reset, stash or clean of the rate-limited
  agent's uncommitted changes. Destroying work is worse than the fallback seeing a dirty tree.
  The state of the tree at handoff (clean or dirty, commit count) is recorded in the
  `agent_handoff` event.

### Proving both arms and the SUPV-01 dependency
- **D-11:** Both arms are asserted by hermetic end-to-end tests that drive the real monitor with
  stub agents emitting a real rate-limit envelope: fallback configured → `agent_handoff` plus a
  same-stage relaunch under the fallback that reaches normal completion; no fallback → cron
  instructions written and no handoff. Each test carries the opposite arm's assertion as its
  negative control (the handoff test asserts no cron file; the cron test asserts no handoff).
- **D-12:** Success criterion 3 is answered by running the failover test with the supervisor
  liveness answer unavailable, and recording the result (works / does not) in a phase artifact,
  not by reasoning from source.
- **D-13:** A real provider rate limit cannot be produced on demand, so criterion 1's proof is the
  hermetic e2e in D-11. If this phase's own dogfood run hits a natural rate limit, that is
  recorded as additional evidence, but it is not a gate.

### Keeping the removed handoff dead (criterion 4)
- **D-14:** A Rust source-scan test (the `doc_check.rs` pattern, comments stripped) asserts no
  `rebase_in`, no `sequentagent` command and no `n` verb. Planner note: `ship.rs` has a test that
  asserts the string `"sequentagent"` is absent from a cron command — a string literal in test
  code, not a comment — so the scan must target definitions and CLI verbs, or allowlist that
  negative assertion with a stated reason, or it will fail correct code.

### Folded backlog items
- **D-15 (999.102):** The idle-timeout reason string and gate context include `terminal_reason`,
  `errors[]` and `origin.kind` from the capture's last `result` event when present
  (`IdleTimeoutRecord`, `crates/devflow-core/src/agent_result.rs`). When they are absent (older
  CLI, child killed before a `result`), today's message is produced unchanged; both cases tested.
- **D-16 (999.71):** Build the measurement harness the entry describes and archive the
  measurements file. Note the entry is partly stale: `claude_stream_events` no longer exists; its
  replacement `ParsedCapture` (`agent_result.rs`) now surfaces torn lines to callers instead of
  dropping them. The open question is still the production rate of torn final lines. Phase 51
  relevance: the rate-limit classification reads the capture's final events, so a torn last line
  could misroute a rate limit; the harness must cover that consumer.
- **D-17 (999.154):** Adopt the review's fix shape: save right after `advance` applies the notify
  variable; re-read the raw field at gate entry as well as at resolution (WR-12); warn on an
  unparseable re-read instead of silently keeping the old copy (IN-10); and add a `resume` test
  with the variable unset that asserts the saved command is kept — the arm a scheduled rate-limit
  resume takes (WR-13). Correct the doc comments at `strip_notify_env` (`monitor.rs`) and
  `gate_notify_cmd` (`state.rs`) that overstate the coverage.

### Claude's Discretion
- Exact field names in `state-NN.json` and the event payload shape, within D-01/D-04/D-10.
- Whether D-04's shared function lives in `pipeline_launch.rs` or `pipeline_outcomes.rs`.
- Plan split and wave layout.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirement and phase definition
- `.planning/REQUIREMENTS.md` — SUPV-02 (and SUPV-01 for the dependency under test)
- `.planning/ROADMAP.md` — Phase 51 entry (success criteria 1-4); backlog entries 999.102, 999.71,
  999.154 (folded in)
- GitHub issue #180 (`gh issue view 180`) — original failover design; its "What exists today" is
  stale (see REQUIREMENTS.md SUPV-02), its "Design attention needed" list is what D-01..D-10 answer

### Prior decisions this phase builds on
- `.planning/phases/50-addressable-monitor-liveness/50-CONTEXT.md` — the liveness answer whose
  necessity criterion 3 measures
- `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-REVIEW.md` — WR-12, WR-13,
  IN-10 (999.154's source)
- `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/30-CODE-REVIEW.md` —
  999.71's source

### Operator docs that must change with the code
- `OPERATIONS.md` — `devflow start` / `devflow resume` rows, `cron-instructions-NN.json` row
- `docs/guides/unattended-mode.md`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `resume_loaded` (`crates/devflow-cli/src/pipeline_launch.rs`): the agent substitution failover
  needs, including the full-preflight-before-mutation guarantee (44-CORE-REVIEW finding 2b).
- `handle_rate_limited_outcome` (`crates/devflow-cli/src/pipeline_outcomes.rs`): the no-fallback
  arm. It writes cron instructions via `build_single_agent_cron_instructions` /
  `write_cron_instructions` (`crates/devflow-core/src/ship.rs`). **There is no function named
  `write_rate_limit_cron`**; the ROADMAP criterion and issue #180 use that name for this path.
- `detect_rate_limit`, `detect_claude_stream_rate_limit` (`agent_result.rs`) and
  `decide_action` (`outcome_policy.rs`, `RateLimited => Action::AutoResume`).
- `gate_or_abort_infra` (`pipeline_outcomes.rs`): the never-silent stall path.

### Established Patterns
- Persisted per-run flags in `State` (`legacy_claude_launch`, `yes_ship`). `State` is
  `#[non_exhaustive]`, so a new field is additive for `devflow-core`'s semver; still run
  `cargo semver-checks check-release -p devflow-core`.
- Named nextest gates (`--no-tests=fail -E 'test(=full::path)'`) and Rust source-scan tests over
  shell counts (CLAUDE.md, Repo Traps).
- Every `DEVFLOW_*` env read and `Command` variant must be documented or allowlisted
  (`doc_check`).

### Integration Points
- The `Action::AutoResume` arm in `pipeline_launch.rs` routes `RateLimited` to
  `handle_rate_limited_outcome`; failover branches there.
- `retry_after_from_reason` lives in `crates/devflow-cli/src/parallel.rs`.

### Known limits found while scouting
- Rate-limit detection exists only for Claude (JSON and stream) and Codex. Pi, OpenCode, Hermes
  and Antigravity primaries are never classified `RateLimited`, so failover can only trigger from
  a Claude or Codex primary.

</code_context>

<specifics>
## Specific Ideas

- Issue #180's target design: the fallback continues "on top of A's commits", one monitor, no
  second worktree, no rebase.
- Dogfood conventions apply to this phase's own run: Ship blocked (`yes_ship` false), close-out by
  hand with `scripts/cut-pr-branch.sh`; any DevFlow defect the run hits is filed as a 999.x entry.

</specifics>

<deferred>
## Deferred Ideas

- Driver-owned rate-limit detection for Pi, OpenCode, Hermes and Antigravity (issue #180's
  "rate-limit detection is per-agent" note; belongs in the driver contract).
- An ordered list of fallback agents (more than one hop).
- Widening `unattended_launch_shape_condition` so non-Claude/Antigravity fallbacks can run in
  `--mode auto`.
- Automatic switch back to the primary agent after its limit resets.

</deferred>

---

*Phase: 51-rate-limit-agent-failover*
*Context gathered: 2026-11-05 (discuss --auto; MemPalace recall/capture hooks skipped: the
MemPalace MCP server failed to connect this session)*
