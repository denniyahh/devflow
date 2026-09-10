# Phase 47: Unattended Decision Policy Consistency - Context

**Gathered:** 2026-09-09
**Revised:** 2026-09-09 — after adversarial external review (codex + agy). See `47-REVIEWS.md`.
**Status:** Ready for planning — D-02 and D-12 re-confirmed by the operator 2026-09-10 (see below)

> **Revision notice.** The first draft of this document rested on an assumption it flagged as
> unverified (old D-13). External review verified it and it is **false**. Correcting it changed
> the shape of this phase: the carve-out narrows, one decision that amended another phase is
> rescinded, and two test designs were unbuildable as written. Decisions carrying a **[REVISED]**,
> **[RESCINDED]** or **[NEW]** tag changed after review; untagged decisions survived it. Every
> claim below that cites `file:line` was verified against source in this repository.

<domain>
## Phase Boundary

Prompt-text correctness for the unattended decision policy. Two things become true:

1. The unattended decision policy (`CODE_STAGE_POLICY`) reaches the Code prompt on the **Validate
   loop-back** path, not only on the first Code pass — for the `FullExecute` fix arm only.
2. An agent resumed into a session that has already seen `CODE_STAGE_POLICY` and is now handed
   `checkpoint_auto_decide_prompt` receives **one** rule about who may resolve a `blocking-human`
   gate, not two contradictory ones.

**Deliberately NOT closed here:** which instruction a model actually *follows*. That is a question
about model instruction-priority, not answerable by reading source. This phase removes the
prompt-level contradiction and hands the behavioural question to Phase 49 with a named evidence
standard (see D-09 – D-12).

**Scope note — this phase is larger than "prompt rendering only".** The ROADMAP describes it as
prompt rendering with no source change beyond `prompt.rs`. The decisions below add: one ROADMAP
amendment (D-06), a docs correction (D-13), and a new dev-dependency with CI wiring
(D-14 – D-16). Planning should treat that as the real surface.

**Scope note 2 — the contradiction is narrower than the first draft assumed, and reaching it
requires a human.** Verified in review:

- The resume path that injects `checkpoint_auto_decide_prompt` is hard-gated to the **claude**
  adapter (`pipeline_launch.rs:1569`), and `exec_resume_command` is defined only on `ClaudeDriver`
  (`claude.rs:122`). Co-residence is therefore **unreachable on opencode, hermes and antigravity**.
- In `--mode auto`, a phase whose plan declares a human-only checkpoint is **refused at preflight**
  (`preflight.rs:1076-1083`) before any agent spawns. The refusal parks at a gate; an operator
  approving that gate (`GateAction::Advance`) calls `launch_stage_inner` and **skips the check**
  (`preflight.rs:1368-1376`). That approval is the doorway through which an unattended run reaches
  the resume path at all.

This does not make the phase unnecessary — the contradictory text is real and reaches a real
prompt — but it does mean **DECN-02 (the `fix_prompt` gap) is the load-bearing half of this phase**
and DECN-03 (the co-residence contradiction) is narrower, claude-only, and human-initiated.

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

- **D-02 [REVISED — REVERSAL RE-CONFIRMED BY THE OPERATOR 2026-09-10]:**
  The carve-out covers **`blocking-human` gates ONLY**. Package-verification checkpoints keep the
  **unconditional** prohibition.

  The first draft widened the carve-out to both human-only classes on the stated grounds that
  `checkpoint_auto_decide_prompt`'s generic "human-blocking checkpoint" wording would otherwise
  need arm-splitting. **Both review lanes independently established that premise is false**, and it
  was confirmed here against source:

  - The resume route fires only when `verify::phase_has_blocking_human_checkpoint` is true
    (`pipeline_launch.rs:1569-1571`), and that predicate matches the literal string
    `gate="blocking-human"` and nothing else (`verify.rs:131-137`).
  - Package-verification checkpoints are **never routed through this injection**. No runtime
    classification sends them here.
  - So the generic prose in `checkpoint_auto_decide_prompt` (`prompt.rs:538-548`) can simply be
    **tightened** to name `blocking-human`, matching what the trigger actually detects. No
    arm-splitting is required, and none of the cost the first draft accepted is incurred.

  — **Reversibility:** the narrow form is **two-way** (text-only). The widened form was the one-way
  one: an unattended run self-approving a package verification cannot unpublish the package. That
  irreversibility was accepted **on a premise that has since been disproven**, so it is not carried
  forward.
  — **Decision history.** The operator explicitly accepted the *widened* version during the
  original discussion. Presented with the disproven premise on 2026-09-10, the operator chose
  **narrow — `blocking-human` only**. This is now **settled**, not open; no `checkpoint:decision`
  is required for it. The narrow form is two-way reversible, so it earns no one-way-door gate
  either. Do not re-widen without a new decision.

- **D-03 [REVISED]:** The contradiction test is **layered** — a constant-level unit test
  (unconditional prohibition string gone, conditional form present) **plus a two-turn delivery
  test**. **Both must go red on the pre-fix tree.**

  The first draft called for "a co-resident render test that builds the real combined prompt as
  `pipeline_launch.rs:1100` injects it". **No such combined prompt exists in production.** Verified:
  the initial Code prompt is delivered as a stdin user turn (`claude.rs:25-32`), while the resume
  instruction is built separately and passed as argv to
  `ClaudeDriver::exec_resume_command(session_id, &instruction)` (`pipeline_launch.rs:1100-1119`).
  Nothing concatenates them. Co-residence happens **across two conversation turns inside the
  resumed Claude session**, not as a string DevFlow assembles.

  A test that manually joins two strings would therefore be a **proxy**, not a test of production
  composition — exactly the class this repo has a rule about. The test must instead capture the
  **initial user turn** and the **resumed instruction** as each is actually launched, and assert the
  shared rule holds in both, with the pair asserted as a delivery pair rather than a concatenation.

- **D-04 [REVISED]:** The gate rule gets **one definition site for the rule**, not one literal
  string forced into two grammatical moods. Hoist the rule into its own constant that
  `CODE_STAGE_POLICY` interpolates directly; `checkpoint_auto_decide_prompt` must be **derived from
  and consistent with** that constant while keeping its affirmative framing.

  Review's objection, accepted: in `CODE_STAGE_POLICY` the rule is a negative exception ("do not
  self-resolve — unless resumed for this gate"); in `checkpoint_auto_decide_prompt` it is an
  affirmative directive ("DevFlow resumed you to resolve this — resolve it"). Forcing one literal
  string into both degrades both. The invariant to enforce mechanically is that **the two texts
  cannot state different conditions**, which the D-14 snapshot suite is what actually pins.
  — **Reversibility:** costly — undoing means re-inlining the rule and rewriting tests that assert
  on the shared constant.

### Fix surface — reaching the loop-back path (DECN-02)

> **This is the load-bearing half of the phase.** Unlike DECN-03, it affects four adapters, needs
> no human in the loop, and is reachable on every Validate loop-back.

- **D-05 [REVISED]:** Extract the "**does this arm carry the policy**" decision into a shared
  helper consulted by both `fix_prompt` and `workflow_code_prompt` — **as a pure extraction that
  changes no observable behaviour of `workflow_code_prompt`**.

  Review flagged that `workflow_code_prompt` (`prompt.rs:454-474`) is already correct and that
  refactoring known-correct code is churn with regression risk. That objection is upheld as a
  *constraint*, not as a reason to abandon the shared site: the arm rule genuinely does live in two
  places, and that duplication is what produced this bug. The extraction proceeds, but
  `workflow_code_prompt`'s existing tests must pass **unmodified** — if a test has to change, the
  extraction was not behaviour-preserving and must be reworked.
  — **Fallback if that proves impossible:** confine the change to `fix_prompt` alone and leave the
  duplication in place with a comment naming this phase. Say so explicitly rather than quietly
  editing a test.

- **D-06:** **Fix all four affected adapters and correct the written record.** The ROADMAP goal and
  DECN-02 name only "Claude/OpenCode"; the actual blast radius is every adapter routed through
  `render_claude_style` — **claude, opencode, hermes, antigravity**. `codex` and `pi` use
  `render_workflow_style` and are already correct. Amend the ROADMAP Phase 47 goal and
  REQUIREMENTS DECN-02 to name hermes and antigravity, so a future reader does not believe those
  two were audited and excluded.
  — **Verified independently by both review lanes and by direct inspection.** All six `AgentKind`
  variants dispatch at `agents/mod.rs:173-197`; the split is exactly 4/2. The other
  `render_claude_style` references (`agents/mod.rs:231,241`, `antigravity.rs:111`) are all inside
  `#[test]` blocks and are not additional production render paths.

- **D-07:** Criterion 2's presence test runs **per-adapter across all six**. Drive `render_prompt`
  on every adapter with `StageIntent::Code { fix: Some(FixType::FullExecute) }` and assert the
  policy is present. **`codex` and `pi` are passing controls that must stay green** — they prove the
  test discriminates rather than asserting a tautology, and they catch a future adapter that
  switches render style.

- **D-08 [REVISED — no longer discretionary]:** The claude-style omission negative control is
  **mandatory**. Assert that `fix_prompt`'s `GapsOnly` and `AuditFix` arms still omit the policy.

  Both lanes independently reached the recommendation already recorded in the first draft, and
  codex named the failure mode precisely: the existing omission control at `prompt.rs:793-822`
  exercises **workflow-style** prompts only. Since this phase changes the **claude-style** path, an
  over-broad edit that added the policy to all claude-style fix arms would pass every existing
  control. The arm actually being modified would be unguarded in its failing direction — the repo's
  own dead-gate class. Promoted from "Claude's discretion" to a decided requirement.

### Phase 49 observation handoff (DECN-03 behavioural arm)

- **D-09:** The observation item lives in **both places** — the full evidence definition in
  `47-VALIDATION.md`, plus a one-line pointer in Phase 49's ROADMAP Success Criteria naming that
  file. Phase 49 is structurally reminded; the detail stays with the phase that found it.

- **D-10 [REVISED]:** The evidence standard is **audit event plus recorded reasoning plus a
  correlated post-resume artifact**.

  The first draft accepted the `checkpoint_auto_decided` event as proof the agent resolved the
  gate. **It is not.** Verified: the event is emitted **before the resumed agent is spawned**, and
  deliberately so — `pipeline_launch.rs:1102-1108` records it first precisely so that a spawn
  failure still leaves the decision attempt on record. It therefore proves **injection was
  attempted**, never that a decision was made. A failed spawn would satisfy the first draft's
  "followed" test.

  Corrected standard:
  - *Followed* = the `checkpoint_auto_decided` event fires **and** the resumed session's capture
    shows the concrete resolution **and** the final message carries the comparison reasoning the
    policy demands **and** the run progresses past that gate.
  - *Not followed* = the agent reported and halted instead, with the session having actually run.
  - *Void* = spawn failed; the event alone is present. Neither reading applies.

- **D-11 [REVISED]:** DECN-03 **cannot be exercised on more than one adapter in this codebase**, so
  it is settled on **claude** alone, and the generalization question is recorded as a known limit
  rather than a test.

  The first draft required exercising the gate on all four claude-style adapters, reasoning that
  instruction-conflict resolution is model-specific. **That reasoning is sound but the test is
  structurally impossible.** Verified on three independent constraints:
  - `pipeline_launch.rs:1569` gates the resume on `state.agent == AgentKind::Claude`.
  - `exec_resume_command` is defined only on `ClaudeDriver` (`claude.rs:122`); no other driver has
    a resume command at all.
  - `--mode auto` preflight admits only claude and antigravity (`preflight.rs:1027-1030`);
    opencode and hermes are refused before launch.

  So three of the four named adapters can never reach the co-resident prompt, and two of them
  cannot start an unattended run. The model-specificity concern is real and is preserved as a
  **stated limit on what DECN-03's closure means**: it closes for claude, and says nothing about
  the other three, because DevFlow cannot currently put them in that situation.
  — **[NEW] Follow-on candidate for the backlog:** adapter-independent checkpoint detection and
  per-driver resume. That is a feature, not a test, and it is not this phase's work.

- **D-12 [RESCINDED — RE-CONFIRMED BY THE OPERATOR 2026-09-10]:** The first draft amended Phase 49's ROADMAP
  criteria to require **four** live `--mode auto` runs, one per claude-style adapter. Per D-11 that
  is unachievable: two of the four are refused at preflight and three of the four cannot reach the
  resume injection. Leaving it in place would hand `gsd-planner` acceptance criteria for Phase 49
  that no implementation can satisfy.

  **Phase 49 stays as written — a single live run, on claude.** No ROADMAP amendment to Phase 49 is
  made by this phase, other than D-09's one-line pointer.
  — **Decision history.** Rescinding shrinks a scope the operator had agreed to, so it was put back
  to them rather than assumed. On 2026-09-10 the operator confirmed **rescind — Phase 49 stays one
  claude run**, and declined the variant that would also file the adapter-independent-resume
  follow-on as a numbered 999.x entry now. That follow-on therefore stays a **deferred note only**
  (see § Deferred Ideas); do not open a backlog entry for it in this phase.

### Policy text ownership

- **D-13 [REVISED — THE FIRST DRAFT'S ASSUMPTION WAS VERIFIED FALSE]:** Update
  `docs/guides/unattended-mode.md`, but **not the line the first draft named, and not for the
  reason it gave**.

  The first draft flagged that it rested on an unverified assumption — that the **preflight
  plan-scan** and the **resume path** are disjoint — and required research to confirm it before
  proceeding. **Research was done. The assumption is false.** Three independent mechanisms let the
  two paths meet:

  1. **The preflight refusal is overridable, and the override skips the check.** A refusal parks at
     a gate; `GateAction::Advance` calls `launch_stage_inner` directly, with the explicit comment
     that approval "skips the just-adjudicated check" (`preflight.rs:1368-1376`). The run's mode is
     unchanged, so it continues unattended afterwards and can reach the resume injection.
  2. **The two predicates are not equivalent.** Preflight uses
     `phase_has_human_only_checkpoint`, which is **line-anchored** — it requires the marker on a
     `<task ...>` opening line (`verify.rs:207-220`). The resume arm uses
     `phase_has_blocking_human_checkpoint`, a **whole-file substring** match (`verify.rs:131-137`).
     A plan carrying `gate="blocking-human"` anywhere *other than* a task-opening line passes
     preflight and still arms the resume. The source comments acknowledge the pair is deliberate
     with opposite failure directions (`verify.rs:191-196`) — which is precisely the wedge.
  3. **Time-of-check/time-of-use.** The preflight check applies only at `Define` and `Code`
     (`preflight.rs:971-973`), and plan files are agent-writable during Code (`verify.rs:118-119`).
     A gate declared after Code's preflight ran is never re-scanned.

  **What this changes in the doc.** The line the first draft targeted — `:29-31` "Checkpoints
  marked `blocking-human` … are never auto-approved by any mode" — is very nearly accurate for the
  ordinary path and should be **qualified, not inverted**. The line that is **actually false** is
  `:85-88`, "A refusal is final … no `--force-unattended` flag": `GateAction::Advance` is exactly
  that override, and it is reachable from the parked gate a refusal creates. Rewrite `:85-88` to
  describe the operator-approval override and its consequence, and qualify `:29-31` to name the
  resumed-to-resolve exception and the human approval that precedes it.
  — **[NEW] Mechanisms 2 and 3 are latent defects, not documentation problems.** The predicate
  mismatch and the TOCTOU window let an unattended run reach a self-decision on a gate preflight
  was meant to catch. **File both to the 999.x backlog.** Fixing them is out of scope here; leaving
  them unrecorded is not.

- **D-14:** Add a **snapshot-testing framework** (`insta` or equivalent) so any wording change in a
  policy-carrying prompt surfaces as a review diff. No such framework exists in the workspace today.
  Rejected: assertions on the shared constant (cheap but only catches phrases someone thought to
  assert) and a doc-to-code prose guard (brittle — the class of guard this repo has shipped broken).
  — **Reversibility:** costly — a new dev-dependency carries `cargo deny check` and `cargo machete`
  obligations, CI wiring, and a review habit the repo does not have yet.

- **D-15:** The snapshot suite covers **policy-carrying prompts only** — the four claude-style
  `FullExecute` fix prompts, the two workflow-style ones, and the two-turn resume delivery pair
  (D-03). Scoped to what this phase changes, so the baseline is *reviewed* rather than blessing
  whatever text exists today (which is how a wrong string becomes the golden value).

- **D-16 [REVISED — THE COMMAND AS WRITTEN DOES NOT EXIST]:** Snapshot mismatches are **required
  and fail the build**, wired as:

  ```
  INSTA_UPDATE=no cargo test --workspace --no-fail-fast
  ```

  placed in **`run_test`** in `scripts/check.sh`, not in the `all` target.

  The first draft specified "`INSTA_UPDATE=no` / `--check`". Verified: **`cargo test --check` is not
  a valid invocation** — it exits with `error: unexpected argument '--check' found`. The `--check`
  form belongs to the `cargo-insta` CLI, which is **not installed** on this host and is not
  provisioned by the devcontainer. Neither workspace manifest lists `insta` today.

  Three requirements follow, all of which planning must carry:
  - **Placement:** CI's required Test job runs `scripts/check.sh test`, so the enforcement must live
    in `run_test`. Putting it only in `all` would leave the required job unguarded — a green CI over
    an unenforced check.
  - **Provisioning:** if `cargo-insta` is ever preferred over the env-var form, it must be installed
    in both the devcontainer image and CI. The env-var form above needs no binary and is preferred
    for that reason.
  - **Negative control (mandatory):** demonstrate an **intentional snapshot mismatch failing the
    build** before accepting the wiring. `scripts/check.sh` sets `set -euo pipefail` (`:9`) and
    `run_test` invokes cargo directly with no pipeline (`:48`), so a real failure does propagate —
    but that is an argument about plumbing, not evidence the guard fires. Produce the red run.

  Rejected: landing it advisory first (Phase 46 D-05 style). A silent-drift guard that is advisory
  is not a guard, and this phase exists because text drifted unnoticed.
  — **[NEW] `.planning/user/DEV-SETUP-CHECKLIST.md` must be updated in the same commit** as the
  dependency and CI change (repo rule).

### Claude's Discretion

- Exact wording of the carve-out sentence and of the shared constant from D-04.
- Whether the snapshot framework lands as its own plan or inside the fix plan.
- Whether the two latent defects named in D-13 are filed as one backlog entry or two.

</decisions>

<review_findings>
## Adversarial Review — what changed and what held

Two external lanes reviewed the first draft (`47-REVIEWS.md` carries the full reports).
Depth: **codex 34 `file:line` citations, agy 30** — neither is a low-citation clean bill.

**Changed after review:** D-02 (narrowed, reverses an accepted tradeoff), D-03 (test was
unbuildable), D-04 (softened), D-05 (constrained), D-08 (promoted to mandatory), D-10 (event
proves injection, not decision), D-11 (test structurally impossible), D-12 (rescinded), D-13
(assumption false; different doc line is the wrong one), D-16 (command does not exist).

**Held under review:** D-01, D-06, D-07, D-09, D-14, D-15. D-06's four/two adapter split was
verified independently three times.

**Where the lanes disagreed, and how it was resolved.** agy concluded the resume path is simply
unreachable in `--mode auto` and that `unattended-mode.md:29-31` is therefore accurate and should
be left alone. codex found the mechanism agy's own report mentions in passing but does not weigh —
`GateAction::Advance` skipping preflight (`preflight.rs:1368-1376`) — which makes the path
reachable after one human approval. **Checked directly against source: codex is right**, and agy's
stronger claim ("never reachable in auto") does not survive that call site. The resolution taken
above keeps agy's insight that the *ordinary* path is closed, while correcting the doc line that is
actually false.

**Not taken:** agy's recommendation to leave `workflow_code_prompt` entirely untouched (D-05 keeps
the shared extraction, under a behaviour-preserving constraint) and agy's recommendation to drop
the doc change altogether (D-13 still changes the doc, at a different line).

</review_findings>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements and milestone framing
- `.planning/ROADMAP.md` § `### Phase 47: Unattended Decision Policy Consistency` — the four
  success criteria, including criterion 1's scope constraint and criterion 4's explicit
  not-closed-here clause. Amended by D-06 only.
- `.planning/ROADMAP.md` § `### Phase 49: Live Unattended Run — The Milestone's Instrument` —
  **not** amended (D-12 rescinded); carries the D-09 pointer only.
- `.planning/REQUIREMENTS.md` — DECN-02 (lines 18-26) and DECN-03 (lines 28-34), plus the
  DECN-03 note at lines 185-187. Amended by D-06.

### Prompt rendering (DECN-02, DECN-03)
- `crates/devflow-core/src/prompt.rs` — the whole surface:
  - `:62` `CODE_STAGE_POLICY` definition; `:86-89` the unconditional prohibition D-01 makes
    conditional — note it names **two** classes, only one of which D-02 now carves out
  - `:379` claude-style Code arm (has policy); `:399` the single production `fix_prompt` call site
  - `:454-474` `workflow_code_prompt` — **already correct** on `Some(FullExecute) | None` at `:465`
  - `:537-548` `checkpoint_auto_decide_prompt` — the generic "human-blocking checkpoint" prose D-02
    tightens
  - `:567-581` `fix_prompt` — **the gap**; omits the policy on all three arms
  - `:793-822` the existing omission negative control — **workflow-style only**, which is why D-08
    is mandatory; criterion 1 requires it pass **unmodified**
  - `:578` `COMPLETION_PROTOCOL` — criterion 2's proof the search range is non-empty
- `crates/devflow-cli/src/pipeline_launch.rs`:
  - `:1092-1094` `relaunch_checkpoint_session` — documents that it does **not** call preflight
  - `:1100-1119` the resume injection and the `ClaudeDriver::exec_resume_command` call — **no
    combined prompt is built here** (D-03)
  - `:1102-1108` the `checkpoint_auto_decided` emission, **before** the spawn (D-10)
  - `:1569-1571` the `AgentKind::Claude` gate on the whole resume route (D-11)
- `crates/devflow-core/src/agents/claude.rs:25-32` — the initial prompt as a stdin user turn;
  `:122` `exec_resume_command`, the only one in the workspace

### Disjointness — the D-13 evidence
- `crates/devflow-cli/src/preflight.rs:971-973` — the check applies at `Define` and `Code` only
- `crates/devflow-cli/src/preflight.rs:1027-1030` — C2 admits only claude and antigravity in auto
- `crates/devflow-cli/src/preflight.rs:1067-1083` — C3, the declared-checkpoint scan
- `crates/devflow-cli/src/preflight.rs:1368-1376` — **`GateAction::Advance` skips preflight**
- `crates/devflow-core/src/verify.rs:131-137` — loose whole-file `blocking-human` match (resume)
- `crates/devflow-core/src/verify.rs:191-196` — the comment acknowledging the deliberate pair
- `crates/devflow-core/src/verify.rs:207-220` — line-anchored human-only match (preflight)

### Adapter surface (D-06, D-07)
- `crates/devflow-core/src/agents/mod.rs:173-197` — the dispatch over all six `AgentKind` variants
- `claude.rs:22`, `opencode.rs:43`, `hermes.rs:26`, `antigravity.rs:41` — `render_claude_style` ❌
- `codex.rs:23`, `pi.rs:37` — `render_workflow_style` ✅ controls, must stay green
- Not production paths: `agents/mod.rs:231,241` and `antigravity.rs:111` are inside `#[test]`

### Policy documentation (D-13)
- `docs/guides/unattended-mode.md` — `:29-31` "never auto-approved by any mode" (**qualify**);
  `:71-76` the preflight plan-scan and refusal; `:85-88` "a refusal is final" (**false — rewrite**)

### CI and tooling (D-14 – D-16)
- `scripts/check.sh:9` `set -euo pipefail`; `:44-49` `run_test`, where the guard belongs
- `.github/workflows/ci.yml:45-63` — the required Test job runs `scripts/check.sh test`
- `crates/devflow-core/Cargo.toml`, `crates/devflow-cli/Cargo.toml` — neither lists `insta`
- `.devcontainer/devcontainer.json:34-35` — provisions Rust components only; no `cargo-insta`
- `deny.toml` — the licence allowlist `insta` and its transitives must satisfy

### Repo conventions this phase leans on
- `CLAUDE.md` § "Verification habits this repo has already paid for" — the dead-gate entry
  (`rg -c` never discriminating) is the direct precedent for D-07's passing controls, D-08, and
  D-16's mandatory red run.
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
  helper should be extracted *from* this logic, not invented — and without changing its behaviour.
- Phase 28's `checkpoint_auto_decided` audit event — emits one of the artifacts D-10 names, but
  **only proves injection was attempted** (`pipeline_launch.rs:1102-1108`). The resumed session's
  capture is the artifact that carries the decision.
- The existing omission test at `prompt.rs:793-822` — the shape D-08's counterpart mirrors, for the
  claude-style path it does not currently cover.

### Established Patterns
- **One definition site for a rule that appears in two places** (Phase 46 D-03, CPU pin). D-04 and
  D-05 both apply it; this contradiction is what its absence produces. D-04 applies it to the
  *rule*, not to a literal string shared across incompatible grammatical moods.
- **Negative controls are mandatory** — a guard must be shown to fail in the direction it claims to
  catch. Drives D-03 (both tests red pre-fix), D-07 (codex/pi green), D-08, and D-16's red run.
- **Adapters delegate to two render styles**; no adapter defines prompt text itself.
  `CODE_STAGE_POLICY` appears only in `prompt.rs`.
- **A proxy measurement is not the measurement.** D-03's first form (concatenating two strings)
  and D-10's first form (an event emitted before the work) were both proxies that would have gone
  green over an unverified claim.

### Integration Points
- `prompt.rs:399` — the single production call site of `fix_prompt`; the only place the DECN-02
  fix changes behaviour at runtime.
- `pipeline_launch.rs:1100-1119` — where the resume instruction is built and launched. Co-residence
  is a property of the **session transcript**, not of any string built here.
- `scripts/check.sh` `run_test` — D-16 folds the snapshot check in here, inside the existing
  required CI context rather than adding a new one.

</code_context>

<specifics>
## Specific Ideas

- The carve-out must read as a **scoped hierarchy**, not a second flat rule — "unless DevFlow
  resumed you specifically to resolve this gate" — so that an agent reading both fragments sees one
  rule with a condition, not two rules in tension.
- **[REVISED] Position effects: recency, not primacy.** The first draft carried a research finding
  that primacy dominates instruction-following (~73%) and that mid-prompt rules lose 30-50%
  compliance, and treated the carve-out's position in a combined prompt as a design variable.
  Review corrected the frame: because the resume delivers `checkpoint_auto_decide_prompt` as the
  **latest turn** of a multi-turn session while `CODE_STAGE_POLICY` sits in turn 1
  (`claude.rs:25-32`, `pipeline_launch.rs:1100-1119`), the governing effect is **recency**, and
  intra-document sentence primacy does not apply. The percentages are also uncited here and must
  **not** become an acceptance threshold. Phase 49's observation should record which turn carried
  which rule alongside the outcome.
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
- **[NEW] Adapter-independent checkpoint detection and per-driver resume.** D-11's model-specificity
  concern is real but untestable while the resume route is claude-only
  (`pipeline_launch.rs:1569`, `claude.rs:122`). A feature, not a test. **Operator decided 2026-09-10
  to leave this as a note and NOT file it as a numbered backlog entry in this phase.**
- **[NEW] The two latent defects from D-13** — the preflight/resume predicate mismatch
  (`verify.rs:131-137` vs `:207-220`) and the TOCTOU window on agent-writable plans
  (`preflight.rs:971-973`, `verify.rs:118-119`). Both let an unattended run reach a self-decision on
  a gate preflight was meant to catch. **To be filed to 999.x by this phase**, fixed by a later one.

</deferred>

---

*Phase: 47-unattended-decision-policy-consistency*
*Context gathered: 2026-09-09 · Revised after external review: 2026-09-09*
