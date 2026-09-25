# Phase 49: Live Unattended Run — The Milestone's Instrument - Context

**Gathered:** 2026-09-24
**Status:** Ready for planning

<domain>
## Phase Boundary

One real `devflow start --mode auto` run on this repository, recorded fork point through Ship merge
target, plus a same-binary negative-control run on an unconfigured scratch repo. It answers VERIFY-01
and records the two carried observations: DECN-03's behavioural arm and SURV-01's field arm.

**The run's workload is a real backlog item, promoted as Phase 49.1:** 999.55, the configurable polling
budget for the `phase7_cli.rs` test helpers. It has a pre-authored plan and a seeded `blocking-human`
checkpoint (D-01..D-05).

**Scope changes this discussion makes. Planning must carry them as plan tasks:**

- Promote 999.55 into ROADMAP as **Phase 49.1** under v3.0.0, with a pre-authored PLAN.md (D-01, D-04).
- Amend ROADMAP Phase 49 **success criterion 1**: the configuration step is satisfied by `050ef7f`, and
  the run still has to observe the persisted `State::base_branch` (D-09).
- Record 999.55's backlog entry as promoted.

**Deliberately not in this phase:** 999.123 (CI trigger de-duplication), making DevFlow merge into
`main`, and any DevFlow defect the run surfaces that does not block the chain (D-12).

**Evidence labels.** *Verified* = checked in source or by a command on 2026-09-24 against
`feature/phase-49` at `e9c6de3` (file:line given). *Reasoned* = derived from reading source, not
reproduced. Line numbers drift, so re-check before citing them in a plan.

**Provenance labels.** *Operator* = chosen by the operator during discussion. *Claude (remit)* = decided
by Claude within its own remit and stated during discussion. Do not treat a Claude (remit) entry as an
operator mandate.

</domain>

<decisions>
## Implementation Decisions

### Run workload

- **D-01 (Operator): The run drives a real backlog item, not a synthetic phase.** The operator asked for
  "the original CI flake bug", with 999.144 as the fallback. That resolved to **999.55**
  (`phase7_cli::wait_for` fixed budget times out under load). It descends from 999.23, recurred
  2026-07-28 and 2026-09-19, and 999.123 records it as the flake the duplicate CI runs keep triggering.
  - *Verified:* six fixed-budget helpers in `crates/devflow-cli/tests/phase7_cli.rs`: `wait_for` (`:200`)
    and `wait_for_pid` (`:215`) at `200 × 25ms` = 5s; `wait_for_state_cleared` (`:233`),
    `wait_for_stopped` (`:246`), `wait_for_settled` (`:1302`) and `wait_for_gate` (`:1314`) at
    `400 × 25ms` = 10s. They match the 999.55 entry's inventory exactly.
  - **999.55's acceptance is structural and deterministic.** All six helpers draw on one configurable
    budget (fix direction from the entry: e.g. `DEVFLOW_TEST_WAIT_SECS`, a longer default under `CI`),
    and a timeout panic reports the budget it actually used. **This run does not establish that the flake
    is gone.** That needs CI run history after the merge. Neither 49.1 nor 49 may claim it.

- **D-02 (Operator): Numbered as decimal Phase 49.1** under v3.0.0, not launched as `999.55`. Decimal
  launch has worked since the 999.97 hotfix. No 999.x number has ever been driven by `devflow start`,
  and the instrument should not add an untested variable.

- **D-03 (Operator): Seed a `gate="blocking-human"` checkpoint so DECN-03 is actually exercised.** The
  checkpoint asks for **999.55's CI default budget value**, a real human decision, not an artificial
  gate. That lets a "Followed" result show the comparison reasoning the evidence standard requires.
  - *Verified constraint:* the resume-with-auto-decide route fires only in the Code stage, and only when
    `verify::phase_has_blocking_human_checkpoint` matches the literal `gate="blocking-human"` in the
    plan. The Plan stage can never auto-approve a checkpoint (35.1 D-04, `scripts/unattended-drill.sh`
    header).

- **D-04 (Operator): Pre-author the 49.1 plan.** Commit it ahead of time, as `scripts/scratch-dogfood-repo.sh`
  does, so the target is fixed and the seeded checkpoint is guaranteed present. Letting Define/Plan
  generate the plan was rejected: planning variance would enter the measurement and the checkpoint
  could not be guaranteed.
  - **Research must establish:** what Define and Plan do, unattended, with an existing CONTEXT.md and
    PLAN.md (skip, overwrite or re-plan), and whether that preserves the seeded checkpoint. 999.59
    (Define attempting an interview it cannot conduct) is prior art.

- **D-05 (Claude, remit): The 49.1 artifacts must be on the base branch before launch.** DevFlow forks
  the 49.1 worktree from `workspace/denniyahh`, so the `### Phase 49.1:` ROADMAP heading and the
  pre-authored plan have to be committed there first. Committing them only on `feature/phase-49` would
  fail the reachability guard. Sequencing is the planner's call. The constraint is not.

### Ship target and blast radius

- **D-06 (Operator): The two base branches are intentionally different.**
  - `devflow.toml` `base_branch = "workspace/denniyahh"` means *the phase worktree forks from, and
    finished phase work is resynced back into, the personal workspace branch.* It stays.
  - GSD's own base (`gsd-tools query git.base-branch` → `main`, from `origin/HEAD`; `.planning/config.json`
    sets no `git.base_branch`) is the **ship / PR target**. It is not duplicated into `devflow.toml`.
  - The operator's reasoning: no reason could be found for the two to be the same, so they stay
    different.
  - *Verified:* `validate_base_branch` refuses `main` by design (`crates/devflow-core/src/config.rs:351`,
    test `validate_base_branch_refuses_main`). `base_branch` is one value for both fork and merge-back
    (45-01 D-01, `config.rs` `DevflowConfig::base_branch` doc; a split key was considered and rejected).
    `main` tracks 0 `.planning/` files. The operator's first answer read "devflow.toml ship target was
    set incorrectly, shipping should happen to main". This review settled it as the split above; the
    file was not changed.

- **D-07 (Operator): Human Ship gate, no `--yes-ship`.** Everything through Validate runs unattended.
  The Ship gate blocks so the operator can see both targets before anything is pushed or merged.
  - *Verified consequences the operator was shown:* `/gsd-ship` pushes the branch and runs
    `gh pr create --base "${BASE_BRANCH}"` (`~/.claude/gsd-core/workflows/ship.md:370-374`). It **never
    merges the PR** (no `gh pr merge` in `ship.md`). A branch forked from `workspace/denniyahh` would
    show **2358 commits / 1263 files** against `origin/main` (measured at `e9c6de3`), so that PR is
    public on GitHub. DevFlow's Merge hook merges locally into `workspace/denniyahh`
    (`hooks.rs:180-235`, `hooks_after_ship` `:115-122`).
  - **If the operator rejects at the Ship gate,** the merge target is recorded as *proposed, not
    executed*, and criterion 2 stays unmet. That is an escalation for re-planning with the operator. It
    does not count as an attempt (D-11).
  - **Research must establish:** where the Ship gate sits relative to `/gsd-ship`'s push and PR creation
    and to the post-Ship Merge hook. The gate is only protective if it comes before the push.

- **D-08 (Operator): Post-Ship VersionBump / tag / changelog: observe, then undo.**
  - *Verified:* `version_bump` (`hooks.rs:294-353`) writes the new version into the version file,
    commits `chore: bump version to …`, and creates an **unsigned local tag** (`git.rs:250-253`,
    `tag.gpgSign=false`). The workspace is at `version = "2.12.0"` and the last release tag is `v2.12.0`.
    `BranchCleanup` deletes the **local** feature branch only (`git branch -d`).
  - The hooks run so the whole post-Ship chain is observed. The bump commit and tag are recorded as
    evidence. A plan task then deletes the local tag and reverts the bump and changelog commits before
    anything is pushed.
  - **Why undo:** the real release process uses signed tags, bumps the version in two places, and goes
    `develop` → `main`. A stray local `v2.13.0`-style tag collides with the next real cut, and could be
    pushed by mistake with `--tags`.

### Setup step (criterion 1)

- **D-09 (Operator): Criterion 1's configuration step is satisfied by `050ef7f`**
  (2026-09-24, `chore(workspace): declare the develop split and enforce it on push`). That commit added
  `devflow.toml` with `base_branch = "workspace/denniyahh"`, on `workspace/denniyahh` only (not on
  `develop`). ROADMAP criterion 1 is amended to cite it.
  - **Still required as run evidence:** the run's persisted `State::base_branch` reads
    `"workspace/denniyahh"` and its source is `ConfigFile`, not the `develop` default. That half is
    something the run produces, not setup.
  - *Verified:* the rest of `devflow.toml` needed no fix. It sets only `base_branch`; `yes_ship` is
    absent (defaults `false`); `review_angles`, `capture_retention` and `external_verify_enabled` are at
    their defaults.

### Failure policy

- **D-10 (Operator): Blocking defects are fixed in-phase and the run is repeated; non-blocking ones go
  to the backlog.** A DevFlow defect that stops the chain before it reaches the remaining observations
  gets a gap-closure plan inside Phase 49, then a full re-run. Any defect that does not block the chain
  is filed as a 999.x entry and left unfixed here.
- **D-11 (Operator): Cap of 3 full live attempts** (the initial run plus two re-runs after in-phase
  fixes). A third failure stops the work and goes back to the operator for re-planning.
  **Validate loop-backs are data, not failures.** Only a run that ends without reaching Ship counts as
  an attempt. If the cause is the 49.1 task rather than DevFlow, the fix goes into the pre-authored
  49.1 plan, not into DevFlow code.
- **D-12 (Claude, remit, follows from criterion 3): Every attempt rebuilds, and both arms re-run on the
  new binary.** A fix changes the binary, and criterion 3 requires the positive and negative-control
  arms to come from the same build. The binary's identity (git SHA plus a checksum) is recorded with
  each arm. Rebuild before every launch after any commit (memory: dogfood rebuild-before-revalidate).

### Claude's Discretion

- The evidence-capture layout: event log, persisted state snapshots per stage, session captures, gate
  ledger and binary identity. It must be sufficient for 47-PHASE49-OBSERVATION.md's Followed / Not
  followed / Void classification. Record which turn carried which rule: `CODE_STAGE_POLICY` in turn 1,
  `checkpoint_auto_decide_prompt` in the resumed turn.
- The SURV-01 detection method, e.g. lock-wait and `advance_failed` events, or instrumented save
  counts. It is recorded as an observation either way, and "no interleaving observed" must not be
  written up as confirming anything.
- Where the negative-control scratch repo lives (`scripts/scratch-dogfood-repo.sh` is the existing
  generator).
- How the live run is launched and supervised. **Constraint:** it must be owned outside any agent
  worktree or subagent turn (memory: long probes live outside agent worktrees; a worktree subagent
  cannot own a run that outlives its turn).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and carried questions
- `.planning/ROADMAP.md` § "Phase 49: Live Unattended Run" covers the five success criteria. Criterion 1
  is amended per D-09.
- `.planning/REQUIREMENTS.md` VERIFY-01 (`:107-116`) holds the requirement. Cross-phase resolution notes
  (`:213-223`) cover DECN-03 and SURV-01.
- `.planning/phases/47-unattended-decision-policy-consistency/47-PHASE49-OBSERVATION.md` is DECN-03's
  evidence standard (Followed / Not followed / Void), its claude-only scope limit, and the
  position-effect recording requirement.
- `.planning/phases/48-survivable-state-writes-and-honest-gate-recovery/48-CONTEXT.md` D-01..D-05 cover
  the writer rule and lock identity, which the SURV-01 observation reads against.

### Workload (Phase 49.1)
- `.planning/ROADMAP.md` § "Phase 999.55" has the defect, the fix direction, the six helpers, and the
  2026-09-19 recurrence.
- `.planning/ROADMAP.md` § "Phase 999.123" explains why the flake recurs (duplicate CI runs). It is
  context only and not in scope.
- `crates/devflow-cli/tests/phase7_cli.rs` `:200-255`, `:1302-1325` are the six helpers.
- `scripts/scratch-dogfood-repo.sh` is the pre-authored-plan pattern and the negative-control scaffold.
- `scripts/unattended-drill.sh` header explains why Plan can never auto-approve and what a checkpoint
  drill does and does not establish.

### Base branch, Ship and post-Ship hooks
- `devflow.toml` is the configured base (D-06, D-09).
- `crates/devflow-core/src/config.rs` `DevflowConfig::base_branch` doc, `validate_base_branch` (`:341`),
  `base_branch()` (`:384`).
- `crates/devflow-cli/src/preflight.rs` `ConditionState` (`:922`) and `unattended_config_condition`
  (`:990`) define criterion 2's `Holds`.
- `crates/devflow-core/src/hooks.rs` `hooks_after_ship` (`:115`), `merge_feature` (`:180`),
  `version_bump` (`:294`), and the test `merge_feature_targets_the_configured_base_not_the_default`
  (`:433`).
- `~/.claude/gsd-core/workflows/ship.md` covers `BASE_BRANCH` resolution (`:46`), the push
  (`:194-199`) and `create_pr` (`:362-377`).
- `crates/devflow-cli/src/main.rs` `start` flags: `--mode`, `--until`, `--yes-ship` (`:50-95`).

### Process
- `.planning/user/DEV-SETUP-CHECKLIST.md` covers `scripts/phase-worktree.sh` and the worktree guard.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `scripts/scratch-dogfood-repo.sh`: builds an unconfigured scratch repo with a pre-written plan. This
  is the negative-control arm's fixture (criterion 3).
- `scripts/unattended-drill.sh`: an established two-arm live drill with a negative control. It is the
  pattern for asserting that the arms disagree.
- `devflow_core::events` (`merge_result`, `checkpoint_auto_decided`, `advance_failed`): the run's
  primary evidence stream.

### Established Patterns
- **Negative controls must fail for the right reason:** assert on the intended refusal text, not the
  exit code (memory: control-must-fail-for-the-right-reason).
- **The event alone is not proof.** `checkpoint_auto_decided` is emitted *before* the spawn, so a failed
  spawn still records it (47-PHASE49-OBSERVATION.md). Classify as Void unless the resumed session
  capture shows the resolution.

### Integration Points
- **Validate runs the full suite during a live run.** 999.141 records that E2E tests write the real
  `~/.cache/devflow/roots` and leak monitors. Research must establish whether that can interfere with
  the live run's own state or root registry, since `phase7_cli.rs` spawns real monitors.
- **Preflight in auto mode with a `blocking-human` checkpoint present.** 999.125 and 999.126 were fixed
  in Phase 48 (CHKPT-01/02). Research must confirm the seeded checkpoint neither makes auto-mode
  preflight refuse the launch nor escapes the Code-preflight scan.

</code_context>

<specifics>
## Specific Ideas

- Operator: "the original CI flake bug … that's continued to plague us this entire time." The live
  run's workload was chosen for its value, not only its cost.
- Operator, on the base branches: the personal workspace branch "should get resynced with a phase
  worktree once development is completed". That is `devflow.toml`'s `base_branch` purpose, distinct
  from GSD's ship target.

</specifics>

<deferred>
## Deferred Ideas

- **999.123: de-duplicate CI triggers.** Not folded into 49.1. It edits the workflows whose required
  contexts gate `develop`, and its own entry requires proving it against the live rulesets on a
  throwaway PR. Stays in the backlog.
- **Making DevFlow merge phase work into `main`.** Raised and not pursued: it reverses 45-01 D-01 and the
  deliberate `main` refusal. The operator settled on keeping the two bases distinct (D-06). If it is
  ever wanted, it is its own phase.
- **Measuring 999.55's effect on the flake rate.** Needs CI run history after the merge, which is
  outside this run. It is a candidate follow-up, not a Phase 49 claim.

</deferred>

---

*Phase: 49-live-unattended-run-the-milestone-s-instrument*
*Context gathered: 2026-09-24*
