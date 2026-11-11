# Adversarial review — `52.1-CONTEXT.md`

Read for: contradictions against backlog/prior phases/code, unstated load assumptions in D-05..D-08 and D-15..D-17, decisions that don't survive the codebase, scope leaks, unfalsifiable acceptance, and the `--force`/pre-push edge cases.

## CONFIRMED

### 1. [Critical] D-06/D-07/D-14 plan to prove a multi-threaded race with a runner that gives every test its own process
D-14 (CONTEXT:87) gates the new tests with `cargo nextest run --no-tests=fail -E 'test(=…)'`. The flake family D-05..D-07 hypothesises is *another test thread's forked child holding a duplicate open file description* — that requires many tests in **one** process. nextest isolates each test in its own process, so the condition cannot arise.

The backlog already contains the negative evidence and CONTEXT does not reconcile it:
```
ROADMAP.md:857: then 10/10 passes run alone under nextest and 3/3 whole-`--bin devflow` runs
                under in-process `cargo test` (669 passed each)
ROADMAP.md:693: 20 of 20 isolated host runs passed.
```
So D-08's "run each fixed test 50 times" (CONTEXT:57) is 50 runs of a runner whose isolation is precisely the thing that hides the bug. 50 green nextest runs are not a weak bound; they are a null measurement.

### 2. [Critical] D-08's evidence step has no mechanism — the tool it names cannot run a named test
CONTEXT:57 says "run each fixed test 50 times under `scripts/check-in-container.sh` load". That script takes one positional (`check-in-container.sh:32 TARGET="${1:-all}"`) and ends with `scripts/check.sh "$TARGET"` (`:155`). `scripts/check.sh:14` accepts only `all|fmt|clippy|test|build|deps`, and `test` is `run_test` → `cargo test --workspace` (`scripts/check.sh:19`). There is no nextest filter and no single-test target. "50 times under container load" therefore means either 50 full workspace container runs, or an undefined ad-hoc harness. D-08 cannot state what was measured, so the number it produces is unreproducible — which is the failure RULE ZERO(b) exists to catch.

### 3. [Major] D-06 fixes one of three racy reads in the exact test 999.221 names
CONTEXT:51 says "replace **the single read** with a bounded poll". The test has **three independent post-release reads**, all labelled `"control: released"`:

```
commands.rs:19062  write_lock_line(&workflow::write_lock_status(&root, phase).unwrap()),
commands.rs:19064         "control: released"
commands.rs:19066  !rendered_status(&state).contains("state write lock"),
commands.rs:19068         "control: released"
commands.rs:19071  !free.iter().any(|finding| finding.detail.contains("state write lock")),
commands.rs:19076         "control: released"
```
The backlog's failure text is only `(control: released)` (ROADMAP:854), so nobody knows which of the three flaked. Polling one leaves the other two single-read, and a green D-08 run then certifies a test still holding the same race.

Additionally, D-05 treats the hypothesis as open while the codebase already asserts it as fact two files away — `workflow.rs:1067-1068`: *"child holds a duplicate of every descriptor, so `cargo test`'s threads can keep a just-released `flock` busy for a moment. A real leak outlasts this."* — and `workflow.rs:1069 wait_for_free_write_lock` is already the 2 s / 5 ms poll D-06 specifies. D-05's "research step" is checking something already decided in the tree, without saying it must reconcile with it.

### 4. [Major] D-05's specified check can only ever return "not confirmed" — it has no negative control
CONTEXT:47: "the research step checks … **which fds are opened without `O_CLOEXEC`**, and which test helpers fork without exec."

- No `custom_flags` appears anywhere under `crates/` (`rg custom_flags` → 0 hits), so nothing overrides std's default; Rust std opens every `File` with `O_CLOEXEC`, and the one explicit open already uses it (`supervisor/runtime.rs:529 libc::pipe2(fds, libc::O_CLOEXEC | libc::O_NONBLOCK)`).
- `O_CLOEXEC` closes at **exec**. The hypothesis lives in the **fork→exec** window, which CLOEXEC by definition does not close.

So the specified search returns "none found" by construction and reads as a refutation of a live hypothesis. The only code that widens the window is the `pre_exec` closures — `liveness.rs:240`, `liveness.rs:666`, `liveness.rs:1549`, `agents/opencode.rs:392` — and `opencode.rs:176` even documents that pre_exec work runs after fork. D-05 names the wrong artefact and gives the step no case that must produce the opposite result.

### 5. [Major] D-13 as written leaves the reported defect unfixed, and narrows coverage
D-13 (CONTEXT:82) assigns all the work to the hook: "reads the manifest from that commit (`git show <sha>:.workspace-divergence`), not the working tree." The *hook* cannot do that — `check-workspace-divergence.sh` is a separate program that still hardcodes the other end of the comparison:

```
check-workspace-divergence.sh:21   BASE_REF="${1:-origin/develop}"        # only one argument
check-workspace-divergence.sh:79   done < <(git diff --name-status --no-renames "$BASE_REF" HEAD)
check-workspace-divergence.sh:39   if [ -f .workspace-divergence ]; then  # working tree
check-workspace-divergence.sh:99   done < <(git ls-files -s -- ...)       # index, not the pushed commit
```
Passing `local_sha` to a script that diffs `$BASE_REF HEAD` reproduces 999.231 exactly (the defect was "the check diffs `origin/develop` against `HEAD`", ROADMAP:705-707).

Three unstated edge cases D-13 must also decide:
- A `refs/heads/workspace/*` push whose commit has **no** `.workspace-divergence` is legal today (`pre-push:208 [ -f .workspace-divergence ]` → skip). Under `set -euo pipefail` plus `git show <sha>:…`, that becomes a hard refusal — an unannounced behaviour change for every workspace branch without a manifest.
- **New fail-open:** coverage moves from "checkout carries the manifest" to "remote ref is `workspace/*`". `pre-push:88` lists only `develop|main|feature/*|fix/*|chore/*|release/*`; a workspace branch pushed to any other ref name (e.g. `origin workspace/x:denniyahh`) is now checked by neither gate. 999.231's fix shape says "only for `refs/heads/workspace/*`"; nothing in D-13 notes that this *removes* the only check that ran there.
- `CONTRIBUTING.md:57-62`: "A `pre-push` drop-in gets no stdin, because the hook has already read the ref list." Any rewrite must keep that contract; D-13 doesn't mention it.

### 6. [Major] D-09's Integration Points list is incomplete, and D-09 contradicts D-10 about naming destructive flags
CONTEXT's Integration Points names three sites (`restart_verb`, the finished-`resume` refusal, the finished-`advance` hint). The operator strings that actually survive are more, and include the two the D-16 proof run reads:

```
commands.rs:1358   "pass --force to remove the worktree and the run's state (a branch with …"   # finished
commands.rs:1366   "… or pass --force to discard the worktree and the run's state …"           # stopped
phase_worktree.rs:61  "worktree already exists at {} — use --force to recreate it …"
```
`kept_stopped_worktree_line` (`commands.rs:1351-1370`) is *the* line a finished-run close-out prints, and it is not in the list.

Worse, D-10 (CONTEXT:73) keeps the Phase-52 refusal "as an invariant" while D-09's Specific Ideas demand "no routine path that names a destructive flag". That refusal's text is a routine path naming two:
```
phase_worktree.rs:101   "git worktree remove --force {} && git branch -D {branch}"
phase_worktree.rs:107   "refusing --force: branch `{branch}` {why} … or delete it yourself with `{delete}` …"
```
After D-09.5 removes `cleanup --force`, this message is the operator's *only* sanctioned way out of an unmerged phase branch — and it is raw `git worktree remove --force && git branch -D`, listing nothing, which is the exact shape 999.224 was filed to end.

### 7. [Major] D-09.1 ("start continues") contradicts 52-CONTEXT D-35 and the code path it must run through
`start` unconditionally builds a brand-new state:
```
commands.rs:374  let mut state = fresh_state_carrying_phase_failures(project_root, phase, agent, mode);
```
So "`start` … continues on its worktree and branch" (CONTEXT:64) keeps the worktree while discarding stage, gate, session id, checkpoint approvals and stop reason — or it is `resume` under a second name. D-09.1 never says which. That is not a wording nit: 52-CONTEXT **D-35** states the opposite policy for the same input — *"a new `start` on the phase says it replaces the finished state"* — and neither document amends the other.

It also deletes a safety property 999.219 relies on: "`devflow start` in worktree mode cannot hit it in the same run (its repair runs before that run's Plan, **and an existing phase worktree makes `start` refuse**)" (ROADMAP:903-904). D-02 puts 999.219 in scope; D-09.1 removes the reason it can't fire in the primary mode. Nothing in the CONTEXT connects the two.

### 8. [Major] D-09.5 + cleanup's existing silent no-op make the D-16 acceptance unprovable
With `--force` gone, the stale/probe-failed path becomes a print-and-continue, and cleanup still exits 0 after doing nothing:
```
commands.rs:1264   if !force && matches!(phase_liveness, PhaseLiveness::Stale | PhaseLiveness::ProbeFailed(_))
commands.rs:1273-1277   println!("keeping worktree … run `devflow stop …` first, …")
commands.rs:1279   continue;
commands.rs:1311   if removed == 0 {
commands.rs:1312       println!("no worktrees to clean up");
```
That is 999.133 verbatim — *"`devflow cleanup` counts only successful removals. When every worktree removal fails, it prints the per-worktree warning, then 'no worktrees to clean up', and exits 0"* (ROADMAP:2452-2454) — an entry **outside** this phase's scope set (D-01..D-04).

D-16 (CONTEXT:98) then accepts on stdout text from a command documented to report success after a no-op, with no negative control. Nothing in D-16 shows that a *merged* branch is deleted on the same path, so "cleanup keeps the unmerged branch" is indistinguishable from "cleanup does nothing ever". Note also that on a finished run the code never reaches the branch at all — it `continue`s at the worktree (`commands.rs:1228-1232`), so the branch survives by skipping, not by the `kept_unmerged_branch` logic D-10 wants exercised.

### 9. [Major] D-11 is internally unsatisfiable
CONTEXT:76: "The flag is removed outright, **and the refusal for the old spelling names the new verb**." Both flags are plain clap bools (`main.rs:61-62` for `Start`, `main.rs:227-228` for `Cleanup`). Remove them and clap emits `error: unexpected argument '--force' found` plus its own usage line — it never names `devflow discard`. Obtaining a refusal that names the new verb requires *keeping* a `--force` argument (hidden, rejecting) — i.e. not removing it. Also unmentioned: the existing test asserting the old text, `commands.rs:16986 assert!(refused.contains("use --force"))`.

### 10. [Major] D-14's factual premise is wrong, and it ignores the existing harness convention
CONTEXT:87: "Neither script has tests today." `pre-push` does: `crates/devflow-cli/tests/pre_push_signing_policy.rs` asserts on the hook's source (it exists specifically because the hook's `|| true` was load-bearing), and `crates/devflow-cli/tests/script_modes.rs` pins the hook's committed mode. Both are structural, not behavioural, so the *plan* (drive the real script against a temp repo) is still new — but the planner is being told the file is untouched territory when there is an established idiom, a `repo_root()` helper and a stated reason for choosing source-assertion over execution that D-14 should override explicitly rather than silently.

### 11. [Minor] D-12 vs 999.167 vs phase 52.1's own close-out
`SCOPE_RE` accepts only a numeric plan suffix:
```
cut-pr-branch.sh:134   SCOPE_RE="^[a-z]+\((phase-)?0*${PHASE_ESC}(-[0-9]+)?\)!?: "
cut-pr-branch.sh:137   if [[ "$SUBJECT" =~ $SCOPE_RE ]]; then
cut-pr-branch.sh:140   done < <(git log --reverse --no-merges --format='%H%x09%s' "$BASE_REF..$CURRENT_BRANCH")
```
Three gaps the CONTEXT leaves to the planner:
- D-12 says "Pair it with 999.167 as the entry says" (CONTEXT:81), but D-01's scope set excludes 999.167. So widening the filter to `(NN-<word>)` is either a scope leak or 999.230 is only half-fixed — and a half-fix still halts silently on the *next* `(52.1-review)`-style commit.
- For this dotted phase, 999.167's shape doesn't help either: `(52-review)` does not match `0*52\.1`, so D-18's "Run the close-out with the fixed script, from this phase's own head" (CONTEXT:109) can still drop this phase's own review fixes. No test is specified for `PHASE=52.1` + review scope.
- `--include <sha>` needs an ordering rule (the list is `--reverse`) and a rule for a sha already in `$BASE_REF..$CURRENT_BRANCH` or outside it. Neither is stated.

### 12. [Minor] D-02 is presented as both open and operator-settled, against a live ROADMAP disposition
CONTEXT:8 asserts the operator "accepted the scope as written … including 999.219", while D-02 (CONTEXT:38-40) is marked `[auto]` and still says "If the operator wants it on its own branch, drop it from planning." The roadmap records the opposite placement and nobody updated it:
```
ROADMAP.md:917  **Disposition (operator, 2026-11-10):** a separate fix branch after Phase 52, not folded into the phase.
```
A planner resolving scope conflicts from ROADMAP will drop 999.219; a planner reading only CONTEXT will keep it. Same shape for D-03/999.229.

### 13. [Minor] D-15 does not pin which scaffold mode, and the mode it describes is the pre-v4 one
CONTEXT:92 says "A throwaway project from `scripts/scratch-dogfood-repo.sh`", and the code-context note (CONTEXT:147-150) describes the *default* mode ("its header comment predates the v4 cuts"). That mode's own header still says so:
```
scratch-dogfood-repo.sh:1146   Define->Ship pipeline mechanism
```
and it ships none of the artefacts the later `--live-payload` mode added for exactly this run:
```
REQUIREMENTS.md   — only live-payload (scratch:833)
01-VALIDATION.md  — only live-payload (scratch:1043); its --check asserts a specific row (scratch:654)
config.json       — default: {"workflow": {}} (scratch:1251-1255)
                    live-payload: 15 keys, each with a stated reason (scratch:989-1017)
```
Conversely, live-payload (which is the only mode with those keys) opens its plan with `gate="blocking-human"` (`scratch:915`), and in supervise mode a human-only checkpoint lands on the checkpoint gate rather than proceeding (`pipeline_launch.rs:3797-3799` and `:3864`, "Supervise mode requires a human decision for the declared blocking-human checkpoint"). So D-15's scripted single-`gate approve` sequence describes neither mode exactly, and the CONTEXT never says which one the proof run uses. Note `{"workflow": {}}` is *not* a problem for the chain flag — `AutoChainGuard` restores its own write byte-for-byte (`pipeline_launch.rs:774-791`), which I checked and is why this is Minor rather than Critical.

## SUSPECTED

### 14. [Major, speculation] D-09.3's replacement verb is untested in the case that motivates it
D-09.3 sends a "stale or unprobeable supervisor" to `devflow stop`. `stop` does branch on `Liveness::Stale` (`commands.rs:3309`, `:3347 stop_stale_launch`), so the verb exists — but nothing in this phase's scope exercises `stop` against Stale/ProbeFailed, and `ProbeFailed` is not covered by that match. Removing `cleanup --force` for two liveness classes without a test for either class is the same shape as D-08's weak bound: a decision whose failure mode is discovered during the Phase 53 dogfood run, not here.

### 15. [Minor, speculation] D-15's pass criterion has no evidence source
"Pass requires no hand-run stage, no edit under `.devflow/`, and no process killed by hand" (CONTEXT:95-96). No artefact is named. "No hand-run stage" is not observable from the finished state (an external `/gsd-*` run leaves no DevFlow event), and "no process killed by hand" needs a `dmesg`/shell-history negative control that nothing specifies. As written it reduces to the operator's recollection.

### 16. [Minor, speculation] A defect found *after* a green proof run is not said to invalidate it
D-15 closes "on one live supervised run" (CONTEXT:92) and D-17 pins the binary to "this phase's head" (CONTEXT:100). Nothing says what happens when the run passes and the post-run cleanup (D-16) or a filed `999.x` defect then produces a commit: the proof then refers to a head that is no longer the phase head. Given this repo's history of one-green-run sign-offs, that should be stated (either "the proof run is the last commit before close-out" or "a later fix re-runs it").

### 17. [Minor, speculation] D-12's skipped-commit listing has no specified population
"list every non-planning commit in the range that the scope filter skips" (CONTEXT:80) requires applying `FORBIDDEN_REGEX` (`cut-pr-branch.sh:119-121`) to every skipped commit — but the script only computes `NON_FORBIDDEN` *inside* the included loop (`:186-190`), so the listing duplicates that logic outside the replay. If the two copies drift, the listing can either hide a dropped code commit (the 999.230 failure) or list a planning-only commit the filter is right to skip. D-12 gives no test that the listing and the filter agree, and no negative control that a genuinely dropped code commit is named.

---

**Method note:** every claim above is read from the snapshot's files at the cited lines. What this does *not* establish: I did not execute any of these commands, so the `nextest` isolation consequence (#1), the clap message text (#9) and the GSD/supervise interaction in #13 are read from documented tool behaviour and source, not reproduced in this session. Also unverified, and deliberately dropped rather than asserted: my first hypothesis that the default scaffold's missing `_auto_chain_active` key would dirty the tree and block the Validate PASS — `AutoChainGuard::restore_own_write` (`pipeline_launch.rs:774-791`) restores the pre-write bytes, so that mechanism does not hold.
