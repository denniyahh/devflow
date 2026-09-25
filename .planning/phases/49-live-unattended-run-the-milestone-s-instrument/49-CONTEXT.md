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
- Amend ROADMAP Phase 49 **success criterion 1**, and the matching "Carries a setup step" sentence in
  REQUIREMENTS.md VERIFY-01: the configuration step is satisfied by `050ef7f`, and the run still has to
  observe the persisted `State::base_branch` (D-09). **This is the first plan task.** Until it lands,
  CONTEXT and ROADMAP contradict each other.
- Amend ROADMAP Phase 49 **success criterion 2**: "unattended stage progression" becomes unattended
  progression *between the two operator-approved preflight gates and the Ship gate*, each named in the
  evidence (D-03, D-07).
- Record 999.55's backlog entry as promoted.

**Adversarial review, 2026-09-24.** Three external lanes (codex `gpt-5.6-terra` high; DeepSeek
`deepseek-flash` via `pi`; agy `gemini-3.8-flash-high`) reviewed the first draft at `0531ef5`. All three
completed, with 35, 59 and 43 `file:line` citations. **All three independently found the same four
defects**, and the orchestrator checked each in source: D-03's seeded checkpoint makes auto-mode
preflight refuse; D-07's Ship gate sits after the push and PR; the Merge hook mutates the launch
checkout; and the negative-control fixture writes the file whose absence it is supposed to prove. D-03,
D-07 and D-12 were re-decided or extended, and D-13..D-16 were added. Raw output is in
`~/.cache/devflow-review/phase49-context-0531ef5/`.

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
  - **The acceptance this phase sets for 49.1 is structural and deterministic.** 999.55's entry has
    a fix direction but no acceptance section (`Plans: - [ ] TBD`), so the pre-authored 49.1 plan
    authors it: all six helpers draw on one configurable budget (fix direction: e.g.
    `DEVFLOW_TEST_WAIT_SECS`, a longer default under `CI`), and a timeout panic reports the budget it
    actually used. **This run does not establish that the flake
    is gone.** That needs CI run history after the merge. Neither 49.1 nor 49 may claim it.

- **D-02 (Operator): Numbered as decimal Phase 49.1** under v3.0.0, not launched as `999.55`. Decimal
  launch has worked since the 999.97 hotfix. No 999.x number has ever been driven by `devflow start`,
  and the instrument should not add an untested variable.

- **D-03 [REVISED after review] (Operator): Seed a `gate="blocking-human"` checkpoint, and the operator
  approves the Define and Code preflight gates.** The checkpoint asks for **999.55's CI default budget
  value**, a real human decision, not an artificial gate. That lets a "Followed" result show the
  comparison reasoning the evidence standard requires.
  - *Verified (all three review lanes, re-checked):* in `Mode::Auto`, any plan declaring `blocking-human`
    makes the unattended-launch condition `DoesNotHold` ("a plan declares a `blocking-human` gate …
    which no mode auto-approves", `crates/devflow-cli/src/preflight.rs:1076-1080`). The check applies at
    Define and Code (`:971-974`). The plan is on the base before launch (D-05), so **the run parks at a
    human preflight gate at Define and again at Code**. `--yes-ship` does not bypass it.
  - *Verified, and why this is the only route:* the agent's auto-decide resume fires only for checkpoints
    in the set recorded when a human approved the **Code** preflight gate
    (`record_checkpoint_set_for_code_evaluation`, `preflight.rs:1432-1434`). Anything outside that set
    parks at a re-scan gate (`pipeline_launch.rs` `checkpoint_approval.unapproved`, Phase 48 CHKPT-02).
    **No fully unattended run can reach DECN-03's path.** Operator approval at preflight is the
    product's designed route, not a workaround.
  - **Rejected:** pre-writing the gate responses before launch (the tests' technique). It is still a
    human approval, it would read as unattended when it is not, and it sidesteps a deliberate gate.
    Dropping the checkpoint was also rejected, since DECN-03 would stay open past the milestone.
  - *Corrected claim:* the resume route has **no `stage == Code` predicate**
    (`pipeline_launch.rs:1775-1779` checks agent kind, the literal `gate="blocking-human"` match at
    `verify.rs:131`, and a capture). Code is the practical producer because it is the only stage whose
    preflight records an approved set, but that is not a stage-gated invariant. The Plan stage can
    never auto-approve a checkpoint (35.1 D-04, `scripts/unattended-drill.sh` header).

- **D-04 (Operator): Pre-author the 49.1 plan.** Commit it ahead of time, as `scripts/scratch-dogfood-repo.sh`
  does, so the target is fixed and the seeded checkpoint is guaranteed present. Letting Define/Plan
  generate the plan was rejected: planning variance would enter the measurement and the checkpoint
  could not be guaranteed.
  - *Verified (review):* a pre-authored plan survives. Define is an unconditional no-op in a DevFlow
    launch, whether or not CONTEXT.md exists (`define_stage_prompt`,
    `crates/devflow-core/src/prompt.rs:374-380`; the interview route was deleted, 999.59/D-14). Plan
    uses `idempotent_stage_prompt` (`:341`), which succeeds without rewriting an existing PLAN.md.

- **D-05 [CORRECTED] (Claude, remit): Two separate requirements on the base branch before launch.**
  1. The `### Phase 49.1:` ROADMAP heading on `workspace/denniyahh` satisfies the **reachability guard**.
     *Verified:* "Only the ROADMAP heading is load-bearing for the refusal (999.63)", and a missing phase
     directory alone never refuses (`crates/devflow-cli/src/preflight.rs:259-262`, `:294-297`).
  2. The pre-authored plan must also be on the base, but for a different reason: the 49.1 worktree
     forks from `workspace/denniyahh` and must **inherit** the plan and its seeded checkpoint. The
     first draft wrongly tied the plan to the reachability guard. A failure must be diagnosed against
     the right mechanism. Sequencing is the planner's call. Both constraints are fixed.

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

- **D-07 [REVISED after review] (Operator): Let `/gsd-ship` open its PR to `main`, observe it, and
  close it unmerged. The human Ship gate (no `--yes-ship`) guards the local steps only.**
  - *The first draft's premise was false, and Claude asserted it without checking.* The operator was
    told the gate lets them "see both targets before anything is pushed". **Verified:** the Ship gate
    fires only after the Ship agent exits. `pipeline_launch.rs:1746` routes `Stage::Ship` to
    `handle_ship_outcome`, whose doc reads "Decide what happens after the Ship stage completes"
    (`crates/devflow-cli/src/pipeline_outcomes.rs:828-850`). The Ship prompt runs `/gsd-ship`
    (`prompt.rs:251`), which pushes (`ship.md:190-202`) and runs
    `gh pr create --base "${BASE_BRANCH}"` (`ship.md:370-374`) before that. The gate therefore protects
    only `hooks_after_ship`: Merge, VersionBump, ChangelogAppend and BranchCleanup (`hooks.rs:115-122`).
  - **The PR against `main` is the observed GSD ship target.** It is evidence for criterion 2.
    `/gsd-ship` never merges it (no `gh pr merge` in `ship.md`). A plan task closes it unmerged after
    the evidence is captured.
  - *Verified exposure:* the repo is **public**, but `workspace/denniyahh` is already pushed to
    `origin`, so the PR exposes no new content. It would show about **2358 commits / 1263 files** against
    `origin/main` (measured at `e9c6de3`). The cost is one noisy PR plus the CI runs it triggers.
  - **Rejected:** halting with `--until validate` and resuming into Ship. It adds a human boundary only
    to avoid a PR whose content is already public.
  - **If the operator rejects at the Ship gate,** the DevFlow merge target is recorded as *proposed, not
    executed*, and criterion 2 stays unmet. That is an escalation for re-planning with the operator. It
    does not count as an attempt (D-11).

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
  - **Retry mechanics (review finding, verified):** a second attempt finds the first attempt's worktree
    and branch. `start` refuses with "worktree already exists … use --force", and `--force` removes the
    worktree and **force-deletes** `feature/phase-49.1` (`crates/devflow-cli/src/parallel.rs:24-44`).
    Before any `--force` relaunch, the plan preserves the previous attempt: its branch under an archive
    ref and its `.devflow/` evidence copied out, so an attempt's evidence is never destroyed by the
    next one.

### Run mechanics (added after review)

- **D-13 (Claude, remit): The launch checkout must be clean, and it is restored afterwards.**
  *Verified:* the Merge hook runs `git checkout <base>` then `git merge --no-ff <feature>`
  (`crates/devflow-core/src/git.rs:185-190`) in `ctx.project_root`, the **launch checkout** (the
  operator's main checkout), not the phase worktree. `run_checkout_hooks` serialises it behind a
  project lock (`pipeline_outcomes.rs:1019-1030`). The plan must:
  - assert and record a clean main checkout on `workspace/denniyahh` immediately before launch and
    again before approving the Ship gate;
  - record the branch and HEAD before and after the post-Ship hooks;
  - state the recovery for a local merge failure **after** the PR was already opened: the PR is
    closed, and the checkout is reset to its recorded pre-merge HEAD.

- **D-14 (Claude, remit): The negative-control fixture must be built to fail, and asserted on text.**
  *Verified (all three lanes):* `scripts/scratch-dogfood-repo.sh` **writes** `.planning/config.json`
  (`:187-191`, `{"workflow": {}}`), so used unmodified it cannot produce criterion 3's "refuses on the
  missing `.planning/config.json`". The negative arm removes that file after scaffolding and commits the
  removal. It then asserts on the refusal **text** ("no `.planning/config.json` under the launch root",
  `preflight.rs` `unattended_config_condition`), not the exit code. It runs with `DEVFLOW_BASE_BRANCH`
  scrubbed from the environment, so the persisted `"base_branch": "develop"` comes from the default arm
  and not from a leaked variable. The positive arm asserts the opposite: config present, source
  `ConfigFile`.

- **D-15 (Claude, remit): Pin the agent.** Launch with an explicit `--agent claude`. The default is
  claude (`crates/devflow-cli/src/main.rs:53`), but DECN-03 closes for claude only. The auto-decide
  route checks `state.agent == AgentKind::Claude` (`pipeline_launch.rs:1777`), and preflight C2 also
  admits antigravity, which would pass preflight and never reach the path. Record the claude CLI
  version with each arm.

- **D-16 (Claude, remit): Define the fork-point measurement.** `State` persists `base_branch` but no
  fork SHA. "Observed fork point" means three values: the base ref's SHA read immediately before
  `start`; the 49.1 worktree's HEAD immediately after creation, which must equal it; and
  `git merge-base` of the feature branch and the base at Ship.

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
- How the operator learns a gate is waiting. The run has three human gates: Define preflight, Code
  preflight and Ship. *Verified:* gate notification is a no-op unless `DEVFLOW_GATE_NOTIFY_CMD` is set
  (`crates/devflow-core/src/gates.rs:388-394`), and the production gate timeout defaults to 3 days
  (`crates/devflow-cli/src/config_parse.rs`, `DEVFLOW_GATE_TIMEOUT_SECS`). The plan must name the
  mechanism and the exact approve and reject commands. It must not leave them implicit.
- The exact undo recipe for D-08 (new revert commits versus a reset to the recorded pre-hook HEAD). It
  must keep the Merge's resync into `workspace/denniyahh` and remove only the bump, changelog and tag.

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
  `~/.cache/devflow/roots` and leak monitors. *Verified (codex):* `phase7_cli` spawns child DevFlow
  processes without setting `HOME`, `XDG_CACHE_HOME` or `DEVFLOW_CACHE_DIR`
  (`crates/devflow-cli/tests/phase7_cli.rs:130`), and cache resolution falls through to
  `HOME/.cache/devflow` (`crates/devflow-core/src/registry.rs:46`). The registry is keyed per
  `(project_root, phase)`, so this pollutes cross-root registry and monitor observations rather than
  the live phase's state file (reasoned, not reproduced). Research must pick an isolation or
  cleanup-and-verify protocol before SURV-01 or monitor observations are recorded. An env override on
  the launch would also move the live run's own registry, so it is not a free fix.
- **Preflight in auto mode with a `blocking-human` checkpoint present.** This was resolved by the
  review. It **does** refuse, by design, and the operator approves both preflight gates (D-03).

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
