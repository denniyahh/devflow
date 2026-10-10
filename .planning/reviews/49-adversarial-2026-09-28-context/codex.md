## CONFIRMED

1. **D-22’s “exactly two human gates per attempt” is false outside the narrow happy path.** `preflight.rs:1403` opens a human gate for any failing preflight; Ship can fail `gh auth status` (`preflight.rs:703-720`) or MAJOR classification (`preflight.rs:793-847`). A terminal-hook failure then reopens a separate Ship gate that `--yes-ship` deliberately cannot auto-answer (`pipeline_outcomes.rs:830-837`). Validate can also gate after its failure ceiling (`pipeline_outcomes.rs:668-748`).

   The only gates excluded by construction are Plan’s overwrite prompt, because the existing plan makes Plan a no-op (`prompt.rs:346-355`), and the routine Ship approval, which `--yes-ship` auto-answers (`pipeline_outcomes.rs:839-852`). A Validate failure loops back to Code (`pipeline_outcomes.rs:751-759`), where the unchanged blocking-human declaration again fails unattended preflight (`preflight.rs:1076-1080`), reopening Code’s human gate. Recommendation: change D-22 to a conditional gate matrix and define whether retries with loop-backs are invalid attempts or may consume additional approvals.

2. **D-21 does not control the first-release classifier.** With no tags, `compute_version` uses `0.0.0` and classifies all reachable non-merge history (`version.rs:672-681`; `version.rs:374-413`). The current scaffold’s initial commit is safely `chore: scaffold devflow probe target` (`scratch-dogfood-repo.sh:193-196`), but the extended payload’s resulting agent commits are unconstrained. A `feat!:` subject or `BREAKING CHANGE:` footer produces MAJOR (`version.rs:368-370, 420-425`).

   `--yes-ship` only supplies the routine Ship response (`pipeline_outcomes.rs:839-850`); MAJOR preflight explicitly ignores it (`preflight.rs:740-744`). Recommendation: require a post-Code classifier assertion over the actual feature branch and a payload commit-message policy; otherwise a first release can create an unplanned third human gate.

3. **The two-base topology is incomplete and the claimed Ship target is not observable from `merge_result`.** A configured `integration` must be a local branch before `start` (`commands.rs:398-405`), and the worktree is forked with `git worktree add -b feature/phase-NN … integration` (`parallel.rs:24-43`; `worktree.rs:64-82`). A remote-tracking `origin/integration` is not required: DevFlow fails open for a local-only branch (`preflight.rs:556-574`). The runbook must therefore create local `integration` before launch; a fresh GitHub repository only supplies `main`.

   GSD pushes the current feature branch and opens its PR against GSD’s independently resolved `BASE_BRANCH` (`_external/gsd-core/workflows/ship.md:44-47,190-202,370-374`). DevFlow’s terminal hooks instead run in the primary checkout (`pipeline_outcomes.rs:1003-1017`), merge into the persisted configured base, then bump/tag/changelog there (`hooks.rs:115-121,180-231,294-351`). Nothing here pushes `integration`. Worse, `merge_result` records only `{"merged": true, "branch": branch}` (`hooks.rs:228-233`), not the base. Recommendation: make the scaffold create and record `integration`, assert GSD’s PR base is `main`, and record the DevFlow merge base explicitly or verify it with post-hook Git ancestry.

4. **D-27 asserts GSD behavior that the copied GSD workflow does not define.** `checkpoint:human-action`’s documented resume signal is `"done"` (`_external/gsd-core/workflows/execute-plan.md:337-348`), not `stopped: …`. On a failed verification GSD says “inform, wait,” not “fail plan without SUMMARY” (`execute-plan.md:348`). `/gsd-execute-phase` only skips plans with SUMMARYs and resumes the first incomplete plan (`execute-phase.md:1454-1457`); it does not promise that a stopped checkpoint is re-presented rather than re-executing prior tasks.

   Recommendation: remove the claimed stop/resume semantics unless they are tested against the exact GSD runtime. Define a documented, D-19-compatible termination path and enumerate what preceding task work is safe to rerun.

5. **D-28’s proposed DevFlow-fix vehicle conflicts with the repository hook.** While `STATE.md` says Phase 49 is current and its worktree exists, staging `crates/**` on any other branch is a hard refusal (`scripts/lint-phase-worktree.sh:65-114`), invoked by pre-commit (`scripts/hooks/pre-commit:127-155`). Thus “`/gsd-quick` … on its own branches” cannot commit a blocking crates/ fix without using the named bypass or changing worktree/state conditions.

   Recommendation: decide in advance whether a blocking fix is made in the Phase 49 worktree, after an explicit phase pause, or with an approved `DEVFLOW_ALLOW_OFF_WORKTREE=1` exception. The context’s current “operator confirms at the stop” leaves the planner unable to produce a legal repair path.

6. **D-20’s claim that Validate runs the payload test is unsupported by DevFlow source.** DevFlow merely tells the agent to run `/gsd-validate-phase {N}` (`prompt.rs:283-321`). It has no project test-command setting and no Cargo test invocation on this path. The copied external material does not include `validate-phase.md`, so the cited source cannot establish that a minimal/non-Rust scaffold executes the plan’s test rather than skips it.

   Recommendation: pin the exact GSD Validate workflow/version and make the scaffold self-test prove both directions: the intended payload test runs, and a deliberately failing payload test yields `verdict: gaps`.

7. **The “Verified” D-03 citation overstates what it proves.** The cited checkpoint predicate is correct: a blocking-human declaration returns `DoesNotHold` (`preflight.rs:1076-1080`), and unattended evaluation applies only at Define and Code (`preflight.rs:963-972`). But cited `preflight.rs:1432-1434` only records the approved checkpoint set; it does not fire the resumed auto-decision. The event is emitted before the resumed spawn (`pipeline_launch.rs:1243-1281`).

   This matters because DECN-03 cannot be closed by event presence. Recommendation: cite both the recording and resume paths separately, and make the evidence plan capture the resumed output before retention/rollover—not merely the event.

8. **D-24 understates irreversible and outward-facing effects.** Before any repository deletion, GSD pushes the feature branch, creates a private PR, later pushes a Ship-note commit, and may push an empty CI-triggering commit (`_external/gsd-core/workflows/ship.md:190-202,370-374,487-521`). These can create GitHub notifications, Actions usage, and persistent remote refs. Deleting the repository is not the only one-way/outward action.

   Recommendation: add explicit operator-owned authorization and evidence steps for repository creation, push/PR/Actions effects, required `gh` scopes (including any `delete_repo` escalation), and cleanup of DevFlow’s per-project registry/cache state.

## SUSPECTED

1. **Negative-control specificity remains incomplete.** The exact intended refusal exists—`"no \`.planning/config.json\` under the launch root"` (`preflight.rs:990-1005`)—and the script still writes that file (`scratch-dogfood-repo.sh:187-191`). But text matching alone can coexist with another aggregated preflight reason (`preflight.rs:1254-1289`). The planned control should also assert persisted `base_branch == "develop"`, `ConfigFile` is absent, no agent-spawn event exists, and the positive arm has the opposite config-source evidence.

2. **“Only a run that ends without reaching Ship counts” is not falsifiable as written.** No named attempt ledger owner, exact Ship-reached event, or adjudicator exists. `merge_result` cannot establish the target, and a Ship-stage failure can occur before finalization. This is a missing decision, not an implementation fact.

3. **The resumed-session capture is persisted, but the runbook capture protocol is missing.** DevFlow writes live stdout under `.devflow/phase-NN-stdout` (`agent_result.rs:3055-3058`) and archives prior generations under `.devflow/history/phase-NN/` (`agent_result.rs:3128-3167`; `pipeline_launch.rs:1036-1060`). That supports later collection, but the context does not specify when to copy the resumed capture, how to associate it with the event session ID, or how to avoid bounded-history pruning.

## CHECKED AND FINE

1. **D-04 is substantively correct, with one stale pinpoint.** Define is an explicit no-op: “There is no agent work to perform here” (`prompt.rs:374-379`). An existing plan makes Plan a no-op without asking for input (`prompt.rs:346-355`). The `:341` citation is only the function start; the operative proof is now `:346-355`.

2. **D-06’s production-base refusal is correct.** `validate_base_branch` rejects `main` and explains that DevFlow must not fork or merge phase work there (`config.rs:341-358`).

3. **D-23 correctly limits `--yes-ship` to the ordinary Ship gate.** It constructs a pre-authorized response only in `handle_ship_outcome` (`pipeline_outcomes.rs:828-852`); it does not cover preflight MAJOR or post-hook recovery gates.

4. **D-14’s cited scaffold and refusal text are current.** The configuration write is at `scratch-dogfood-repo.sh:187-191`; the literal refusal is at `preflight.rs:993-996`.


