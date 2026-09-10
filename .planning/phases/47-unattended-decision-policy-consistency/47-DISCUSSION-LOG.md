# Phase 47: Unattended Decision Policy Consistency - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-09
**Phase:** 47-unattended-decision-policy-consistency
**Areas discussed:** blocking-human rule direction, DECN-02 fix surface, Phase 49 observation handoff, Policy text ownership

---

## blocking-human rule direction

### Q1 — Which instruction should win for a `blocking-human` gate?

| Option | Description | Selected |
|--------|-------------|----------|
| Carve-out: resume wins | `CODE_STAGE_POLICY` gains an explicit "unless DevFlow resumed you specifically to resolve this gate" exception; authority narrow, audited, traceable to `pipeline_launch.rs:1100` | ✓ |
| Policy wins: never self-resolve | Rewrite `checkpoint_auto_decide_prompt` to "report and stop"; matches the researched block-tier norm, but makes Phase 28's auto-decide inert | |
| Never co-resident | Leave both texts, make co-residence structurally impossible; smallest change but the contradiction stays in source | |

**User's choice:** Carve-out: resume wins → **D-01**
**Notes:** Research presented beforehand — standard three-tier design (auto-approve / notify / block); block tier normally enters a durable pending state, absence of a human is not itself authorization; timeout auto-resolve exists but as an explicit bounded policy. This cut *against* the chosen option, and was presented as such. Operator chose the carve-out on DevFlow's unattended-overnight premise.

### Q2 — Does the carve-out cover both human-only classes?

| Option | Description | Selected |
|--------|-------------|----------|
| blocking-human only | Package-verification stays absolutely human-only; narrower carve-out, one class keeps zero exceptions | |
| Both classes | Single rule; matches `checkpoint_auto_decide_prompt`'s generic wording so no arm-splitting needed | ✓ |
| You decide | | |

**User's choice:** Both classes → **D-02**
**Notes:** The risk was stated in the option text before selection — an unattended run can self-approve a package verification. Recorded in CONTEXT.md as `Reversibility: one-way`, so `gsd-planner` raises a `checkpoint:decision` before the implementing task.

### Q3 — What should the contradiction test assert?

| Option | Description | Selected |
|--------|-------------|----------|
| Co-resident render, one rule | Build the real combined prompt, assert absolute sentence absent / carve-out present | |
| Constant-level invariant | Assert on `CODE_STAGE_POLICY`'s own text; cheap, but proves only that the constant changed | |
| Both, layered | Constant-level unit test plus co-resident render test; two failures pre-fix, render test carries the claim | ✓ |

**User's choice:** Both, layered → **D-03**

### Q4 — Single definition site for the gate rule?

| Option | Description | Selected |
|--------|-------------|----------|
| Extract to one constant | Hoist into a const both prompts interpolate; the Phase 46 D-03 pattern | ✓ |
| Keep separate, add a guard test | Preserves per-context wording; weaker, guards only catch what they were written to look for | |
| Neither — out of scope | Fix as-is, file drift prevention as backlog | |

**User's choice:** Extract to one constant → **D-04**

---

## DECN-02 fix surface

**Scout finding presented before questions:** `workflow_code_prompt` (`prompt.rs:465`) already carries the policy on its `Some(FullExecute) | None` arm. The gap is only `fix_prompt` (`:567`), reached from one production call site (`:399`).

### Q1 — How should `fix_prompt`'s FullExecute arm be fixed?

| Option | Description | Selected |
|--------|-------------|----------|
| Mirror the arm shape | Split `fix_prompt` by arm exactly as `workflow_code_prompt` does; parallel structure makes drift visible | |
| Shared helper both call | Extract the arm rule into one function both consult; single definition site | ✓ |
| You decide | | |

**User's choice:** Shared helper both call → **D-05**

### Q2 — The phase names two adapters; four are affected. How to handle?

| Option | Description | Selected |
|--------|-------------|----------|
| Fix all 4, correct the record | Amend ROADMAP goal and DECN-02 to name hermes and antigravity | ✓ |
| Fix all 4, note in CONTEXT only | Same code, less document churn mid-milestone | |
| You decide | | |

**User's choice:** Fix all 4, correct the record → **D-06**
**Notes:** Adapter map established during scout — `render_claude_style`: claude, opencode, hermes, antigravity (affected); `render_workflow_style`: codex, pi (already correct).

### Q3 — What surface should criterion 2's presence test cover?

| Option | Description | Selected |
|--------|-------------|----------|
| Per-adapter, all six | Drive `render_prompt` on every adapter; codex and pi as passing controls | ✓ |
| `fix_prompt` directly + one adapter | Cheaper; misses a future adapter re-pointing its render style | |
| Shared-helper invariant | Tests the single definition site; proves consultation, not rendered output | |

**User's choice:** Per-adapter, all six → **D-07**

### Q4 — Does the claude-style path need its own omission negative control?

| Option | Description | Selected |
|--------|-------------|----------|
| Yes — add the counterpart | Mirror `prompt.rs:810` for the newly-changed renderer | |
| No — the existing test suffices | D-05's shared helper already pins the arm rule in one place | |
| You decide | | ✓ |

**User's choice:** You decide → **D-08** (Claude's discretion)
**Notes:** Recommendation recorded in CONTEXT.md — add it. The arm this phase changes should be guarded in its failing direction; a guard covering only the untouched renderer is the repo's documented dead-gate class.

---

## Phase 49 observation handoff

**Research presented:** primacy dominates in instruction-following (~73% where positional bias is detectable); mid-prompt rules lose 30-50% compliance; up to ~62% variance from repositioning alone; conflict resolution is model-specific.

### Q1 — Where should the observation item live?

| Option | Description | Selected |
|--------|-------------|----------|
| ROADMAP Phase 49 criteria | Cannot be missed; edits a future phase's criteria from inside 47 | |
| `47-VALIDATION.md` item | Keeps 47's record honest; nothing forces 49 to read it | |
| Both — item plus pointer | Detail in `47-VALIDATION.md`, one-line pointer in 49's criteria | ✓ |

**User's choice:** Both — item plus pointer → **D-09**

### Q2 — What observable counts as evidence?

| Option | Description | Selected |
|--------|-------------|----------|
| Audit event + recorded reasoning | `checkpoint_auto_decided` fires AND final message carries comparison reasoning; halt-and-report is the other direction | ✓ |
| Run progression only | Continued vs halted; simplest, but progression can happen for unrelated reasons | |
| You decide | | |

**User's choice:** Audit event + recorded reasoning → **D-10**

### Q3 — What adapter coverage should the item demand?

| Option | Description | Selected |
|--------|-------------|----------|
| One adapter, rest explicitly unexercised | Matches existing SURV-01 discipline; realistic for one overnight run | |
| All four claude-style adapters | Only version that answers the model-specific question; four live runs | ✓ |
| One adapter, treat as representative | Cheapest; contradicts the model-specificity finding | |

**User's choice:** All four claude-style adapters → **D-11**

### Q4 — All-four exceeds Phase 49's single-run scope. How to record that?

| Option | Description | Selected |
|--------|-------------|----------|
| DECN-03 stays open past 49 | Honest about the gap; milestone shows DECN-03 unclosed longer | |
| Widen Phase 49 now | Amend 49's criteria to four runs; keeps DECN-03 closeable in-milestone | ✓ |
| You decide | | |

**User's choice:** Widen Phase 49 now → **D-12**
**Notes:** Flagged before selection that this materially expands a future phase from inside Phase 47.

---

## Policy text ownership

**Scout finding presented:** `docs/guides/unattended-mode.md` currently states the opposite of D-01/D-02 — "never auto-approved by any mode" (`:29`), preflight refusal (`:71-76`), "a refusal is final" (`:85-88`). Whether the preflight scan and the resume path are disjoint was explicitly flagged as **not established**. Also established: no snapshot/golden test framework exists in the workspace.

### Q1 — How should the docs contradiction be handled?

| Option | Description | Selected |
|--------|-------------|----------|
| Research question, then reconcile | Make disjointness a MUST-ANSWER for the researcher, reconcile after | |
| Update docs to match the carve-out | Treat paths as disjoint and rewrite the doc line; assumes what was not verified | ✓ |
| Docs out of scope | Ship code contradicting documented behaviour; the 999.124 drift class | |

**User's choice:** Update docs to match the carve-out → **D-13**
**Notes:** The unverified disjointness assumption was stated before selection and is recorded prominently in CONTEXT.md with an explicit revisit trigger.

### Q2 — Drift protection for the policy text?

| Option | Description | Selected |
|--------|-------------|----------|
| Assertions on the shared const | No new framework; reuses D-03's tests | |
| Add a snapshot framework | `insta` or equivalent; strongest against silent drift; new dev-dependency | ✓ |
| Doc-to-code guard | Directly targets the failure just found; brittle prose matching | |

**User's choice:** Add a snapshot framework → **D-14**

### Q3 — What should the snapshot suite cover?

| Option | Description | Selected |
|--------|-------------|----------|
| Policy-carrying prompts only | Scoped to what this phase changes; baseline is reviewed | ✓ |
| Every stage prompt | Max coverage; risks blessing unreviewed text as golden | |
| You decide | | |

**User's choice:** Policy-carrying prompts only → **D-15**

### Q4 — How should CI treat snapshot mismatches?

| Option | Description | Selected |
|--------|-------------|----------|
| Required, fails on mismatch | `INSTA_UPDATE=no` / `--check` in the existing required `check.sh` job | ✓ |
| Advisory first, promote later | Phase 46 D-05 style; lower risk of wedging merges | |
| You decide | | |

**User's choice:** Required, fails on mismatch → **D-16**

---

## Claude's Discretion

- **D-08** — whether to add a claude-style `GapsOnly`/`AuditFix` omission negative control. Recommendation recorded: add it.
- Exact wording of the carve-out sentence and of D-04's shared constant.
- Whether the snapshot framework lands as its own plan or inside the fix plan.

## Deferred Ideas

- Snapshot coverage for every stage prompt across all six adapters — scoped out by D-15; revisit once the reviewed baseline is stable.
- A doc-to-code guard asserting `docs/guides/unattended-mode.md` and the shared constant agree — rejected in D-14 as brittle, but the underlying docs-drift problem is real and now has a second instance. Candidate for the 999.124 backlog cluster.

## Scope observations recorded

- The phase is larger than the ROADMAP's "prompt rendering only" framing: two ROADMAP amendments (D-06, D-12), a docs rewrite (D-13), and a new dev-dependency with CI wiring (D-14 – D-16).
- No todos matched phase 47 (`todo.match-phase 47` → `todo_count: 0`), so nothing was folded or reviewed.
