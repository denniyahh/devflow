# Phase 51: Rate-Limit Agent Failover - Context

**Gathered:** 2026-11-05
**Status:** Ready for planning
**Mode:** `/gsd-discuss-phase 51 --auto` (operator convention 2026-11-04/05). Every decision below
was auto-selected as the recommended option and has NOT been individually confirmed by the operator.
The draft was adversarially reviewed before planning (see the revision note under Decisions); the
review after Plan is the next check.

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

**Revised 2026-11-05 after an adversarial review of the first draft** (codex gpt-6.1-sol and
DeepSeek Flash via pi; agy dropped on a quota error). Every finding below marked *(review)* was
checked against source before it changed a decision. The review files are not committed; the
findings that changed something are summarised in `51-DISCUSSION-LOG.md`.

### Operator rulings (2026-11-05) — these supersede the decisions they name
The operator reviewed the auto-selected decisions after planning started and ruled:
- **R-1 (D-15):** keep `IdleTimeoutRecord`'s public shape strict. It is all-pub and not
  `#[non_exhaustive]` in the published `devflow-core`, so a new field is a semver break; the cause
  fields live in a separate type or record, and the release stays 3.1.0.
- **R-2 (D-09):** failover applies to the **Code stage only**. A `RateLimited` Plan stage takes the
  cron arm even when a fallback is configured, and the fallback stays unused. The Plan half of
  D-09 (finishing a partial plan) is dropped from this phase; Code's half (replay the interrupted
  fix intent) stays.
- **R-3 (D-01):** add a `devflow.toml` key for the fallback agent, combined with the
  `--fallback-agent` flag the way `yes_ship` combines its key with `--yes-ship` (the flag wins
  when given). No new environment variable.
- **R-4 (D-16):** do the classifier fix (the rate-limit check sees a torn final line, tested) and
  return the production-rate measurement (harness, controlled measurement, archived measurements
  file) to backlog 999.71.
- **R-5 (D-13):** stub-agent proof is enough for criterion 1; no live forced-rate-limit run.

### Fallback configuration surface
- **D-01:** One fallback agent, given as `--fallback-agent <AGENT>` on `devflow start`, and on
  `devflow resume` of a run that is not live (stopped, or paused on the cron arm), persisted in
  `state-NN.json` (the same persisted-flag pattern as `legacy_claude_launch`). No `devflow.toml`
  key and no environment variable: issue #180 and 999.41 argue against another config × env pair.
  Not an ordered list. *(review)* A fallback **cannot** be armed on a live run: `resume` refuses
  over one (`refuse_launch_over_a_live_run`, `crates/devflow-cli/src/commands.rs`), and this phase
  does not add a non-launching write path. Setting a fallback with `resume` clears the
  consumed mark (D-05), so a run can be re-armed after a hop.
  — **Reversibility:** costly — the persisted field and the CLI flag become part of the state
  format and operator surface; widening to a list later is additive, removing it is not.
- **D-02:** A fallback equal to the primary agent is refused at `start`/`resume` with a message
  naming both.
- **D-03:** *(review: revised)* Launch-time validation is **best effort, not a guarantee**.
  `generic_preflight_checks` is stage-dependent (`major_bump_check_applies` is Ship-only;
  `unattended_launch_check_applies` covers only Define and Code, `crates/devflow-cli/src/preflight.rs`),
  so it cannot be fully evaluated at `start` for a handoff that may happen at any stage. At launch,
  check what is stage-independent or knowable: the binary (`ensure_agent_binary`), the driver's
  `health`, and the fallback against the Code-stage launch-shape condition. Consequence: under
  `--mode auto`, `unattended_launch_shape_condition` holds only for `claude` and `antigravity`, so
  any other fallback is refused at launch in auto mode; that guard is **not** widened here. The
  full stage-dependent bundle runs again at handoff (D-07).

### Handoff mechanism
- **D-04:** On `RateLimited`, if a fallback is configured and unused, perform the substitution
  `resume_loaded` does today (`crates/devflow-cli/src/pipeline_launch.rs`): full preflight bundle
  for the current stage, binary check, health, `commit` the agent change, emit `agent_handoff`
  with `"reason": "rate_limited"`, then relaunch the same stage at once. One shared function serves
  both `resume --agent` and failover. Two additions *(review)*:
  - **Canary evidence is per agent.** `canary_gate` reuses a recorded `state.canary` without
    re-running it, and the canary launcher differs per agent (`canary_launcher_for`, same file).
    The substitution must invalidate the recorded canary so the fallback's own canary runs. This
    also fixes `resume --agent` today. Negative control: a Claude `Confirmed` canary must not
    admit an Antigravity launch whose canary returns `Absent`.
  - **A relaunch that fails after the commit is never silent.** `resume_loaded` re-marks the
    phase stopped when its launch fails (44-CORE-REVIEW finding 2a); failover needs an equivalent
    recovery, ending in a gate or the cron arm, never in a phase recorded as handed off with
    nothing running. Tested by injecting a spawn failure after the commit.
- **D-05:** One hop per run. The handoff marks the fallback consumed in state. A `RateLimited`
  from the fallback takes the cron arm. The run stays on the fallback; no automatic switch back.
  A later cron resume relaunches with the saved (fallback) agent.
- **D-06:** *(review: revised)* The failover branch is taken **before** the infra-ceiling check and
  does **not** bump `infra_failures`. D-05's one-hop mark is what bounds failover. Reason: the
  ceiling (`MAX_INFRA_FAILURES` = 5) is shared with `Ambiguous` re-drives, so charging failover to
  it would let earlier transport cancels cost the run its configured fallback. The cron arm keeps
  today's bump-then-check behaviour. Boundary test: with `infra_failures` at
  `MAX_INFRA_FAILURES - 1` and an unused fallback, a rate limit hands off instead of gating.
- **D-07:** If the handoff is refused at runtime (the current stage's preflight, binary or
  health fails), take the cron arm and emit an event naming the refusal reason. Never a silent
  skip. Accepted limit: because D-03 cannot check every stage at launch, a refusal here can still
  end the night; the event makes it visible.
- **D-08:** Zero-commit case (issue #180's open question): continue with the fallback.

### Mid-stage continuation
- **D-09:** *(review: revised)* The fallback must continue **the interrupted operation**, not the
  stage's default prompt. Two concrete gaps make the default unsafe:
  - **Plan:** `idempotent_stage_prompt` (`crates/devflow-core/src/prompt.rs`) treats the mere
    presence of a `PLAN.md` as a completed stage and forbids touching it. A Plan rate-limited
    midway can leave a partial plan file, which the fallback would then accept as done. A Plan
    failover must not take that shortcut.
  - **Code:** a relaunch rebuilds `StageIntent::Code { fix: None }`, but a Code pass may have been
    an audit-fix loop-back (`FixType::AuditFix`, `crates/devflow-cli/src/pipeline_outcomes.rs`),
    which runs a different workflow. The interrupted intent, including its fix type, must be
    persisted and replayed to the fallback.
  Acceptance (falsifiable): a Plan failover with an incomplete plan file on disk does not report
  the stage done without completing it (negative control: a complete plan still short-circuits);
  a Code failover during an audit-fix pass runs the audit-fix workflow (negative control: a plain
  Code failover runs execute-phase). The mechanism is left to the planner.
- **D-10:** The worktree is handed over as-is: no reset, stash or clean of uncommitted changes.
  This matches today's cron arm, which already restarts on whatever tree the rate-limited agent
  left. It differs from the idle-timeout rule (`outcome_policy.rs`: an idle timeout is terminal
  because the run's extent is unknown) because a rate limit is a reported stop. The tree state at
  handoff (clean or dirty, commit count) is recorded in the `agent_handoff` event.

### Proving both arms and the SUPV-01 dependency
- **D-11:** Both arms are asserted by hermetic end-to-end tests that drive the real monitor with
  stub agents emitting a real rate-limit envelope: fallback configured → `agent_handoff` plus a
  same-stage relaunch under the fallback that reaches normal completion; no fallback → cron
  instructions written and no handoff. Each test asserts the opposite arm's absence as its
  negative control.
- **D-12:** *(review: revised)* "Liveness unavailable" is not a switch that exists. The review
  traced the failover relaunch through `launch_stage` inside the running supervisor, not through
  `reserve_launch`, which is where foreground launches consult liveness; the cron arm's scheduled
  `devflow resume` does go through `reserve_launch`. The planner must first trace which liveness
  reads each arm actually makes and record that trace. Then, for each read found, withhold that
  specific answer, with a healthy baseline and an ordinary relaunch under the same intervention as
  controls, so a failure can be attributed to failover rather than to launching being disabled.
  If the failover arm makes no liveness read, that trace **is** the measured result for
  criterion 3 and is recorded as such. Do not record "works" from a test whose outcome could not
  have differed.
- **D-13:** A real provider rate limit cannot be produced on demand, so criterion 1's proof is the
  hermetic e2e in D-11. A natural rate limit during this phase's own dogfood run is recorded as
  additional evidence, not a gate.

### Keeping the removed handoff dead (criterion 4)
- **D-14:** A Rust source-scan test (the `doc_check.rs` pattern, comments stripped) asserts no
  `rebase_in` definition or call, no `sequentagent` command and no `n` verb. *(review)* Verbs are
  clap variants of `enum Command` (`crates/devflow-cli/src/main.rs`): the scan checks variant
  names, `#[command(name = …)]` and `alias` attributes, not free text. The test-code literal
  `"sequentagent"` in `ship.rs`'s negative assertion must not trip it. Negative control: a
  fixture source containing a `Sequentagent` variant or an `alias = "n"` fails the scan.

### Folded backlog items
- **D-15 (999.102):** *(review: revised)* `IdleTimeoutRecord` (`agent_result.rs`) has **no**
  free-text field (999.102's premise is stale), and the idle-timeout reason is built from that
  side-channel record, not from the capture. So the monitor captures `terminal_reason`,
  `errors[]` and `origin.kind` from the last `result` event when it writes the record, as new
  **optional** fields (`#[serde(default)]`), and the reason string and gate context include them
  when present. Tests: fields present → named in the reason; fields absent (older CLI, killed
  before a `result`) → today's message; **a pre-51 record on disk still parses** (a required
  field would turn every existing record into the "unreadable" message).
- **D-16 (999.71):** Build the measurement harness the entry describes and archive the
  measurements file. The entry is partly stale: `claude_stream_events` no longer exists, and
  `ParsedCapture` (`agent_result.rs`) now reports torn lines via `line_shapes`. *(review)* The
  rate-limit classifier is called with `ParsedCapture::parse(&raw).events` only, so it cannot see
  a torn final line today. In scope: the classifier is made to receive the torn-line signal and
  is tested with a torn final line (a torn rate-limit event must not be read as a clean,
  non-rate-limited ending). The production-rate measurement is a separate deliverable and is not
  a gate on the phase's other criteria.
- **D-17 (999.154):** *(review: revised)* Part of this entry is superseded by Phase 50.1:
  `advance` already commits the notify command (`commit_notify_env`), and gate entry commits only
  `gate_pending` through `commit`, which reloads the on-disk state. Reproduce WR-12 and IN-10
  against current code before fixing them; close either one that no longer reproduces, with the
  reproduction attempt as evidence. WR-13 stands as is: add a `resume` test with the variable
  unset that asserts the saved command is kept, which is the arm a scheduled rate-limit resume
  takes. Correct the `strip_notify_env` (`monitor.rs`) and `gate_notify_cmd` (`state.rs`) doc
  comments if they still overstate the coverage.

### Claude's Discretion
- Exact field names in `state-NN.json` and the event payload shape, within D-01/D-04/D-10.
- Where the shared handoff function and the persisted interrupted intent live.
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
