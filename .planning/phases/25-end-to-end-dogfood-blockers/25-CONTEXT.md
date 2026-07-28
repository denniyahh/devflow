# Phase 25: End-to-End Dogfood Blockers — Start, Progress, Finish, Recover - Context

**Gathered:** 2026-07-27
**Status:** Ready for planning

<domain>
## Phase Boundary

Close the six individually-evidenced defects that stop an unattended
`devflow start --phase N --agent claude --mode auto --yes-ship` from reaching a
completed Ship stage. Each unit maps to exactly one filed, reproduced backlog
item; the fix directions were written before scoping and are not re-derived
here.

| Unit | Closes | What it fixes |
|---|---|---|
| **25a** | 999.51 / DEN-76 | A run **starts** on a current base ref |
| **25b** | 999.48 / DEN-73 | A run **progresses** past the Validate boundary when the phase modifies DevFlow's own source |
| **25c** | 999.49 / DEN-74 | A run **finishes** with a correct version, not `~1.11.359` |
| **25d** | 999.44 / DEN-68 | A stalled run **recovers** without `kill -9` |
| **25e** | 999.47 / DEN-72 | A CI flake stops feeding the 3-strike Validate gate |
| **25f** | (no backlog entry) | CONTRIBUTING.md release-procedure drift introduced by PR #38 |

Plus **999.38** (test-suite `PATH` race), folded in — see D-14.

**Scope changed during this discussion — read `<decisions>` D-01 before
planning.** Two of the ROADMAP entry's own statements were checked against the
code and do not hold. The corrections are recorded as decisions, not as
research questions.

**Explicitly out of scope,** with reasons recorded in the ROADMAP entry — do
not re-add: 999.31/DEN-56 (modular agent driver, a *Codex* blocker), 999.25/
DEN-50 (release-cut executor), 999.15/DEN-40, 999.21/DEN-46, 999.4/DEN-29,
999.5/DEN-30, 999.39.

</domain>

<decisions>
## Implementation Decisions

### Scope corrections — the ROADMAP entry is wrong in two places

- **D-01 [informational] (corrects the ROADMAP's 25b sizing rationale):** The entry states that
  `launch_stage`'s `archived_stage: Option<Stage>` parameter already
  distinguishes fresh start from transition, so the fix is
  `if archived_stage.is_none() { enforce_build_staleness(…)?; }`. **It does
  not.** `None` is passed by *three* callers, not one:

  | Call site | Situation |
  |---|---|
  | `crates/devflow-cli/src/commands.rs:236` | fresh `devflow start` |
  | `crates/devflow-cli/src/pipeline_launch.rs:233` | `resume` after a rate-limit or infra pause — **mid-run** |
  | `crates/devflow-cli/src/preflight.rs:435` | preflight `LoopBack` retry — **mid-run** |

  Under the one-liner, a rate-limited self-modifying phase still hard-blocks on
  resume — the exact halt 25b exists to remove. The planner must not implement
  the entry's proposed one-liner. See D-02 for what replaces it.
  — **Reversibility:** reversible.

- **D-02 (corrects the ROADMAP's 25e framing):** The entry treats 25e as
  tightening a live production guard. **The guard is already fixed.**
  `crates/devflow-core/src/lock.rs:123-181` records `(pid, starttime)` on the
  lock file and exposes `holder_identity`; `stop` matches against it at
  `crates/devflow-cli/src/commands.rs:1191-1200`. So 999.47's prescribed
  production fix ("identity must be a recorded pair") has landed.
  `agent::looks_like_devflow_process` (`crates/devflow-core/src/agent.rs:158`)
  now has **no production callers** — only test code
  (`agent.rs` tests, and `commands.rs:3308` instrumentation). 25e is therefore
  about a dead `pub fn` whose tests flake, not about a live guard. See D-12.
  — **Reversibility:** reversible.

### 25b — staleness pin (999.48 / DEN-73)

- **D-03:** **Hoist `enforce_build_staleness` out of `launch_stage` into the
  `start` path.** Move the call from `crates/devflow-cli/src/pipeline_launch.rs:93`
  to `crates/devflow-cli/src/commands.rs`, immediately before the
  `launch_stage(&mut state, None, None)` at line 236. Net ~7 lines moved. No
  new `State` field, no serde migration, no pin/mismatch semantics.

  **Verified precondition:** `state.worktree_path` is populated at
  `commands.rs:199`, *before* line 236 — so the check still evaluates against
  the phase's worktree HEAD, which is what the D-18 block message explicitly
  promises ("evaluated against this phase's WORKTREE HEAD, not the main
  checkout"). A hoist above line 199 would silently change that and is wrong.

  **Chosen over a persisted pin explicitly on simplicity grounds** (operator,
  2026-07-27): *"Which solution is the simplest and cleanest from a code
  perspective? I don't want to overengineer this since this is only to support
  dogfooding."* A `staleness_pin: Option<String>` on `State` would need a
  serde default, a write path, a comparison, event-log plumbing, and tests for
  both migration and mismatch — for a guard that by construction fires only in
  this one repository.
  — **Reversibility:** reversible — one call site moved.

- **D-04 (accepted trade, recorded not hidden):** Under D-03, `resume`
  (`pipeline_launch.rs:233`) no longer re-checks staleness, so a *different*
  binary resuming a phase mid-run is never re-adjudicated. **Accepted.** The
  scenario is already forbidden by 999.48's rejected alternative #1 — the
  operator's standing decision of 2026-07-27: *"I don't want unvalidated code
  to be used to rebuild the binary mid-run. Only validated and pushed code
  should ever be used."*
  — **Reversibility:** reversible — a pin can be added later if the trade
  proves wrong.

- **D-05 (inherited, non-negotiable):** Do **not** re-propose either
  alternative 999.48 rejected: mid-run rebuild of the driving binary, or a
  dogfood bypass flag. Both were adjudicated by the operator on 2026-07-27 with
  reasons recorded in the ROADMAP. Do not weaken D-18 generally — its scope is
  the defect, not its existence.

### 25c — versioning (999.49 / DEN-74)

**Standing policy change.** The operator elected on 2026-07-27 to *fully
automate* versioning, explicitly authorising deviation from the prior policy:
*"I want to fully automate the versioning starting from now. Help me determine
the best versioning scheme for this type of project to the degree that deviates
from the policy I originally defined."*

- **D-06:** **The June 2026 ban on commit-message-based versioning is lifted.**
  `.planning/ROADMAP.md:36` records it as a bare bullet — *"Conventional commits
  deprecated — no commit-message-based versioning"* — in a reorg list, with **no
  rationale, incident, or evidence attached**. `.planning/PROJECT.md`'s
  Constraints section restates it. Both must be updated as part of this phase;
  a planner or verifier reading the un-amended constraint would treat D-07 as a
  violation.
  — **Reversibility:** costly — undo means re-deriving versions for anything
  already shipped under the new scheme, and `crates.io` versions can never be
  reused.

- **D-07:** **Baseline = the highest reachable semver tag.** Enumerate `git
  tag`, keep only values parsing as `v?MAJOR.MINOR.PATCH`, keep only those
  reachable from `HEAD` (`git merge-base --is-ancestor`), take the max by
  **semver ordering** — not by string sort, not by count. **No `git describe`
  anywhere.**

  Verified on this repository 2026-07-27: `v1.0.1` … `v2.0.0` are *all*
  reachable from `HEAD` (the `-X ours` sync-merge-back restores the ancestry
  that squashing to `main` destroys), and the single non-reachable tag is
  `archive-planning-docs-2026-07-24` — precisely the non-semver tag inflating
  `count_git_tags` to 11. So this one predicate kills both halves of 999.49 and
  resolves to `v2.0.0` today.
  — **Reversibility:** reversible.

- **D-08:** **Bump = conventional-commit classification** over `--no-merges`
  commits in `baseline..HEAD`. `!` or a `BREAKING CHANGE:` footer → major;
  `feat` → minor; `fix`/`perf` → patch; `docs`/`test`/`chore`/`ci`/`refactor`/
  `style` → no bump. Highest precedence wins.

  **The input signal was measured, not assumed:** of the last 120 non-merge
  commits, **118 conform** to `type(scope): subject`. The two exceptions are
  `merge:` and `release:`, both structurally excluded (`--no-merges`, and a
  release commit must not bump). Distribution: 79 `docs`, 18 `test`, 13 `feat`,
  4 `ci`, 3 `fix`, 1 `chore`. The convention is also mandated upstream of
  DevFlow by the operator's global `git-workflow.md`, including `!` for
  breaking changes.
  — **Reversibility:** reversible.

  **AMENDED during plan review, 2026-07-27 — the literal range is wrong; see
  `25-01-PLAN.md` §`<measured_correction>`.** `baseline..HEAD` as written above
  yields **678 of this repository's 797 non-merge commits**, reaching back to
  2026-06-20, because every release squash-merges `develop`→`main` — so no
  develop-side commit is ever an ancestor of a release tag, while the `-X ours`
  sync merge-back restores ancestry only in the direction D-07 needs. Measured
  live: `v2.0.0..HEAD` = 678 commits / 62 `feat` → `2.1.0`, against a correct
  `2.0.1`. **The permanence trap is the disqualifier:** the first commit
  anywhere carrying `!` or `BREAKING CHANGE:` never leaves the range, so D-09's
  gate would fire on every subsequent ship forever — turning the unattended run
  this phase exists to unblock into a permanent halt.

  The rule above is unchanged; only the commit set fed to the classifier
  changes, via a two-branch anchor verified on both topologies present here.
  `--first-parent` was measured as an alternative and does **not** fix it (383
  commits). The operator reviewed the simpler alternatives — implementing D-08
  literally, and replacing the whole classifier with a suggest-and-confirm
  mechanism — and elected on 2026-07-27 to **keep the full classifier including
  the anchor**.

  **This deepens D-12:** the sync merge is now load-bearing in a *second* way —
  it is the range anchor, not only the ancestry restorer.

- **D-09:** **A major bump opens a gate; it never ships unattended.** Detection
  runs as a **named preflight check** inside `run_preflight`
  (`crates/devflow-cli/src/preflight.rs`), reusing the existing gate + notify
  machinery (D-13–D-16) and bounded by `preflight_retries`.

  **Placement is load-bearing, not cosmetic.** `hooks_after_ship` runs
  Merge → VersionBump → ChangelogAppend → BranchCleanup as a fail-fast batch
  with **no rollback** (stated in `merge_feature`'s doc comment). A gate that
  opened inside `VersionBump` would open *after* the merge to `develop` had
  already committed. The classification must therefore be evaluated **before
  `hooks_after_ship` runs at all**.
  — **Reversibility:** reversible.

- **D-10:** **Floors and failure directions:**
  - A range where nothing bumps (e.g. a docs-only phase) → **bump patch
    anyway**, so every completed ship yields a distinct version and the tag
    `VersionBump` cuts can never collide.
  - A commit whose type is unrecognised or malformed → **treat as patch**.
  - The highest semver tag overall is **not** reachable from `HEAD` (a squashed
    sync broke ancestry again) → **refuse**, naming the unreachable tag and the
    command that repairs it. Do not silently fall back to the highest
    *reachable* tag: that computes a version below the real release history,
    which is the same false-evidence shape 999.51 warns about, arriving through
    tags instead of the base ref.
  — **Reversibility:** reversible.

- **D-11:** **`Cargo.toml` stops being an input to the computation.** Today
  `compute_version` reads major from the version file
  (`crates/devflow-core/src/version.rs:141-153`). Under D-07/D-08 the whole
  version derives from tags + commits, and `Cargo.toml` becomes purely an
  *output* that `VersionBump` writes. This also collapses the long-standing
  two-places-to-bump drift. `read_version` (same file) keeps its existing
  distinct role and must not be conflated with this path.
  — **Reversibility:** costly — `hooks.rs` and any caller reading the version
  file as authoritative would need revisiting.

- **D-12 (coupling — record in the plan, do not silently depend on it):**
  D-07's correctness depends on 999.52's sync discipline. If a `develop` →
  `main` sync PR is ever squashed again, ancestry breaks and the baseline would
  regress. D-10's refuse-on-unreachable is the mitigation; 999.52 itself stays
  in the backlog and is **not** in this phase.

### 25e — dead predicate (999.47 / DEN-72)

- **D-13:** **`#[deprecated]` the predicate, retarget the tests.** Mark
  `agent::looks_like_devflow_process` `#[deprecated]` and rewrite both flaky
  tests — `agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process`
  (`crates/devflow-core/src/agent.rs:296`) and
  `commands::tests::stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check`
  (`crates/devflow-cli/src/commands.rs:3259`) — to assert the
  `(pid, starttime)` identity guard production actually uses.

  **The retarget is what fixes the flake, and it fixes it by construction.**
  Testing the identity guard means writing a lock file with a deliberately
  mismatched starttime: no `spawn()`, therefore no `execve` race. Today's tests
  must spawn a real `sleep` and race its exec — and that race *is* the
  confirmed mechanism (999.47, "MECHANISM CONFIRMED 2026-07-26"), so it can be
  made rarer but never eliminated while the tests are shaped that way.

  **Deletion was considered and rejected on cost, not principle.**
  `devflow-core` has no `publish = false` and `lib.rs:54` is `pub mod agent`,
  so the function is public API of a published crate; removing it trips D-08's
  breaking classifier and spends `3.0.0` on a function with zero known callers.
  `PROJECT.md` reserves the major slot for a genuinely breaking change.
  Removal rides the next major that something real earns.

  **Tightening to `argv[0]` was rejected as ineffective:** between `spawn()`
  returning a pid and the child completing `execve`, `/proc/<pid>/cmdline`
  still reports the *parent's* argv — `argv[0]` sits inside that same inherited
  data, and 999.47 records `/proc/<pid>/exe` as inherited in that window too.
  Any `/proc`-derived inference races exec; only a recorded pair is immune,
  which is why production already moved.
  — **Reversibility:** reversible.

### 999.38 — folded in

- **D-14:** **Fold 999.38 into 25b, not 25e.** Its flake is
  `staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking`
  (`crates/devflow-cli/src/staleness.rs:891`), and 25b already edits that
  module's call path — one pass over one module, with the fix landing next to
  25b's new tests. Pairing it with 25e would pair files that share nothing but
  flakiness, since 25e's work is now in `agent.rs` and `commands.rs`.

  Fix direction per the backlog entry: per-`Command` `env`/`env_remove` (as
  999.37's `test_support` now does for git), **not** process-global
  `set_var`/`remove_var` — which Rust 2024 marks `unsafe` precisely because it
  is unsound in a threaded test binary.
  — **Reversibility:** reversible.

### Acceptance — decoupled from phase closure

- **D-15 (standing policy change, confirmed by the operator 2026-07-27):** The
  end-to-end acceptance run is **unofficial and continuous**. It is run when
  the operator chooses and **gates no phase's completion, until further
  notice** — not this phase, not later ones.

  **Phase 25 is therefore complete when 25a–25f are implemented and verified on
  their own unit-level merits.** Each unit needs its own verifiable acceptance
  (a test, a closed reproduction), because the end-to-end run no longer
  backstops any of them.

  Anything a future unofficial run surfaces is filed to the backlog the usual
  way, exactly as Phase 23's runs were.
  — **Reversibility:** reversible — the criterion can be reinstated.

- **D-16:** **The ROADMAP's Phase 25 "Acceptance" paragraph must be rewritten**
  to match D-15 — fold this into **25f**, which is already the docs-drift unit.
  Left as-is, a verifier reads *"the phase is done when a single `devflow start
  …` reaches Ship … and `devflow evidence --phase N --require-shipped` exits
  0"* and marks a correctly-completed phase unmet. This is a required
  deliverable, not a nicety.
  — **Reversibility:** reversible.

### 25a and 25d — not discussed, ROADMAP directions stand

- **D-17:** 25a (999.51) and 25d (999.44) were offered for discussion and the
  operator elected not to open them. Their backlog entries' fix directions
  stand as written and are the planner's input:
  - **25a** — ~~unresolved~~ **RESOLVED during plan review, 2026-07-27. See
    `25-05-PLAN.md` §`<resolved_decision>` (D-18a).** Selected: fetch, compare,
    **fast-forward the local base when it is safe, else refuse loudly.** The
    deciding fact is that local `develop` is behind `origin/develop` in the
    *normal steady state* after any PR-merged ship, so refuse-loudly would halt
    on the common path rather than an edge case. The operator's simplicity
    tiebreaker was applied and found not to govern — simplicity decides between
    options that both work, and refuse-loudly does not make an unattended run
    start. Consequence: `25-05-PLAN.md` dropped its `checkpoint:decision` and is
    now `autonomous: true`; the "no `git fetch` in the start path" property at
    `commands.rs:1877`/`:1980` is **reversed for the start path** and must be
    re-worded there rather than left contradicting the code.
    The entry's binding constraint still governs: *"the 'heading present but
    code stale' case must be closed, not just the 'heading absent' one."*
  - **25d** — registry-independent PID discovery, safe on a shared machine;
    **must** escalate `TERM` → `KILL` with a bounded wait and *verify* death
    rather than assume it; **must** reap the wrapper/child pair together
    (killing only the wrapper manufactures a fresh orphan). Add a regression
    test asserting a `TERM`-ignoring child is still cleared. Note the census
    must be **subreaper-aware** — these processes reparent to `systemd --user`,
    not to pid 1, so a `ppid == 1` test reports zero orphans while dozens
    exist.

### Claude's Discretion

- Sequencing within the phase. The ROADMAP's spine (25a → 25b → 25c, with 25d
  and 25e parallel) stands, and **25b + 25c must ship together** — 25b makes an
  unattended run reachable and 25c fires on the first run that succeeds;
  shipping 25b alone converts a phase that cannot finish into one that finishes
  by writing a garbage version and tagging it, with no rollback after Merge.
- Where the semver-tag parsing and reachability predicate live
  (`version.rs` vs a new helper), and whether an existing crate is used for
  semver ordering or it is hand-rolled — note the project's zero-network-deps
  constraint applies to runtime, not to a parsing dependency, but keep the
  dependency budget in mind.
- The exact shape of 25d's discovery mechanism, within D-17's constraints.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and the six units
- `.planning/ROADMAP.md` §"Phase 25: End-to-End Dogfood Blockers" (lines
  1420-1468) — the phase entry, the four-requirement decomposition, the
  sequencing rules, the 25b+25c ship-together constraint, and the exclusion
  list. **Its "Acceptance" paragraph is superseded by D-15/D-16, and its 25b
  sizing rationale is corrected by D-01.**
- `.planning/ROADMAP.md` §"Phase 999.51" (lines 947-965) — 25a fix directions
- `.planning/ROADMAP.md` §"Phase 999.48" (lines 859-886) — 25b, incl. the two
  rejected alternatives that must not be re-proposed
- `.planning/ROADMAP.md` §"Phase 999.49" (lines 888-904) — 25c root cause,
  incl. the corrected `git describe` analysis
- `.planning/ROADMAP.md` §"Phase 999.44" (lines 702-720) — 25d, incl. the
  `SIGTERM`-immunity escalation
- `.planning/ROADMAP.md` §"Phase 999.47" (lines 791-857) — 25e, incl.
  "MECHANISM CONFIRMED" and the `argv[0]` weakness
- `.planning/ROADMAP.md` §"Phase 999.38" (lines 663-670) — folded in per D-14
- `.planning/ROADMAP.md` §"Phase 999.46" (lines 736-773) — **do not weaken the
  E2E tests that spawn real children**; fixes belong in teardown
- `.planning/ROADMAP.md` §"Phase 999.52" (lines 967+) — the sync-discipline
  coupling named in D-12; **not** in this phase

### Project-level constraints this phase amends
- `.planning/PROJECT.md` §Constraints — the versioning ban lifted by D-06, and
  the "hybrid git-based SemVer via `version.rs`" statement that D-07/D-11
  supersede
- `.planning/PROJECT.md` §Context — the milestone note reserving `2.0.0`/major
  for a genuinely breaking change (cited by D-13)
- `.planning/ROADMAP.md:36` — the bare, unreasoned bullet D-06 lifts
- `~/.claude/rules/git-workflow.md` — the operator's global Conventional
  Commits contract that D-08's classifier depends on, including `!` and
  `BREAKING CHANGE:` for breaking changes

### Evidence for the reproductions
- `.planning/phases/23-end-to-end-dogfood/23-FINDINGS.md` §A1 — 999.44's
  original orphan evidence
- `.planning/phases/23-end-to-end-dogfood/23-FINDINGS.md` §A3 — scratch-root
  accumulation (999.46 scope, not this phase)
- `.planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-RUN-3.md` — the
  Define→Plan→Code run that halted at Validate with
  `self_dogfood_stale_blocked`; 25b's primary evidence
- `.planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-SETUP-2.md` — the
  earlier base-ref failure of the same class as 999.51
- `.planning/phases/23-end-to-end-dogfood/23-ORPHAN-FORENSICS.md` — orphan
  population forensics relevant to 25d

### Precedent this phase inherits
- `.planning/phases/24-release-check-signing-key-inline-classification/24-CONTEXT.md`
  — D-01/D-02 (mirror the tool's documented contract, never a
  safer-*looking* heuristic) and D-06 (fail-soft: a new code path must not
  introduce a false hard-block on correct work)

### Docs to update
- `CONTRIBUTING.md` §"release procedure" step 5 — 25f's target: the
  agent-signed-tag hazard and the stale `tag.gpgsign=false` warning

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`lock::holder_identity` + `agent::process_start_time`**
  (`crates/devflow-core/src/lock.rs:123-181`,
  `crates/devflow-core/src/agent.rs:124-139`) — the `(pid, starttime)` identity
  pair. Already the production guard in `stop`; D-13's retargeted tests assert
  against it, and 25d's reaper should use it rather than inventing a second
  identity mechanism.
- **`run_preflight`** (`crates/devflow-cli/src/preflight.rs`) — the named-check
  + gate + notify machinery D-09 reuses for major-bump detection, already
  bounded by `preflight_retries`.
- **`version::read_version`** (`crates/devflow-core/src/version.rs`) — reports
  what was last written without touching git. Distinct role from
  `compute_version`; D-11 does not change it.
- **`events::emit`** (`crates/devflow-cli/src/commands.rs:230` call site) — the
  provenance channel. **WR-02 constraint:** `.devflow/events.jsonl` is
  advertised in `OPERATIONS.md` as safe to tail, so it must never carry a
  filesystem path (which on Linux/macOS embeds the operator's username).
- **`test_support`** per 999.37 — the per-`Command` env pattern D-14 adopts.

### Established Patterns
- **Persisted counters over in-process ones.** `preflight_retries`
  (`crates/devflow-core/src/state.rs:44-55`) is persisted specifically because
  the wedge it bounds spans separate `devflow` invocations. That is the
  precedent a staleness pin would have followed — D-03 chose not to, on
  simplicity grounds, and D-04 records the accepted trade.
- **Fail-closed, fail-loud, no standing bypass.** D-05 (typed per-invocation
  authorization, never a default) governs 25b; D-10's refuse-on-unreachable
  applies it to 25c.
- **No rollback after Merge.** `hooks_after_ship` = Merge → VersionBump →
  ChangelogAppend → BranchCleanup, fail-fast, stated in `merge_feature`'s doc
  comment. Anything that could halt or gate must do so *before* this batch —
  the constraint that places D-09 in preflight.
- **`is_self_dogfood_workspace`** (`crates/devflow-cli/src/staleness.rs:240`)
  gates the entire staleness module on a `Cargo.toml` whose `members` is
  exactly `crates/devflow-core` + `crates/devflow-cli`. Nothing in that module
  fires outside this repository — which is why D-03 weighs its complexity
  budget accordingly.

### Integration Points
- `crates/devflow-cli/src/pipeline_launch.rs:93` → `crates/devflow-cli/src/commands.rs:236`
  — the D-03 hoist. `state.worktree_path` is set at `commands.rs:199`; the new
  call must sit **after** it.
- `crates/devflow-core/src/version.rs:141-153` (`compute_version`) — rewritten
  by D-07/D-08/D-10/D-11. Its consumer is `VersionBump` in
  `crates/devflow-core/src/hooks.rs`.
- `crates/devflow-core/src/version.rs:90-106` (`count_git_tags`) and
  `:110-135` (`commits_since_last_minor_tag`) — both superseded by D-07. No
  `git describe` survives.
- `crates/devflow-core/src/agent.rs:158-172` — `#[deprecated]` per D-13.
- `crates/devflow-cli/src/staleness.rs:891` — the 999.38 flake folded in by
  D-14; same module 25b touches.

</code_context>

<specifics>
## Specific Ideas

- **"Simplest and cleanest from a code perspective"** is the operator's
  explicit tiebreaker for dogfood-only support code (D-03). When two designs
  both work, the planner should prefer the smaller diff and say so, rather than
  reaching for the more general mechanism.
- **Measure the premise before designing on it.** Two of this phase's ROADMAP
  claims were checked against the code during discussion and both were wrong
  (D-01, D-02). 999.47's own closing lesson says the same thing: *"a negative
  result from a probe is evidence about the probe's sensitivity, not proof
  about the system. Reproduce in the environment that fails."*
- **Prefer determinism by construction over flake reduction.** D-13's retarget
  was chosen because it removes the race entirely (no `spawn()`), not because
  it makes the race rarer.

</specifics>

<deferred>
## Deferred Ideas

- **A `commit-msg` hook enforcing Conventional Commits.** Considered under
  D-08's classifier hygiene and deferred — it is a new enforcement surface on
  every contributor's machine and expands the phase beyond its six units.
  D-10's "unrecognised → patch" makes the classifier safe without it. Worth its
  own entry if commit hygiene ever measurably drifts from the current 118/120.
- **Reporting unrecognised-commit counts in ship output.** The fail-soft
  variant of the above (make drift visible without blocking). Not chosen; D-10
  took "treat as patch" instead.
- **Deleting `looks_like_devflow_process` outright.** Deferred to the next
  major that something real earns (D-13).
- **Deleting the `staleness.rs` module entirely.** The ROADMAP entry considers
  and rejects it — 1,794 lines and 21 tests of genuine dogfooding-only
  overhead, but the outstanding work is one moved call. Recorded so it is not
  re-litigated.
- **999.52** — DevFlow imposes a branch model whose repair step it does not
  ship. Named as a coupling in D-12; stays in the backlog.
- **999.45, 999.43, 999.46, 999.50** — all open, all out of scope, all
  untouched by this phase.
- **Reinstating the end-to-end run as a phase gate.** D-15 suspends it "until
  further notice", which is deliberately reversible.

</deferred>

---
*Phase: 25-end-to-end-dogfood-blockers*
*Context gathered: 2026-07-27*
