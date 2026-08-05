# Phase 34: Stream-JSON Coverage and the Validate Trust Boundary (999.73 + 999.74) - Context

**Gathered:** 2026-08-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Two independent trust boundaries in the pipeline, bundled because both are open-ended
investigation rather than known mechanical fixes (see ROADMAP.md § "Why two phases, not four or
one").

**999.73 — a rollout backed by evidence.** `STREAM_JSON_STAGES`
(`crates/devflow-cli/src/pipeline_launch.rs:446`) lists exactly `Stage::Code` today. Widen it to
Define/Plan/Validate/Ship, but only where a **real per-stage production capture** exists — never on
reasoning alone. Re-derive the close rule's drain-gate arm per newly-widened stage rather than
assuming it carries over from Code's backgrounding behaviour.

**999.74 — a classification correctness fix.** `classify_validate_outcome`
(`crates/devflow-cli/src/pipeline_outcomes.rs:203`) matches `(_, Some(Verdict::Pass))` **first**,
with `_` discarding the derived status entirely. Gate the `Passed` arm on the status the outcome
cascade actually derived, confirmed explicitly for `Failed`, `Unknown`, `ResourceKilled` and
`IdleTimeout` — and establish, rather than assume, whether the inversion can manufacture a pass on
a run that would otherwise have gated.

**Not in this phase:** D-14 per-child declared tokens (see Deferred Ideas); parser or monitor
repair work that a capture happens to reveal (filed, not fixed — D-04).

</domain>

<decisions>
## Implementation Decisions

> **Label collision warning for downstream agents.** This phase's decisions are `D-01`…`D-12`.
> Phase 31's decisions — which this phase repeatedly cites — are written `31/D-09`, `31/D-10`,
> `31/D-11`, `31/D-14`, `31/D-18`, `31/D-19` throughout. They are **different decisions** with
> overlapping numbers. Do not conflate them.

### Rollout granularity (999.73)

- **D-01:** **`STREAM_JSON_STAGES` stays an explicit named const**, widened to name all five
  stages. Rejected: inverting to a `LEGACY_STAGES` deny-list (a new `Stage` variant would silently
  join the stream path with no evidence — exactly what `31/D-09` guards against), and deleting the
  constant entirely (loses per-stage narrowing, leaving only `31/D-11`'s all-or-nothing opt-out).
  The const stays greppable and the per-stage evidence story lives in its doc comment.
  — **Reversibility:** reversible — one named constant, one predicate, no persisted state and no
  published contract depends on its contents.

- **D-02:** **A stage whose real production capture cannot be obtained inside this phase stays off
  the list**, with the reason recorded in the const's doc comment the same way Code's single-entry
  list is recorded today. The phase then closes **PARTIAL** on success criterion 1 and the
  remainder returns as a numbered `999.x` backlog entry. Rejected: blocking the whole phase until
  all four captures exist (`31/D-19`'s shape — one hard-to-stage capture, Ship in particular,
  blocks indefinitely), and widening anyway with the gap flagged (that is literally "extend the
  adapter to four stages on zero evidence", the reason 999.73 was deferred out of Phase 31).

- **D-03:** **No new runtime per-stage dial.** `31/D-11`'s `legacy_opt_out` remains the single
  predicate governing launch shape, the `31/D-15` canary gate, and the loud notice together —
  `claude_stream_launch_enabled`'s own doc comment (`pipeline_launch.rs:466-471`) states that two
  notions of "is this the stream path?" would be free to drift, and that the drift shows up as a
  guard firing on a launch it does not protect. A per-stage env-var subtraction reintroduces
  exactly that. Accepted cost, stated explicitly: one bad stage forces the whole run onto the
  legacy path.

- **D-04:** **A parser or monitor defect that a capture reveals is filed, not fixed in-phase.** The
  stage stays off the list, the defect gets a numbered `999.x` entry plus a Linear issue, and the
  capture is committed as its evidence. Keeps this phase a rollout-backed-by-evidence rather than
  letting it become an open-ended parser-repair phase while it is already carrying 999.74.

### Validate trust boundary (999.74)

- **D-05:** **A status/verdict disagreement routes to `ValidateOutcome::Ambiguous`**, not `Failed`.
  This is two independent signals disagreeing, which is the case `Ambiguous` was created for, and
  `D-18e`'s recorded reasoning applies verbatim (`pipeline_outcomes.rs:150-158`): collapsing
  disagreement onto `Failed` routes it through the counter-based auto-loop — a DELAYED gate
  indistinguishable from an ordinary retry to an operator watching it — where the binding operator
  decision requires an IMMEDIATE one. `Ambiguous` also never touches `consecutive_failures`, and
  its payload already names which two signals disagreed for the `[never-silent]` gate context.
  Accepted cost, stated explicitly: an unattended run now stops where it previously advanced.
  — **Reversibility:** costly — this changes observable unattended-run behaviour at a gate, so
  undoing it after operators have built expectations around Validate stopping is a behavioural
  reversal, not just a code edit. The code change itself is local.

- **D-06:** **The fix is an exhaustive match on `(status, verdict)` with no wildcard arm reaching
  `Passed`**, so a future `AgentStatus` variant is a compile error rather than a silent join.
  Rejected: a minimal `status == AgentStatus::Success` equality guard on the existing arm. The
  function's own doc comment (`pipeline_outcomes.rs:184-188`) already admits the current equality
  test was "audited by hand" precisely because an equality test compiles untouched against a new
  variant, and `IdleTimeout` arriving during Phase 31 is proof the variant set grows. This repo
  consistently prefers structural over hand-audited — the wildcard-free match guarding
  `decide_action`, and `ParsedCapture` making dropped lines representable.
  — **Reversibility:** reversible — a pure function over `&AgentResult` with no I/O, directly
  unit-testable, no callers depend on the match's internal shape.

- **D-07:** **Success criterion 4 is answered by an executable demonstration AND a written
  finding.** A test pins the pre-fix behaviour (status `Failed` + `verdict: pass` reaching
  `Stage::Ship`), plus a short written trace with `file:line` in the phase artifacts. Rejected: a
  written audit alone — that is an assertion about code, verified once by reading, with nothing
  preventing drift, and this project has shipped green tests over a broken feature more than once
  (`31/D-19`'s stated reason for a live gate). Rejected: the test alone — the reasoning about *why*
  auto mode skips the gate would live nowhere a future reader finds it.

- **D-08:** **Criterion 3 is verified by a full matrix sweep** over every
  `(AgentStatus × Option<Verdict>)` pair asserting the whole classification matrix, plus a **named
  positive control** (`Success` + `Pass` → `Passed`, which must still pass) and a negative control.
  Phase 30's constraint-9 sweep is direct precedent. Rejected: four named per-status mirror tests as
  the roadmap entry proposes — they cover only the four pairs someone thought to write, leaving
  `verdict: gaps` and `verdict: none` against each status unasserted.

### Drain-gate re-derivation (success criterion 2)

- **D-09:** **Criterion 2 is satisfied empirically, from each stage's own capture** — does
  `background_tasks_changed` appear at all, with what arity, and did it drain before the marker —
  recorded per stage. The capture is being taken for criterion 1 regardless, so this is nearly
  free, and it functions as a negative control: a stage showing `Pending(n>0)` *refutes* the
  vacuous-drain assumption for that stage rather than confirming it. Rejected: a reasoned argument
  from the existing design alone — that is precisely the "reasoned, not witnessed" standard
  `31/D-09` declined to accept.

- **D-10:** **n=1 production capture per stage.** Matches criterion 1's literal wording ("a real
  per-stage production capture") and keeps the phase inside its size cap. **The summary must state
  what n=1 does not establish** — that the shape occurred once, not that it is the stage's steady
  behaviour across prompts, phase shapes, or CLI versions. Phase 30 needed n=2–3 trials before its
  drain measurements meant anything (30c reliability trials, 30d exit-timing).

- **D-11:** **A capture showing a `background_tasks_changed` list that never drains still widens
  the stage** — a non-draining list means the close rule correctly held stdin open, which is the
  999.64 orphan being *prevented*, not a defect. Record it as a stage where the drain arm is
  **load-bearing rather than defensive**, which is a genuine finding: 30-04 measured it defensive
  on Code (n=2 Mode B trials delivered everything without it), and 999.73's own entry says that
  reasoning does not transfer. Consequence to check in the capture: stdin held open means the run
  leans on the idle timeout, so the capture must show the timeout behaved.

- **D-12:** **D-14 (per-child declared tokens) stays out and stays deferred**, and is **re-filed as
  its own numbered `999.x` backlog entry** so the 999.73 pairing note does not quietly expire.
  `31/D-14` deferred it on size, not merit; this phase already carries a four-stage evidence
  campaign plus 999.74's behavioural change.

### Claude's Discretion

The operator explicitly declined to discuss these — they are the planner's to resolve, not open
questions to re-surface:

- **Capture acquisition** — where the four real per-stage captures come from (a purpose-built
  minimal run per stage per `31/D-16`, this phase's own execution instrumented, or a subsequent
  dogfood run), and what the per-stage pass bar is. Constrained by D-02 (an unobtainable capture
  leaves its stage narrow rather than blocking) and D-10 (n=1). The operator's standing preference
  is the cheapest workload that still crosses the seams under test.
- **Plan sequencing within the phase.** 999.73 and 999.74 have no structural dependency on each
  other; 999.74 is the cheaper and more self-contained half.
- **Where the exhaustive-match rewrite physically lands** in `classify_validate_outcome` versus a
  helper, and how the pre-fix demonstration in D-07 is scaffolded.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Binding scope and requirements
- `.planning/ROADMAP.md` § "Phase 34: Stream-JSON Coverage and the Validate Trust Boundary" — the
  authoritative goal and its four success criteria.
- `.planning/ROADMAP.md` § "Phase 999.73: Widen `STREAM_JSON_STAGES` Beyond `Stage::Code`" — state
  at Phase 31 close, why it was deferred rather than widened, and the proposed work.
- `.planning/ROADMAP.md` § "Phase 999.74: `classify_validate_outcome` Trusts the Agent's Verdict
  Over Its Own Status" — the defect, how it surfaced twice, and the open question carried
  forward unrelaxed.
- `.planning/REQUIREMENTS.md` — DOGFOOD-03 and DOGFOOD-04.

### Prior decisions this phase inherits (cited as `31/D-NN` above)
- `.planning/milestones/v2.3.0-phases/31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl/31-CONTEXT.md`
  § "Rollout shape" (D-09, D-10, D-11, D-12), § "CLI-behaviour guard" (D-13, D-14, D-15),
  § "Acceptance run mechanics" (D-16, D-17, D-18, D-19).
- `.planning/milestones/v2.3.0-phases/31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl/31-ACCEPTANCE.md`
  — the Code-stage acceptance run; the only real production stream capture that exists today, and
  the template for what a per-stage capture must show.
- `.planning/milestones/v2.3.0-phases/31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl/31-VERIFICATION.md`
  — how constraint 4's AND rule was verified.

### Evidence the drain-gate decisions rest on
- `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/30d-MEASUREMENTS.md`
  — the 4.54–11.51s drain-to-final-`result` lag across 14 trials; closing at the drain would have
  truncated the final orchestrator turn in all seven 30d trials.
- `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/30c-evidence/`,
  `30c-evidence-scrubbed/`, `30a-evidence/` — the archived raw `.jsonl` capture layout this
  phase's per-stage captures should follow, including the scrubbed/operator split.
- `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/30-04-SUMMARY.md` —
  where the drain arm was measured *defensive rather than load-bearing* on Code.

### Source under change
- `crates/devflow-cli/src/pipeline_launch.rs:439-480` — `STREAM_JSON_STAGES` and
  `claude_stream_launch_enabled`. Read both doc comments in full: they record what widens the
  constant and why the opt-out is folded into one predicate.
- `crates/devflow-cli/src/pipeline_outcomes.rs:149-215` — `ValidateOutcome` and
  `classify_validate_outcome`, including the flagged-not-fixed note at lines 195-202.
- `crates/devflow-cli/src/pipeline_outcomes.rs:300-400` — `handle_validate_outcome`, the routing
  criterion 4 must be established against.
- `crates/devflow-core/src/monitor.rs:488-593` — `CloseRule`, `BackgroundTaskState`, and
  `should_close`. `NeverAnnounced` is vacuously drained **by design**, for exactly the
  non-backgrounding-stage case this phase widens into.
- `crates/devflow-core/src/agent_result.rs:1746-1750` — `idle_timeout_result` sets `verdict: None`
  with a doc comment naming the 999.74 defect as the reason. That comment becomes stale once D-05
  and D-06 land.
- `crates/devflow-core/src/stage.rs:16-27` — the five `Stage` variants and what command each runs.

### Repository rules that bind any live run this phase performs
- `CLAUDE.md` § "Never run git operations while an executor holds the working tree" — binding, and
  it has already caused two real failures on 2026-08-02.
- `CLAUDE.md` § "Verification habits this repo has already paid for" — `cargo test --exact` exits 0
  on a name that matches nothing; the package is `devflow`, not `devflow-cli`.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- **`monitor::CloseRule`** (`crates/devflow-core/src/monitor.rs:553`) — already handles both the
  backgrounding and non-backgrounding cases correctly. `BackgroundTaskState::NeverAnnounced` is
  vacuously drained by construction, `Pending(n>0)` blocks, and `Unreadable` blocks (the 999.75 /
  DEN-96 fix, 2026-08-04). D-09's per-stage re-derivation is an inspection of which of these three
  states each stage actually produces, not a redesign.
- **`close_rule_is_vacuously_drained_when_no_background_tasks_event_appears`**
  (`monitor.rs:1303`) — the existing test that establishes the non-backgrounding-stage case.
- **`event_is_top_level_result_marker`** — the marker arm's only satisfier; a composition of
  `is_top_level` and the marker parser, never a looser text search. Any new per-stage assertion
  must reuse it rather than introduce a second notion of a trustworthy line.
- **`ValidateOutcome::Ambiguous(String)`** — already exists with the immediate-gate routing D-05
  needs (`pipeline_outcomes.rs:326-340`), including its `[never-silent]` context formatting. D-05
  is a re-routing, not a new mechanism.
- **Phase 30's evidence-directory layout** — `NNx-evidence/raw_output.jsonl` with scrubbed and
  operator variants, committed into the phase directory.

### Established Patterns

- **Evidence before widening** (`31/D-09`) — every gate fixture today is labelled SYNTHETIC
  in-source. A stage joins `STREAM_JSON_STAGES` only behind a real capture.
- **Structural guards over hand audits** — the wildcard-free match guarding `decide_action`;
  `ParsedCapture` making dropped lines representable; `BackgroundTaskState` making "unreadable"
  distinguishable from "never announced". D-06 continues this line.
- **Negative controls in verification** — Phase 33's 33-05 added a mirrored negative control that
  no test in the workspace supplied. D-08's named positive control and D-09's refutation framing
  both follow it.
- **Pass is a landed artifact, never a reported status** (`31/D-18`) — the completion oracle
  already scored the orphaned Phase 29 stage as `Success`.

### Integration Points

- `claude_stream_launch_enabled` is consulted at `pipeline_launch.rs:86` and `:93`. Note what it
  does **not** reach: `relaunch_checkpoint_session` hardcodes `MonitorLaunch::Legacy` and calls
  `spawn_agent_and_record` directly — a pre-existing deliberate legacy route, recorded rather than
  silently covered.
- `crates/devflow-cli/tests/phase7_cli.rs:654` already carries a comment tying a CLI-surface
  assertion to `STREAM_JSON_STAGES`; widening the constant is expected to surface there.
- `pipeline_launch.rs:1763` and `:2329-2334` contain tests that assert Plan is **not** in
  `STREAM_JSON_STAGES` and that Code **is** — both will need updating, and `:2330`'s assertion
  message ("Stage::Code must be in STREAM_JSON_STAGES for this test to mean anything") is a
  meaningfulness guard that must keep meaning something after the change.
- `ValidateOutcome::Passed` → `ValidateResult::Passed` → `transition(.., Stage::Ship)` at
  `pipeline_outcomes.rs:395`.

</code_context>

<specifics>
## Specific Ideas

### Findings surfaced during discussion — both need confirming in-phase, neither is established

- **F-01 (preliminary, and it argues for the aggressive fix).** A one-pass read of the Validate
  routing suggests the 999.74 inversion is **stronger than 999.67's analogue**, which could only
  reach `Ambiguous` and still gated. `ValidateOutcome::Passed` becomes `ValidateResult::Passed`,
  and when `state.mode.should_gate(Stage::Validate, ..)` is false it calls
  `transition(project_root, state, Stage::Ship)` directly at `pipeline_outcomes.rs:395` — **no
  gate at all** in `auto` mode. When the mode does gate, the prompt reads *"Validation passed —
  approve to ship?"* (`:377`), which misdescribes a failed run to the operator being asked to
  approve it. **This is one read, not the end-to-end audit criterion 4 demands.** It is recorded
  here as the starting hypothesis for D-07's work, and D-07 exists to confirm or refute it — not
  as a settled answer.

- **F-02 (contradicts an inherited premise, and this is why D-09 is empirical).** `31/D-10` states
  Code "is the only stage that actually backgrounds, so it is the only one exercising
  task-notification delivery and the drain gate at all" — and that claim is reproduced verbatim in
  `STREAM_JSON_STAGES`' own doc comment at `pipeline_launch.rs:441-445`. A recorded run
  contradicts it: during the Phase 26 dogfood (`devflow start --phase 26 --agent claude --mode
  auto --yes-ship`, 2026-07-29), the **Plan** stage's orchestrator backgrounded its
  `gsd-phase-researcher` Agent() call and ended its turn. Under one-shot `claude -p` there was no
  next turn to resume in; the research and all downstream planning was lost and the process still
  exited 0, caught only by DevFlow's never-silent zero-commit gate. Evidence trail:
  `.devflow/phase-26-stdout` from that run, and the GSD-core-side defect (the orchestrator
  violating `plan-phase.md`'s own explicit "wait synchronously" instruction). **This does not make
  the close rule wrong** — `Pending(n>0)` correctly blocks and `NeverAnnounced` is correctly
  vacuous — but it does mean "which stages background" cannot be answered by assumption, which is
  precisely what criterion 2 says. If the Plan-stage capture confirms it, the doc comment at
  `pipeline_launch.rs:441-445` needs correcting in the same commit that widens the constant.

</specifics>

<deferred>
## Deferred Ideas

- **D-14 per-child declared tokens** (per D-12) — one declared token per dispatched child rather
  than `31/D-13`'s startup-canary-only scope. Would defeat constraint 7's coalescing undercount
  directly instead of leaning on the drain gate. Deferred on size, not merit, for the second time
  (`31/D-14` was the first). **Action item: file it as its own numbered `999.x` ROADMAP entry plus
  a Linear issue**, so the 999.73 pairing note does not expire silently when 999.73 closes.

- **Any parser or monitor defect a per-stage capture reveals** (per D-04) — filed as a numbered
  `999.x` entry with its capture as evidence, not fixed here.

- **Un-widened stages** (per D-02) — if a stage's capture cannot be obtained, the remaining
  widening returns as a numbered `999.x` entry and the phase closes PARTIAL on criterion 1.

</deferred>

---

*Phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-999-74*
*Context gathered: 2026-08-05*
