# Phase 34: Stream-JSON Coverage and the Validate Trust Boundary (999.73 + 999.74) - Context

**Gathered:** 2026-08-05
**Amended:** 2026-08-05, after adversarial review — see `34-REVIEW.md`
**Status:** Ready for planning

> **AMENDMENT NOTICE — read this before any decision below.** An adversarial pass over this
> document (six independent lanes, `34-REVIEW.md`) found that several decisions rested on premises
> the code refutes. Amended decisions are marked **[AMENDED]** with the superseded text kept
> visible, because the superseded reasoning is what a reader would otherwise reconstruct. The two
> findings F-01 and F-02 are both **downgraded**. 999.76 has been folded into this phase (D-13).
> `ROADMAP.md`'s Phase 34 entry was rewritten to match; its criteria, not this document's original
> framing, are binding.
>
> **SECOND PASS (same day) reversed the first pass's headline conclusion.** The first pass judged
> the 999.74 inversion unreachable and F-01 refuted. It IS reachable — through
> `reconcile_layer0_verdict`, not the `(_, Some(Pass))` wildcard. See **D-15**, which is the
> phase's real 999.74 defect. The second pass also found nine defects introduced *by* the first
> pass's own amendments; those are corrected in place and marked. Where an amendment is itself
> amended, both prior states are kept.

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
`IdleTimeout`. **The open question is now answered: yes, it can** — but by
`reconcile_layer0_verdict` grafting the agent's verdict onto a Layer-0 success without reading
Layer 1's status, not by the wildcard. That graft is in scope (D-15) and the wildcard fix alone
does not close it.

**Not in this phase:** `31/D-14` per-child declared tokens (see Deferred Ideas); parser or monitor
repair work that a capture happens to reveal (filed, not fixed — D-04).

</domain>

<decisions>
## Implementation Decisions

> **Label collision warning for downstream agents.** This phase's decisions are `D-01`…`D-14`.
> Phase 31's decisions — which this phase repeatedly cites — are written with a `31/` prefix
> (`31/D-09`, `31/D-10`, `31/D-11`, `31/D-13`, `31/D-14`, `31/D-15`, `31/D-16`, `31/D-18`,
> `31/D-19`). They are **different decisions** with overlapping numbers. Do not conflate them.
>
> One further prefix: **`D-18e` is Phase 18's decision**, not this phase's and not Phase 31's — it
> is the three-way external-verify matrix, cited as `(18e, D-18e)` at `pipeline_outcomes.rs:149`.
> It appeared unprefixed in an earlier draft of D-05, which is exactly the conflation this warning
> exists to prevent.

### Rollout granularity (999.73)

- **D-01:** **`STREAM_JSON_STAGES` stays an explicit named const**, widened to name all five
  stages. Rejected: inverting to a `LEGACY_STAGES` deny-list (a new `Stage` variant would silently
  join the stream path with no evidence — exactly what `31/D-09` guards against), and deleting the
  constant entirely (loses per-stage narrowing, leaving only `31/D-11`'s all-or-nothing opt-out).
  The const stays greppable and the per-stage evidence story lives in its doc comment.
  — **Reversibility:** reversible — one named constant, one predicate, no persisted state and no
  published contract depends on its contents.

- **D-02 [AMENDED]:** **A stage whose real production capture cannot be obtained inside this phase
  stays off the list**, with the reason recorded in the const's doc comment the same way Code's
  single-entry list is recorded today. Rejected: blocking the whole phase until all four captures
  exist (`31/D-19`'s shape — one hard-to-stage capture, Ship in particular, blocks indefinitely),
  and widening anyway with the gap flagged (that is literally "extend the adapter to four stages on
  zero evidence", the reason 999.73 was deferred out of Phase 31).

  **Amendment 1 — the gate is COMMIT-TIME, not build-time.** As originally worded this rule was
  circular: a stage produces a stream-json capture only via the pipe-owning path, which requires
  membership in `STREAM_JSON_STAGES`, and the predicate offers an opt-*out* only. Evidence
  therefore cannot precede widening. The rule is: widen in the working tree, capture, and let the
  evidence decide what gets **committed**.

  **Corrected: `devflow __monitor` is NOT an equivalent route.** An earlier version of this
  amendment sanctioned it two sentences after forbidding harnesses that skip `resolve_launch_shape`
  — and `__monitor` skips exactly that (`run_monitor`, `pipeline_launch.rs:493`, calls
  `run_pipe_owning_monitor` directly). Use it to smoke-test the monitor if useful, never to produce
  a capture offered as criterion 1 evidence.

  **The working-tree route needs its lifecycle stated, or it silently produces legacy captures.**
  `devflow start` creates its worktree from `develop` *before* the staleness check, so an
  uncommitted widened constant is absent from the worktree and the run captures the legacy path.
  A real capture therefore requires `--no-worktree` (which puts the executor on the main checkout,
  where `CLAUDE.md` forbids all git activity until it exits) plus a rebuild, so the running binary
  actually contains the widened constant. The planner must specify that sequence explicitly.
  See `34-REVIEW.md` R-04.

  **Amendment 2 — RESOLVED 2026-08-05 by rewording DOGFOOD-03, not by a PARTIAL close.** The
  original text said the remainder "returns as a numbered `999.x` backlog entry." That had nowhere
  to land: Phase 34 is the active milestone's last phase, `REQUIREMENTS.md` models requirements as
  checkboxes with no partial state, and `999.x` backlog is explicitly Out of Scope for this
  milestone. Phase 26 is precedent for a PARTIAL close, but a materially different one — it was
  never shipped and its goal was re-opened wholesale.

  **The fix was upstream of the phase.** DOGFOOD-03 named four specific stages, making it an
  implementation plan rather than an operator-facing guarantee — the only one of the four
  requirements phrased that way. It now states the evidence discipline itself, so a stage left
  narrow is a *satisfied* requirement (visibly, deliberately on the legacy path) rather than an
  unsatisfiable checkbox. Operator decision, 2026-08-05. See `REQUIREMENTS.md`'s inline note.

  **Amendment 3 — the first reword was a coverage relaxation described as a tightening; repaired
  2026-08-05.** Calling it "stricter, not looser" was true on the evidence axis and false on the
  coverage axis, and only the first was stated. Under it the weakest conforming delivery was: widen
  *zero* stages and record four reasons, satisfying criteria 1, 2 and 7 vacuously. Two repairs,
  both operator-decided: DOGFOOD-03 gained a **delivery floor** (at least one stage newly widened on
  a newly captured run, or an explicit escalation saying why none could be), and the "visibly and
  deliberately" clause moved into binding criterion 1 — it previously lived only here, in a document
  whose own notice disclaims its bindingness. Also newly owned: **`Stage::Code` is in scope**, since
  the requirement quantifies over every stage on the path and Code's raw capture was deleted during
  Phase 31's cleanup.

  **What this does NOT license.** Leaving a stage narrow still requires the recorded reason in the
  const's doc comment, and D-04's filing obligation is unchanged. "Not evidenced" must be visible
  and deliberate — the requirement now turns on exactly that, so a silent omission fails it.

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

- **D-05 [AMENDED — its premise is refuted; the decision stands, its cost does not]:** **A
  status/verdict disagreement routes to `ValidateOutcome::Ambiguous`**, not `Failed`.
  This is two independent signals disagreeing, which is the case `Ambiguous` was created for, and
  `D-18e`'s recorded reasoning applies verbatim (`pipeline_outcomes.rs:150-158`): collapsing
  disagreement onto `Failed` routes it through the counter-based auto-loop — a DELAYED gate
  indistinguishable from an ordinary retry to an operator watching it — where the binding operator
  decision requires an IMMEDIATE one. `Ambiguous` also never touches `consecutive_failures`, and
  its payload already names which two signals disagreed for the `[never-silent]` gate context.
  ~~Accepted cost, stated explicitly: an unattended run now stops where it previously advanced.~~
  ~~**Reversibility:** costly.~~

  **Amendment — the stated cost is FALSE and the reversibility rating was wrong.** The four
  `(non-Success, Some(Pass))` pairs this decision routes are **unreachable in production**:
  `decide_action` intercepts every non-`Success` status before `classify_validate_outcome` is
  called (`34-REVIEW.md` R-01). No unattended run stops where it previously advanced, because no
  run reaches those pairs. **D-05's runtime behaviour delta is zero** — it is a matrix-cell
  decision for an unreachable input, which is a materially different decision from the one put to
  the operator. It remains correct as defence-in-depth and is retained on that basis.
  — **Reversibility:** reversible — a pure function over `&AgentResult`, no I/O, no reachable
  behaviour change to unwind.

  **What D-05 does NOT cover, and a planner must not extend it to:** the only *reachable* rows are
  `(Success, Gaps)` and `(Success, None)` — plus `(Success, Pass)`, the positive control. Those are
  **not** status/verdict disagreements and D-05 says nothing about them. Collapsing them to
  `Ambiguous` would turn the ordinary "validation found gaps" auto-loop into an immediate gate on
  cycle one — the loop 999.65/999.66 and Phase 33 just repaired. See D-06's amendment.

- **D-06 [AMENDED — the tuple was wrong]:** **The fix is an exhaustive match with no wildcard arm
  reaching `Passed`**, so a future `AgentStatus` variant is a compile error rather than a silent
  join.

  **Amendment — the match tuple is `(decided_by_layer, status, verdict)`, NOT `(status, verdict)`.**
  The originally-recommended `(status, verdict)` tuple cannot express `external`, which is
  `decided_by_layer == Some(0) && status == AgentStatus::Success` (`pipeline_outcomes.rs:204`) and
  which the two existing `Ambiguous` arms are conditioned on. A two-field match must collapse
  `(Success, Gaps)` and `(Success, None)` to one value each, silently erasing the external-verify
  policy or breaking the ordinary auto-loop. The structural goal — exhaustive, no wildcard to
  `Passed`, new variant is a compile error — is sound and unchanged; only the tuple was wrong. This
  was the orchestrator's error in the option recommended to the operator. See `34-REVIEW.md` R-05.

  **Shape, specified — the amendment above was under-specified and had a trap.** Normalise
  `decided_by_layer` to a two-state value FIRST — `let layer0 = result.decided_by_layer == Some(0);`
  or a two-variant provenance enum — then `match (layer0, status, verdict)`, 2 × 7 × 3 = 42 cells.
  **The normaliser must be layer-only.** Reusing the existing composite `external`
  (`layer == Some(0) && status == Success`) is the trap: it folds an `AgentStatus` equality test
  back in — the exact hand-audited construct D-06 exists to eliminate — and `external == false`
  then conflates "Layer 1, Success" with "Layer 0, Failed", so the arms can no longer tell
  provenance from status. The wildcard ban is **positional**: `_` in the layer or verdict position
  is fine; only the status position must be enumerated.

  Verified writable, not assumed: a wildcard-free match compiles in 10 arms, and two negative
  controls hold — deleting a status arm and adding an 8th `AgentStatus` variant both produce E0004.
  The two `external`-gated `Ambiguous` arms survive verbatim as `(true, Success, Some(Gaps))` and
  `(true, Success, None)`; `(false, Success, Some(Gaps) | None)` stays `Failed`, preserving the
  ordinary auto-loop D-05's "what this does NOT cover" paragraph demands.

  **Sequencing [CORRECTED]:** ~~D-13 (999.76) lands before this rewrite.~~ There is no such
  dependency. The match's input type is unchanged by 999.76 and its `external` arms are live in
  main-checkout runs, so the rewritten match is byte-identical whichever lands first. The premise
  was also false — `evaluate_layer0` does return `Some(0)` in worktree mode, with `status: Failed`;
  what is unreachable there is `external == true`. The real constraint runs the other way and is
  recorded in D-15: **999.76 must not land without the graft fix.**

  **A wildcard to `Failed` is also forbidden.** The original wording banned only a wildcard
  *reaching `Passed`*, which permits `_ => Failed` and preserves the same
  compiles-untouched-against-a-new-variant weakness in the other direction. Enumerate all seven
  statuses.
  Rejected: a minimal `status == AgentStatus::Success` equality guard on the existing arm. The
  function's own doc comment (`pipeline_outcomes.rs:184-188`) already admits the current equality
  test was "audited by hand" precisely because an equality test compiles untouched against a new
  variant, and `IdleTimeout` arriving during Phase 31 is proof the variant set grows. This repo
  consistently prefers structural over hand-audited — the wildcard-free match guarding
  `decide_action`, and `ParsedCapture` making dropped lines representable.
  — **Reversibility:** reversible — a pure function over `&AgentResult` with no I/O, directly
  unit-testable, no callers depend on the match's internal shape.

- **D-07 [AMENDED — the specified demonstration is unwritable]:** ~~A test pins the pre-fix
  behaviour (status `Failed` + `verdict: pass` reaching `Stage::Ship`).~~ **That test cannot be
  written.** It could only be constructed by calling `classify_validate_outcome` directly,
  bypassing `advance()` — it would pass, look like a demonstrated exploit, and measure a state
  production cannot produce. That is precisely the proxy-measurement shape criterion 4 exists to
  avoid, encoded in a locked decision.

  **Superseded rationale, restored — it was deleted outright in the first amendment, which was the
  one place this document's strike-through discipline lapsed, and it argues against that
  amendment:** *"Rejected: a written audit alone — that is an assertion about code, verified once
  by reading, with nothing preventing drift, and this project has shipped green tests over a broken
  feature more than once (`31/D-19`'s stated reason for a live gate). Rejected: the test alone — the
  reasoning about why auto mode skips the gate would live nowhere a future reader finds it."* That
  objection turned out to be right: the written-audit-alone half is exactly what got the answer
  wrong, and a live demonstration is what corrected it.

  **The amended deliverable [AMENDED AGAIN 2026-08-05]:** (a) the **written finding**, now carrying
  the *corrected* answer per D-15 — the inversion IS reachable, via the graft; and (b) an
  **executable demonstration of the graft**, which is writable end-to-end and was in fact already
  run: seed a Validate stage with a passing Layer-0 probe and a `{"status":"failed","verdict":
  "pass"}` marker, assert the stage transitions to Ship pre-fix and gates post-fix, with the
  verdict-removed and Layer-0-disabled negative controls beside it.

  ~~An executable test asserting `decide_action` routes every non-`Success` variant away from
  `Advance`.~~ **Dropped: it is a no-op.** All six non-`Success` variants already have named tests
  doing exactly that (`cargo test -p devflow-core --lib outcome_policy::` → 9 passed, 0 failed, 538
  filtered out). The proposed test was strictly weaker than what exists.

- **D-08 [AMENDED — the sweep was under-dimensioned]:** **Criterion 3 is verified by a full matrix
  sweep**, plus named controls. Phase 30's constraint-9 sweep is direct precedent. Rejected: four
  named per-status mirror tests as the roadmap entry proposes — they cover only the four pairs
  someone thought to write.

  **Amendment — the sweep is `(layer0 ∈ {true,false}) × 7 statuses × 3 verdicts` = 42 cells, not
  21.** D-06's tuple widened and D-08 was not updated with it. The failure this would have caused
  is concrete, not theoretical: `decided_by_layer` is `#[serde(default)]` and its own doc reserves
  `None` for test fixtures, so the natural sweep fixture leaves `external` false in all 21 cells —
  **both `Ambiguous` arms go unexercised, and a regression deleting them both is green.** D-08's
  positive control does not catch it either, because `(Success, Pass) → Passed` is
  layer-independent.

  **Required named controls:** the positive control `(_, Success, Some(Pass)) → Passed`; and the
  two `Ambiguous` cells `(true, Success, Some(Gaps))` and `(true, Success, None)` as their own
  named positive controls, each paired with its `layer0 = false` mirror asserting `Failed` — the
  pairing is what makes the layer dimension load-bearing rather than decorative.

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

- **D-11 [AMENDED — the cost was understated when the question was put to the operator]:** **A
  capture showing a `background_tasks_changed` list that never drains still widens the stage** —
  a non-draining list means the close rule correctly held stdin open, which is the 999.64 orphan
  being *prevented*, not a defect. Record it as a stage where the drain arm is **load-bearing
  rather than defensive**: 30-04 measured it defensive on Code (n=2 Mode B trials delivered
  everything without it), and 999.73's own entry says that reasoning does not transfer.

  **Amendment — what "leans on the idle timeout" actually means.** The question as put to the
  operator described the cost as "the capture must show the timeout behaved." That understated it.
  If the rule never fires, the supervise loop's only reachable exit is
  `RecvTimeoutError::Timeout` with `close_signalled == false` → `fire_idle_timeout` → **the child
  is terminated**, an authoritative `AgentStatus::IdleTimeout` is written, and `decide_action`
  routes it to `GateReview`. A stage widened on such a capture is **killed and gated on every
  subsequent run** where the shape recurs — unusable unattended. See `34-REVIEW.md` R-03.

  **Therefore the decision is narrowed, not reversed:** widening on a non-draining capture is
  permitted only where the phase states *why the shape was pathological rather than routine* for
  that stage. D-10's n=1 does not supply that basis on its own, and the criterion-2 rewrite in
  ROADMAP.md now requires naming the observed `BackgroundTaskState` and what a recurrence costs.

  **`Unreadable` is explicitly EXCLUDED from this decision** — see D-14.

- **D-12:** **`31/D-14` (per-child declared tokens) stays out and stays deferred**, and is
  **re-filed as its own numbered `999.x` backlog entry** so the 999.73 pairing note does not
  quietly expire. `31/D-14` deferred it on size, not merit; this phase already carries a four-stage
  evidence campaign plus 999.74's behavioural change. (Renamed here from "D-14" to `31/D-14` to
  free the local `D-14` label, and because the bare form was itself a label collision.)

### Folded scope — 999.76 (added 2026-08-05)

- **D-13:** **999.76 is folded into this phase**, on scope freed by the criterion 1 and 4
  corrections. Operator decision, 2026-08-05. It is not unrelated work: `classify_validate_outcome`'s
  `external` predicate requires `decided_by_layer == Some(0)` **and** `status == Success`
  together, and 999.76 makes that *combination* unreachable in worktree mode — DevFlow's default
  shape. (Corrected: `Some(0)` alone IS produced there, with `status: Failed`, by the mis-discovery
  veto. An earlier version said worktree runs never produce `Some(0)`, which is false.)

  **The dependency runs the opposite way from what this decision first recorded.** 999.76 does not
  need to land first — see D-06's corrected sequencing note. It must not land **without** D-15's
  graft fix, because making Layer 0 discovery work in worktree mode makes `Some(0)` common, which is
  the graft's precondition.

  Within my remit and decided: **999.76 sequences before D-06's match rewrite**, and its second
  call site (`phase_has_blocking_human_checkpoint`, `pipeline_launch.rs:957`) is fixed in the same
  change — the 999.76 entry states the class recurs otherwise. Rejected: also folding in 999.71
  (capture-writer torn lines), which is adjacent but would re-inflate a scope that shrank for good
  reasons.

### The Layer 0 verdict graft — added after the second review pass

- **D-15:** **`reconcile_layer0_verdict` is in scope, and it is where 999.74's real defect lives.**
  Operator decision, 2026-08-05. The function (`agent_result.rs:2143-2156`) grafts Layer 1's
  `verdict` onto an affirmative Layer-0 probe success while checking only **Layer 0's** status. A
  marker of `{"status":"failed","verdict":"pass"}` therefore yields `(Success, Some(Pass),
  Some(0))`, which `decide_action` advances and `classify_validate_outcome` reads as `Passed` —
  Ship, in `Mode::Auto`, on a run whose agent reported failure.

  **The fix is to consult Layer 1's status before transplanting its verdict.** A verdict attached
  to a self-reported failure is not a pass.

  **Why this was missed the first time, recorded because the error is repeatable.** The first pass
  proved the classifier's inputs are always `Success` and inferred the inversion was unreachable.
  The proof was right; the inference was wrong. The status is **laundered upstream** — it genuinely
  *is* `Success` by the time the classifier sees it. Checking a function's inputs does not establish
  a whole-system property.

  **Consequences for the other decisions, all binding:**
  - **D-06's fix does not close this.** Gating the `Passed` arm on the derived status passes
    cleanly here. Criterion 3 and criterion 4 are separate deliverables.
  - **D-13 (999.76) must not land without this fix.** Moving Layer 0 discovery to the execution
    root makes `decided_by_layer == Some(0)` common rather than rare, which is exactly the graft's
    precondition.
  - **Do NOT "correct" `idle_timeout_result`'s `verdict: None` doc comment.** An earlier amendment
    instructed that; it is wrong. `reconcile_layer0_verdict` sources its verdict from
    `evaluate_layer1`, whose first statement is the idle-timeout side channel — a timeout carrying
    a verdict would graft and ship. That comment documents a live guard. Only
    `reconcile_stream_success_against_exit_code`'s sibling note is overstated.

  **Established by demonstration, not reading:** six cases against a HEAD-built `advance` binary in
  out-of-repo temp projects. Negative controls: verdict removed or set to `gaps` → gates; Layer 0
  disabled → `decide_action` intercepts. **Not established:** whether a real agent emits a
  self-contradictory marker in practice — no parser cross-checks `status` against `verdict`.

### Unreadable — the D-04/D-11 tie-break

- **D-14:** **`BackgroundTaskState::Unreadable` is governed by D-04, not D-11 — file it, and the
  stage stays narrow.** Operator decision, 2026-08-05, taken after review surfaced the conflict:
  D-04 (capture-revealed parser defect → file, stage stays narrow) and D-11 (a list that never
  drains → widen, the rule is working) both describe `Unreadable`, and the disambiguating option
  had been declined during discussion in favour of D-11's unconditional form.

  **The governing reason is the parser gap, not the timeout.** An `Unreadable` in a capture means
  DevFlow could not read a `background_tasks_changed` shape the CLI actually emitted — the
  999.75 / DEN-96 class — and that is a defect worth filing whether or not the run recovered.

  **Precision correction, recorded because an earlier draft of this document and of `34-REVIEW.md`
  R-03 overstated it:** `Unreadable` does **not** block `should_close()` permanently.
  `CloseRule::observe` (`crates/devflow-core/src/monitor.rs:571-581`) reassigns
  `background_tasks` on *every* `background_tasks_changed` event, so a subsequent readable
  announcement clears `Unreadable` to `Pending(n)`. It blocks until a later readable announcement
  arrives, and only blocks through to the idle timeout when it is the run's last announcement. The
  decision does not rest on the overstated version.

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
  authoritative goal and its seven success criteria (four originally; rewritten 2026-08-05).
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
  — the Code-stage acceptance run. **Corrected 2026-08-05: this is a written report, NOT a
  capture.** `31-VERIFICATION.md` records that the raw stream-json capture was deleted during
  cleanup and never committed; it survives only as transcription, and Phase 31 has no evidence
  directory. **Two consequences.** (a) No real production stream capture exists in-repo today, so
  this phase's captures are the first — committing them is a deliberate act the plan must specify
  (`.devflow/.gitignore` is `*`). (b) Its pass bar must **not** be reused verbatim: `31-ACCEPTANCE.md:25`
  reads *"VOID unless the capture shows a `background_tasks_changed` event with a NON-EMPTY `tasks`
  array followed by a drain to `[]`"*, which a non-backgrounding stage cannot satisfy by
  construction — followed literally it voids all four of this phase's captures.
- `.planning/milestones/v2.3.0-phases/31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl/31-VERIFICATION.md`
  — how constraint 4's AND rule was verified.

### Evidence the drain-gate decisions rest on
- `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/30d-MEASUREMENTS.md`
  — 30d's own exit-timing trials. **Citation corrected 2026-08-05:** the 4.54–11.51s
  drain-to-final-`result` figure is **30c's** measurement, primary source
  `30c-VERDICT-reliability.md:114`; `30d-MEASUREMENTS.md:118` only attributes it. The "truncated in
  all seven trials" claim is `30-02-SUMMARY.md:190`. The pooled "14 trials" framing comes from
  `monitor.rs:507`, spanning 30c and 30d together.
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
- **Corrected 2026-08-05.** The test that breaks is `canary_gate_only_applies_to_the_stream_launch_path`
  at `pipeline_launch.rs:1754`: its assertion at `:1771-1774` requires `Stage::Plan` to resolve to
  the legacy path, and its negative control at `:1777-1780` requires the predicate to "still say yes
  somewhere". Once all five stages are widened, **no Claude stage yields `false`** — the test is
  unconstructible as written and must be rebuilt on the legacy opt-out or a non-Claude agent.
  `:2329-2334` (`legacy_launch_flag_forces_the_single_document_path`) uses `Stage::Code` throughout
  and does **not** need updating; the earlier claim that both need changes was wrong.
- `ValidateOutcome::Passed` → `ValidateResult::Passed` → `transition(.., Stage::Ship)` at
  `pipeline_outcomes.rs:395` — reachable only from `Action::Advance`, i.e. only on
  `AgentStatus::Success`.

### Traps found in review (2026-08-05) — mechanical, and each will bite during planning

- **The launch argv is stage-blind.** `ClaudeAgent::exec_command` (`agents/claude.rs:46`) ignores
  `_phase`, `_prompt` and `_extra_writable_roots` and returns a fixed argv. Nothing about the
  transport varies per stage — a per-stage capture is behavioural evidence only.
- **`DEFAULT_CAPTURE_RETENTION = 5`** (`crates/devflow-core/src/config.rs:12`) evicts an earlier
  stage's capture if the phase takes any Validate→Code loop-back. Raise `DEVFLOW_CAPTURE_RETENTION`
  or copy each capture as it lands; do not assume all four survive to the end of the run.
- **Widening relocates the `31/D-15` canary refusal from Code to Define.** `canary_gate` memoizes
  into `state.canary`, so cost does not multiply — but a run whose canary returns
  `Absent`/`Unverified` now refuses at the *first* stage instead of completing Define and Plan on
  the legacy path. A real change to unattended behaviour.
- **`devflow __monitor`** (`crates/devflow-cli/src/main.rs:133`) is a hidden subcommand that runs
  `run_pipe_owning_monitor` directly and never consults `STREAM_JSON_STAGES` — the non-circular
  capture route under D-02's amendment. It advances the stage machine on child reap, so point it at
  a scratch phase.
- **`AgentStatus` has seven variants**, not four. `RateLimited` and `AgentUnavailable` need decided
  destinations under D-06's exhaustive match. Note that `decide_action` routes `RateLimited` to
  `AutoResume` deliberately, so sending it to an immediate gate would contradict a live, defended
  choice — even though the cell is unreachable at the classifier.

### 999.76 — the folded defect (D-13)

- `evaluate_layer0` (`crates/devflow-core/src/agent_result.rs:2036-2042`) returns `None` unless
  external-verify is enabled, then computes the worktree-aware `execution_root` and **discovers
  commands from `project_root` on the very next line**, using `execution_root` only later to run
  probes. `.planning/` is tracked, so an in-flight phase's `{N}-PLAN.md` lives on
  `feature/phase-{N}` inside the worktree and is absent from the main checkout for the phase's whole
  duration.
- Second call site, same root cause, fix together: `verify::phase_has_blocking_human_checkpoint`
  at `pipeline_launch.rs:957`, which silently kills the plan-28-03 checkpoint auto-decide path in
  worktree mode.
- The negative control the 999.76 entry records, and which a test should mirror:
  `git ls-tree -r develop --name-only -- .planning/phases | grep -c '/33-'` returns **0** while the
  same command against `HEAD` returns **17**. The non-recursive form returns 0 for every ref and
  proves nothing — use `-r`.

</code_context>

<specifics>
## Specific Ideas

### Findings surfaced during discussion — BOTH have since been downgraded by review

- **F-01 — REFUTED (2026-08-05).** ~~A one-pass read suggests the 999.74 inversion is stronger than
  999.67's analogue and reaches `Stage::Ship` ungated in `auto` mode.~~ The **local** read was
  accurate: `ValidateOutcome::Passed` → `ValidateResult::Passed` → `transition(.., Stage::Ship)` at
  `pipeline_outcomes.rs:395` with no gate when `Mode::Auto` and `consecutive_failures < 3`; and the
  gate prompt at `:377` does read *"Validation passed — approve to ship?"*. **But the state is
  unreachable.** `classify_validate_outcome` is called only inside `Action::Advance`, and
  `decide_action` maps only `AgentStatus::Success` there — so a non-`Success` status never reaches
  the arm. The hypothesis was wrong, and D-05's accepted cost was calibrated on it. Full evidence
  and negative controls: `34-REVIEW.md` R-01.

  Recorded rather than deleted because the superseded hypothesis is what a reader retracing the
  999.74 entry would independently reconstruct — and because two in-source doc comments still
  assert it.

- **F-02 — DOWNGRADED (2026-08-05). It does not refute `31/D-10`.** During the Phase 26 dogfood
  (`devflow start --phase 26 --agent claude --mode auto --yes-ship`, 2026-07-29), the **Plan**
  stage's orchestrator backgrounded its `gsd-phase-researcher` Agent() call and ended its turn;
  under one-shot `claude -p` there was no next turn, the work was lost, and the process still
  exited 0 — caught only by DevFlow's never-silent zero-commit gate. That much is recorded history.

  Three corrections, all found in review:

  1. **The quotation was strengthened.** `31/D-10` says Code "is **the stage** that actually
     backgrounds" (`31-CONTEXT.md:109-111`), not "the **only** stage". F-02's "contradicts D-10"
     framing rested on the strengthened wording.
  2. **The cited artifact no longer holds the evidence.** `.devflow/phase-26-stdout` exists but has
     been overwritten by a later run — a single result envelope recording a 429 session limit, with
     no `background_tasks_changed` and none of the backgrounding narrative. It is also gitignored
     and untracked, so it was never a citable trail for a downstream reader.
  3. **Decisively: that run could not have produced stream-json evidence at all.** Plan is not in
     `STREAM_JSON_STAGES`, so it ran the legacy single-document path. "The GSD orchestrator
     backgrounded an `Agent()` call" and "the stage emits `background_tasks_changed` under the
     pipe-owning monitor" are different claims; this document asserted the second from the first.

  **What survives.** F-02 still supports D-09 being empirical — "which stages background" cannot be
  answered by assumption in *either* direction — and it is a real reason not to trust
  `pipeline_launch.rs:441-445`'s reproduction of the claim. It is not evidence that Plan backgrounds
  under the stream path. If a Plan-stage capture settles it either way, that doc comment should be
  corrected in the same commit that widens the constant.

</specifics>

<deferred>
## Deferred Ideas

- **`31/D-14` per-child declared tokens** (per D-12) — one declared token per dispatched child rather
  than `31/D-13`'s startup-canary-only scope. Would defeat constraint 7's coalescing undercount
  directly instead of leaning on the drain gate. Deferred on size, not merit, for the second time
  (`31/D-14` was the first). **Action item: file it as its own numbered `999.x` ROADMAP entry plus
  a Linear issue**, so the 999.73 pairing note does not expire silently when 999.73 closes.

- **Any parser or monitor defect a per-stage capture reveals** (per D-04) — filed as a numbered
  `999.x` entry with its capture as evidence, not fixed here.

- ~~**Un-widened stages** (per D-02) — if a stage's capture cannot be obtained, the remaining
  widening returns as a numbered `999.x` entry and the phase closes PARTIAL on criterion 1.~~
  **RETIRED 2026-08-05** — this is the rule D-02 Amendment 2 replaced, and it survived here
  un-struck through the first rewrite. A reviewer reading it would reintroduce the milestone
  contradiction the amendment resolved. Current rule: an un-widened stage carries a recorded reason
  and *satisfies* the reworded DOGFOOD-03; the phase does not close PARTIAL on that account, and a
  zero-widening outcome is an explicit escalation to the operator.

</deferred>

---

*Phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-999-74*
*Context gathered: 2026-08-05*
