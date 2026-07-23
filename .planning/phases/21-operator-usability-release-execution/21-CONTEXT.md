# Phase 21: Operator Usability & Release Execution - Context

**Gathered:** 2026-07-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 21 makes DevFlow's operator surface **legible** and takes the release cut
from a hand-run checklist to an **executed** command — while keeping the base
of every launched phase **explicit**. It is the operator-facing, single-writer
half of the post-v1.7.0 work. Everything that requires two phases running or
shipping *concurrently* to be correct is deliberately Phase 22's, not this one.

The ROADMAP goal was left `[To be planned]` on purpose (scaffold commit
`56a1835`: "goals TBD pending discuss/plan"). This CONTEXT therefore also
**proposes the scope boundary** for the phase, drawn from the phase name and the
backlog candidates that match it. Confirm the unit set at plan/review time; it
has **not** been through a `/gsd-review-backlog` promotion, and REQUIREMENTS.md
carries no REQ-IDs for it.

**Proposed in scope (three units, single-writer / operator-facing):**

- **21a — Operator discoverability (999.3).** `devflow gate show` for truncated
  gate reasons, surface rate-limit reset times out of raw agent JSON, in-stage
  progress in `status`, and make recovery verbs (`advance`, `resume`)
  discoverable from a stuck state. UX, not correctness.
- **21b — Explicit `--base` branch override (999.28).** `devflow start --base
  <branch>` defaulting to `develop`, so a phase can be cut onto an unmerged
  predecessor to honor a `depends_on` chain. The design is substantially
  settled in `999.28`'s CONTEXT (see decisions below).
- **21c — Release-cut executor (999.25).** `devflow release --execute` that
  drives version-bump PR → merge to `main` → signed tag → sync `develop` →
  publish `devflow-core` then `devflow`. This is the large, **irreversible**
  unit; it needs the failure/rollback design pass captured below and an operator
  gate before the publish step.

**Optional / stretch (defer if the phase is already heavy):**

- **999.5 — `ChangelogAppend` real content.** Replace the "Released phase via
  DevFlow" placeholder. Cosmetic, lowest priority, and blocked on choosing a
  content source (SUMMARY.md extraction vs plan diffs) — carry only if 21c lands
  with room to spare.

**Explicitly OUT of scope — belongs to Phase 22 (Concurrency & Governance
Correctness):** 999.4 (version-tag contention on concurrent ship), 999.26
(`parallel` git object-store race), 999.2 (a-phase-tracks-two-processes model),
and the *concurrency* half of base selection (`parallel` shared-base
derivation). **OUT — Phase 23 (Test/CI):** 999.15/17/18/19/20/22.
</domain>

<decisions>
## Implementation Decisions

Ratings follow `gsd-core/references/planner-reversibility.md`. Because this ran
autonomously (no interactive operator selection), treat the 21c decisions below
as the **recommended design to validate at plan time**, not operator-locked
choices — especially anything touching irreversible release operations.

### 21b — Explicit `--base` branch override

- **D-01:** Add an explicit `--base <branch>` flag to `devflow start` (and the
  worktree launch path), **defaulting to `develop`**. Base is always explicit or
  the stated default — **never** inferred from the operator's current branch.
  Rationale carried from `999.28`: an implicit current-branch default silently
  roots a phase on a dirty throwaway branch. — **Reversibility:** costly —
  once operators script `--base`, the flag name and default become a CLI
  contract; changing the default would silently re-root phases.
- **D-02:** The develop-rooted base is load-bearing, not incidental —
  `feature_start` hardcodes it (`git.rs:53`, `checkout develop` then `checkout
  -b`), and ship → Merge-to-develop → VersionBump → ChangelogAppend, the
  `sync-main-to-develop.sh` script, and `release --check` all assume
  feature→develop→main. So `--base` must **thread through** without regressing
  that chain: default path stays byte-for-byte develop-rooted.
- **D-03:** Ship/merge target when a phase is based on `feature/phase-NN` rather
  than `develop` is an **open design question** the planner must resolve (likely
  still merge to `develop` after the predecessor lands). Flag it; do not guess
  it into code. — **Reversibility:** one-way — a wrong merge target on a stacked
  phase lands commits on the wrong branch and corrupts the release lineage.
- **D-04:** Validation: **reject** a `--base` that does not exist; **warn (not
  block)** if the base is not an ancestor of `develop`.
- **D-05 (scope guard):** 21b threads `--base` through **`start` only** for this
  phase. `parallel` shared-base derivation and `resume`/`recover` base
  reconstruction from state are Phase 22 concerns — do not expand into them here.

### 21c — Release-cut executor (`devflow release --execute`)

- **D-06:** `--execute` runs the **same four `release --check` preflight checks
  first** (workspace self-pin match, develop/main divergence, crates.io publish
  order, tag-signing viability via 20d's `gpg.format`-aware check) and
  **hard-stops** on any failure before touching anything. Reuse the existing
  `release_check` path (`commands.rs:1304`), do not fork a second checker.
- **D-07:** There is an **explicit operator gate immediately before the
  irreversible publish step**. Merge-to-main, the signed tag, and the crates.io
  publish can never be un-published or reused. — **Reversibility:** one-way — a
  published crate version and a pushed signed tag are permanent; this gate is the
  last human checkpoint.
- **D-08:** Reuse the existing after-ship hook batch machinery (`VersionBump`,
  `ChangelogAppend`) rather than a second version-writing path — same
  "one effect, don't reimplement" principle 20e applied to `finish_workflow`.
  Must replicate the operator's manual-merge discipline
  (`[[feedback-manual-merge-must-replicate-ship]]`): version+pin, changelog,
  signed tag, publish core-then-cli, sync.
- **D-09:** Publish ordering **encodes** the crates.io constraint 20d only
  asserts: `devflow-core` must be live at a satisfying version on the registry
  before `devflow`'s publish/verify resolves its path dependency. Not a warning —
  a sequenced, verified step.
- **D-10 (design pass required):** The planner/researcher must specify
  **failure/rollback semantics** for the partial-failure cases 999.25 names
  explicitly: tag lands but publish fails; `core` publishes but `cli` does not.
  These are non-obvious and irreversible-adjacent — do not leave them implicit.

### 21a — Operator discoverability

- **D-11:** Purely additive UX surfacing — `gate show`, rate-limit reset time in
  human output, in-stage progress in `status`, recovery-verb hints from a stuck
  state. No behavioral/correctness change to the pipeline. Sequence it first;
  it is the lowest-risk unit and unblocks nothing downstream. — **Reversibility:**
  reversible.

### Claude's Discretion
- Exact CLI flag surface for `gate show` (positional vs `--phase`), progress
  representation in `status`, and whether 21a ships as one plan or splits by
  sub-gap — planner's call.
- Whether 999.5 is folded in at all — include only if 21c leaves capacity.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase-21 unit sources (backlog dossiers — read before scoping)
- `.planning/phases/999.3-cli-operator-discoverability/CONTEXT.md` — 21a: the
  four discoverability gaps and why they are UX, not correctness.
- `.planning/phases/999.28-explicit-base-branch-override/CONTEXT.md` — 21b: the
  full `--base` design, the load-bearing develop-hardcode analysis, and the open
  ship/merge-target question. **Most important ref for 21b.**
- `.planning/phases/999.25-release-cut-executor/CONTEXT.md` — 21c: executor
  goal, why it was deferred out of Phase 20, possible shapes, and the
  partial-failure cases that need a rollback design.
- `.planning/phases/999.5-changelog-placeholder-content/CONTEXT.md` — optional
  21d, and the open "where does real per-phase content come from" question.

### Release flow & prior-art the executor must reuse (not rediscover)
- `CONTRIBUTING.md` §"Cutting a Release" (line 170) — the manual checklist 21c
  automates; the executor must reproduce it exactly.
- `crates/devflow-cli/src/commands.rs:1304` (`release_check`, 20d preflight) —
  reuse as `--execute`'s hard-stop gate.
- `crates/devflow-core/src/version.rs:382` — 20a workspace self-pin invariant
  the preflight asserts.
- `crates/devflow-core/src/git.rs:53` (`feature_start`) and `:113`/`:121`
  (`release_start`/`release_finish`) — the develop-rooted branch model `--base`
  extends and the executor drives.
- `scripts/sync-main-to-develop.sh` — the post-merge sync step the executor must
  not skip.
- Phase 20 artifacts: `.planning/phases/20-release-correctness-operator-control/`
  (20d `release --check`, 20e manual ship override) — the direct predecessors.

### Scope-fence refs (what is NOT this phase)
- `.planning/phases/999.4-*`, `999.26-*`, `999.2-*` — Phase 22 (concurrency).
- ROADMAP.md §"Phase 22/23" — the boundary these units sit behind.
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `release_check(project_root)` (`commands.rs:1304`) — the 20d preflight; 21c's
  `--execute` runs it verbatim as its hard-stop gate.
- After-ship hook batch (`VersionBump`, `ChangelogAppend`) — 21c drives these
  rather than writing a second version path (D-08).
- `Git::release_start`/`release_finish` (`git.rs:113`/`:121`) — existing
  git-flow release tagging the executor sequences.
- `gpg.format`-aware tag-signing viability check (from 20d) — 21c reuses as a
  precondition, per 999.25.

### Established Patterns
- `feature_start` (`git.rs:53`) hardcodes `checkout develop` → `checkout -b
  feature/phase-NN`. 21b generalizes the base while keeping this exact behavior
  as the default branch (D-02).
- "One effect, don't reimplement" (20e's `finish_workflow` reuse) — governs D-08.
- Command surface lives in `crates/devflow-cli/src/main.rs` + `commands.rs`;
  `release_check` is already wired at `main.rs:498`. 21a/21c extend the same
  dispatch.

### Integration Points
- `devflow start` argument parsing (`main.rs`) ← 21b `--base` flag.
- `devflow release` subcommand (`main.rs:498` / `commands.rs`) ← 21c `--execute`.
- `devflow gate` / `devflow status` output paths ← 21a surfacing.
</code_context>

<specifics>
## Specific Ideas

- `devflow start --phase 22 --base feature/phase-21` is the concrete motivating
  example for 21b (intentional stacking to honor `depends_on`).
- The executor's guiding constraint is the operator's own rule
  (`[[feedback-manual-merge-must-replicate-ship]]`): a button-merge that skips
  VersionBump/ChangelogAppend is the failure class 21c exists to retire.
</specifics>

<deferred>
## Deferred Ideas

- **Concurrency governance (Phase 22):** version-tag contention on concurrent
  ship (999.4), `parallel` git object-store race (999.26), the two-processes-
  per-phase tracking model (999.2), and `--base` threading through `parallel`
  shared-base derivation and `resume`/`recover` state reconstruction.
- **Test/CI hardening (Phase 23):** 999.15/17/18/19/20/22.
- **999.5 `ChangelogAppend` real content** — only if 21c leaves capacity;
  otherwise carry forward. Blocked on choosing a per-phase content source.
</deferred>

---

*Phase: 21-operator-usability-release-execution*
*Context gathered: 2026-07-23*
