# Phase 28: Close the Checkpoint Answer Return Path - Context

**Gathered:** 2026-07-30
**Status:** Ready for planning

<domain>
## Phase Boundary

Make `gate="blocking-human"` checkpoints resolvable inside a DevFlow-driven
headless run, without building any human-interactivity infrastructure this
phase. This discussion substantially narrowed the phase from its ROADMAP
promotion: the original goal was "get an operator's answer back to the
agent that asked." The goal that survived discussion is "recognize a
checkpoint correctly, and let DevFlow resolve it unattended" — because no
usable notification/response channel exists today to build a human-answer
path on top of, and building one is out of scope for this phase.

Four units, reshaped from the original 999.57/999.59/999.60 promotion:
- **28a** — checkpoint recognition + unconditional autonomous resolution
  (repurposed from "session resume for a human's answer")
- **28b** — durable audit record of what was auto-decided (repurposed from
  "legible gate for a human to read before answering")
- **28c** — Define must never attempt an interactive interview headlessly
- **28d** — unchanged: preserve an unfired `--until` cap on resume

</domain>

<decisions>
## Implementation Decisions

### Checkpoint classification

- **D-01:** DevFlow statically scans the stage's `PLAN.md`(s) for any task
  carrying `gate="blocking-human"` **before** launching the stage. If none
  exist, any non-success exit is an ordinary error — exactly today's
  behavior, unchanged. If one or more exist, a non-success exit is
  *possibly* that checkpoint, and DevFlow confirms which by reading the
  `Gate:` field GSD's own `checkpoint_return_format` already produces in
  the executor's structured return (`checkpoints.md`). No new agent-side
  contract is invented — DevFlow reads a field the executor already emits
  when a checkpoint fires.
  — **Reversibility:** reversible — a lookup against static plan content,
  no data format or contract exposed externally.
- **D-02:** Rejected during discussion: pattern-matching the failure
  reason's text for checkpoint-shaped content. This is structurally
  identical to the tag-signing-viability predictor rejected twice in Phase
  26 (999.50/999.54, see `26-CONTEXT.md` D-10) — a second implementation of
  "what does the agent mean" that has to stay in sync with the agent's
  actual behavior forever.

### Checkpoint resolution — unconditional autonomous decide

- **D-03:** DevFlow's default, unconditional behavior for a `gate=
  "blocking-human"` checkpoint is to have the agent decide it itself. There
  is **no flag or config toggle** for this — it is not an opt-in. The
  reasoning that arrived here (recorded in full since it reverses explicit
  GSD-core guidance): no usable notification/response channel exists today
  (see D-08/D-09), so a "wait for a human" default would not degrade
  gracefully — it would hang or require infrastructure this phase
  deliberately does not build. Given that, gating this behind a flag that
  implies a working "off" state would be misleading.
  — **Reversibility:** costly — this is a deliberate, informed override of
  `checkpoints.md` rule 6 ("`gate="blocking-human"` is never auto-approved
  ... in every mode"), a GSD-core-wide invariant, applied unconditionally
  rather than as an auto-mode exception. It was adopted after two rounds of
  explicit pushback: the Phase 26 near-miss (a plan mistagged `gate=
  "blocking"` instead of `"blocking-human"`, which would have silently
  authorized `cargo publish` via auto-mode's first-option auto-select) was
  raised as concrete evidence, and the operator considered it and
  reaffirmed the decision both times. Undoing this later is a policy
  reversal, not a migration, but any autonomous decision executed while
  this policy was live (e.g. a package install the agent judged safe) is a
  real-world action that cannot be undone by reverting code.
- **D-04:** Mechanism: on a confirmed checkpoint, DevFlow relaunches via
  `claude -p --resume "$session_id" "<synthesized instruction>"` — the same
  exited session, not a fresh process. `session_id` is captured from the
  JSON envelope DevFlow already receives (Claude is already invoked with
  `--output-format json`; `agent_result.rs`'s `parse_devflow_result`
  currently reads only the inner `result` text and discards the rest of
  the envelope, so capturing `session_id` is reading one more already-
  present key, not new invocation plumbing). Persist it on `State`, same
  shape as the existing `worktree_path`/`monitor_pid` fields. Directory-
  scope (Claude's headless `--resume` requires invoking from the session's
  original working directory) is already satisfied by `spawn_monitor`
  always launching from `state.worktree_path`, consistently, across
  relaunches — no new plumbing needed there.
  — **Reversibility:** reversible — additive field + a Claude-specific
  relaunch code path.
- **D-05:** Claude-only. No `AgentAdapter` trait change, no Codex/OpenCode
  accommodation. `--resume` is a Claude CLI–specific, documented feature
  (verified: headless `claude -p --resume <id>` is real, supported, and
  intended for scripting); building an agent-agnostic fallback (what
  999.57's original entry called "Part B," a structured answer file) was
  considered and explicitly declined for now, since the actual target is
  automated resolution, not answer relay, and cross-agent portability was
  stated as out of scope for this phase.
  — **Reversibility:** reversible — narrowing scope now doesn't preclude a
  cross-agent design later.
- **D-06:** Rejected during discussion: an explicit `--auto-decide-
  checkpoints`-style opt-in flag/config, and a proposal to consolidate it
  with `yes_ship` into one umbrella flag. Rejected the umbrella specifically
  because Ship approval (a single, coarse, already-known proceed/don't-
  proceed decision) and checkpoint content (arbitrary, unpredictable,
  planner-judged case by case — `checkpoints.md`'s own flagship example is
  package-legitimacy verification before install) are different risk
  shapes; collapsing them into one dial removes the operator's ability to
  express "auto-ship, but never auto-trust an unfamiliar package install."
  Superseded by D-03 (unconditional, no flag at all).
- **D-07:** DevFlow still records what was auto-decided (D-04's mechanism),
  even though nothing blocks on it and no config disables it. Mirrors Phase
  23's D-06 principle (`23-CONTEXT.md`): "the audit trail must show a
  decision, never a missing checkpoint" — now more load-bearing than in
  Phase 23's original case, since with no flag and no human in the loop
  beforehand, this record is the *only* way anyone learns after the fact
  what the agent decided on its own.
  — **Reversibility:** reversible.

### Why not build a human-answer path (the reasoning, for the record)

> **All four decisions in this subsection are `[informational]`.** They record the
> reasoning behind a deliberate *non*-decision — why no human-answer path is built
> this phase — and are superseded in effect by D-03. None of them describes work to
> implement, so none is tracked by the decision-coverage gate. The substantive
> outcomes they lead to are carried by D-03 (unconditional auto-decide) and by
> `<deferred>` (what a future phase would need to build first).

- **D-08 [informational]:** DevFlow has no built-in, zero-config notification. The only
  push mechanism is `fire_gate_notify` / `DEVFLOW_GATE_NOTIFY_CMD`, an
  operator-supplied shell command; unset, it's a silent no-op
  (`gates.rs`). The only pull mechanism is manually running `devflow
  status` / `devflow gate list`.
- **D-09 [informational]:** The existing gate-response mechanism (`Gates::poll_response`)
  is a real blocking loop inside the process that fired the gate, with a
  **7-day production timeout** (`gates.rs`, test constant `SEVEN_DAYS`).
  This is the documented cause of DevFlow's known "gates hang forever"
  failure class (leaked monitor/process pairs). Routing a new checkpoint-
  answer mechanism through this same primitive would have inherited that
  weakness.
- **D-10 [informational]:** Considered and rejected: routing checkpoint pauses through the
  `stopped`/`stop_reason`/`resume` primitive instead of the blocking-poll
  gate primitive (no live process, pull-based discovery via `devflow
  status`). This removes the blocking-process problem but is still
  fundamentally human-in-the-loop — it still requires a human to notice and
  act for the phase to complete, which does not meet the actual goal
  (a process that doesn't require a human at all). Superseded by D-03.
- **D-11 [informational]:** Given D-08–D-10, a human-answer path for checkpoints (a
  dedicated `devflow gate answer <phase> "<text>"` verb, or reusing
  `approve --note`) is explicitly **not built this phase** — deferred to a
  future phase that builds real notification/response infrastructure first
  (see `<deferred>`).

### Ship approval (`yes_ship`)

- **D-12:** `yes_ship` gains a persistent config option (settable in the
  project config file), in addition to its existing per-invocation CLI
  flag; the CLI flag still overrides the config value when passed. This is
  a **deliberate reversal** of Phase 23's own D-05
  (`23-CONTEXT.md`): *"`--yes-ship` is a per-run flag only — never
  config-persistable ... so a standing unattended auto-merge can never
  become the silent default."* That decision explicitly flagged this exact
  reversal as the costly direction: *"relaxing this later is easy, but
  tightening it after operators depend on a persisted setting is not."*
  Raised explicitly during this discussion, twice, before being confirmed.
  — **Reversibility:** costly — matches Phase 23's own stated cost
  assessment for this exact reversal; operators may come to depend on the
  persisted default before it could be tightened back.
- **D-13:** Phase 23's D-06 (auto-answer the Ship gate, don't bypass it;
  gate still fires, still records an explicit pre-authorized approval)
  is **unaffected** — D-12 only changes where `yes_ship`'s value can come
  from, not what happens once it's true.

### Define stage headless safety

- **D-14:** When `CONTEXT.md` doesn't exist, Define must never invoke
  `/gsd-discuss-phase` headlessly — the exact command this discussion is
  running under. `Stage::Define.gsd_command()` currently returns
  `/gsd-discuss-phase {N}` unconditionally in the "artifact does not exist"
  branch of `idempotent_stage_prompt` (`prompt.rs`), which under `claude -p`
  with nobody present hangs or errors on `AskUserQuestion`. The fix is
  deletion, not disambiguation: proceed without a `CONTEXT.md` (same as any
  other early phase with no context file, already an expected case for the
  researcher/planner) rather than adding a flag to choose between two arms.
  This was originally framed as 999.59 ("a missing CONTEXT.md is
  ambiguous"); discussion concluded there is no ambiguity to resolve — the
  operator decides whether to run an interview, entirely before ever
  invoking `devflow start`, and DevFlow has no accommodation to make for
  that choice at runtime.
  — **Reversibility:** reversible.

### `--until` cap preservation

- **D-15:** Unit 28d is unchanged from the original promotion: `resume`
  clears `stopped`/`stop_reason`/`stop_until` unconditionally
  (`pipeline_launch.rs:226-228`), silently discarding a `--until` cap that
  never fired. Gate the clear on `state.stopped`.
  — **Reversibility:** reversible.

### Claude's Discretion

Not constrained here; the researcher and planner decide:

- Exact wording of the synthesized "no operator available, use your own
  judgment" instruction relayed via `--resume` (D-04) — content is
  DevFlow's to generate, not user-specified.
- Exact shape/location of the audit record (D-07) — a new file under
  `.devflow/`, an extension of the existing gate-ledger/`events.jsonl`
  pattern Phase 23's D-06 already established, or something else.
- Exact config key name/shape for `yes_ship`'s new config option (D-12) and
  where in `devflow-core::config` it's parsed.
- Where the static `PLAN.md` scan (D-01) lives and how it locates the
  relevant plan file(s) for the current stage/phase.
- Mechanical shape of `session_id` persistence on `State` (D-04) — new
  field name, serde defaults for pre-existing state files.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Backlog source and scope history
- `.planning/ROADMAP.md` § "Phase 28: Close the Checkpoint Answer Return
  Path" — the original promotion (999.57/999.59/999.60), its "Verified at
  promotion" table, and its capacity note. This discussion substantially
  narrowed the phase's actual scope (see `<domain>`); this CONTEXT.md is
  authoritative over the ROADMAP entry's original framing where they
  diverge.
- `.planning/superseded/26-release-cut-automation/26-CONTEXT.md` § D-10 — the
  tag-signing-viability predictor precedent behind D-02 (why pattern-
  matching failure text was rejected).
- `.planning/phases/23-end-to-end-dogfood/23-CONTEXT.md` § D-05, D-06 — the
  original `yes_ship` per-run-only decision now partially reversed (D-12)
  and the auto-answer-not-bypass audit-trail pattern being reused (D-07).

### GSD-core checkpoint protocol
- `$HOME/.claude/gsd-core/references/checkpoints.md` — canonical checkpoint
  type reference; rule 6 (`gate="blocking-human"` never auto-approved, in
  any mode) is the rule D-03 deliberately and unconditionally overrides.
  Also defines `checkpoint_return_format` / the `Gate:` field D-01 reads.
- `$HOME/.claude/gsd-core/workflows/execute-phase.md` § `checkpoint_
  handling` — GSD-core's own in-session checkpoint continuation model.
  Explicitly avoids `claude --resume` in favor of a fresh continuation
  agent ("Resume relies on internal serialization that breaks with
  parallel tool calls"). Verified this caveat does not transfer to
  DevFlow's shape (one agent per stage, no concurrent Agent-tool subagent
  spawns) — cited here so a future reader doesn't re-raise it without that
  context.
- `$HOME/.claude/gsd-core/references/planner-reversibility.md` — the
  reversibility taxonomy used for the ratings in `<decisions>` above.

### DevFlow source — checkpoint/gate/resume mechanics
- `crates/devflow-core/src/gates.rs` — gate file protocol; `GateAction::
  from_response`; `Gates::poll_response`'s blocking loop and 7-day
  production timeout (D-09); `fire_gate_notify` / `DEVFLOW_GATE_NOTIFY_CMD`
  (D-08).
- `crates/devflow-core/src/agent_result.rs` — `AgentResult` struct and
  `parse_devflow_result`; where `session_id` capture (D-04) is added.
- `crates/devflow-core/src/agents/claude.rs` — `ClaudeAgent::exec_command`;
  where the `--resume` relaunch path (D-04) is added; confirms `--output-
  format json` is already in use.
- `crates/devflow-core/src/agents/mod.rs` — `AgentAdapter` trait; D-05
  confirms this stays untouched (no shared trait change).
- `crates/devflow-core/src/prompt.rs` — `idempotent_stage_prompt` (the
  Define branch to fix per D-14); `Stage::Define`'s prompt construction.
- `crates/devflow-core/src/stage.rs` — `Stage::gsd_command` mapping
  (`Stage::Define => "/gsd-discuss-phase {N}"`, the command D-14 must never
  reach headlessly).
- `crates/devflow-core/src/monitor.rs` — `spawn_monitor`; confirms the
  agent always launches from `state.worktree_path` consistently across
  relaunches (satisfies `--resume`'s directory-scope requirement, D-04).
- `crates/devflow-core/src/state.rs` — `State` struct; `yes_ship`,
  `stop_until`, `stopped`, `worktree_path`, `monitor_pid` fields; where
  `session_id` (D-04) is added following the same pattern.
- `crates/devflow-cli/src/pipeline_launch.rs` — `resume()` (the unguarded
  `stopped`/`stop_reason`/`stop_until` clear D-15 fixes); `launch_stage()`.
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `handle_stage_failure`;
  `truncate_reason`/`render_gate_context` (300-char cap applied at
  construction, before `write_gate` — not at render/notify time, a
  correction made during this discussion); `yes_ship`'s existing
  auto-response wiring and its config-exclusion test
  (`config_file_with_yes_ship_key_loads_but_never_sets_the_flag`, the test
  D-12 changes the assertion of).

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `AgentResult`/`parse_devflow_result`'s existing JSON-envelope handling —
  `session_id` capture (D-04) is one more field read from an envelope
  already being parsed, not a new parsing path.
- `State`'s existing per-phase-persisted-field pattern (`worktree_path`,
  `monitor_pid`, `stop_until`) — `session_id` follows the same shape.
- The existing gate-ledger/`events.jsonl` audit pattern from Phase 23's
  D-06 — candidate home for the new auto-decide audit record (D-07).

### Established Patterns
- `idempotent_stage_prompt`'s "check whether the artifact already exists,
  branch accordingly" pattern — the model D-14's fix follows, just
  removing the branch that runs an interactive command rather than adding
  a new one.
- Phase 23's D-05/D-06 pairing (per-run-only + always-audited) was the
  direct precedent this discussion engaged with for both the checkpoint
  mechanism (D-03/D-07, keeping the audit half, dropping the per-run-flag
  half) and `yes_ship` itself (D-12, explicitly reversing the per-run-only
  half).

### Integration Points
- `agents/claude.rs::exec_command` needs a resume-variant entry point
  (D-04/D-05) — Claude-specific, no trait change.
- `agent_result.rs` needs a `session_id` field threaded through to `State`.
- Wherever `handle_stage_failure`/the gate-write dispatch happens
  (`pipeline_outcomes.rs`, `pipeline_gate.rs`) needs the D-01 classification
  check inserted before today's generic error-gate path.
- `prompt.rs`'s `Stage::Define` branch of `idempotent_stage_prompt`.

</code_context>

<specifics>
## Specific Ideas

- The Phase 26 near-miss (a plan mistagged `gate="blocking"` instead of
  `"blocking-human"`, which auto-mode's first-option auto-select would have
  used to silently authorize `cargo publish`) was raised twice during this
  discussion as concrete evidence against an auto-decide mechanism, and
  explicitly considered and overridden both times — recorded in D-03.
- `checkpoints.md` rule 6 was surfaced explicitly as the GSD-core-wide
  invariant D-03 overrides; the operator was shown the exact rule text
  before confirming.
- Phase 23's D-05/D-06 (`23-CONTEXT.md`) were engaged with directly and by
  name — D-05 reversed (D-12), D-06's principle reused (D-07).

</specifics>

<deferred>
## Deferred Ideas

- **Human-answer path for checkpoints** (a dedicated `devflow gate answer`
  verb, or a resume-relaunch carrying a human-typed answer instead of a
  synthesized one) — needs a real notification/response interface first;
  none exists today (D-08/D-09). Candidate for its own future phase.
- **The notification/response interface itself** — proactive notification
  beyond the operator-supplied `DEVFLOW_GATE_NOTIFY_CMD` hook, and a
  non-blocking (non-7-day-poll) response mechanism. Prerequisite for the
  item above.
- **Ship-gate redundancy** — `yes_ship` already fully expresses the
  operator's intent ahead of time, yet DevFlow still always writes a Ship
  gate and blocks on `poll_response` when `yes_ship` is false
  (`handle_ship_outcome_without_yes_ship_writes_gate_but_no_response`).
  Raised during discussion as a candidate simplification (route through
  `stopped`/`resume` instead of the blocking-poll gate), explicitly left
  unbuilt this phase — not one of the three backlog items promoted here.
- **Cross-agent (Codex/OpenCode) checkpoint resolution** — D-05 scopes
  this phase to Claude only. 999.57's original "Part B" (a structured,
  agent-agnostic answer-file fallback) remains a candidate if/when
  Codex/OpenCode support is prioritized.

</deferred>

---

*Phase: 28-close-the-checkpoint-answer-return-path*
*Context gathered: 2026-07-30*
