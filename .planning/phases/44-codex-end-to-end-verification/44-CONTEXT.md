# Phase 44: Codex End-to-End Verification - Context

**Gathered:** 2026-08-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 44 proves `devflow`'s Codex driver through a real end-to-end phase run and uses the run as
dogfood for three rate-limit/resume hardening issues surfaced from Phase 43. Scope is locked:
CODE-01 plus GitHub issues #147, #148, and #153. The phase must not re-open the milestone boundary
or add unrelated driver work.

In scope:
- CODE-01: `--agent codex` is verified through a real phase run; any surfaced gaps are closed or
  re-filed with evidence.
- #147 / Linear DEN-60: supported agent handoff for an active run, centered on
  `devflow resume --phase N --agent <agent>`.
- #148: make emitted Hermes cron resume instructions usable by removing the nonexistent
  `--from-devflow` path and fixing UTC-vs-local schedule semantics.
- #153: delete consumed cron-instruction records after successful resume/ship.

Out of scope:
- New agent drivers or broad driver framework redesign.
- Planning or executing a different Phase 44 scope.
- Advancing beyond discuss during this workflow run.
- Running cargo build/test during discussion.

</domain>

<decisions>
## Implementation Decisions

### Codex E2E Dogfood (CODE-01)
- **D-01: Phase 44 is both verification and hardening vehicle.** The Codex run is not a separate
  ceremonial acceptance pass; it should exercise the hardening surface in #147/#148/#153 where
  practical, then record which gaps were closed and which were re-filed.
- **D-02: Codex must not be asked to run interactive Define/Plan prompts.** Codex's driver declares
  `Define` and `Plan` as `RequiresExistingArtifact`; this context exists so downstream planning can
  create executable artifacts before any live Codex E2E run expects headless progress.
- **D-03: Success evidence must distinguish "completed a phase" from "surfaced a gap."** A real
  Codex run may satisfy CODE-01 by completing or by producing concrete re-filed gaps, but the final
  verification must say which happened and must not treat local green tests alone as Codex E2E
  proof.
- **D-04: Driver parity guard is required.** Any handoff/resume changes must preserve current Codex
  launch invariants: `codex -a never exec --sandbox workspace-write --json`, writable roots for
  linked-worktree git metadata, scoped signing disable env, Codex-native workflow prompt rendering,
  and existing conformance tests.

### Agent Handoff via Resume (#147 / DEN-60)
- **D-05: Implement handoff as `devflow resume --phase N --agent <AGENT>`.** This is the primary
  surface because it preserves the existing "continue the saved stage" contract. A separate
  `devflow handoff` command is deferred unless planning finds the resume flag creates an
  unresolvable CLI ambiguity.
- **D-06: Handoff mutates only the saved agent, not the run identity.** Preserve stage, mode,
  worktree path, branch, gates, verification baseline/fingerprint, `stop_until`, `yes_ship`,
  failure counters, checkpoint counters, and any other persisted state not directly about the
  selected driver.
- **D-07: Persist the new agent before monitor relaunch and emit an audit event.** The state file
  must show the new agent before the detached monitor can read it. `.devflow/events.jsonl` gets a
  handoff event with phase, from-agent, to-agent, stage, and reason/source (`resume --agent`).
- **D-08: Refuse unsafe handoffs with the same preflight seriousness as start.** Handoff to a driver
  that cannot run the current saved stage headlessly must fail before state mutation. Codex at
  `Define`/`Plan` is the key negative case unless the required artifact already exists and the
  existing `RequiresExistingArtifact` rule allows continuation.
- **D-09: Rate-limited state is an allowed handoff source.** A parked/rate-limited run is the
  motivating case; `resume --agent codex` must be able to replace the current agent when the saved
  stage can safely be relaunched under Codex.

### Hermes Cron Resume Contract (#148)
- **D-10: Remove the printed `hermes cron create --from-devflow` instruction.** The installed Hermes
  CLI does not support that flag, and current source still prints it in `cron_hint_line`; leaving it
  would keep the operator-facing instruction unusable.
- **D-11: Use the existing Hermes cron surface, not a speculative hidden intake flag.** Emit a
  command that can be built from current documented flags, preferably a one-shot `--script` /
  `--no-agent` style watchdog that runs the persisted `devflow resume --phase N` command from the
  cron record. If planning discovers a stable Hermes contract has landed, it may use that only after
  verifying the installed CLI help exposes it.
- **D-12: Generate schedules in the timezone Hermes will interpret, or avoid cron-field ambiguity.**
  Current `ship.rs` normalizes retry timestamps to UTC cron fields; that is wrong when Hermes cron
  interprets `M H D M W` in America/New_York/local scheduler time. The planned fix must either
  convert absolute retry time into the scheduler's local time before emitting cron fields or use a
  timezone-aware Hermes mechanism if one exists and is verified.
- **D-13: Keep unparseable retry times fail-closed.** Existing behavior that avoids turning bad
  retry text into `* * * * *` is correct and must be retained.
- **D-14: Validate both sides of the cron hint.** Tests need a positive local-time conversion case
  and a negative control showing the old UTC-field schedule would fire at the wrong local instant.
  The CLI hint test must assert the unsupported `--from-devflow` string is absent.

### Cron Record Consumption (#153)
- **D-15: Delete cron-instruction records only after genuine consumption.** Consumption means a
  resume relaunch actually started, or the phase completed/ship cleanup reached a terminal success
  point. Invoking `devflow resume` and failing before monitor launch must keep the record.
- **D-16: Tie resume-side deletion to confirmed launch evidence.** The safest local signal is after
  `launch_stage` succeeds and the state/monitor pid has been written. Do not delete before the
  relaunch is durable.
- **D-17: Ship-side cleanup is belt-and-braces.** Delete any remaining phase cron record on
  successful ship/completion, using the existing idempotent `delete_cron_instructions`.
- **D-18: Emit an audit event for deletion.** Record phase, path kind (`per-phase` vs legacy if
  knowable), and trigger (`resume_consumed` or `ship_complete`) in `.devflow/events.jsonl`.
- **D-19: Do not change recover/clean semantics for unconsumed records.** Existing `recover::clean`
  deletion remains a reset/cleanup path, not the consumption definition.

### Claude's Discretion
- Exact event field names for handoff and cron deletion, provided they are documented in tests and
  consistent with existing event naming style.
- Whether `resume --agent <same-agent>` is accepted as an idempotent resume or rejected as a no-op.
- Exact helper boundaries for local-time conversion and Hermes command rendering.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Scope & Requirements
- `.planning/ROADMAP.md` § "Phase 44: Codex End-to-End Verification" — CODE-01 goal and success
  criteria.
- `.planning/REQUIREMENTS.md` § "Codex" — CODE-01.
- GitHub issue #147 / Linear DEN-60 — active-run handoff via resume; fetched live during discussion:
  `https://github.com/denniyahh/devflow/issues/147`.
- GitHub issue #148 — unsupported `--from-devflow` and UTC-vs-local Hermes schedule mismatch;
  fetched live during discussion: `https://github.com/denniyahh/devflow/issues/148`.
- GitHub issue #153 — consumed cron-instruction cleanup; fetched live during discussion:
  `https://github.com/denniyahh/devflow/issues/153`.

### Source Surfaces
- `crates/devflow-cli/src/main.rs` — `Resume` CLI currently has `--phase` and
  `--legacy-claude-launch`, but no `--agent`; dispatch calls `resume(..., legacy_claude_launch)`.
- `crates/devflow-cli/src/pipeline_launch.rs` — `resume` loads saved state, clears fired stop
  markers, applies legacy Claude opt-out, repairs leaked auto-chain flags, saves state, and calls
  `launch_stage`.
- `crates/devflow-core/src/state.rs` — `AgentKind` parsing/display and persisted state fields that
  handoff must preserve.
- `crates/devflow-core/src/agents/codex.rs` — Codex launch argv, prompt rendering, writable roots,
  signing-disable environment, and `RequiresExistingArtifact` policy for Define/Plan.
- `crates/devflow-core/src/agents/mod.rs` — `AgentDriver` trait, driver conformance suite, prompt
  compatibility assertions, and capability defaults.
- `crates/devflow-core/src/ship.rs` — cron-instruction schema, schedule construction,
  `build_single_agent_cron_instructions`, and idempotent `delete_cron_instructions`.
- `crates/devflow-cli/src/commands.rs` — `cron_hint_line` still prints
  `hermes cron create --from-devflow`; status/log recovery hints use resume.
- `crates/devflow-cli/src/pipeline_outcomes.rs` — rate-limit/auto-resume scheduling and
  `rate_limit_resume_scheduled` event path.
- `crates/devflow-core/src/recover.rs` — current cleanup-only call sites for
  `delete_cron_instructions`.

### Prior Driver & Dogfood Precedents
- `.planning/phases/43-opencode-driver-completion/43-CONTEXT.md` — recent driver-completion
  precedent; emphasizes live capture evidence and parser/health negative controls.
- `.planning/phases/42-hermes-driver/42-CONTEXT.md` — supervised dogfood precedent and headless
  Hermes launch contract.
- `.planning/phases/41-antigravity-driver/41-CONTEXT.md` — cautionary precedent against assuming
  CLI transport/schema from help output alone.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ship::delete_cron_instructions` is already idempotent and legacy-compatible.
- `workflow::save_state`, `launch_stage`, and existing `resume` save-before-launch ordering provide
  the right mutation point for `resume --agent`.
- Existing event emission in `pipeline_outcomes.rs` gives a naming/style precedent for
  `handoff` and cron-consumption events.
- Test support uses real temporary repositories, stub agent binaries on PATH, bounded polling, and
  state/event assertions.

### Established Patterns
- Agent behavior is modular behind `AgentDriver`, but launch preflight and resume state mutation
  live in the CLI pipeline.
- Headless-safety is stage-specific, not agent-global.
- Rate-limit recovery writes per-phase `.devflow/cron-instructions-{NN}.json` and must never fall
  back to every-minute cron for unparseable retry text.
- Git/worktree behavior should be tested with real repositories rather than mocked Git.

### Integration Points
- Add `agent: Option<AgentKind>` to `Command::Resume` and thread it to
  `pipeline_launch::resume`.
- In `resume`, validate requested handoff, update `state.agent`, save state, emit handoff event,
  then relaunch.
- Replace `cron_hint_line` output and tests with a Hermes-supported command contract.
- Update `ship` schedule rendering to match scheduler-local interpretation or a verified
  timezone-aware Hermes intake.
- Delete cron records after confirmed resume launch and successful ship completion; leave failed
  relaunch paths preserving records.

</code_context>

<specifics>
## Specific Ideas

- The old #148 example is the canonical negative control: retry reset at 14:40 UTC must not emit a
  local cron field that Hermes interprets as 14:40 America/New_York.
- For #147, a useful acceptance fixture is a saved `state-NN.json` at a non-Define stage with
  `agent: claude`, a worktree path, counters, gates/baseline fields, and a stubbed Codex launch;
  after `resume --agent codex`, only the agent and monitor-launch fields should change.
- For #153, keep a negative-control test where `launch_stage` fails; the cron record must remain.

</specifics>

<open_questions>
## Open Questions

None before planning. The phase scope and preferred direction are locked for planning. Planner may
surface operator questions only if installed Hermes exposes multiple verified, incompatible cron
contracts that would change user-facing behavior.

</open_questions>

<deferred>
## Deferred Ideas

- A separate `devflow handoff --phase N --to <AGENT>` command is deferred unless `resume --agent`
  proves structurally unsuitable.
- General scheduler abstraction beyond Hermes is out of scope.
- Broad cleanup of historical Codex research/docs is out of scope except where needed for CODE-01
  evidence.
- New agent drivers, extra dashboard/status UI, and generic event-schema redesign are out of scope.

### Reviewed Todos (not folded)
None — `gsd-tools query todo.match-phase 44` returned no matches.

</deferred>

---

*Phase: 44-Codex End-to-End Verification*
*Context gathered: 2026-08-25*
