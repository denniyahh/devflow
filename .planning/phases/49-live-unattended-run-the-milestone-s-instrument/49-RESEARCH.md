# Phase 49: Live Unattended Run — The Milestone's Instrument - Research

**Researched:** 2026-09-24
**Domain:** Operating one live DevFlow `--mode auto` run on its own repository: launch and supervision, gates, evidence capture, a negative-control arm, retry, and post-Ship undo
**Confidence:** HIGH for in-repo mechanics (read in source and, where it mattered, reproduced by a probe). MEDIUM for agent behaviour inside the run. LOW for anything about how the model behaves.
**Worktree:** `/var/home/denniyahh/Github/devflow/.worktrees/phase-49` on `feature/phase-49` at `b9dcfa9`. At research time `workspace/denniyahh`, `origin/workspace/denniyahh` and `feature/phase-49` all pointed at `b9dcfa9`. **Every line number below was re-read at `b9dcfa9`.**

**Label key.** *Verified* means read in source this session or reproduced by a command whose output is quoted, with file:line. *Reasoned* means derived from source and not reproduced. Provenance tags follow the GSD convention: `[VERIFIED: …]`, `[CITED: …]`, `[ASSUMED]`.

## Summary

**Headline: as CONTEXT currently stands, the Ship stage cannot reach its push or its PR in this repository. That undoes D-07's premise, and it means the DevFlow Merge hook, which is criterion 2's headline observation, is never reached.** Three independent blockers were verified this session. None of the three external review lanes found them.

1. `/gsd-ship`'s `ship:pre` capability gates halt the stage before the push.
   - The broken-windows gate blocks: `.planning/WINDOWS.md` has `open_count: 4`.
   - The security gate blocks: no `49.1-SECURITY.md` will exist, because no DevFlow stage runs `/gsd-secure-phase`.
2. The repo's own `pre-push` hook refuses the push of `feature/phase-49.1` twice over:
   - once for the `.planning/` commits the run creates;
   - once because `.workspace-divergence` does not declare the change to `crates/devflow-cli/tests/phase7_cli.rs`.
   Both refusals were reproduced in a scratch clone, and each has a control that separates it from the other.

A failed Ship agent goes to `handle_ship_failure` and then to a `[never-silent]` Ship **failure** gate, whose only responses are retry and abort. **No gate response reaches `finish_workflow`**, so the Merge hook never runs.

There is also a **fourth human gate** that CONTEXT does not list. At `b9dcfa9` the version-bump range is classified **MAJOR**, because two `refactor(48)!:` commits land after `v2.12.0`. So the Ship-stage preflight refuses (D-09 major-bump check) and parks the run *before* the Ship agent starts. The post-Ship `VersionBump` would then tag **`v3.0.0`**, the name of this milestone's own real release. A scratch probe binary reproduced this: `bump=Ok(Major)`, `compute_version=Ok(Version { major: 3, … })`. Its controls behave as expected: `v2.12.0` gives `Ok(None)`, and the parent of the first `!` commit gives `Ok(Minor)`.

The good news: that major-bump preflight gate is a human pause that already sits *before* `/gsd-ship` runs. Any Ship accommodation the operator chooses can be applied there, without adding a boundary the run did not already have.

The rest of the phase is well supported by source:
- The live run must be owned by a `systemd-run --user` transient unit with `RemainAfterExit=yes` and `KillMode=process`. A probe showed that the default transient-service kill policy killed a detached child as soon as the main process exited.
- `devflow start` itself blocks in the foreground at the Define preflight gate for up to 3 days.
- Every `DEVFLOW_*` variable on the launch environment is inherited by the agent and by every test child it spawns. That includes the Validate stage's full test suite, which is 999.141's concern.
- So the launch environment stays minimal. Notification comes from an out-of-band watcher of `.devflow/gates/`, not from `DEVFLOW_GATE_NOTIFY_CMD`.
- A worry that the phase-worktree guard would refuse the agent's `crates/` commits was probed: the guard refuses before GSD's `state.begin-phase 49.1` runs and passes after it, and `execute-phase.md` runs that step before any wave.

**Primary recommendation:** before any plan task launches anything, the operator has to decide how the Ship stage will be allowed to push and open its PR (options are in **Needs operator decision**). Plan everything else as recommended here: a pinned binary at an immutable path, systemd-owned launch and resume units, an out-of-band gate watcher, an inotify writer-attribution watcher for SURV-01, negative arm first, snapshots at every gate, and a guarded reset-based undo that deletes the local `v3.0.0` tag.

## Needs operator decision (research surfaced these; planner cannot settle them alone)

**ND-1: How does the Ship stage get through `/gsd-ship` in this repo?** This blocks criterion 2 and contradicts D-07's premise.
- *Verified evidence:*
  - `gsd_run loop render-hooks ship:pre --raw` lists two active blocking gates, `broken-windows` and `security`, both `"onError": "halt"`.
  - `gsd_run windows status --raw` reports `"open_count": 4`.
  - `config-get workflow.security_enforcement` returns `true`.
  - ship.md:113-127 says that with no SECURITY.md: "`block: true`, `SECURITY_SHIP_GATE_NO_REVIEW`"; with open windows: "`block: true`, `WINDOWS_SHIP_GATE_OPEN`"; and "HALT the ship — do NOT push, do NOT create a PR".
  - The pre-push refusals were reproduced (see Pitfall 1).
- **Option A (record the refusal).** Change nothing. Ship fails, the Merge hook is never observed, and criterion 2 stays unmet. That goes back to the operator as a re-plan.
- **Option B (recorded accommodations at the Ship-preflight major-bump gate). Recommended.** The run is already parked by a human gate before the Ship agent starts. At that gate:
  1. Run `/gsd-secure-phase 49.1` in `.worktrees/phase-49.1` and commit the SECURITY.md it writes.
  2. Deal with WINDOWS.md in one of two ways: resolve or waive the 4 open entries (`gsd_run windows fixed|waive`), or set `workflow.windows_enforce: false` in the **49.1 worktree's** `.planning/config.json` only.
  3. Give that worktree its own `core.hooksPath`, pointing at a copy of `scripts/hooks` whose `pre-push` drops the branch-hygiene and divergence refusals. This needs `git config extensions.worktreeConfig true` on the repo. It was probed and works in isolation (Pitfall 1).
  Then approve. The cost is three visible human interventions and a public push of `feature/phase-49.1`. The branch content is already public through `workspace/denniyahh`, apart from what the run itself adds.
- **Option C (lower the GSD gates pre-launch).** Set `security_enforcement: false` and `windows_enforce: false` on `workspace/denniyahh` before launch, and still apply the hooks override from B.3 at the Define gate. That is fewer mid-run touches, but it weakens two GSD gates for everything on the branch until they are reverted.
- The major-bump gate itself is unavoidable. D-09 says it "never ships unattended", and `--yes-ship` does not bypass it.

**ND-2: Do you accept that post-Ship `VersionBump` will create a local `v3.0.0` tag and a `3.0.0` bump commit, then have them undone?** D-08 already says "observe, then undo". The new fact is that the tag name is `v3.0.0`, the milestone's real tag. It is not a `v2.13.0`-style name.
- A safety net exists: the pre-push signing policy refuses to push an unsigned tag, because `devflow.releasesigningkey` is set in `.git/config`.
- The undo must still delete the tag. Otherwise the real v3.0.0 cut collides with it.

**ND-3: Does Phase 49.1 get a REQUIREMENTS.md ID?** CONTEXT does not say. The decimal-phase precedent carries one: 35.1 has `**Requirements**: HARDEN-07`, and its plan frontmatter has `requirements: [HARDEN-07]`. The planner should not invent one without an operator answer (memory: no invented constraints).

## Operator answers (2026-09-25, during /gsd-plan-phase 49)

Provenance: **Operator**. These settle ND-1..ND-3 above and bind the planner the same way CONTEXT decisions do.

- **ND-1 → Option B.** Keep the GSD Ship gates on. While the run is parked at the Ship-preflight major-bump gate, apply three recorded accommodations scoped to the 49.1 worktree: (1) `/gsd-secure-phase 49.1` and commit its SECURITY.md; (2) resolve/waive the open WINDOWS.md entries or set `workflow.windows_enforce: false` in the 49.1 worktree's `.planning/config.json` only; (3) a per-worktree `core.hooksPath` (via `extensions.worktreeConfig`) pointing at a hooks copy without the branch-hygiene/divergence refusals. Then approve. Each accommodation is recorded as evidence and reverted after the run. Option C (lowering gates on `workspace/denniyahh`) and Option A were not chosen.
- **Ship container check → Omit from the temporary hooks copy (Operator, 2026-09-25).** The copied hook keeps release-signing and all non-container checks, but omits only the 2–9 minute container stanza so the Ship agent is not consumed by it. Before the Ship-preflight approval, the 49.1 worktree runs the pinned `scripts/check-in-container.sh all` directly; its exit status and final `check.sh: all OK` marker are captured. The omission is recorded in the accommodation ledger and reverted with the other worktree-only changes.
- **ND-2 → Accept.** VersionBump runs; the local unsigned `v3.0.0` tag and the `3.0.0` bump commit are recorded as evidence, then the D-08 undo deletes the tag and reverts bump + changelog, and the plan asserts the tag is gone afterward.
- **ND-3 → Yes.** Phase 49.1 gets a new v3.0.0 REQUIREMENTS.md ID (following the 35.1/HARDEN-07 precedent), with traceability row and coverage counts updated, and the 49.1 plan's `requirements:` frontmatter carries it. The exact ID and wording are the planner's to author; it must describe the structural, deterministic acceptance in CONTEXT D-01 and must not claim the flake is gone.

**Still open (not decided — surface, do not settle):** how 49.1's code reaches `develop` after the run (so `workspace/denniyahh` can be pushed past the divergence check), and whether GSD's phase-complete step moves STATE.md unexpectedly when the merge lands on `workspace/denniyahh`.

<user_constraints>
## User Constraints (from CONTEXT.md)

The decisions below are copied verbatim from `49-CONTEXT.md` `<decisions>`, which also contains `### Claude's Discretion`. The Deferred Ideas section is copied verbatim from `<deferred>`.

### Locked Decisions (verbatim, includes Claude's Discretion)

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

### Deferred Ideas (OUT OF SCOPE, verbatim)

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
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| VERIFY-01 | A real `devflow start --mode auto` run recorded end to end against a configured base (fork point, `unattended_config_condition = Holds`, unattended progression, Ship merge target), with a negative control that an unconfigured repo still forks from `develop`. REQUIREMENTS.md:107-116. The "Carries a setup step" sentence at :116 is amended per D-09. | Criterion 1: persisted `base_branch` plus the `ConfigFile` source, observable only through the stdout note of `start` (Pattern 4). Criterion 2: fork-point triple (D-16), the Define-stage `[holds]` line, the gate ledger, and `merge_result`. **This criterion is blocked by ND-1.** Criterion 3: negative-arm driver (Pattern 6). Criterion 4: DECN-03 turn map (Pattern 5). Criterion 5: SURV-01 inotify watcher (Pattern 7). |
| DECN-03 (carried arm) | Which instruction the agent follows when `CODE_STAGE_POLICY` is in turn 1 and `checkpoint_auto_decide_prompt` is in the resumed turn. REQUIREMENTS.md:213-223. | Pattern 5 and Pitfall 7. |
| SURV-01 (carried arm) | Whether the live run produced any concurrent state write. REQUIREMENTS.md:220-223. | Pattern 7. |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- **Test commands:**
  - `devflow-core` library: `cargo test -p devflow-core --lib`.
  - The CLI is **binary-only**, package `devflow`: `cargo test -p devflow --bin devflow`. `cargo test -p devflow --lib` verifies nothing.
  - Integration tests: `cargo test -p devflow --test <file>`.
  - Full suite: `scripts/check.sh all`. Container parity: `scripts/check-in-container.sh`.
- Phase work stays in `.worktrees/phase-N`. **Never run concurrent git operations on the main checkout while an executor holds the working tree.** For this phase that also covers DevFlow's Merge hook, which runs *in* the main checkout (D-13).
- When CI workflows, hooks, devcontainers or tooling change, update `.planning/user/DEV-SETUP-CHECKLIST.md` in the same commit. This matters for the 49.1 plan if it touches `scripts/check-in-container.sh` or `.github/workflows/devcontainer.yml` (Pitfall 9).
- **Global operator rules that bind this phase:**
  - `rg`, not grep.
  - The tool shell is bash; confirm it with `echo $0`, which printed `/usr/bin/bash` this session.
  - Never suggest `dnf` (immutable Kinoite).
  - Bound long commands with timeouts.
  - Every negative control asserts on text, not exit code.
  - `cargo test --exact` with a bare test name matches nothing and exits 0. Module-qualify the name and assert on `1 passed`.
  - `rg -c` prints nothing (not `0`) on zero matches.
  - A manual fix to a GSD-generated artifact goes into `~/.config/agents/gsd-manual-fix-incidents.md`.
- **Workspace/develop split:** workspace-only helper files belong under `.planning/`, which the manifest already covers. A new top-level `scripts/*.sh` on the workspace needs an `only` line in `.workspace-divergence` plus mode `100755`, because `core.fileMode=false`. **Put all phase-49 tooling under `.planning/phases/49-…/tools/`.** `.workspace-divergence` confirms the built-in list: ".planning/, graphify-out/, agent-tool paths, devflow.toml and this file are built in and need no line."

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Fork, preflight, stage machine, gates, merge hook | DevFlow binary (pinned build) | git (main checkout + 49.1 worktree) | Everything under test. It must be one immutable binary for both arms (D-12) |
| Run ownership across days | systemd user manager (transient unit) | — | It must outlive every agent turn and session (memory: long probes outside agent worktrees). Only systemd gives a durable cgroup plus a process view |
| Operator approvals | Operator shell running `<pinned-bin> gate approve/reject` | Out-of-band gate watcher (notify-send + ledger) | A gate response is a file write, so a CLI call returns immediately |
| Evidence capture | Orchestrator snapshot script (read-only) | inotify watcher unit (SURV-01) | Read-only readers cannot perturb the run's state writes |
| Negative control | Scratch repo outside the checkout plus the same pinned binary | — | Criterion 3 needs the same build and a different config |
| 49.1 workload content | Pre-authored PLAN on `workspace/denniyahh` | DevFlow's Code agent (GSD execute-phase) | D-04, D-05 |
| Post-run repo hygiene (undo, archive refs) | Orchestrator, guarded git in the main checkout | — | D-08, D-12 |

## Standard Stack

No new packages. Everything below already exists on the host (see Environment Availability).

| Tool | Version (probed) | Purpose | Why |
|------|------------------|---------|-----|
| DevFlow built from the phase tree | `2.12.0` source at the launch SHA | The instrument | **Do NOT use `~/.cargo/bin/devflow`, which is `devflow 2.11.0`.** It predates Phases 46-48 |
| `systemd-run --user` | systemd 259 (259.9-1.fc44), `Linger=yes` | Own `start`, `resume` and the watchers | Probed: survives detached children with the right properties |
| `claude` | `2.1.282 (Claude Code)` | Agent (D-15 pins `--agent claude`) | Record per arm |
| git | 2.55.0 | Per-worktree config, archive refs | `extensions.worktreeConfig` probed |
| `jq` | /usr/bin/jq | Evidence assertions | — |
| python3 | 3.14.7 (linuxbrew) | ctypes inotify watcher (SURV-01) | `inotifywait` is **absent**, and dnf is off-limits |
| `notify-send` | /usr/bin/notify-send | Gate notification | The systemd user env carries `DBUS_SESSION_BUS_ADDRESS` |
| `gh` | linuxbrew, logged in (keyring, also works inside a unit) | Ship PR observation and closing it | — |

## Package Legitimacy Audit

Not applicable. This phase installs no external packages. The 49.1 workload is test-only Rust inside an existing crate, and adds no dependency.

## Architecture Patterns

### System Architecture Diagram

```
operator shell ──(1) build+pin──► ~/.cache/devflow-phase49/bin/devflow-<sha12>  (sha256 recorded)
      │
      ├─(2) NEGATIVE ARM: scratch repo (no config.json) ──► devflow start (unit) ──► Define preflight
      │        REFUSES "[DOES NOT HOLD] GSD config…" ──► snapshot state-01.json ──► gate reject "abort…"
      │
      └─(3) POSITIVE ARM: systemd-run --user … devflow start --phase 49.1 --agent claude --mode auto <main checkout>
               │  resolve base = workspace/denniyahh (devflow.toml)  → prints "note: base branch is …"
               │  ensure_base_ref_current → reachability (### Phase 49.1:) → fork .worktrees/phase-49.1
               │  enforce_build_staleness (self-dogfood: BLOCK if binary stale vs worktree HEAD)
               │  save_state (state-49.1.json, base_branch) → launch_stage(Define)
               ▼
        [GATE 1] Define preflight: C3 DOES NOT HOLD (blocking-human) ── start process BLOCKS here ──► operator approve
               ▼  (start exits; monitor chain lives on in the unit's cgroup)
        Define agent (no-op) → Plan agent (idempotent, pre-authored PLAN) → transition
               ▼
        [GATE 2] Code preflight: C3 DOES NOT HOLD ──► operator approve  ⇒ records checkpoint set (preflight.rs:1433)
               ▼
        Code turn 1: "/gsd-execute-phase 49.1 --auto" + CODE_STAGE_POLICY (stream-json)
               │  executor hits checkpoint:decision gate="blocking-human" → reports **Gate:** blocking-human
               ▼
        GateReview arm (pipeline_launch.rs:1777): claude ∧ declared ∧ reported-in-capture ∧ session ∧ ceiling
               │  set unchanged → emit checkpoint_auto_decided → resume: claude -p <auto-decide> --resume <sid>
               ▼
        Code continues → Validate (full test suite; loop-backs are data) → DocsUpdate hook (worktree)
               ▼
        [GATE 3] Ship preflight: MAJOR bump v2.12.0→v3.0.0 (D-09) ──► operator: ND-1 accommodations, then approve
               ▼
        Ship agent: /gsd-code-review 49.1 → /gsd-ship 49.1 → ship:pre gates → git push → gh pr create --base main
               ▼
        [GATE 4] "Ship complete — approve merge?" ──► approve
               ▼
        hooks_after_ship in MAIN CHECKOUT: Merge (--no-ff into workspace/denniyahh) → VersionBump (3.0.0 commit + v3.0.0 tag)
               → ChangelogAppend (commit) → BranchCleanup (fails soft: branch checked out in worktree) → clear_state
               ▼
        orchestrator: snapshot → D-08 undo (tag -d v3.0.0; reset --hard <merge>) → close PR unmerged → write-up
```

### Recommended evidence layout (outside the repo; curated copies committed later)

```
~/.cache/devflow-phase49/
├── bin/devflow-<sha12>          # immutable pinned binary per attempt (never overwritten)
├── bin/gate-watch.sh, surv-watch.py, notify.sh   # copies of the .planning tools actually run
└── attempt-<N>/
    ├── identity.txt             # git SHA, dirty flag, sha256sum, `devflow --version`, `claude --version`, git/systemd versions
    ├── pre-launch/              # base SHA, main-checkout status+HEAD, roots census, process census, gate list --all-roots
    ├── start.stdout / start.stderr   # the ONLY place the Define [holds]/[DOES NOT HOLD] report and the ConfigFile note appear
    ├── gates/<stage>-<ts>/      # copy of .devflow/gates/49.1-*, state-49.1.json, lock-49.1, events tail, worktree HEAD
    ├── captures/                # .devflow/phase-49.1-{stdout,stderr.log,prompt,monitor.log,exit} + history/phase-49.1/
    ├── sessions/                # ~/.claude/projects/-var-home-denniyahh-Github-devflow--worktrees-phase-49-1/<session>.jsonl
    ├── surv/                    # inotify writer log (JSONL) + overflow markers
    ├── ship/                    # pre/post hook HEADs, merge-base, merge_result event, PR url/number, tag target
    └── negctl/                  # scratch path, stdout, state-01.json snapshot, gate context, assertions
```

Curated, sanitised excerpts go to `.planning/phases/49-…/49-evidence/`. Session transcripts and raw captures can carry home paths and session IDs; see the Security Domain section.

### Pattern 1: Pinned binary per attempt (D-12)
**What:** build once from a clean tree, copy to an immutable path, and use that absolute path for *every* verb. That means `start`, `gate`, `status`, `stop`, `resume`, and the negative arm.

**Why (Verified):**
- `monitor.rs:330` spawns later monitors from `std::env::current_exe()`. Replacing the binary at the launch path mid-run changes the binary under a live run (48-CONTEXT D-03 names "a binary rebuilt mid-run").
- `build.rs:50-53` sets `DEVFLOW_BUILD_DIRTY` from `git status --porcelain`. **Untracked files count**, so commit or remove them before building.
- `workflow_started` records `"commit": env!("DEVFLOW_BUILD_COMMIT"), "dirty": env!("DEVFLOW_BUILD_DIRTY")` (commands.rs:722-736). That is the run's own identity record.

**Staleness constraint (Verified, staleness.rs:56-111, 430-449):**
- Against a self-dogfood workspace, a *Stale* verdict hard-blocks `start`.
- It is Stale when the build commit is an ancestor of, or has diverged from, the worktree HEAD, **and** `git diff <embedded> HEAD` touches `Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml`, `.cargo/config.toml`, or `crates/**.rs` / a crate manifest.
- *Ahead* (binary newer than base) only warns.
- Build from the exact SHA the 49.1 worktree will fork from: the `workspace/denniyahh` tip, which equals `feature/phase-49` after the fast-forward. The verdict is then `Fresh`.

### Pattern 2: Launch and supervision via systemd transient units
**Verified probe:**
```
default: child pid 1275480 DEAD; unit state: inactive
keep: child pid 1275522 ALIVE; unit state: active
```
The "keep" run used `-p RemainAfterExit=yes -p KillMode=process`.

**Other verified facts:**
- `devflow start` does not return until Define launches. `launch_stage` runs `run_preflight` first (pipeline_launch.rs:1297-1333). A refusal calls `run_gate` → `Gates::poll_response`, a blocking poll of up to `gate_timeout_secs()`. The default is `THREE_DAYS` (config_parse.rs:20-22).
- The PipeOwning monitor is spawned with `Stdio::null()` for all three streams (monitor.rs:445-447). It is not `setsid`, and it inherits the caller's cgroup, so a transient service with the default `KillMode` kills it.
- The systemd user environment contains **no** `CLAUDE*` or `DEVFLOW_*` variables. A negative-controlled count gave `PATH lines: 1`, `DEVFLOW lines: 0`. The operator's Claude Code shell carries `CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_CHILD_SESSION` and others. **A `setsid` launch from an agent's Bash tool would leak those into the nested `claude`; `systemd-run` does not.**
- Inside a probe unit, `claude --version` and `gh auth status` both worked.

**Liveness without `devflow advance`:**
- `systemctl --user status <unit>` shows the unit's cgroup process tree.
- `<bin> status <root>` is read-only. Verified: no `save_state`, `lock::`, `clear_state` or `emit(` in commands.rs:1056-1195.
- `<bin> gate list <root>`.
- `ls .devflow/gates/49.1-*`.
- `jq . .devflow/state-49.1.json`.

**Re-attach:** unit name plus evidence dir; `journalctl --user -u <unit>`.

**Resume:** if a gate times out after 3 days, the monitor emits `gate_timeout` and exits. `devflow resume --phase 49.1` must **also** run in a new `systemd-run` unit, or its monitor chain dies with the caller.

### Pattern 3: Gates: exact verbs and semantics (all Verified)
Every command uses the pinned binary. `ROOT=/var/home/denniyahh/Github/devflow`. The project root walks up to the nearest `.devflow` ancestor (main.rs:731-751), so running from inside a worktree resolves to the main checkout.

| Gate | Fires in | Context | Approve | Reject |
|------|----------|---------|---------|--------|
| 1. Define preflight | the `start` process (blocking) | `[never-silent] preflight failed for stage define: unattended launch refused (D-07) — no plan declares a human-only checkpoint [DOES NOT HOLD] — a plan declares a \`blocking-human\` gate …` | `<bin> gate approve 49.1 --stage define --note "…" --project $ROOT` → skips re-check, launches Define (preflight.rs:1418-1437) | a note without "abort" means LoopBack, which re-runs preflight and **refuses again**, bounded by `MAX_PREFLIGHT_RETRIES = 3` (mode.rs:80). A note containing "abort" aborts (gates.rs:73-83) |
| 2. Code preflight | the Plan monitor's advance | same shape, stage `code` | as above; **approval records the checkpoint set** (preflight.rs:1433) | as above |
| 3. Ship preflight (major bump) | the Validate→Ship advance | `classified bump is MAJOR — baseline \`v2.12.0\`, resulting version \`v3.0.0\`; a major version bump never ships unattended (D-09) — deciding commit(s): …` (preflight.rs:836-839) | approve (explicit override) | abort |
| 4. Ship completion | after the Ship agent exits | `Ship complete — approve merge?` (pipeline_outcomes.rs:847) | approve → `finish_workflow` → hooks | a note without "abort" loops back to Code (`GapsOnly`); with "abort" it aborts. The branch stays unmerged |

- Inspect: `<bin> gate list $ROOT` and `<bin> gate show 49.1 --stage <s> $ROOT` (the project is positional).
- `gate approve` refuses when no confirmed waiter holds the phase lock. The exception is Ship with `NoHolder`, for which it points to `devflow ship --phase` (commands.rs:1404-1430).
- The response records `responded_by` from `$USER` (commands.rs:1422-1425). The ledger is the `gate_fired` → `gate_response_written` → `gate_resolved` events.
- **Notification: do not set `DEVFLOW_GATE_NOTIFY_CMD`.** Run an out-of-band watcher unit instead, polling `$ROOT/.devflow/gates/49.1-*.json` that lack a `.response.json`. On a new gate it calls `notify-send` and appends to the ledger. The reason (Verified): `hermetic_command` strips only git redirect variables (git.rs:85-92), so every `DEVFLOW_*` variable reaches the agent and the test children the Code and Validate stages spawn. If the operator still prefers the env hook, the script must filter `DEVFLOW_GATE_PHASE == 49.1`, because test fixtures fire gates for phases like 7 and 11.

### Pattern 4: Criterion 1 and 2 observations (Verified sources)
- **Persisted base:** `.devflow/state-49.1.json` holds `"base_branch": "workspace/denniyahh"`. The path is `state-` + `49.1` + `.json` (workflow.rs:43, 140-144). **Snapshot it at gate 1**, because `finish_workflow` and `abort` clear it.
- **Source `ConfigFile`:** `State` persists only the value. The source is observable **only** in `start`'s stdout: "note: base branch is \`workspace/denniyahh\` (from devflow.toml (base_branch)) — phase worktrees fork from it and phase work merges back into it (D-01, 45-CONTEXT.md)" (commands.rs:417-427). The default arm prints nothing, since the note is gated on `base != DEVELOP`. Capture `start.stdout`.
- **`unattended_config_condition = Holds`:** only the Define report prints per-condition lines: `  [holds] GSD config can hold the chain flag` (preflight.rs:1179-1186). The **Code-stage report is lost**, because it is written to the monitor's stdout, which is `/dev/null`. That contradicts the doc comment at preflight.rs:1124-1128. *Reasoned* from monitor.rs:445-447. It is a candidate non-blocking 999.x. At Code, "Holds" is observable only by its absence from the refusal list in `gates/49.1-code.json`.
- **Fork point (D-16):**
  1. `git rev-parse refs/heads/workspace/denniyahh` immediately before `start`.
  2. `git -C .worktrees/phase-49.1 rev-parse HEAD` while parked at gate 1. The worktree already exists then, because `ensure_phase_worktree` runs before `launch_stage` (commands.rs:525-532 vs 700).
  3. `git merge-base feature/phase-49.1 workspace/denniyahh` at gate 4.
- **Merge target:** the `merge_result` event `{"merged": true, "branch": "feature/phase-49.1"}` (hooks.rs:180-239), plus `git merge-base --is-ancestor feature/phase-49.1 workspace/denniyahh` after the hooks.

### Pattern 5: DECN-03, which turn carries which rule (Verified)
- **Turn 1.** The Code stage's first stream-json stdin document is the `code_stage_prompt` (prompt.rs:408-420). It contains `/gsd-execute-phase 49.1 --auto`, where `AUTO_CHAIN_PRESERVING_FLAG = "--auto"` (prompt.rs:40), followed by `CODE_STAGE_POLICY` (prompt.rs:97-123).
  - It is observable in three places: `.devflow/phase-49.1-prompt`, which is **overwritten per launch**, so snapshot it at Code launch; the session transcript's first user message; and the Code capture, which is archived to `.devflow/history/phase-49.1/` on the next launch.
- **Resumed turn.** `claude -p <checkpoint_auto_decide_prompt> --resume <sid> --output-format json --dangerously-skip-permissions` (claude.rs:122-135), on the Legacy monitor (pipeline_launch.rs:1236-1290).
  - The instruction is quoted, truncated to 300 characters, in the `checkpoint_auto_decided` event, emitted *before* the spawn (pipeline_launch.rs:1245-1258).
  - The resumed turn's final message is `result` in the single-document `.devflow/phase-49.1-stdout`.
  - The full two-turn record is `~/.claude/projects/-var-home-denniyahh-Github-devflow--worktrees-phase-49-1/<session_id>.jsonl`. *Reasoned* by analogy: this session's own directory is `-var-home-denniyahh-Github-devflow--worktrees-phase-49`.
- **Route preconditions (pipeline_launch.rs:1777-1779):**
  - `state.agent == AgentKind::Claude`;
  - a declared `gate="blocking-human"` in the worktree's plans (marker at verify.rs:154);
  - `checkpoint_reported_in_capture`, which needs a line reading `Gate: blocking-human` (agent_result.rs:730-749; the executor's return format emits `**Gate:** blocking-human`, gsd-executor.md:359);
  - a `session_id`;
  - `checkpoint_resumes < MAX_CHECKPOINT_RESUMES = 3` (mode.rs:106);
  - a checkpoint set unchanged since gate 2. Otherwise a re-scan gate fires.
- **Classify per 47-PHASE49-OBSERVATION.md.** Also record the positions: CODE_STAGE_POLICY is in user message 1; the auto-decide prompt is the latest user message.

### Pattern 6: Negative-control arm (D-14), same pinned binary, run FIRST
**Verified facts:**
- `scripts/scratch-dogfood-repo.sh:187-191` writes `.planning/config.json` = `{"workflow": {}}`.
- The script ends with develop checked out (`:195-196`).
- It refuses destinations inside the checkout (`:48-54`).
- The refusal text is preflight.rs:994-996, rendered as `no \`.planning/config.json\` under the launch root — nowhere for the chain flag to live`. The rendered form was confirmed in `44-evidence/cargo-test-workspace.txt:244`.
- The full report line: `  [DOES NOT HOLD] GSD config can hold the chain flag — no \`.planning/config.json\` under the launch root — nowhere for the chain flag to live`.
- The gate context keeps that text whole. The refusal is about 175 characters, under the 300-character `truncate_reason` cap, and in the scratch arm C2 and C3 hold.

**Steps:**
1. `D=~/.cache/devflow-phase49/attempt-N/negctl/repo`, then `scripts/scratch-dogfood-repo.sh "$D"`.
2. `git -C "$D" rm -q .planning/config.json && git -C "$D" commit -q -m "chore: remove config.json for negative control"`. This lands on `develop`, the fork base.
3. Record `git -C "$D" rev-parse develop`.
4. Launch: `systemd-run --user --unit p49-negctl-aN -p RemainAfterExit=yes -p KillMode=process -- env -u DEVFLOW_BASE_BRANCH sh -c 'exec "$BIN" start --phase 1 --agent claude --mode auto "$D" >"$EV/negctl/start.stdout" 2>"$EV/negctl/start.stderr"'`.
5. Poll for `$D/.devflow/gates/01-define.json`.
6. Assert:
   - `jq -r .base_branch "$D/.devflow/state-01.json"` is `develop`;
   - the stdout has the `[DOES NOT HOLD] GSD config …` line;
   - the stdout has **no** `note: base branch is` (default source);
   - the worktree HEAD equals the recorded develop SHA.
7. `"$BIN" gate reject 1 --stage define --note "abort: negative control complete" --project "$D"`.
8. Assert the `workflow_aborted` event.
- **Positive control for the same assertion script:** on the positive arm the config line must read `[holds]`, and the note must name `devflow.toml (base_branch)`.

### Pattern 7: SURV-01 detection (writer attribution, not absence)
**Verified mechanics:**
- `save_state` writes a temp file then renames it. The temp name is `format!(".{name}.{pid}.{sequence}.tmp")` (workflow.rs:205-208, rename at :250), so every state write leaves a pid-stamped create/rename pair in `.devflow/`.
- `advance` waits on the per-phase lock silently (lock.rs:80-100) for up to `ADVANCE_LOCK_WAIT = 600s` (pipeline_launch.rs:55). Only an *expired* wait emits `advance_failed` `{"reason":"lock wait expired","holder_pid":…,"waited_secs":…}` (pipeline_launch.rs:1626-1644). A stage-binding mismatch emits `advance_failed` `"stage mismatch"`.

**Recommended instrument:** a python ctypes `inotify` watcher unit on `$ROOT/.devflow/`, recording `IN_CREATE`, `IN_MOVED_FROM`, `IN_MOVED_TO` and `IN_DELETE` with timestamps. Interleaving is defined as a create of `.state-49.1.json.<pidB>.*.tmp` between the create and the rename of `<pidA>`'s temp, with `pidA ≠ pidB`. Log `IN_Q_OVERFLOW` as a void window.

**Wave-0 negative control:** a synthetic two-writer overlap in a scratch directory *must* be flagged, and a single sequential writer *must not* be. Record the events, lock snapshots and watcher log as an observation. "No interleaving observed" confirms nothing (REQUIREMENTS.md:220-223).

### Pattern 8: Retry preservation before `--force` (D-12)
**Verified:**
- `ensure_phase_worktree` with `force` removes the worktree (`worktree::remove(…, true)`) and runs `delete_branch(&branch, true)` (parallel.rs:33-40), a force delete. Without `force`: "worktree already exists at {} — use --force to recreate it" (parallel.rs:46-47).
- `start` also runs `Gates::cleanup` for **every** stage of the phase (commands.rs:371-381) before forking, and refuses when a recorded monitor or agent pid is alive (`refuse_start_over_a_live_run`, :370).
- `phase_validate_failures` carries over a `--force` (commands.rs:255-269).

**Recipe:**
1. `<bin> stop --phase 49.1 $ROOT`, and confirm with `systemctl --user status`.
2. `git update-ref refs/archive/phase49/attempt-N/feature-phase-49.1 feature/phase-49.1`.
3. Copy `$ROOT/.devflow/` in full: gates, `state-49.1.json`, `lock-49.1`, `phase-49.1-*`, `history/phase-49.1/`, `events.jsonl`.
4. Copy the worktree's untracked and ignored-but-relevant files (`git -C .worktrees/phase-49.1 status --porcelain --ignored`), and the session JSONLs.
5. Only then rebuild, re-pin, re-run the negative arm, and relaunch with `--force` in a new unit.

### Pattern 9: D-08 undo, reset rather than revert (recommended)
**Verified hook order (hooks.rs:115-122, 294-353, 259-292; git.rs:185-190, 250-253):**
- Merge `M` = `git checkout <base>` + `git merge --no-ff feature/phase-49.1`.
- Bump commit `B` = `chore: bump version to 3.0.0`, which writes the root `Cargo.toml` `[workspace.package] version` (version.rs:69-88).
- Tag `v3.0.0` = a lightweight, unsigned tag on `B`.
- Changelog commit `C` = `docs: add changelog entry for 3.0.0`.
- `BranchCleanup` then fails soft, because the branch is checked out in the worktree (*Reasoned*: git refuses `branch -d` on a checked-out branch; hooks.rs:131-156 warns and returns Ok).

**Recipe, in the main checkout, only after the evidence snapshot:**
1. `git update-ref refs/archive/phase49/attempt-N/post-ship-hooks HEAD`. This keeps `B` and `C` reachable as evidence.
2. Assert all of the following:
   - `git rev-parse HEAD~2` equals `M`;
   - `git rev-parse M^1` equals the recorded pre-hook HEAD;
   - `M`'s subject starts with `Merge branch 'feature/phase-49.1'`;
   - `git rev-parse 'v3.0.0^{commit}'` equals `HEAD~1`;
   - `git diff --name-only M HEAD | sort` is exactly `CHANGELOG.md` and `Cargo.toml`;
   - `git status --porcelain` is empty.
3. `git tag -d v3.0.0 && git reset --hard M`.
4. Verify:
   - `git tag -l v3.0.0` prints nothing;
   - `git rev-parse HEAD` equals `M`;
   - `rg -n '^version = "2.12.0"' Cargo.toml` matches.

Why reset and not revert: nothing has been pushed, and the pushes stay blocked (see Pitfall 5). Reverting would leave two noise commits, and the tag has to be deleted either way.

### Pattern 10: Phase 49.1 promotion (D-01, D-02, D-04, D-05)
- **ROADMAP heading:** `### Phase 49.1: <name>` inserted between the end of the Phase 49 block (`**Plans**: TBD`, ROADMAP.md:279) and `### Phase 50:` (:281). It stays inside the active milestone window. Add rows to the milestone table (Phase 49 is at :31) and to the Progress table (Phase 49 is at :490). Mark `### Phase 999.55` (:4039) as `(PROMOTED — Phase 49.1)`.
- **The reachability guard** matches `### Phase {phase}:` with the trailing colon (preflight.rs:176-181) on `git show workspace/denniyahh:.planning/ROADMAP.md`. `PhaseId` displays `49.1`.
- **Directory:** `.planning/phases/49.1-<slug>/`. **Plan file:** `49.1-01-PLAN.md`. Plan discovery uses the prefix `49.1-` and the suffix `-PLAN.md` (verify.rs:37-62), and the Plan-stage idempotency probe is `ls .planning/phases/49.1-*/49.1-*PLAN.md` (prompt.rs:341-366).
- **Seeded checkpoint:** a `<task type="checkpoint:decision" gate="blocking-human">` element. It follows the GSD structure (`<decision>`, `<context>`, `<options>`/`<option id>`, `<resume-signal>`; checkpoints.md:137-167) and carries `autonomous: false`.
  - The declaration compared at resume is the *whole task element* (verify.rs:171-183), so the Code agent must not edit it.
  - **Discriminating design ([ASSUMED], planner's call):** do not put the merit-best option first. Otherwise a positional first-option pick is indistinguishable from following the policy (see Pitfall 7).
- **TDD mode is on** (`workflow.tdd_mode: true`). The `tdd.review-checkpoint` gate applies only to `type: tdd` plans (capability-registry.cjs:755-790). A `type: tdd` 49.1 plan must satisfy RED→GREEN commit order, or the Code stage stops before verification.
- **Linting on commit:** the pre-commit bashism scanner checks the staged 49.1 PLAN's `<automated>` blocks. Bash-only syntax must sit inside `bash -c '…'` (scripts/hooks/pre-commit:100-122).
- **Landing on the base (D-05):** commit on `feature/phase-49`. Then, in the main checkout (clean, on `workspace/denniyahh`), run `git merge --ff-only feature/phase-49`; that works only while the workspace has not moved. Memory "Mid-phase backup" records that this is how phase-49 commits reached `workspace/denniyahh` (both are at `b9dcfa9`).
  - A push is optional for the launch: `ensure_base_ref_current` accepts `Ahead` (preflight.rs:556-560).
  - `feature/phase-49` itself can never be pushed while it carries `.planning` (pre-push:92-104).
- **Pre-launch validation:** `gsd_run query find-phase 49.1`, `phase-plan-index 49.1`, and a dry `git show workspace/denniyahh:.planning/ROADMAP.md | rg -n '^### Phase 49\.1:'`.

### Anti-Patterns to Avoid
- **`setsid … &` or `nohup` from an agent's Bash tool.** It inherits `CLAUDECODE` / `CLAUDE_CODE_*` and the agent's cgroup and session, and gets orphaned (memory).
- **The installed `devflow` on PATH (2.11.0) for any verb.** Mixed-binary gate responses break criterion 3's same-binary claim.
- **Moving `workspace/denniyahh` between launch and the Merge hook.** Do not fast-forward phase-49 docs into it mid-run. The merge target changes, and a STATE.md / ROADMAP.md conflict leaves the operator's main checkout mid-merge.
- **Asserting on exit codes for either arm.** Assert on the verbatim texts quoted above.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Process ownership across days | pid files plus custom reapers | `systemd-run --user -p RemainAfterExit=yes -p KillMode=process` | cgroup view, journal, survives sessions; probed |
| Gate answers | writing `*.response.json` by hand | `<bin> gate approve/reject` | exclusive publish (48 D-04), waiter check, `gate_response_written` event |
| State inspection | parsing monitor logs | `jq` on `state-49.1.json` + `<bin> status` (read-only) | status is verified read-only |
| Writer attribution | polling `ls` | inotify on `.devflow/` (temp names carry the pid) | temp files live for microseconds |
| Scratch fixture | a new generator | `scripts/scratch-dogfood-repo.sh` + one `git rm` commit | D-14; guarded destination |
| Version prediction | hand-reading commit subjects | call `devflow_core::version::{reachable_semver_baseline, release_range_start, classify_range_bump, compute_version}` (the scratch probe used here) | the same code the gate uses |

## Runtime state the run creates (cleanup inventory)

| Category | Items | Action |
|----------|-------|--------|
| Stored data | `$ROOT/.devflow/{state-49.1.json, lock-49.1, gates/49.1-*, phase-49.1-*, history/phase-49.1/}`; entries appended to the shared `events.jsonl` (`"phase":"49.1"` is a **string**, phase_id.rs:182-194) | snapshot; `finish_workflow` / `abort` clear the state |
| Machine registry | `~/.cache/devflow/roots/<entry>` for `(ROOT, 49.1)` and for `(negctl repo, 1)`, plus 999.141 test-root pollution from the Validate / Code test runs | census before and after; one-off cleanup is the operator's call (999.141 text) |
| OS-registered state | transient units `p49-*` (start, resume, watchers) | `systemctl --user stop` + `reset-failed` after the run |
| Git refs | `feature/phase-49.1` (+ worktree `.worktrees/phase-49.1`); local tag `v3.0.0`; merge / bump / changelog commits on `workspace/denniyahh`; `refs/archive/phase49/*`; possibly `extensions.worktreeConfig` + `config.worktree` (ND-1 B) | Pattern 9 undo; unset the worktree config after the run |
| Remote | `origin/feature/phase-49.1` + a PR against `main` (only if ND-1 B/C lets the push through) | close the PR unmerged; delete the remote branch (decision) |
| Build artifacts | `~/.cache/devflow-phase49/bin/devflow-<sha12>` | keep as evidence |

## Common Pitfalls

### Pitfall 1: The Ship push is refused by repo policy (Verified, with controls)
A scratch clone ran the repo's `pre-push` hook against a new ref.

Arm A (a `.planning` commit on `feature/phase-49.1`):
```
pre-push: REFUSING to push personal/agent artifacts to 'refs/heads/feature/phase-49.1':
  .planning/phases/49.1-probe/49.1-01-SUMMARY.md
```

Arm B (a crates-only commit): it got past the hygiene block and was then refused by the divergence check:
```
check-workspace-divergence: this branch diverges from origin/develop in ways .workspace-divergence does not declare:
  undeclared change to a develop file: crates/devflow-cli/tests/phase7_cli.rs (move it to develop, or declare: modified crates/devflow-cli/tests/phase7_cli.rs)
```

The hygiene block has no override (pre-push:92-104). The divergence check runs whenever `.workspace-divergence` exists in the pushing checkout.

**Mitigation probe (Verified):** after `git config extensions.worktreeConfig true` and `git -C <wt> config --worktree core.hooksPath /per/worktree/hooks`, the result was `main=/repo/level/hooks wt=/per/worktree/hooks`.

**Also:** the pushed branch runs the container gate (2-9 min, memory) inside the Ship agent's Bash call. A copied hook should keep or drop it deliberately.

### Pitfall 2: `/gsd-ship` capability gates halt before the push (Verified)
See ND-1. `/gsd-code-review` may also halt on `config-get workflow.code_review_depth_overrides --default '[]'` returning the JSON string `"[]"` (code-review.md:419; memory, gsd-core 1.14.0). A headless agent's reaction to that is unknown.

### Pitfall 3: A fourth human gate, and the tag is `v3.0.0` (Verified by probe)
```
=== phase-49 worktree (HEAD b9dcfa9):   bump=Ok(Major)  compute_version=Ok(Version { major: 3, minor: 0, patch: 0 })
=== control: clone at v2.12.0:          bump=Ok(None)   compute_version=…patch: 1
=== control2: parent of first refactor(48)! (0f3f5a5): bump=Ok(Minor) compute_version=…minor: 13…
```
The deciding commits are `530db1a` / `39017ad`, subject `refactor(48)!: mark WorkflowError and RecoverError non-exhaustive`.

A side effect (*Reasoned*): after the bump, the root `Cargo.toml` reads `3.0.0`, but `[workspace.dependencies] devflow-core = { …, version = "2.12.0" }` (Cargo.toml:20) is untouched. **Do not build from the main checkout until the undo.**

### Pitfall 4: The phase-worktree guard (Verified; resolved by GSD ordering, not a blocker)
With `current_phase: 49` and a `feature/phase-49` worktree present, a staged `crates/` file on `feature/phase-49.1` gives "pre-commit: refusing to commit phase 49 source on 'feature/phase-49.1'."

After `gsd-tools query state.begin-phase --phase 49.1`, the frontmatter reads `"49.1"` and the guard exits `rc=0`.

`execute-phase.md` runs `state.begin-phase` in `validate_phase` (:306-317), before `execute_waves` (:406). **Do not set `DEVFLOW_ALLOW_OFF_WORKTREE`.** If a Code-stage failure quotes the refusal text above, that is the diagnosis.

### Pitfall 5: After the Merge hook, `workspace/denniyahh` cannot be pushed (Reasoned from the verified hook)
The merged `crates/` change trips the same divergence check (Pitfall 1, Arm B) until 49.1's code reaches `develop` through the close-out PR flow (memory: phase close-out), or until the manifest declares it. Also, `feature/phase-49` can no longer be fast-forwarded into the workspace; the phase-49 docs need a real merge.

### Pitfall 6: Evidence overwritten or pruned
- `.devflow/phase-49.1-prompt` and `-stdout` are replaced on every launch.
- History keeps 12 generations (`DEFAULT_CAPTURE_RETENTION = 12`, config.rs:36).
- Gate files are cleaned at `start` and on loop-backs.
- State is cleared at finish or abort.

Snapshot at every gate and at the Code launch. Do not raise retention through env; that would be another inherited variable.

### Pitfall 7: DECN-03 outcomes outside the three-way standard
- **Turn 1 itself resolves the checkpoint.** That violates CODE_STAGE_POLICY's "otherwise report it instead", and the resume path is never exercised. Record it as "not exercised", plus a separate finding.
- **The final message has no `Gate: blocking-human` line.** That falls through to a never-silent Code gate, so again "not exercised".
- **A positional first-option pick.** If the first-listed option is also the best one, "Followed" cannot be told apart from positional selection.
- **The `checkpoint_auto_decided` event is present but the resumed spawn failed.** Classify as Void (per the standard).

### Pitfall 8: Leaked test monitors inside the unit's cgroup (999.141)
The Validate and Code test runs spawn real `devflow` children rooted in `/tmp/.tmp*`, registered in `~/.cache/devflow/roots` because `phase7_cli.rs:130-138` sets no `HOME` / cache variables and `registry::cache_dir` falls through to `HOME/.cache/devflow` (registry.rs:53-62).
- They appear in `systemctl --user status`. Attribute processes by project root in argv.
- They **cannot** write the live `state-49.1.json`, because `state_path` sits under each process's own `project_root` (workflow.rs:140-144). *Reasoned.*
- The gate-sweep e2e tests scope with `--root` (gate_sweep_e2e.rs:188, :228), and reap-strays tests act only on their own pid (reap_strays_e2e.rs:1-40).
- **Protocol:** do not override `HOME` / `XDG_CACHE_HOME` / `DEVFLOW_CACHE_DIR` on the launch; that would move the live registry and still be inherited by the tests. Census before and after, attribute by root, file the delta under 999.141, and do not clean it automatically.

### Pitfall 9: 49.1 design, "longer default under CI" cannot see `CI` where the flake happens
- The devcontainer job runs `scripts/check.sh all` through `devcontainers/ci@v0.3` (devcontainer.yml:42-52). That action forwards host env only with `inheritEnv` or explicit `env` [CITED: github.com/devcontainers/ci docs/github-action.md].
- `.devcontainer/devcontainer.json` has no `containerEnv` (rg: 0 matches).
- The local pre-push container passes only `-e CARGO_TARGET_DIR` and `-e CARGO_TERM_COLOR` (check-in-container.sh:146-155).
- Both recorded 999.55 recurrences were in the devcontainer job or the local container gate.

### Pitfall 10: `printf "%02d" 49.1` in execute-phase's review step
It prints `49` plus an error, so the advisory REVIEW path is wrong for decimal phases (execute-phase.md code_review_gate). It is non-blocking by that step's own contract. Note it; do not chase it.

## Code Examples

### Positive-arm launch (unit owns `start`; env minimal)
```bash
# Source: probe above + commands.rs:306-712. BIN/EV/ROOT set by the plan; verify BIN sha256 first.
systemd-run --user --unit "p49-start-a${N}" \
  -p RemainAfterExit=yes -p KillMode=process \
  --working-directory="$ROOT" \
  -- sh -c 'exec "$0" start --phase 49.1 --agent claude --mode auto "$1" >>"$2/start.stdout" 2>>"$2/start.stderr"' \
     "$BIN" "$ROOT" "$EV"
```

### Gate answer
```bash
"$BIN" gate show 49.1 --stage define "$ROOT"
"$BIN" gate approve 49.1 --stage define --note "Define preflight: seeded blocking-human checkpoint is intentional (D-03)" --project "$ROOT"
# abort form:  "$BIN" gate reject 49.1 --stage ship --note "abort: <reason>" --project "$ROOT"
```

### Positive / negative assertion core (text, not exit code)
```bash
bash -c 'set -euo pipefail
rg -F -q "[holds] GSD config can hold the chain flag" "$EV/start.stdout"
rg -F -q "(from devflow.toml (base_branch))" "$EV/start.stdout"
test "$(jq -r .base_branch "$EV/gates/define/state-49.1.json")" = "workspace/denniyahh"
rg -F -q "[DOES NOT HOLD] GSD config can hold the chain flag — no \`.planning/config.json\` under the launch root — nowhere for the chain flag to live" "$EV/negctl/start.stdout"
! rg -F -q "note: base branch is" "$EV/negctl/start.stdout"
test "$(jq -r .base_branch "$EV/negctl/state-01.json")" = "develop"'
```

### SURV watcher skeleton (python ctypes inotify)
```python
# [ASSUMED] shape; Wave 0 must prove it with the two-writer / one-writer controls.
import ctypes, os, struct, json, time, sys
libc = ctypes.CDLL("libc.so.6", use_errno=True)
IN_CREATE, IN_MOVED_FROM, IN_MOVED_TO, IN_DELETE, IN_Q_OVERFLOW = 0x100, 0x40, 0x80, 0x200, 0x4000
fd = libc.inotify_init1(0)
libc.inotify_add_watch(fd, sys.argv[1].encode(), IN_CREATE | IN_MOVED_FROM | IN_MOVED_TO | IN_DELETE)
while True:
    buf = os.read(fd, 65536); i = 0
    while i < len(buf):
        wd, mask, cookie, ln = struct.unpack_from("iIII", buf, i)
        name = buf[i + 16:i + 16 + ln].rstrip(b"\0").decode(errors="replace")
        print(json.dumps({"t": time.time_ns(), "mask": mask, "cookie": cookie, "name": name}), flush=True)
        i += 16 + ln
```

## Line-number drift (CONTEXT cited `e9c6de3`; re-read at `b9dcfa9`)

| CONTEXT citation | Current location | Status |
|---|---|---|
| phase7_cli.rs wait_for :200, wait_for_pid :215, wait_for_state_cleared :233, wait_for_stopped :246, wait_for_settled :1302, wait_for_gate :1314 | same; loops at :201, :216, :234, :247, :1303, :1315 | unchanged |
| phase7_cli.rs:130 (no HOME) | `fn run_devflow_inner` :130 | unchanged |
| preflight.rs ConditionState :922, unattended_config_condition :990, C3 :1076-1080, applies :971-974 | :922, :990 (text :994-996), :1078-1080, :971 | unchanged |
| preflight.rs record :1432-1434 | call :1433 (Advance arm), :1461 (pass path); fn def :1315 | unchanged |
| preflight.rs :259-262, :294-297 (reachability) | doc :258-262; `ensure_phase_reachable_on_base` :289 | ok |
| pipeline_launch.rs :1746 Ship route, :1775-1779, :1777 | :1746, :1777 | unchanged |
| pipeline_outcomes.rs :828-850, :1019-1030 | handle_ship_outcome :838; run_checkout_hooks :1019 | ok |
| prompt.rs :374-380, :341, :251 | define :374, idempotent :341, ship fn :249 (`/gsd-ship` at :251) | ok |
| hooks.rs :115-122, :180, :294-353, :433 | :115, :180, :294, :433 | unchanged |
| git.rs :185-190, :250-253 | :185, :250 | unchanged |
| gates.rs :388-394 | :388 | unchanged |
| config.rs :341, :351, :384 | :341, (main refusal in body), :384 | unchanged |
| parallel.rs :24-44 | fn :24; force delete :39; exists msg :46 | unchanged |
| main.rs :53 (agent default) | `default_value = "claude"` at **:54** | **drifted +1** |
| verify.rs :131 | `phase_has_blocking_human_checkpoint` :131; marker literal :154 | ok |
| registry.rs :46 | doc :46; `cache_dir` fn :53 | ok |
| scratch-dogfood-repo.sh :187-191 | :187 | unchanged |
| 48-CONTEXT monitor.rs:319 current_exe | **:330** | **drifted** |
| ship.md :46, :194-199, :362-377 | :46, push :194, `gh pr create` :370-374 | unchanged |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Session transcript path is `~/.claude/projects/-var-home-denniyahh-Github-devflow--worktrees-phase-49-1/<sid>.jsonl` | Pattern 5 | DECN-03 turn evidence must be found another way (the capture + event remain) |
| A2 | `git branch -d` refuses a branch checked out in a worktree, so BranchCleanup leaves `feature/phase-49.1` | Pattern 9 | Branch deleted by the hook; the archive ref preserves it anyway |
| A3 | The Code-stage per-condition report is lost to `/dev/null` | Pattern 4 | Harmless if it lands somewhere; evidence plan does not rely on it |
| A4 | The python ctypes inotify watcher works on this kernel/python | Pattern 7 | Wave-0 controls catch it; fall back to events + lock snapshots and mark SURV "weak instrument" |
| A5 | Claude CLI 2.1.282 still emits `tool_progress` keepalives often enough to stay under the 120s idle floor (measured at 2.1.220, monitor.rs:57-95) | Pitfalls | Idle kill during a long foreground test run is an infra outcome (retry), not a DevFlow defect |
| A6 | GitHub job containers (ci.yml) receive `CI=true` | Pitfall 9 | Only affects the 49.1 design's CI detection |
| A7 | Placing the merit-best checkpoint option non-first is compatible with the operator's real decision | Pattern 10 | Operator may prefer a natural order; then record that positional pick cannot be excluded |
| A8 | `gsd_run query find-phase 49.1` / `phase-plan-index 49.1` validate a decimal phase as expected | Pattern 10 | Planner verifies before relying on them |

## Open Questions (RESOLVED)

1. **ND-1 / ND-2 / ND-3** — RESOLVED by the 2026-09-25 operator answers above: use ND-1 Option B, observe then undo the `v3.0.0` tag/bump, and assign Phase 49.1 the TEST-02 requirement.
2. **How does 49.1's code reach `develop`?** — RESOLVED FOR PLANNING: it remains an operator decision after the live evidence exists, captured by Phase 49 Task 49-14. The planner must not fold the normal close-out into Phase 49 or choose a route itself.
3. **Does GSD's post-execution `update_roadmap` / `phase.complete` in the 49.1 worktree advance STATE.md unexpectedly?** — RESOLVED FOR PLANNING: Phase 49 Task 49-14 presents the captured post-Merge diff to the operator and records the chosen disposition; it does not predict or silently repair the result.
4. **The Code-stage report doc/behaviour mismatch (A3).** — RESOLVED: it is a candidate non-blocking 999.x item under D-10, unless the run proves it blocks the chain and triggers D-10's in-phase remediation policy.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| systemd user manager + linger | run ownership | ✓ | 259, `Linger=yes` | — |
| cargo / rustc | build + 49.1 tests | ✓ | 1.97.1 | — |
| claude CLI | agent | ✓ (also inside a unit) | 2.1.282 | — |
| gh (auth) | Ship PR | ✓ (keyring, also inside a unit) | — | — |
| docker | pre-push container gate | ✓ | 29.8.1 | `DEVFLOW_SKIP_CONTAINER_CHECK=1` (not equivalent) |
| git | everything | ✓ | 2.55.0 | — |
| jq, python3, notify-send | assertions, SURV watcher, notification | ✓ | —, 3.14.7, — | — |
| inotifywait / strace / fatrace | — | ✗ | — | python ctypes inotify (A4) |
| Commit signing inside units | agent + hook commits | ✓ | a probe commit signed `G` with the devflow key file; the key is **not** in ssh-agent, and signing reads the private key file | — |
| Installed `devflow` | — | 2.11.0 (stale) | — | use the pinned build only |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust 1.97.1) for 49.1; bash + jq evidence assertions for Phase 49 |
| Config file | none; tools live under `.planning/phases/49-…/tools/` (Wave 0) |
| Quick run command | `cargo test -p devflow --test phase7_cli` (49.1 structure); `bash .planning/phases/49-*/tools/check-evidence.sh "$EV"` |
| Full suite command | `scripts/check.sh all`; container: `scripts/check-in-container.sh all` |

### Phase Requirements → Test Map
| Req / criterion | Behavior | Test Type | Automated Command | File Exists? |
|---|---|---|---|---|
| VERIFY-01 c1 | persisted base plus ConfigFile note | evidence assertion | `jq -r .base_branch …state-49.1.json` = `workspace/denniyahh` AND `rg -F "(from devflow.toml (base_branch))" start.stdout` | ❌ Wave 0 |
| VERIFY-01 c2 | fork triple equal; `[holds]` line; transitions; `merge_result merged:true` | evidence assertion | `jq -c 'select(.phase=="49.1")' events.jsonl` + git SHA compares | ❌ Wave 0 (**blocked by ND-1**) |
| VERIFY-01 c3 | negative arm refusal text, develop base, no source note, same sha256 | evidence assertion | see Code Examples; assert `sha256sum` equal across arms | ❌ Wave 0 |
| DECN-03 | Followed / Not followed / Void / not-exercised | manual classification over captured evidence | `jq 'select(.event=="checkpoint_auto_decided")'` + transcript review | manual-only (judgement) |
| SURV-01 | writer-attributed interleaving detection | watcher + controls | `python3 tools/surv-watch.py --selftest` (two-writer → flagged; one-writer → clean) | ❌ Wave 0 |
| 49.1 acceptance | six helpers share one budget; the panic names the budget | cargo test in the 49.1 worktree | `cargo test -p devflow --test phase7_cli <module-qualified name> -- --exact` and assert `1 passed` | written by the 49.1 plan |

### Sampling Rate
- **Per task commit:** the evidence-tool self-tests (each tool's negative control).
- **Per gate:** the snapshot script, plus an assertion subset for the facts available at that gate.
- **Phase gate:** the full evidence check, then `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `tools/snapshot.sh`: read-only copy of `.devflow/` + git facts per gate.
- [ ] `tools/gate-watch.sh`: poll, notify, ledger; filtered to `49.1-*`; test with a planted gate file in a scratch `.devflow/`.
- [ ] `tools/surv-watch.py`: with `--selftest` two-writer and one-writer controls.
- [ ] `tools/negctl.sh`: Pattern 6; the control-inversion check runs against positive-arm stdout.
- [ ] `tools/undo-ship-hooks.sh`: Pattern 9 guards; test in a scratch repo where each guard must refuse a wrong layout.
- [ ] `tools/check-evidence.sh`: criteria 1-3 assertions.

## Security Domain

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2/V3 Authentication, Session | no | — |
| V4 Access Control | partial | Human gates are the access control. Never pre-write responses (D-03 rejected). Every approval carries `responded_by` |
| V5 Input Validation | yes | Assertions use fixed-string `rg -F` and `jq -r`; no `eval` of captured agent text (gate contexts are agent-influenced, pipeline_outcomes.rs:869-893) |
| V6 Cryptography | yes | Never push the unsigned local `v3.0.0` tag. The pre-push signing policy refuses it (pre-push:128-160; `devflow.releasesigningkey` is set). Delete it anyway (Pattern 9) |
| V8 Data Protection | yes | Raw captures and transcripts contain absolute paths, the username and session IDs (v2.3.0 deferred item). Commit only sanitised excerpts. Never commit env dumps. `ANTHROPIC_API_KEY` / `ANTHROPIC_TOKEN` exist, empty, in both environments; never print values |

| Threat | STRIDE | Mitigation |
|--------|--------|------------|
| Stray `v3.0.0` tag pushed | Tampering | tag deletion + signing-policy backstop |
| Evidence leaks via a public PR body (`/gsd-ship` builds the body from `.planning`) | Information disclosure | content already public on `workspace/denniyahh`; close the PR; review the body before approving gate 4 |
| Gate answered by a stale binary or the wrong root | Spoofing / Repudiation | pinned absolute binary, `--project $ROOT`, ledger events |
| Main-checkout mutation during an operator edit | Tampering | D-13 clean assertions; freeze the workspace mid-run |

## Sources

### Primary (HIGH confidence)
- Source files read this session at `b9dcfa9`:
  - `crates/devflow-cli/src/{main,commands,preflight,pipeline_launch,pipeline_outcomes,pipeline_gate,parallel,staleness,config_parse}.rs`
  - `crates/devflow-core/src/{gates,hooks,git,config,version,verify,prompt,agent_result,workflow,lock,events,registry,monitor,phase_id,state}.rs`
  - `crates/devflow-core/src/agents/claude.rs`
  - `scripts/{scratch-dogfood-repo.sh, lint-phase-worktree.sh, check-in-container.sh}`
  - `scripts/hooks/{pre-commit, pre-push, commit-msg}`
  - `.workspace-divergence`, `devflow.toml`, `.github/workflows/devcontainer.yml`
- `~/.claude/gsd-core/workflows/{ship.md, execute-phase.md, code-review.md}`, `references/checkpoints.md`, `bin/lib/{state-transition,state,capability-registry}.cjs` (gsd-core 1.14.0).
- Probes (scratchpad):
  - the pre-push two-arm refusal;
  - the worktree guard before and after `begin-phase`;
  - the per-worktree `hooksPath` override;
  - systemd `KillMode` survival, in two arms;
  - the version-bump classification, with two controls;
  - in-unit `claude` / `gh` access;
  - a signing probe.

### Secondary (MEDIUM confidence)
- [CITED: github.com/devcontainers/ci/blob/main/docs/github-action.md]: `inheritEnv` defaults false; `env` passes only listed variables.
- Operator memory files (close-out procedure, workspace split, gate-hang, long probes, GSD quirks) were used as pointers and re-verified in source where cited.

### Tertiary (LOW confidence)
- A1-A8 in the Assumptions Log.

## Metadata

**Confidence breakdown:**
- Launch, gates, evidence paths, undo mechanics: HIGH. Read in source, and the key behaviours were probed with controls.
- Ship-stage blockers: HIGH that they fire. What the Ship agent does when blocked is unknown (agent behaviour).
- DECN-03 / SURV-01 instruments: MEDIUM. The mechanisms are verified; the watcher is unproven until its Wave-0 controls run.

**Research date:** 2026-09-24
**Valid until:** the next commit that touches `crates/`, `scripts/hooks/`, `.planning/WINDOWS.md`, `.planning/config.json` or gsd-core. Re-verify line numbers and the bump classification before launch.
