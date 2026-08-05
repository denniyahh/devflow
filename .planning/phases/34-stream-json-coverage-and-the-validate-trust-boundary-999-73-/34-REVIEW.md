# Phase 34 — Adversarial Review of the Phase Definition

**Date:** 2026-08-05
**Target:** `34-CONTEXT.md` at commit `f600d2a` (decisions D-01..D-12, findings F-01/F-02)
**Trigger:** the standing practice of running an adversarial pass on CONTEXT.md after
discuss-phase, *before* any planner builds on the locked decisions — the pass that catches a wrong
decision while it is still cheap. Precedent: Phase 31's CONTEXT.md shipped with a decision citing a
`DEVFLOW_GATE_TIMEOUT_SECS` clamp-and-log precedent that does not exist anywhere in the workspace,
caught at plan time rather than definition time.

**Outcome:** the phase definition was **not safe to plan on**. Both halves rested on premises the
code refutes. `ROADMAP.md`'s Phase 34 entry and the 999.73 / 999.74 entries were rewritten on the
strength of this review, and 999.76 was folded in.

---

## Method

Six independent lanes, run in parallel:

| Lane | Scope |
|---|---|
| internal agent A | verify every citation, path, precedent and `file:line` in CONTEXT.md against HEAD |
| internal agent B | attack the twelve locked decisions for defects, contradictions, unimplementability |
| internal agent C | goal-backward: is each success criterion achievable as written |
| `codex` | full adversarial review of the definition |
| `hermes` | full adversarial review of the definition |
| `opencode` | full adversarial review of the definition |

**Every finding below was re-derived from source by the orchestrator before being recorded.** That
mattered: it caught `hermes` citing a file path that does not exist (`outcome_policy.rs` at the
`devflow-cli` path — the real module is `crates/devflow-core/src/outcome_policy.rs`), `opencode`
overstating a match-arm count (`Verdict` has two variants, so the sweep is 7 × 3 = 21 pairs, not
~50), and internal agent A contradicting itself on the very citation it was flagging (asserting the
string `4.54` "does not appear anywhere in `.planning/`" while quoting a `.planning/` line
containing it).

---

## Findings that changed the phase

> ## ⚠ R-01 IS WRONG — see the SECOND PASS section at the end of this document
>
> R-01's *narrow* claim (the classifier's own inputs are always `Success`) is true and was verified
> many times over. The conclusion drawn from it — that the trust inversion is unreachable — is
> **false**. The status is laundered upstream by `reconcile_layer0_verdict`. R-01 is left intact
> below because the reasoning error is the point: checking a function's inputs does not establish a
> whole-system property, and that error propagated into ROADMAP.md before it was caught.

### R-01 — The 999.74 trust inversion is not reachable in production (CRITICAL) — **SUPERSEDED**

Reached independently by internal agents B and C, `hermes`, and direct verification.

`classify_validate_outcome` has exactly one production call site,
`crates/devflow-cli/src/pipeline_launch.rs:937`, inside the `Action::Advance` arm of
`match outcome_policy::decide_action(stage, result.status)` in `pub(crate) fn advance`
(`:836`). `decide_action` (`crates/devflow-core/src/outcome_policy.rs:38`) is wildcard-free and
maps only `AgentStatus::Success` to `Advance`.

| status | action | reaches classifier? |
|---|---|---|
| `Success` | `Advance` | **yes** |
| `Failed` | `GateReview` | no |
| `Unknown` | `GateReview` | no |
| `IdleTimeout` | `GateReview` | no |
| `ResourceKilled` | `GateInfra` | no |
| `AgentUnavailable` | `GateInfra` | no |
| `RateLimited` | `AutoResume` | no |

Negative controls run, not merely read:

- At Validate the `GateReview` arm calls `handle_validate_outcome(.., ValidateOutcome::Failed)` at
  `pipeline_launch.rs:990` — a **different code path**, verdict-blind, passing a literal. So the
  two routes are genuinely distinguishable, not one path read two ways.
- The apparent second call site at `pipeline_gate.rs:584` is inside `#[cfg(test)]` (`:579`); its
  only `ValidateOutcome` construction (`:1150`) is `Failed`.
- `cargo test -p devflow-core --lib outcome_policy::` → **9 passed, 538 filtered out** (non-zero
  filtered count, so the selector matched real tests), with a named test per non-`Success` variant.

**Consequences.** Success criterion 4's answer is "no". Criterion 3's effect is already achieved
upstream, so the fix is defence-in-depth rather than closing an exploitable hole. CONTEXT.md's
**F-01 is refuted** — it hypothesised that `Passed` reaches `Stage::Ship` ungated in `auto` mode;
the local read was accurate (`Mode::Auto` gates Validate only at `consecutive_failures >= 3`) but
the state is unreachable. **D-05's stated accepted cost is false**: no unattended run stops where it
previously advanced, because no run reaches those pairs; its runtime delta is zero and its `costly`
reversibility rating was calibrated against a change that will not occur. **D-07 is unwritable as
specified** — its end-to-end test could only be written by bypassing `advance()`, measuring a state
production cannot produce, which is the proxy-measurement shape criterion 4 exists to avoid.

**What this does not establish.** It is a static reachability argument over the current call graph.
It proves the arm unreachable *today*, not harmless: `decide_action`'s own comment marks the
`Failed`/`Unknown` collapse "DEFERRED… Revisit if 18d requires divergent routing." Nobody
established whether a real agent ever emits a self-contradictory marker; the parsers deserialize
`status` and `verdict` independently with no cross-check, and no archived capture contains such a
line — absence here is weak evidence, not a bound.

### R-02 — The stream-json launch argv is identical across all five stages (CRITICAL)

Found by internal agent C, verified directly. `ClaudeAgent::exec_command`
(`crates/devflow-core/src/agents/claude.rs:46`) takes `_phase`, `_prompt` and
`_extra_writable_roots` — all unused — and returns a fixed argv. Nothing about the transport,
monitor wiring or parser varies per stage.

So criterion 1's "real per-stage production capture" is evidence about **agent behaviour under that
stage's prompt**, which is criterion 2's question, not about the launch mechanism. Criterion 1 as
originally worded is close to mechanically vacuous.

### R-03 — Widening can make a stage unusable unattended (CRITICAL)

Found by internal agent B, verified directly at `crates/devflow-core/src/monitor.rs:765-812`.

`CloseRule::should_close()` requires `marker_seen` AND `NeverAnnounced | Pending(0)`. If a stage
backgrounds work still pending when its marker lands, the rule never fires; the supervise loop's
only reachable exit is `RecvTimeoutError::Timeout` with `close_signalled == false`, which calls
`fire_idle_timeout` — child terminated, authoritative `AgentStatus::IdleTimeout` written, routed to
`GateReview`. Phase 30's premise is that the CLI stays alive while stdin is held, so the child does
not self-exit into `Disconnected`.

**CONTEXT.md's D-11 says to widen such a stage anyway** ("the rule working"). That is defensible for
a genuinely pathological run, but D-11 provides no way to distinguish "this run was pathological"
from "this stage always does this" — and **D-10's n=1 removes the only means of finding out.** The
discussion log shows the operator was offered the disambiguating option ("still-pending at run end
→ file") and declined it; the question as put to them understated this cost as "the capture must
show the timeout behaved."

> **Correction (2026-08-05), after this finding was first written.** An earlier version of R-03 and
> of the amended D-11 said `Unreadable` blocks `should_close()` **permanently**. That is wrong.
> `CloseRule::observe` (`monitor.rs:571-581`) reassigns `background_tasks` on *every*
> `background_tasks_changed` event, so a later readable announcement clears `Unreadable` to
> `Pending(n)`. It blocks until a subsequent readable announcement, and blocks through to the idle
> timeout only when it is the run's last announcement. The `Pending(n>0)`-never-drains case in the
> body above is unaffected — that one does hold to timeout. D-14 resolves `Unreadable` under D-04
> on the parser-gap ground, which does not depend on the overstated claim.

### R-04 — D-02's evidence rule is circular, and two escapes exist (HIGH)

Confirmed by every external lane and directly. `resolve_launch_shape`
(`pipeline_launch.rs:158-176`) routes a stage in `STREAM_JSON_STAGES` to `adapter.exec_command` +
`MonitorLaunch::PipeOwning`, and everything else to `exec_command_single_document` + legacy. Only
the pipe-owning path produces a stream-json capture, and `claude_stream_launch_enabled` offers an
opt-*out* only — a sweep of every `DEVFLOW_*` env var found no force-on.

So evidence cannot precede widening through the normal pipeline. Two escapes, neither named in
CONTEXT.md:

1. Widen in the working tree, build, run, and let the evidence decide what gets **committed** —
   making D-02 a commit-time gate rather than a build-time one.
2. `devflow __monitor` (`crates/devflow-cli/src/main.rs:133`), a hidden subcommand that runs
   `run_pipe_owning_monitor` directly and never consults `STREAM_JSON_STAGES`. It advances the
   stage machine on child reap, so it must be pointed at a scratch phase.

### R-05 — D-05 and D-06 are mutually incompatible as written (HIGH)

Found by `codex`, `hermes` and internal agent B; verified. `external` is a composite:
`result.decided_by_layer == Some(0) && result.status == AgentStatus::Success`
(`pipeline_outcomes.rs:204`). D-06's recommended tuple — `match (result.status, result.verdict)` —
**drops `decided_by_layer`**, so it cannot express the two existing `Ambiguous` arms, which are
conditioned on it. A `(status, verdict)` match must collapse `(Success, Gaps)` and `(Success, None)`
to one value each, risking turning the ordinary auto-loop into an immediate gate — the loop Phase 33
just repaired.

This was the orchestrator's error: the tuple appeared in the option recommended to the operator.

### R-06 — 999.76 makes the arms D-05/D-06 preserve inert anyway (HIGH)

`evaluate_layer0` (`crates/devflow-core/src/agent_result.rs:2036-2042`) returns `None` unless
external-verify is enabled, then discovers commands from `project_root` while the worktree-aware
`execution_root` is computed on the line above and used only later, to *run* probes. So
`decided_by_layer == Some(0)` — and therefore `external == true` — is unreachable in worktree mode,
DevFlow's default shape.

**Corrected 2026-08-05 (second pass), twice over.** (1) `Some(0)` *is* produced in worktree mode —
`evaluate_layer0` returns it with `status: Failed` in four arms, including the mis-discovery veto.
What is unreachable there is `external == true`, i.e. affirmative Layer-0 success. (2) The
sequencing claim built on this ("999.76 before the match rewrite") does not hold: the match's input
type is unchanged by 999.76 and its `external` arms are live in main-checkout runs, so the rewritten
match is byte-identical whichever lands first. The real constraint is the reverse — **999.76 must
not land without the graft fix** (second-pass finding S-01), because it makes `Some(0)` common,
which is the graft's precondition.

**Also omitted here, and material:** the source records this behaviour as deliberate.
`agent_result.rs:2022-2031` reads "Two roots are intentionally kept distinct (review Plan 03 MEDIUM,
OpenCode)". The 999.76 backlog entry addresses that ("the doc comment … asserts the opposite and is
false"); this section presented it as a plain oversight, so a reader of R-06 alone would not know
the fix must overturn a prior peer-review decision.

### R-07 — `BackgroundTaskState::Unreadable` is ungoverned (HIGH)

Found by all three external lanes and internal agent B. D-11 ("a list that never drains → widen")
and D-04 ("a capture-revealed parser defect → file it, stage stays narrow") give opposite answers
for `Unreadable`, which is by construction a parser gap — the 999.75 / DEN-96 failure class. The
disambiguating option was offered in discussion and declined. The stakes are R-03's: `Unreadable`
blocks `should_close()` until a later readable announcement arrives — and through to the idle
timeout if it is the run's last — so widening on it risks the forced-timeout shape. (Corrected
2026-08-05: an earlier version of this sentence said "permanently", repeating the overstatement
already corrected under R-03.)

### R-08 — Criterion 3's enumeration was incomplete (MEDIUM)

`AgentStatus` has **seven** variants (`agent_result.rs:47`). Criterion 3 named four, omitting
`RateLimited` and `AgentUnavailable`. D-08's full matrix sweep forces a decided value for all 21
`(status, verdict)` pairs, so two statuses would have needed semantics nobody chose. Noted for
`RateLimited` specifically: `decide_action` routes it to `AutoResume` deliberately so it reaches the
resume path rather than a human gate.

---

## Citation defects in `34-CONTEXT.md` (the class this pass exists to catch)

| Claim | Verdict |
|---|---|
| `31-ACCEPTANCE.md` is "the only real production stream capture that exists today" | **Wrong.** `31-VERIFICATION.md` records the raw capture was deleted during cleanup and never committed; it survives only as transcription. Phase 31 has no evidence directory. |
| `30d-MEASUREMENTS.md` is the source of the 4.54–11.51s drain lag across 14 trials | **Mis-attributed.** `30d-MEASUREMENTS.md:118` attributes the figure to **30c**; the primary is `30c-VERDICT-reliability.md:114`. The "truncated in all seven trials" claim is `30-02-SUMMARY.md:190`. The 14-trial framing is a pooled 30c+30d figure taken from `monitor.rs:507`. |
| `31/D-10` says Code "is the **only** stage that actually backgrounds" | **Misquoted.** `31-CONTEXT.md:109-111` reads "it is **the stage** that actually backgrounds". Strengthened inside quotation marks — and F-02's "contradicts D-10" framing rested on the strengthened version. |
| `.devflow/phase-26-stdout` is F-02's evidence trail | **Wrong.** The file exists (1172 bytes) but has been overwritten by a later run: a single result envelope recording a 429 session limit. No `background_tasks_changed`, none of the backgrounding narrative. It is also gitignored and untracked, so it is not a citable trail. |
| The tests at `pipeline_launch.rs:1763` **and** `:2329-2334` both need updating | **Half false.** `:2329-2334` uses `Stage::Code` throughout and still passes. The one that breaks is at `:1771-1780`, including a negative control asserting the predicate "must still say yes somewhere". |
| `D-18e` cited without a phase prefix | Phase **18**'s decision, neither this phase's `D-NN` nor `31/D-NN` — the exact conflation the document's own label-collision warning exists to prevent. |

**Verified correct and worth recording:** all 18 cited paths exist; the great majority of
`file:line` references are exact at HEAD. Both inherited Phase 31 staleness claims still hold —
"every gate fixture is labelled SYNTHETIC in-source" and "no archived capture contains a prompt
echo" were each confirmed two independent ways, the second after a broken proxy (a `"type":"user"`
pattern that missed the captures' `"type": "user"` spacing, caught by a `result`-count control).

---

## F-02, restated honestly

CONTEXT.md claimed the Phase 26 dogfood run refutes `31/D-10`. It does not carry that weight:

- The cited artifact no longer contains the evidence (above).
- Decisively: **that run could not have produced stream-json evidence at all**, because Plan is not
  in `STREAM_JSON_STAGES` and therefore ran the legacy single-document path.
- "The GSD orchestrator backgrounded an `Agent()` call" and "the stage emits
  `background_tasks_changed` under the pipe-owning monitor" are different claims; CONTEXT.md
  asserted the second from the first.

F-02 still justifies criterion 2 being empirical — "which stages background" cannot be answered by
assumption in either direction — but it does not refute `31/D-10`.

---

## Traps recorded for the planner

- **`31-ACCEPTANCE.md`'s pass bar must not be reused verbatim.** It is *"VOID unless the capture
  shows a `background_tasks_changed` event with a NON-EMPTY `tasks` array followed by a drain to
  `[]`"* — which a non-backgrounding stage cannot satisfy by construction. CONTEXT.md pointed at it
  as "the template for what a per-stage capture must show"; followed literally it voids all four
  captures.
- **`DEFAULT_CAPTURE_RETENTION = 5`** (`crates/devflow-core/src/config.rs:12`) evicts an earlier
  stage's capture if the phase takes any Validate→Code loop-back.
- **Widening relocates the D-15 canary refusal from Code to Define**, so a run whose canary comes
  back `Absent`/`Unverified` now refuses at the first stage instead of completing Define and Plan on
  the legacy path.
- **`canary_gate_only_applies_to_the_stream_launch_path`** (`pipeline_launch.rs:1754`) becomes
  unconstructible as written: with all five stages widened, no Claude stage yields
  `stream_launch == false`, so the test must be rebuilt on the legacy opt-out or a non-Claude agent.
- **Milestone accounting — RESOLVED 2026-08-05.** DOGFOOD-03 is a v1 requirement of the active
  v2.4.0 milestone, Phase 34 is its last phase, `REQUIREMENTS.md` models requirements as checkboxes
  with no partial state, and `999.x` backlog is explicitly out of scope for this milestone, so
  D-02's "close PARTIAL, backlog the rest" had nowhere to land.

  Fixed upstream of the phase rather than inside it: DOGFOOD-03 named four specific stages, making
  it the only one of the four requirements phrased as an implementation plan instead of an
  operator-facing guarantee. It was reworded to state the evidence discipline itself, so a stage
  left visibly and deliberately narrow *satisfies* the requirement rather than blocking it. The
  rewrite is **stricter** than the original — the original could be met by widening four stages on
  four thin captures; the rewrite cannot be met by any unevidenced widening. Operator decision;
  alternatives considered were splitting DOGFOOD-03 into 03a/03b (changes coverage accounting and
  relocates the same problem), adding a Phase 35 (largest lift, and premature while no stage is
  *known* unobtainable), and deferring to close (rejected — by then the only options are an untrue
  checkbox or an open milestone).

---

*Phase: 34-stream-json-coverage-and-the-validate-trust-boundary-999-73-999-74*
*Review conducted: 2026-08-05*


---

# SECOND PASS — reviewing the fixes (2026-08-05)

The amendments above were written by the same person who ran the first pass, so a second
six-lane pass reviewed **the fixes**, not the original problems. Same structure: three external
CLIs (`codex`, `hermes`, `opencode`) and three internal agents — attack the amendments, refute
criterion 4's new answer, audit this document's own accuracy.

**Headline: the first pass's findings held; its remediation did not.** Every first-pass finding
was independently re-verified as factually exact (R-01's narrow claim, R-02, R-03's mechanism,
R-05, R-06's mechanism, R-08, the citation-defect table, and the test evidence — `cargo test -p
devflow-core --lib outcome_policy::` → 9 passed, 0 failed, 538 filtered out, run independently
twice and matching this document's citation exactly). Nine defects were introduced by the fixes.

## S-01 — R-01's conclusion is false; the inversion IS reachable (CRITICAL)

`reconcile_layer0_verdict` (`crates/devflow-core/src/agent_result.rs:2143-2156`), reached from
`evaluate_agent_result_inner:2305` on the Layer 0 arm:

```rust
if state.stage != Stage::Validate
    || result.status != AgentStatus::Success        // Layer 0's status
    || result.decided_by_layer != Some(0) { return result; }
let verdict = evaluate_layer1(project_root, state.phase)
    .and_then(|layer1| layer1.verdict);             // Layer 1's verdict, status unread
AgentResult { verdict, ..result }
```

It grafts Layer 1's `verdict` onto an affirmative Layer-0 probe success while checking only Layer
0's status. A marker `{"status":"failed","verdict":"pass"}` parses to `(Failed, Some(Pass))`; the
graft yields `(Success, Some(Pass), Some(0))`; `decide_action` advances it; the classifier computes
`external == true` and returns `Passed`; `Mode::Auto` transitions to Ship.

**Demonstrated, not reasoned** — six cases against a HEAD-built `advance` binary in out-of-repo
temp projects. Negative controls: verdict removed → gates; verdict `gaps` → gates; Layer 0 disabled
→ `decide_action` intercepts exactly as R-01 describes, which is the control proving the harness is
not manufacturing the result. Both the single-envelope and stream-json capture paths reproduce it.

**Criterion 3's fix does not close it** — the derived status genuinely is `Success`. Preconditions
are production-reachable: `external_verify_enabled` defaults to `true` (`config.rs:81`), plus a
matching `DEVFLOW_TRUST_EXTERNAL_VERIFY`, a PLAN declaring `external_verify:`, and passing probes.
**Still unestablished:** whether a real agent emits a self-contradictory marker in practice; no
parser cross-checks `status` against `verdict`.

**Corollary — do NOT "correct" `idle_timeout_result`'s comment.** Criterion 4 originally instructed
that. `reconcile_layer0_verdict` sources its verdict from `evaluate_layer1`, whose first statement
is the idle-timeout side channel; a timeout carrying a verdict would graft and ship. That comment
documents a live guard. Only `reconcile_stream_success_against_exit_code`'s sibling note is
overstated, and only because `decide_action` does protect that path.

## S-02 — The DOGFOOD-03 reword was a coverage relaxation described as a tightening (HIGH)

Found by all three external lanes and one internal. The weakest conforming delivery under the first
reword: widen **zero** stages, record four "not evidenced" reasons — satisfying criteria 1, 2 and 7
vacuously, since nothing widened means nothing to evidence and no collateral to fix. "Stricter, not
looser" was true on the evidence axis and false on the coverage axis; only the first was stated, in
three separate documents.

Two further problems in the same reword: the clause carrying the strictness ("visibly and
deliberately still on the legacy path") lived only in `34-CONTEXT.md`, which disclaims its own
bindingness; and the reword quantifies over *every* stage on the path, which includes `Stage::Code`
— whose raw capture was deleted during Phase 31's cleanup, making the requirement false at HEAD with
no criterion requiring it be fixed.

Repaired by operator decision: a delivery floor in DOGFOOD-03, the visibility clause moved into
binding criterion 1, and Code brought into criterion 1's scope.

## S-03 — Defects introduced by the first pass's own amendments (HIGH)

| # | Defect | Status |
|---|---|---|
| 1 | D-02 sanctioned `devflow __monitor` two sentences after forbidding harnesses that skip `resolve_launch_shape` — which is exactly what it skips | fixed |
| 2 | D-02's working-tree route unspecified: `devflow start` worktrees from `develop` before the staleness check, so an uncommitted constant yields a legacy capture | fixed |
| 3 | Deferred Ideas still carried the PARTIAL-close rule D-02 Amendment 2 retired, un-struck | fixed |
| 4 | D-08's sweep left at 21 cells after D-06's tuple widened to 42 — and `decided_by_layer` is `#[serde(default)]`, so the natural fixture leaves both `Ambiguous` arms unexercised and a regression deleting them green | fixed |
| 5 | D-06's replacement tuple under-specified; the trap is reusing the composite `external` as the normaliser, which folds an `AgentStatus` equality test back in | fixed |
| 6 | D-13's sequencing premise false, and the ordering blocked the cheap classifier work behind the riskiest change in the phase | fixed |
| 7 | D-14 label collision reintroduced — two stale `D-14 per-child` references, one in Phase Boundary, inverting the operator's decision | fixed |
| 8 | D-07's original "Rejected:" rationale deleted outright, no strike-through — the one lapse in this document's own discipline, and the deleted text argued against the amendment that replaced it | restored |
| 9 | D-07's replacement test is a no-op — all six non-`Success` variants already have named tests | fixed |

**On D-06's tuple, the lanes disagreed and the disagreement was resolved by compiling.** `opencode`
judged it unwritable; `hermes` and an internal agent each compiled a wildcard-free match over
`(Option<u8>, AgentStatus, Option<Verdict>)` in 10 arms, with two negative controls (deleting a
status arm, and adding an 8th `AgentStatus` variant) both producing E0004. It is writable; it was
under-specified.

## S-04 — Citation defects in this document (LOW, all fixed)

`monitor.rs:765-812` was cited for `should_close()`, which is at `:586-593` (the range is right for
the supervise loop). `main.rs:133` was cited for `run_pipe_owning_monitor`, which appears nowhere in
`main.rs` — the call is `pipeline_launch.rs:513` inside `run_monitor`. `pipeline_gate.rs:584` was
called "the apparent second call site" when it is a `use` import and the function appears zero times
in that file. R-04's "everything else to `exec_command_single_document`" over-generalises — only the
Claude branch does that. Criterion 1 named two ignored `exec_command` arguments where R-02 names
three. The substantive claims each of these supports all survived independent re-verification.

## Still open after the second pass

- **`Pending(n>0)`-still-pending-at-run-end has no owner** when the phase cannot establish that the
  shape was pathological rather than routine. D-11 governs the widen case, D-04 excludes itself
  (D-11 calls a non-draining list "the rule working, not a defect"), D-02 excludes itself (the
  capture *was* obtained), D-14 covers only `Unreadable`. The safe default is obvious; no rule
  attaches the recorded-reason obligation, which is what DOGFOOD-03 now turns on.
- **D-11's "pathological rather than routine" bar is unenforceable under D-10's n=1**, by the
  amendment's own admission.
- **Criterion 2's framing sentence is unanswerable for a `NeverAnnounced` stage** — the close rule
  always fires vacuously there, so no capture can answer "what happens when it does not fire". The
  operative clause that follows is what should be graded.
- **Criterion 4 named two in-source comments asserting the inversion; at least four do.** The
  durable one is `agent_result.rs:5863-5866` (`MARKER_SUCCESS_CLAIMING_PASS`'s doc comment), which
  no fix touches.
- **999.74's entry cites `pipeline_outcomes.rs:260` for `ValidateResult`**, which is at `:226-229`;
  line 260 is inside `select_loop_back_fix`'s doc comment. Pre-existing, missed by the rewrite.
- **ROADMAP.md's 999.73 entry still carries the "only stage" strengthening** that the rewrite
  corrected in `34-CONTEXT.md` — and is probably where that phrasing originated.
