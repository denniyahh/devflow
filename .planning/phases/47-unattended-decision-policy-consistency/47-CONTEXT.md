# Phase 47: Unattended Decision Policy Consistency - Context

**Gathered:** 2026-09-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Prompt-text correctness for the unattended decision policy. Two things become true:

1. The unattended decision policy (`CODE_STAGE_POLICY`) reaches the Code prompt on the **Validate
   loop-back** path, not only on the first Code pass — for the `FullExecute` fix arm only.
2. An agent resumed into a session carrying both `CODE_STAGE_POLICY` and
   `checkpoint_auto_decide_prompt` receives **one** rule about who may resolve a `blocking-human`
   gate, not two contradictory ones.

**Deliberately NOT closed here:** which instruction a model actually *follows*. That is a question
about model instruction-priority, not answerable by reading source. This phase removes the
prompt-level contradiction and hands the behavioural question to Phase 49 with a named evidence
standard (see D-09 – D-12).

**Scope note — this phase is larger than "prompt rendering only".** The ROADMAP describes it as
prompt rendering with no source change beyond `prompt.rs`. The decisions below add: two ROADMAP
amendments (D-06, D-12), a docs rewrite (D-13), and a new dev-dependency with CI wiring
(D-14 – D-16). Planning should treat that as the real surface.

</domain>

<decisions>
## Implementation Decisions

### Gate rule — who may resolve a `blocking-human` gate (DECN-03)

- **D-01:** The **resume wins, via an explicit carve-out**. `CODE_STAGE_POLICY`'s absolute
  prohibition becomes conditional: the agent may self-resolve *when DevFlow resumed it specifically
  to resolve that gate*. The authority is narrow, audited, and traceable to the resume injection at
  `pipeline_launch.rs:1100`. Rejected: making the policy win (would render Phase 28's auto-decide
  inert and halt every headless run at a gate) and keeping both texts but preventing co-residence
  (leaves two contradictory rules in source for the next injection site to rediscover).

- **D-02:** The carve-out covers **both human-only classes** — `blocking-human` gates *and*
  package-verification checkpoints. This matches `checkpoint_auto_decide_prompt`'s existing generic
  "human-blocking checkpoint" wording, so that text needs no arm-splitting.
  — **Reversibility:** one-way — the *text* is trivially revertible, but the consequence is not: an
  unattended run may now self-approve a package verification, and a package published under a
  self-approved verification cannot be unpublished. Rated one-way deliberately so `gsd-planner`
  raises a `checkpoint:decision` before the task that implements it, giving the operator one more
  look at this specific widening. **Operator accepted this tradeoff explicitly during discussion.**

- **D-03:** The contradiction test is **layered** — a constant-level unit test (unconditional
  prohibition string gone, conditional form present) *plus* a co-resident render test that builds
  the real combined prompt as `pipeline_launch.rs:1100` injects it. **Both must go red on the
  pre-fix tree.** The render test is the one that carries the claim; the constant test only proves
  the constant changed.

- **D-04:** The gate rule gets **one definition site**. Hoist it into its own constant that both
  `CODE_STAGE_POLICY` and `checkpoint_auto_decide_prompt` interpolate — the Phase 46 D-03
  one-definition-site pattern, applied to the exact drift that produced this contradiction. One
  shared wording must serve both contexts.
  — **Reversibility:** costly — undoing means re-inlining the rule at both sites and rewriting the
  tests that assert on the shared constant.

### Fix surface — reaching the loop-back path (DECN-02)

- **D-05:** Extract the "**does this arm carry the policy**" decision into a shared helper consulted
  by both `fix_prompt` and `workflow_code_prompt`, rather than mirroring the arm shape by hand.
  Single definition site for the arm rule, consistent with D-04.
  — **Reversibility:** costly — touches `workflow_code_prompt`, which is currently correct.

- **D-06:** **Fix all four affected adapters and correct the written record.** The ROADMAP goal and
  DECN-02 name only "Claude/OpenCode"; the actual blast radius is every adapter routed through
  `render_claude_style` — **claude, opencode, hermes, antigravity**. `codex` and `pi` use
  `render_workflow_style` and are already correct. Amend the ROADMAP Phase 47 goal and
  REQUIREMENTS DECN-02 to name hermes and antigravity, so a future reader does not believe those
  two were audited and excluded.

- **D-07:** Criterion 2's presence test runs **per-adapter across all six**. Drive `render_prompt`
  on every adapter with `StageIntent::Code { fix: Some(FixType::FullExecute) }` and assert the
  policy is present. **`codex` and `pi` are passing controls that must stay green** — they prove the
  test discriminates rather than asserting a tautology, and they catch a future adapter that
  switches render style.

- **D-08:** *(Claude's discretion — see below.)* Whether the claude-style path gets its own
  `GapsOnly`/`AuditFix` omission negative control.

### Phase 49 observation handoff (DECN-03 behavioural arm)

- **D-09:** The observation item lives in **both places** — the full evidence definition in
  `47-VALIDATION.md`, plus a one-line pointer in Phase 49's ROADMAP Success Criteria naming that
  file. Phase 49 is structurally reminded; the detail stays with the phase that found it.

- **D-10:** The evidence standard is **audit event plus recorded reasoning**.
  *Followed* = a `checkpoint_auto_decided` event fires for a `blocking-human` gate **and** the final
  message carries the comparison reasoning the policy demands.
  *Not followed* = the agent reported and halted instead.
  Both directions are artifacts Phase 28's machinery already emits, so neither reading depends on
  interpretation. Rejected: run-progression-only, which can happen for unrelated reasons and says
  nothing about whether reasoning was recorded.

- **D-11:** DECN-03 is not settled until the gate is exercised on **all four claude-style adapters**
  (claude, opencode, hermes, antigravity). Basis: instruction-conflict resolution is
  model-specific — one adapter's result does not generalize to the others.

- **D-12:** **Widen Phase 49 now.** Amend its ROADMAP criteria to require four live runs, one per
  claude-style adapter, so DECN-03 stays closeable within the milestone rather than trailing past
  it. Phase 49 as currently written is a single `devflow start --mode auto` run; this is a real
  expansion of that phase, made deliberately from inside Phase 47.

### Policy text ownership

- **D-13:** **Update `docs/guides/unattended-mode.md` to match the carve-out.** The guide currently
  states the opposite of D-01/D-02 — "Checkpoints marked `blocking-human` … are never auto-approved
  by any mode" (`:29`), a preflight refusal for phases declaring one (`:71-76`), and "A refusal is
  final … no `--force-unattended` flag" (`:85-88`). Rewrite the "never auto-approved by any mode"
  line to name the resumed-to-resolve exception.
  — ⚠️ **RESTS ON AN UNVERIFIED ASSUMPTION.** This decision assumes the **preflight plan-scan** (which
  reads declared gates in `*-PLAN.md` *before* launch) and the **resume path** (which fires on a
  session that already hit a gate at runtime) are **disjoint** — i.e. a run refused at preflight can
  never reach `checkpoint_auto_decide_prompt`. **That was NOT established during discussion.**
  Research MUST confirm it. **If the two paths can meet, D-13 and possibly D-02 must be revisited,
  not silently allowed to stand** — the carve-out would then contradict a documented refusal that
  has no override.

- **D-14:** Add a **snapshot-testing framework** (`insta` or equivalent) so any wording change in a
  policy-carrying prompt surfaces as a review diff. No such framework exists in the workspace today.
  Rejected: assertions on the shared constant (cheap but only catches phrases someone thought to
  assert) and a doc-to-code prose guard (brittle — the class of guard this repo has shipped broken).
  — **Reversibility:** costly — a new dev-dependency carries `cargo deny check` and `cargo machete`
  obligations, CI wiring, and a review habit the repo does not have yet.

- **D-15:** The snapshot suite covers **policy-carrying prompts only** — the four claude-style
  `FullExecute` fix prompts, the two workflow-style ones, and the co-resident resume render. Scoped
  to what this phase changes, so the baseline is *reviewed* rather than blessing whatever text
  exists today (which is how a wrong string becomes the golden value).

- **D-16:** Snapshot mismatches are **required and fail the build** — `INSTA_UPDATE=no` / `--check`,
  folded into the existing required `check.sh` test job. Rejected: landing it advisory first (Phase
  46 D-05 style). A silent-drift guard that is advisory is not a guard, and this phase exists
  because text drifted unnoticed.

### Claude's Discretion

- **D-08 — claude-style omission negative control.** Operator deferred to Claude.
  **Recommendation recorded: add it.** Assert that `fix_prompt`'s `GapsOnly` and `AuditFix` arms
  still omit the policy, mirroring `prompt.rs:810` for the newly-changed renderer. Without it, the
  arm this phase actually modifies is unguarded in its failing direction, and only the *untouched*
  workflow-style renderer would object to an over-broad edit — the repo's own dead-gate class.
- Exact wording of the carve-out sentence and of the shared constant from D-04.
- Whether the snapshot framework lands as its own plan or inside the fix plan.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements and milestone framing
- `.planning/ROADMAP.md` § `### Phase 47: Unattended Decision Policy Consistency` — the four
  success criteria, including criterion 1's scope constraint and criterion 4's explicit
  not-closed-here clause.
- `.planning/ROADMAP.md` § `### Phase 49: Live Unattended Run — The Milestone's Instrument` —
  amended by D-12; carries the D-09 pointer.
- `.planning/REQUIREMENTS.md` — DECN-02 (lines 18-26) and DECN-03 (lines 28-34), plus the
  DECN-03 note at lines 185-187. Amended by D-06.

### Prompt rendering (DECN-02, DECN-03)
- `crates/devflow-core/src/prompt.rs` — the whole surface:
  - `:62` `CODE_STAGE_POLICY` definition; `:86-89` the unconditional prohibition D-01 makes conditional
  - `:379` claude-style Code arm (has policy); `:399` the single production `fix_prompt` call site
  - `:454-474` `workflow_code_prompt` — **already correct** on `Some(FullExecute) | None` at `:465`
  - `:537` `checkpoint_auto_decide_prompt`
  - `:567-581` `fix_prompt` — **the gap**; omits the policy on all three arms
  - `:800-822` the existing omission negative control that criterion 1 requires pass **unmodified**
  - `:578` `COMPLETION_PROTOCOL` — criterion 2's proof the search range is non-empty
- `crates/devflow-cli/src/pipeline_launch.rs:1100` — where the resume prompt is injected; the
  co-resident render test in D-03 must reproduce this composition.

### Adapter surface (D-06, D-07)
- `crates/devflow-core/src/agents/claude.rs:21` — `render_claude_style` ❌ affected
- `crates/devflow-core/src/agents/opencode.rs:43` — `render_claude_style` ❌ affected
- `crates/devflow-core/src/agents/hermes.rs:26` — `render_claude_style` ❌ affected, unnamed in ROADMAP
- `crates/devflow-core/src/agents/antigravity.rs:41` — `render_claude_style` ❌ affected, unnamed in ROADMAP
- `crates/devflow-core/src/agents/codex.rs:22` — `render_workflow_style` ✅ control, must stay green
- `crates/devflow-core/src/agents/pi.rs:37` — `render_workflow_style` ✅ control, must stay green

### Policy documentation (D-13)
- `docs/guides/unattended-mode.md` — `:29-31` "never auto-approved by any mode"; `:71-76` the
  preflight plan-scan and refusal; `:85-88` "a refusal is final". **The disjointness question in
  D-13 is answered here or not at all.**

### Repo conventions this phase leans on
- `CLAUDE.md` § "Verification habits this repo has already paid for" — the dead-gate entry
  (`rg -c` never discriminating) is the direct precedent for D-07's passing controls and D-08.
- `.planning/user/DEV-SETUP-CHECKLIST.md` — must be updated in the same commit as D-14/D-16's
  toolchain and CI changes.
- `.planning/phases/46-ci-load-shape-and-operator-input-validation/46-CONTEXT.md` — D-03
  (one definition site) and D-05 (advisory-by-omission) are the precedents cited above.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `workflow_code_prompt` (`prompt.rs:454`) — already implements the correct arm behaviour
  (`Some(FullExecute) | None` carries the policy, `GapsOnly`/`AuditFix` do not). D-05's shared
  helper should be extracted *from* this logic, not invented.
- Phase 28's `checkpoint_auto_decided` audit event — already emits the artifact D-10 names as
  evidence. No new instrumentation needed for the Phase 49 observation.
- The existing omission test at `prompt.rs:800-822` — the shape D-08's counterpart would mirror.

### Established Patterns
- **One definition site for a rule that appears in two places** (Phase 46 D-03, CPU pin). D-04 and
  D-05 both apply it; this contradiction is what its absence produces.
- **Negative controls are mandatory** — a guard must be shown to fail in the direction it claims to
  catch. Drives D-03 (both tests red pre-fix), D-07 (codex/pi green), D-08.
- **Adapters delegate to two render styles**; no adapter defines prompt text itself.
  `CODE_STAGE_POLICY` appears only in `prompt.rs`.

### Integration Points
- `prompt.rs:399` — the single production call site of `fix_prompt`; the only place the DECN-02
  fix changes behaviour at runtime.
- `pipeline_launch.rs:1100` — where co-residence actually happens; D-03's render test must build
  the prompt the way this site does, not an approximation.
- `scripts/check.sh` test job — D-16 folds the snapshot check in here, inside the existing
  required CI context rather than adding a new one.

</code_context>

<specifics>
## Specific Ideas

- The carve-out must read as a **scoped hierarchy**, not a second flat rule — "unless DevFlow
  resumed you specifically to resolve this gate" — so that an agent reading both fragments sees one
  rule with a condition, not two rules in tension.
- Research finding worth carrying into planning: **primacy dominates** in instruction-following
  (~73% where positional bias is detectable), and **mid-prompt rules lose 30-50% compliance**.
  Where the carve-out sentence sits inside the combined prompt is therefore a design variable, not
  a formatting detail. Phase 49's observation should record the rendered position alongside the
  outcome.
- Phase 46 D-06's precedent — "acceptance evidence is this phase's own PR" — is the model for how
  D-07's six-adapter test result should be evidenced.

</specifics>

<deferred>
## Deferred Ideas

- **Snapshot coverage for every stage prompt across all six adapters.** Considered and scoped out
  by D-15 for this phase, because accepting a large unreviewed baseline blesses current text as
  correct by default. Worth revisiting once the D-15 baseline has been reviewed and has proven
  stable in CI.
- **A doc-to-code guard** asserting `docs/guides/unattended-mode.md` and the shared constant state
  the same rule. Rejected in D-14 as brittle prose-matching, but the underlying problem — docs
  drifting from prompt text — is real and now has a second instance (D-13). Candidate for the
  999.124 backlog cluster.

</deferred>

---

*Phase: 47-unattended-decision-policy-consistency*
*Context gathered: 2026-09-09*
