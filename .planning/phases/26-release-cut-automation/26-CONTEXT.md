# Phase 26: Release-Cut Automation - Context

**Gathered:** 2026-07-29
**Status:** Ready for planning

<domain>
## Phase Boundary

Make `devflow release` *execute* the release-cut sequence — version bump →
direct push to `develop` → develop→main release PR (human-merged) → signed
tag → sync back to `develop` (direct push) → crates.io publish, in order —
not just the read-only `--check` preflight Phase 20's 20d delivered. Adds a
real `devflow sync` subcommand (999.52) as both a standalone command and the
executor's own sync step, and fixes the changelog's placeholder content
(999.5) by generating it from the same conventional-commit classification the
version-bump step already computes.

**Scope narrowed during discussion** (see `<decisions>` below): 999.54 and
999.50 (the `release --check` signing-viability predictor) and 999.4
(concurrent-ship tag-race protection) were promoted into this phase originally
but were dropped after discussion — see D-08/D-09 and D-10. Both were also
removed from the backlog entirely, not merely deferred.

</domain>

<decisions>
## Implementation Decisions

### Automation ceiling — what "execute" actually means

- **D-01:** `develop`-bound merges (version-bump commit, and the sync-back
  merge) are **direct pushes to `origin/develop`, not PRs.** No human click,
  no `gh pr merge`. The operator (not this phase) will set up a GitHub
  ruleset bypass entry for whatever credential DevFlow pushes with — Phase 26
  assumes that bypass exists and just implements the push; it does not touch
  GitHub settings and does not need to document the ruleset change (the
  operator handles this out-of-band, on their own timeline).
  — **Reversibility:** reversible — a later change back to PR-based develop
  merges would just remove the direct-push call.
- **D-02:** The `develop → main` release PR **stays PR-gated and
  human-merged** for now. `main`'s squash-only merge setting is unchanged by
  this phase. (Operator's stated future direction — not in this phase's
  scope — is eventually having Claude review that PR instead of a human; not
  built here.)
- **D-03:** `devflow release --yes-release` covers the **entire**
  bump→tag→sync→publish sequence as one typed authorization, mirroring
  `--yes-ship`'s existing all-or-nothing shape and its non-negotiable rule:
  a dangerous operation must be typed per-invocation, never a standing
  default/config flag. This is a **new, separate flag** from `--yes-ship`
  — the operator explicitly wants "shipping" (ends at merge to `develop`)
  and "releasing" (ends at merge to `main` + full version release) kept as
  distinct authorized operations.
  — **Reversibility:** reversible.
- **D-04:** `cargo publish` for both crates **is** part of the automated
  sequence (devflow-core, then devflow, per the order `publish_order`
  already computes) — today this is 100% manual and has never been driven
  by any DevFlow code. The executor queries crates.io before attempting each
  publish, to distinguish "already published at this version, skip" from a
  genuine failure (this also gives D-06's resume/idempotency its anchor for
  the publish step specifically, since cargo itself has no built-in
  "already done" signal beyond its own duplicate-version rejection).
  — **Reversibility:** one-way — a crates.io publish can never be
  un-published or reused at that version.

### Failure and resume semantics

- **D-05:** The executor follows the **same fail-fast, no-automatic-rollback
  philosophy** already documented for `hooks_after_ship` (Merge→VersionBump→
  ChangelogAppend→BranchCleanup). Whatever succeeded stays; there is no
  automatic undo of a landed commit, tag, or publish. The fix is always
  forward (retry the failed step), never an automatic compensating action.
  — **Reversibility:** reversible as a policy — but the state it protects
  (tags, publishes) is not.
- **D-06:** Re-running `devflow release --yes-release` after a partial
  failure **auto-skips steps already completed**, detected from live
  git/registry state (tag already exists and is reachable → skip tagging;
  crate already live on the registry at this version → skip that publish;
  `develop` already ahead of the computed bump → skip the bump push) rather
  than requiring the operator to manually diagnose where to resume. This
  mirrors the existing sync script's own "is origin/main already an
  ancestor? — nothing to do" idempotency check, applied to every step.
  — **Reversibility:** reversible.

- **D-06a — AMENDMENT, operator-decided 2026-07-30. D-06's "live-state
  predicate only, never a persisted progress file" constraint is RE-OPENED
  and relaxed for the publish step specifically.**

  **Why it was re-opened.** Phase 26's Ship review raised **C-02** as a
  Critical: a failed publish step is *unresumable*. Because `compute_version`
  derives the version from live git state, and a partially-completed release
  has already moved that state, the re-run does not resume the interrupted
  release — it computes a **new** version, pushes **another** version bump,
  and exits 0. The operator gets a silent second release instead of a
  completed first one. The audit-fix pass deliberately did **not** attempt a
  fix, because the only workable remedy is a persisted step ledger, which
  D-06 as originally written forbids; overwriting a recorded decision
  unilaterally was correctly refused and escalated instead.

  **What is now permitted.** A persisted step ledger may be written and read
  for the purpose of resuming an interrupted release cut. It is the
  authority on *what this in-flight release already did*; live-state
  predicates remain the authority on *what is actually true in git and on
  the registry*. Where the two disagree, **live state wins** — the ledger
  may never be used to assert that a step succeeded when git or crates.io
  says otherwise. This preserves D-06's real intent (never trust a stale
  file over reality) while removing the constraint that made C-02
  unfixable.

  **Scope limit — deliberately narrow.** This amendment authorizes a ledger
  for the release-executor resume path only. It does **not** license
  progress files elsewhere in DevFlow, and it does not relax D-05's
  fail-fast, no-automatic-rollback policy: the ledger records what happened,
  it never triggers a compensating action.

  **A design consequence that must be resolved when C-02 is implemented:**
  the ledger must distinguish "this release is mid-flight" from "the last
  release finished cleanly." Without that, the existing gotcha stands —
  re-running after a *complete* release starts the next one, because
  `UnreachableBaseline` is by-construction true mid-sequence and
  indistinguishable from a fresh start. That distinction is the ledger's
  primary job, not an incidental detail.
  — **Reversibility:** costly — once a ledger format is persisted by a
  released binary, changing or removing it means handling ledgers written by
  older versions. Choose the format deliberately.

### Sync (999.52)

- **D-07:** `devflow sync` (porting `scripts/sync-main-to-develop.sh`) is
  **both a standalone subcommand** (callable any time ancestry drifts, not
  only during a release) **and internally reused** by the executor as its
  own sync step — one implementation, two entry points.
- **D-08:** Sync **direct-pushes to `develop`**, consistent with D-01 — this
  removes the original 999.52 failure mode (a human squash-merging the sync
  PR, which broke ancestry twice: going into v1.5.0 and again for v2.0.0) by
  construction, rather than relying on a human clicking the correct GitHub
  merge-strategy button a third time.
- **D-09:** The script's existing safety check is **preserved exactly**:
  the `-X ours` merge must produce a byte-identical tree to `develop`'s
  pre-merge tree; any mismatch **refuses and leaves `develop` untouched**
  rather than pushing a merge that changed content. Fail-closed, matching
  the proven script's behavior.
  — **Reversibility:** reversible (it's a refusal, not a destructive action).

### Signing-check scope (999.54 / 999.50) — DROPPED from this phase and from the backlog

- **D-10:** After discussion, the operator determined `release --check`'s
  signing-viability *predictor* (`check_ssh_signing_viability`, the function
  999.54 and 999.50 targeted) should **not be fixed, extended, or reused by
  the executor at all** — and explicitly does not want signing-viability
  *prediction* built into DevFlow, ever. Rationale surfaced during
  discussion: a predictor is a second implementation of "will signing work?"
  that must stay in sync with what git actually does, which is exactly the
  bug class 999.50/999.54 are about (the predictor disagreeing with
  reality). The executor's tag step instead just **runs the real signed
  `git tag` command** (the exact form already documented in CONTRIBUTING.md
  § "Cutting a Release" step 5 —
  `git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" tag -s vX.Y.Z <commit> -m "vX.Y.Z"`)
  and reports git's own real exit code / `git tag -v` verification. No
  viability guess, no new abstraction — the answer is authoritative because
  it comes from actually doing the operation, not predicting it.

  **The "Claude can't sign main" guarantee the operator wants** falls out of
  D-02, not from any signing-check code: Claude's unattended path stops at
  `develop`; only a human explicitly typing `--yes-release` ever reaches the
  tag step, so whatever `devflow.releaseSigningKey` resolves to in *that
  human's* environment is what signs the tag. DevFlow does not need to know
  or verify signing-key policy ahead of time — the environment is expected
  to already be configured correctly (Claude's `user.signingkey` for
  ordinary commits, the maintainer's `devflow.releaseSigningKey` for
  releases), matching how these two git-config values already work today.

  **999.54 and 999.50 were removed from the backlog entirely** (not left
  filed) — operator stated no intention of ever implementing signing
  prediction in DevFlow. (999.27, a different and already-shipped
  classification bug in the same function from Phase 24, is unaffected —
  historical record of delivered work, not touched.)
  — **Reversibility:** reversible — nothing was ever built, so there is
  nothing to undo; a future operator preference change would just mean
  re-filing a backlog item from scratch.

### Concurrent-ship tag race (999.4) — DROPPED from this phase and from the backlog

- **D-11:** 999.4's race scenario is specific to `devflow parallel`
  (running multiple whole phases concurrently in separate worktrees, each
  independently reaching Ship/computing a version/creating a tag around the
  same moment) — confirmed by reading `main.rs:147-158`'s `Parallel`
  command. The operator does not use, and would never want to use, `devflow
  parallel` to run multiple phases at once for a single DevFlow user ("that's
  just asking for trouble"). Since the race cannot occur in the operator's
  actual usage, **999.4 was removed from the backlog entirely**, not merely
  dropped from this phase.
  — **Reversibility:** reversible — same reasoning as D-10; nothing built,
  easy to re-file if usage changes.

### Changelog content (999.5)

- **D-12:** The changelog entry generated during a release is **derived
  from the same conventional-commit classification** Phase 25's version-bump
  step already computes (feat/fix/docs/etc. over `baseline..HEAD`), rather
  than a new content source. Replaces the current hardcoded
  `"Released phase via DevFlow."` (`ship.rs:394`) with an actual list of
  what changed, reusing data already computed in the same code path — no
  new design needed for content sourcing (this was the reason 999.5 had
  been deferred three times previously: no content source had been chosen
  until Phase 25's classifier gave it one for free).
  — **Reversibility:** reversible.

### Project-root resolution for mutating commands (added 2026-07-30)

- **D-13 — operator-decided 2026-07-30, from Ship review finding C-06.**
  A **mutating** command (`release --execute`, `sync` — anything that
  pushes, tags, or publishes) must **refuse** when `project_root` resolves
  to a directory *different from the one it was invoked in*, printing both
  paths and directing the operator to `cd` or pass `--project` explicitly.
  **Read-only commands keep today's upward-walking behavior unchanged**
  (`status`, `doctor`, `gate`, `release --check`, etc.) — they legitimately
  need to find the owning `.devflow` from a subdirectory.

  **The defect this closes.** `project_root` (`crates/devflow-cli/src/main.rs:662-683`)
  walks *up* to the nearest `.devflow` ancestor. A phase worktree
  (`.worktrees/phase-NN/`) has no `.devflow`; the parent checkout does.
  Phase 26 newly routed `release --execute` (`main.rs:629`) and `sync`
  (`:637`) through that resolver — the first *irreversible* commands to use
  it. So a maintainer running `devflow release --execute --yes-release` from
  a phase worktree, which is this project's ordinary working posture, cuts a
  release from the **main checkout's** branch, commits, and manifest,
  without ever being shown the redirect. **Worse: all four entry guards
  (clean tree, on-develop, has-remote, pre-gate) test the redirected root**,
  so a dirty worktree with a clean parent makes the executor *more* likely
  to proceed, not less — the safety checks validate the wrong repository.

  **Verified, and it simplifies the fix:** neither `release.rs` nor
  `sync.rs` reads `.devflow` at all — zero references to `devflow_dir`,
  `.devflow`, or `events::emit` in either module. They need a **git
  repository root** (branches, tags, remotes, the Cargo manifest), not
  DevFlow pipeline state. `.devflow` is merely the marker `project_root`
  happens to search for, because that helper was built for pipeline commands
  that genuinely do read `events.jsonl` / `state-NN.json` / `gates/`. So
  refusing on redirect removes nothing these commands actually needed.

  **Implementer's latitude:** resolving mutating commands via
  `git rev-parse --show-toplevel` (the repository the operator is standing
  in) instead of the `.devflow` upward walk is an acceptable — arguably
  cleaner — way to satisfy this, since it makes a silent redirect
  structurally impossible rather than merely detected. Either shape is fine
  provided the refusal is loud and names both paths.

  **Note the interaction with the existing on-`develop` guard:** a phase
  worktree is on `feature/phase-NN`, so the executor would refuse there
  anyway — but today it refuses for the *wrong* reason (it redirected first
  and then checked the parent, which may well be on `develop` and pass).
  D-13 makes it refuse for the honest reason.
  — **Reversibility:** reversible — a guard plus a resolver change, no
  persisted state or published contract involved.

### Claude's Discretion

Not constrained here; the researcher and planner decide:

- **Exact shape of the `--execute`/`--yes-release` CLI surface** (a new
  `Release { execute: bool, yes_release: bool, ... }` arg shape vs. a
  distinct `Command::ReleaseExecute` variant) — either is acceptable;
  match the existing `Release { check, project }` pattern as closely as
  sensible.
- **Where the direct-push code lives** (a new `GitFlow` method alongside
  `merge_feature_into_develop`, vs. a standalone function) — follow the
  established module conventions in `git.rs`.
- **How `devflow sync`'s standalone-vs-internal duality is implemented**
  (shared function called from both a CLI command handler and the
  executor's internal sequence) — D-07 only fixes that both entry points
  must exist and share one implementation, not the exact function
  signature.
- **Retry/backoff shape for the crates.io pre-publish check** (D-04) — a
  single synchronous query is sufficient; no polling loop is implied unless
  the researcher finds a reason one is needed.
- **Doc-comment and CONTRIBUTING.md updates** reflecting that the manual
  7-step "Cutting a Release" checklist is now partially automated — the
  planner should note which of the 7 steps `--yes-release` covers and update
  CONTRIBUTING.md accordingly, but the exact wording is not constrained here.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Backlog sources this phase was promoted from
- `.planning/phases/26-release-cut-automation/999.25-BACKLOG-DOSSIER.md` —
  the original 999.25 backlog context: possible shapes, publish-ordering
  constraint, prior deferral reasoning from Phase 20 D-03. Predates this
  discussion; D-01 through D-06 above supersede its "possible shapes" list
  where they conflict.
- `.planning/ROADMAP.md` § "Phase 999.52" — the sync-discipline backlog
  entry (now promoted here); documents the exact ancestry-breaking incidents
  (v1.5.0, v2.0.0) that motivate D-07/D-08/D-09.
- `.planning/ROADMAP.md` § "Phase 999.5" — the changelog-placeholder entry
  (now promoted here); documents the two prior deferrals (17-10, 17-12) for
  want of a content source, resolved by D-12.
- `.planning/ROADMAP.md` § "Phase 26" — the phase entry itself, including
  the explicit exclusion notes for 999.55 and 999.39 (handled outside this
  phase) and the promotion rationale for 999.25/999.52/999.5.

### Code this phase changes or depends on
- `crates/devflow-cli/src/main.rs:233` — `Command::Release { check, project }`
  — the CLI surface to extend per D-03's discretion note.
- `crates/devflow-core/src/git.rs:82-88` — `merge_feature_into_develop` — the
  existing local-merge-only pattern D-01's direct-push code should sit
  alongside (note: this function itself never pushes; the executor's
  develop-push is new code, not a change to this function).
- `crates/devflow-core/src/git.rs:510-` — `publish_order` — already computes
  the devflow-core-then-devflow publish sequence D-04 needs; reuse, don't
  recompute.
- `crates/devflow-core/src/version.rs:474` (`compute_version`) and the
  conventional-commit classifier it uses (Phase 25 D-08) — D-12's changelog
  content source.
- `crates/devflow-core/src/ship.rs:394` — the hardcoded
  `"Released phase via DevFlow."` string D-12 replaces.
- `scripts/sync-main-to-develop.sh` — the proven `-X ours` + tree-identity
  logic D-07/D-09 port into `devflow sync`. Read in full before planning;
  every check it performs (clean working tree, on `develop`, fetch first,
  already-ancestor short-circuit, tree-identity verification) must survive
  the port.
- `CONTRIBUTING.md` §§ "Release signing" (line 52) and "Cutting a Release"
  (line 236) — the authoritative 7-step manual checklist this phase
  automates, and the exact `git tag -s` invocation form D-10 requires
  verbatim (explicit `-c user.signingkey=` override, never a bare
  `git tag -s`).
- `crates/devflow-cli/src/preflight.rs:611-620` and
  `crates/devflow-cli/src/commands.rs:2085-2110` — the only two existing
  `gh` CLI call sites in the codebase (`gh auth status`, `gh --version`
  doctor check) — precedent for how `gh` is invoked, though D-02 means this
  phase does not add a new `gh pr merge`/`gh pr create` call site itself
  (the main-branch PR stays human-driven).

### Adjacent phase context (do not re-derive — read the source)
- `.planning/phases/25-end-to-end-dogfood-blockers/25-CONTEXT.md` §§ D-07
  through D-12 — **D-12 there is directly load-bearing for this phase**:
  `compute_version`'s tag-reachability baseline depends on the sync merge
  never being squashed again. This phase's D-08 (direct-push sync) is what
  actually closes that dependency by removing the squashable-PR step
  entirely.
- `.planning/phases/24-release-check-signing-key-inline-classification/24-CONTEXT.md`
  — the established pattern for `check_ssh_signing_viability` (fail-soft,
  `NotViable` reserved for proven-bad, redaction discipline, `HOME_ENV_MUTEX`
  test pattern). Relevant context for *why* that function's conventions
  exist, even though this phase (per D-10) does not touch it.
- `.planning/PROJECT.md` § "Constraints" — the git-derived SemVer scheme
  (Phase 25) that `compute_version` implements; this phase's tag/publish
  steps consume its output, they don't recompute it.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`publish_order`** (`git.rs:510-`) — topologically-sorted crates.io
  publish order for workspace-local-path members; D-04's publish step calls
  this, does not reimplement ordering logic.
- **`scripts/sync-main-to-develop.sh`** — fully proven `-X ours` +
  byte-identical-tree-verification logic; D-07/D-09 port this almost
  verbatim into Rust, preserving every one of its safety checks (clean tree,
  correct starting branch, fetch-first, already-ancestor short-circuit).
- **Conventional-commit classifier** (Phase 25, `version.rs`'s
  `compute_version` path) — already parses `baseline..HEAD` commits into
  feat/fix/docs/etc.; D-12 reuses this exact classification for changelog
  content instead of building a second parser.
- **`--yes-ship` flag pattern** (`main.rs`) — the precedent D-03's
  `--yes-release` follows: a dangerous authorization that must be typed
  per-invocation, attributed in the gate ledger, never a standing default.

### Established Patterns
- **Fail-fast, no rollback** (`hooks_after_ship`'s documented policy) — D-05
  extends this same posture to the release executor rather than inventing
  new semantics.
- **Idempotent re-entry via live-state checks, not a persisted progress
  file** — the sync script's "already an ancestor? nothing to do" check is
  the established shape; D-06 generalizes it to every step (tag existence,
  registry publish state, push-ahead state) rather than introducing a
  separate release-state tracking file.
- **No prediction where the real operation is cheap to just run** — D-10's
  reasoning (run the real signed tag command instead of a viability
  predictor) is a house principle worth carrying into any future step this
  phase's researcher/planner designs: prefer "do it and read the real
  result" over "guess first."

### Integration Points
- The executor's develop-push (D-01) and sync-push (D-08) are **new
  integration surface** — no push-of-`develop` exists anywhere in
  production code today (confirmed: the only production `git push` calls
  are branch-create and branch-delete, `git.rs:211,224`). This is genuinely
  new code, not a reuse of an existing push path.
- The tag-creation step is likewise **entirely new** — no production code
  anywhere creates a git tag today; every release so far has been tagged by
  hand per CONTRIBUTING.md step 5.
- `cargo publish` execution is similarly new — `publish_order` only computes
  the order; nothing today actually invokes `cargo publish`.

</code_context>

<specifics>
## Specific Ideas

- **Operator's exact framing of the ship/release split:** "shipping" ends
  at merge to `develop` (existing `--yes-ship`); "releasing" ends at merge
  to `main` + full version release (new `--yes-release`). Keep this
  language in CLI help text and CONTRIBUTING.md updates — it's the
  operator's own mental model, not just an implementation detail.
- **Operator's stated future direction (not this phase):** eventually
  having Claude review the develop→main release PR instead of requiring a
  human. Explicitly not built here (D-02) — noted so a future phase doesn't
  need to re-derive why `main` is still human-gated.
- **Operator was explicit and emphatic about never wanting signing
  prediction in DevFlow** — this isn't a scope-trim, it's a standing
  preference. Don't resurface `check_ssh_signing_viability` fixes as a
  "while we're in the area" suggestion in a future phase without revisiting
  D-10's reasoning first.

</specifics>

<deferred>
## Deferred Ideas

- **`devflow parallel`'s future** — whether to remove whole-phase
  concurrency entirely (simplifies code, closes 999.4/999.26 by deletion
  rather than by fix, but is a breaking CLI change to a shipped command —
  possibly the first change that would genuinely earn the open `v2.0.0`
  milestone slot), repurpose the underlying mechanism for parallelizing
  independent workstreams *within* a single phase (though GSD's own
  wave-based plan execution may already cover this need one layer above
  DevFlow's CLI), or leave it alone. Surfaced during this discussion,
  explicitly not decided — needs its own phase with real investigation
  (code footprint, dependents, breaking-change classification) before any
  action.
- **`gh pr merge`-driven auto-merge for the develop→main release PR** — the
  operator's stated future direction (Claude reviewing that PR instead of a
  human) implies this eventually gets automated too, but D-02 keeps it
  fully manual for this phase. Revisit once Claude-based PR review exists
  as a real capability.

### Reviewed Todos (not folded)
None — no todo-cross-reference matches were found for this phase.

</deferred>

---

*Phase: 26-release-cut-automation*
*Context gathered: 2026-07-29*
