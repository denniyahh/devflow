# Phase 31: Claude Adapter Launch Path — Pipe-Owning Monitor (999.64 arc close) - Context

**Gathered:** 2026-08-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace the detached `sh` monitor with a pipe-owning one for the Claude adapter, and make
`stream-json` the adapter's mode — so 30b's stream parser becomes reachable in production for the
first time and a multi-plan wave stops orphaning delegated work.

This closes the 999.64 arc. The phase is verified against the arc goal itself: a DevFlow-driven
phase containing a multi-plan wave completes that wave without orphaning delegated work.

**Not in this phase:** 999.65, 999.66, 999.46, 999.70, 999.71, anything release-related. 999.67
folds in (XS, same file).

</domain>

<decisions>
## Implementation Decisions

### Idle-timeout policy

- **D-01:** The idle timer resets on **every stream line** the monitor reads, not on milestone
  events only. 30d measured every-line gaps at 6.02–7.09s versus milestone gaps at 7.70–13.73s;
  the tighter signal carries more margin under a given timeout. It also rejects the
  "chatty but stuck" blind spot that a both-signals rule would have created.

- **D-02:** Idle timeout is **30s**, the measured constraint-8 floor.
  **Note for downstream readers:** the ≥30s floor was derived from the *milestone* signal (pooled
  max 13.73s). Against the every-line signal chosen in D-01 the observed max is 7.09s, so 30s is
  ~4.2x margin — comfortable, not marginal. Do not "correct" this to a larger value on the
  assumption that 30s is tight; it is tight only against a signal this phase is not using.

- **D-03:** **No outer wall-clock bound.** Idle-only. Constraint 5 rejected fixed wall-clock
  because no single value is safe for both hangs and legitimate ~47-minute stages; a healthy long
  stage keeps resetting the idle timer and is already protected.

- **D-04:** The timeout is **configurable but clamped** so it can never be set below the 30s floor,
  and the clamp **logs loudly** when it engages. Mirrors the existing `DEVFLOW_GATE_TIMEOUT_SECS`
  handling. Because the default is the floor, the value can only be raised. A silent clamp would
  be the exact failure class this project keeps paying for.

### What firing the timeout does

- **D-05:** On firing, **write the authoritative result to disk first, then terminate the child.**
  The result must exist before anything can race it, so neither Layer 2 nor an exit-code path can
  overwrite the verdict. This is the ordering constraint 5 implies without stating.
  — **Reversibility:** costly — the ordering is the whole defense; reversing it reintroduces the
  window where the child is dead and no authoritative result exists, which is what Layer 2 fills
  in wrongly.

- **D-06:** An idle timeout records a **distinct first-class status** (e.g.
  `AgentStatus::IdleTimeout`), separate from both `Failed` and `ResourceKilled`. Constraint 5's
  complaint is precisely that a kill mis-classifies as OOM; only a distinct variant makes
  "we gave up waiting" legible to the completion oracle rather than to prose alone.
  — **Reversibility:** costly — adding a status variant touches every exhaustive match on
  `AgentStatus`; removing it later means re-deciding how every call site treats the case.

- **D-07:** The recorded result **fails loudly and enumerates the commits** the agent made before
  going quiet. That is the exact 999.64 shape — real commits, no summary — and naming them turns a
  silent miscount into something an operator can act on. Do **not** roll the commits back: that
  destroys real work on what may be a false-positive timeout, and this repo treats irreversible
  operations as needing review rather than tests.

- **D-08:** An idle timeout is **terminal, not retryable**. The run is in an unknown state with
  possibly orphaned work; stop at a never-silent gate and report. A retry would start from a
  dirty, partly-done state — the condition 999.65 and 999.66 already describe.

### Rollout shape

- **D-09:** **Sequence the rollout: one stage first, then widen.** Not a launch-time prediction of
  agent behaviour (constraint 1 forbids that) — an explicit sequencing choice, which constraint 1
  permits. The reason is evidentiary: every gate fixture today is labelled SYNTHETIC in-source and
  no archived capture contains a prompt echo, so the parser's production correctness is currently
  *reasoned, not witnessed*. Landing one stage produces the first real capture to verify against.

- **D-10:** **Code is the first stage.** It is where 999.64 was observed — Phase 29 wave 2
  dispatched two executors from Code and orphaned both — and it is the stage that actually
  backgrounds, so it is the only one that exercises task-notification delivery and the drain gate
  at all. Define would have been a proxy measurement.

- **D-11:** An **explicit opt-out flag, off by default**, can force the old single-document path.
  Recovery without a release. Its use must be logged loudly — an escape hatch used routinely
  erodes what it protects. **Automatic fallback on parse failure is rejected:** a silent downgrade
  is the same invisible-degradation class as the bug being fixed.

- **D-12:** The shipped single-document path is protected by **30b's existing isolation tests** —
  which already prove neither shipped capture shape is hijacked, and guard against keying the
  stream gate on `type: "result"` — plus the inverse assertion for the new argv. Treat those as
  the contract this phase must not break, rather than adding a full adapter-matrix test that would
  strain the M cap.

### CLI-behaviour guard (review M2)

- **D-13:** The guard is a **startup canary with a declared token**, not a version-string check.
  The premise this arc rests on — `task-notification` delivery — is undocumented CLI behaviour
  observed only on `claude_code_version: 2.1.220`, so a version number is a *proxy* for the
  behaviour rather than the behaviour. One throwaway task at pipeline start declares the success
  token it will return; the orchestrator records the token up front and confirms it comes back.

  **Two traps this must be built around, both already discovered in Phase 30:**
  1. **Prompt echo.** The stream echoes the prompt back — that is what created the checkpoint
     false-positive 30-05 fixed. The planted token *will* appear in the stream as an echo, so it
     must be matched **only inside a top-level `result` event**, reusing `is_top_level` /
     `claude_stream_gate_shape` rather than introducing a new trust path.
  2. **It proves delivery, not work.** The agent can see the token in its own prompt and could
     emit it without doing anything — that is 999.67's shape, folded into this phase. The token
     means "the notification path works", never "the work happened". Summaries and merges remain
     the evidence of work (see D-16).

- **D-14:** Scope of the token mechanism is the **startup canary only**, not every dispatched
  child. Per-child tokens would give strictly better signal — they would defeat constraint 7's
  coalescing undercount directly instead of leaning on the drain gate — but that is a second
  mechanism layered on the monitor rewrite and would push the phase past its M cap. Recorded as a
  deferred idea, not discarded.

- **D-15:** When the guard reports the behaviour absent or unverified, **refuse to run and report
  clearly**. If delivery is gone, every multi-plan wave silently orphans work. Warning-and-
  proceeding fails in unattended mode (the warning scrolls past), and falling back to sequential
  dispatch is a silent capability downgrade. The guard runs **once per run**, with its outcome
  recorded in the run's provenance, so every run carries evidence of what was verified when.

### Acceptance run mechanics

- **D-16:** The acceptance workload is a **minimal purpose-built two-plan wave** — the cheapest
  workload that still crosses every seam under test (two plans dispatched concurrently from one
  wave, real agents, real background completion, real drain). Not a literal Phase 29
  reconstruction, which is expensive and whose plans no longer exist in runnable form.

- **D-17:** It runs on the **main checkout**, with the orchestrator touching no git at all while
  the executor holds the tree. Precedent: 30-02 and 30-04 ran this way deliberately, because a
  worktree adds an uncontrolled variable to the process behaviour being measured — and a
  worktree's `.git` file points at a host path the container cannot see, which already breaks the
  pre-push gate here. The no-git-during-executor rule is in `CLAUDE.md` and is binding.

- **D-18:** **Pass = both plans produce a `SUMMARY.md` and merge.** This is the exact inverse of
  the 999.64 failure, where both executors made real commits on orphaned branches, neither wrote a
  summary, and nothing merged. Explicitly **not** "the stage reports Success" — the completion
  oracle already scored the orphaned Phase 29 stage as Success. Also not "both completions observed
  in the stream", because constraint 7 makes an observed count the very signal that can undercount.

- **D-19:** If the acceptance run fails, **the phase does not close** — diagnose and re-run. The
  arc goal is the acceptance criterion, so a failing run means 999.64 is not closed whatever the
  unit tests say. This project has already shipped green tests over a broken feature more than
  once; the live run is the gate that catches that class.

### Claude's Discretion

- Where the constraint-9 exit-code guard physically lives in the layer cascade (monitor vs
  `evaluate_agent_result`) — implementation approach, deliberately not asked.
- The internal structure of the pipe-owning monitor and how it replaces `spawn_monitor`'s
  `sh` script.
- Near-simultaneous-completion test design (review M3) — the planner resolves it against
  constraint 7's coalescing evidence.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Binding scope and constraints
- `.planning/ROADMAP.md` — the Phase 31 entry, and binding constraints 1, 4, 5, 7, 8, 9. This is
  the authoritative statement of what binds this phase.
- `.planning/ROADMAP.md` §"Phase 999.67" — the Layer-0 provenance defect folded into this phase.

### Evidence the decisions rest on
- `.planning/phases/30-keep-the-session-alive-past-turn-end/30d-MEASUREMENTS.md` — the idle-gap
  distributions behind D-01 and D-02, and the finding that the drain gate is defensive rather than
  load-bearing.
- `.planning/phases/30-keep-the-session-alive-past-turn-end/30c-VERDICT.md` — `delivery: confirmed`,
  the gate that permitted this phase to be planned at all.
- `.planning/phases/30-keep-the-session-alive-past-turn-end/30c-VERDICT-reliability.md` — the
  seven-trial reliability set behind the idle-timeout floor.
- `.planning/phases/30-keep-the-session-alive-past-turn-end/30-H1-CONTEXT-FOR-31.md` — the
  constraint-9 handoff. **Read its superseded banner:** items 1 and 2 were CLOSED in Phase 30 by
  `a557805`; only the boundary-truncation residual survives.

### Known-stale documents — read with correction
- `.planning/phases/30-keep-the-session-alive-past-turn-end/30-VERIFICATION.md` — its "Deferred —
  open items owned by Phase 31" table lists constraint 9 items 1–2 as open. They are **closed**;
  the rows carry a deprecation banner. ROADMAP constraint 9 is authoritative. W-01 and W-02 in the
  same file record two further corrections worth reading before planning.

### Source under change
- `crates/devflow-core/src/monitor.rs` — `spawn_monitor`; line 171 `.stdin(Stdio::null())` is why
  the detached `sh` script cannot hold a pipe open.
- `crates/devflow-core/src/agents/claude.rs` — lines 26 and 72 still emit `--output-format json`;
  this is the argv D-09/D-10 flip.
- `crates/devflow-cli/src/pipeline_launch.rs` — the launch path; `:416` `evaluate_agent_result`,
  `:443` `session_id`, `:491` checkpoint detection.
- `crates/devflow-core/src/agent_result.rs` — 30b's parser, `is_top_level`,
  `claude_stream_gate_shape`, `ParsedCapture`.

### Repository rules that bind the acceptance run
- `CLAUDE.md` — never run git operations while an executor holds the working tree (binds D-17);
  and the verification habits this repo has already paid for.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `is_top_level` / `claude_stream_gate_shape` (`agent_result.rs`): the single provenance predicate
  Phase 30 built. D-13's token match reuses it rather than adding a new trust path.
- `ParsedCapture` (`agent_result.rs`): makes dropped/torn lines representable — the root-cause fix
  that closed constraint 9 items 1–2.
- 30b's isolation tests: already prove neither shipped capture shape is hijacked (D-12).
- `DEVFLOW_GATE_TIMEOUT_SECS` handling: the precedent for D-04's clamped-configurable pattern.

### Established Patterns
- Layer cascade in `evaluate_agent_result`: Layer 1 (`evaluate_layer1`) short-circuits Layer 2 on
  the first `Some`. This is why D-05's write-then-terminate ordering matters and why constraint 9's
  residual is about not letting a stream-derived Success beat a contradicting exit code.
- Scope fences: every Phase 30 plan carried one forbidding changes to `monitor.rs`,
  `agents/claude.rs`, `pipeline_launch.rs`. Phase 31 is the phase that lifts them — deliberately.

### Integration Points
- `spawn_monitor` → the new pipe-owning monitor (the M-sized core of this phase).
- `ClaudeAgent::exec_command` → argv switch to `--input-format stream-json --output-format stream-json`.
- Monitor → `agent_result.rs` parser, now fed real production output for the first time.

</code_context>

<specifics>
## Specific Ideas

The declared-token canary (D-13) was the operator's proposal, replacing both options originally
offered (bare canary, version-string assertion). Its advantage over a bare canary is that it
answers "did *this specific* helper's message arrive" rather than "did *a* message arrive" — the
same distinction constraint 7 shows the CLI destroys when it coalesces completions. The two traps
recorded in D-13 were surfaced when validating the idea, not designed around after the fact.

</specifics>

<deferred>
## Deferred Ideas

- **Per-child declared tokens** (from D-14) — extending the token mechanism from the startup canary
  to every dispatched child. Would defeat constraint 7's coalescing undercount by construction
  rather than relying on the drain gate as belt-and-braces. Deferred purely on size: it is a second
  mechanism on top of the monitor rewrite and would push this phase past M. Strong candidate for
  its own phase once 999.64 is closed and real captures exist.

</deferred>

---

*Phase: 31-Claude Adapter Launch Path — Pipe-Owning Monitor (999.64 arc close)*
*Context gathered: 2026-08-03*
