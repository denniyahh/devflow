---
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
reviewed: 2026-09-02T12:42:26Z
depth: deep
files_reviewed: 14
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/parallel.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/ship_evidence.rs
  - docs/guides/unattended-mode.md
  - OPERATIONS.md
findings:
  critical: 4
  warning: 8
  info: 4
  total: 16
status: issues_found
---

# Phase 45: Code Review Report

**Reviewed:** 2026-09-02T12:42:26Z
**Depth:** deep (cross-file: import graph, call chains through `GitFlow`/`GitFlowConfig`, prompt renderer dispatch)
**Files Reviewed:** 14
**Status:** issues_found

## Summary

Phase 45 makes the git-flow trunk configurable (`base_branch`), narrows the
self-dogfood staleness predicate to Cargo workspace members, and adds an
unattended decision policy to the Code prompt. The five carried-forward
round-2 findings I was asked to adjudicate are largely resolved or not
applicable, and the shipped tests carry real negative controls — that part of
the work is genuinely better than the plans were.

The defects are in the **blast radius of the trunk substitution**, which was
applied by mechanically swapping `GitFlow::new` → `GitFlow::for_project` at
call sites without auditing what each downstream method does with
`config.develop`. Two of those methods use `develop` as a *protected-branch
list* and as a *merge target*, not merely as a fork point:

- `cleanup_merged` protects `[main, config.develop]`. With a configured base,
  `develop` leaves the protected set while simultaneously appearing in
  `git branch --merged <base>` — `devflow cleanup` force-deletes it (CR-01).
- The resolved base is never persisted to `State`, so an env-sourced base is
  authoritative for `start` and silently absent from a later manual
  `advance`/`resume`, which then merges phase work into `develop` (CR-02).

Separately, 45-03's prompt policy is delivered to only one of the two
renderers on the loop-back path (CR-03) and directly contradicts the
pre-existing checkpoint-resume prompt injected into the same session (CR-04).

Baseline noted and not re-litigated: `cargo test --workspace` 1225 passed,
clippy clean, fmt clean on 8c86c8c. None of the findings below are build or
lint breakage; every one of them passes the current suite.

## Adjudication of carried-forward round-2 findings

Per-item verdicts against the **shipped code**, as required.

| # | Round-2 claim | Verdict | Citation |
|---|---|---|---|
| 1 | Scoped staleness rule contradicts its own `..` negative control | **CONFIRMED (narrow)** — see WR-02 | `staleness.rs:267-269` vs `staleness.rs:232-241` |
| 2 | Required RED test cannot be produced by deleting the match arm | **PARTIALLY RESOLVED** — see WR-05 | `preflight.rs:513-518`, `preflight.rs:566-570`, `preflight.rs:3288-3311` |
| 3 | Base validation permits commit-ish aliases and production refs | **RESOLVED at `start`, CONFIRMED elsewhere** — see WR-01 | `commands.rs:147-171`, `commands.rs:330-333` (only production call site) |
| 4 | Operator output still says branches are behind `develop` | **RESOLVED** (one cosmetic residue, IN-02) | `commands.rs:2180-2201`, `commands.rs:371-380`; residue at `commands.rs:1132` |
| 5 | SUSPECTED: monitor commit enumeration resolves config from wrong root | **RESOLVED** | `monitor.rs:1254-1261` (config from `project_root`, `git log` in `workdir`); pinned with a swap-the-arguments negative control at `monitor.rs:1588-1597` |
| 6 | SUSPECTED: `main` refusal bypassed by resolver fail-soft fallback | **RESOLVED for `main`; CONFIRMED for aliases** | `config.rs:368-389` is fail-hard on an explicit value; `config.rs:404-418` still falls soft, and `validate_base_branch` (`config.rs:325-343`) does not reject `origin/main` / `refs/heads/main` — see WR-01 |

Round-2 items marked VERIFIED-and-fixed, re-checked in shipped code:

- **F3 (`--no-worktree` forking from hardcoded `develop`)** — **FIXED.**
  `commands.rs:444` now calls `GitFlow::for_project(project_root)` before
  `feature_start`/`feature_start_force`, and `GitFlow::for_project`
  (`git.rs:146-151`) routes through `config::git_flow_for_project`. Pinned by
  `commands.rs:5540` (`no_worktree_start_forks_the_feature_branch_from_the_configured_base`)
  with a `merge-base --is-ancestor` negative control.
- **F5 (`commands::reference` ignoring the configured base)** — **FIXED.**
  `commands.rs:657` resolves the default through
  `config::git_flow_for_project(project_root).develop`. Note: no test covers
  this fix (IN-04).

---

## Critical Issues

### CR-01: `devflow cleanup` force-deletes `develop` on any project with a configured base branch

**File:** `crates/devflow-cli/src/commands.rs:780` → `crates/devflow-core/src/git.rs:343-345`

**Issue.** Phase 45 changed `commands::cleanup` from `GitFlow::new` to
`GitFlow::for_project` (`commands.rs:777-780`). `cleanup_merged` then does two
things with `config.develop`:

```rust
let output = self.git_output(["branch", "--merged", &self.config.develop])?;
let protected = [self.config.main.as_str(), self.config.develop.as_str()];   // git.rs:345
```

With `base_branch = "workspace/example"`, `protected` becomes
`["main", "workspace/example"]` — **`develop` is no longer protected** — while
the listing baseline moves to the planning branch, which for the motivating
use case (a personal branch kept ahead of `develop`) has `develop` as an
ancestor. Deletion uses `-D` (`git.rs:364`), so unmerged-ness is not a barrier.

Verified empirically with a negative control (real git repo, `develop` →
`workspace/example` one commit ahead):

```
$ git branch --merged workspace/example      # baseline used by cleanup_merged
  develop
* workspace/example
$ git branch --merged develop                # NEGATIVE CONTROL: pre-45 baseline
  develop                                    # (protected, so nothing deleted)
```

`develop` is listed, is not protected, and is force-deleted. `devflow cleanup`
needs no `--force` to reach this: the `force` flag gates worktree removal, not
`cleanup_merged`, which runs unconditionally at `commands.rs:871`.

The commit comment at `commands.rs:777-779` asserts the opposite outcome
("or this sweep can delete a branch that was never merged"); the change in fact
*creates* deletion of a branch that must never be swept.

**What this does not establish:** if the configured base has *diverged* from
`develop` (not a fast-forward descendant), `--merged` will not list `develop`
and nothing is deleted. The loss is reachable, not universal. It is also
recoverable via reflog — by an operator who knows to look.

**Fix.** Make the protected set independent of the configured trunk, and keep
the built-in constants in it:

```rust
// git.rs, cleanup_merged
let protected = [
    self.config.main.as_str(),
    self.config.develop.as_str(),
    crate::config::MAIN,      // never sweep the production branch
    crate::config::DEVELOP,   // never sweep the default integration branch
];
```

Add a regression test with the shape of the probe above: configure
`base_branch = "workspace/example"` on a repo where `develop` is an ancestor,
run `cleanup_merged`, and assert `develop` still exists — plus the negative
control that an ordinary merged `topic` branch *is* deleted, so the test cannot
pass against a `cleanup_merged` that deletes nothing.

---

### CR-02: the resolved base branch is not persisted, so `advance`/`resume` can merge phase work into the wrong trunk

**File:** `crates/devflow-cli/src/commands.rs:325-333`, `crates/devflow-core/src/state.rs` (no field), `crates/devflow-cli/src/pipeline_launch.rs:1474`, `crates/devflow-core/src/hooks.rs:181`

**Issue.** `DEVFLOW_BASE_BRANCH` is a documented, first-class configuration
route — `OPERATIONS.md:117` and `docs/guides/unattended-mode.md:117` both tell
operators to export it, and it outranks `devflow.toml`. `commands::start`
resolves it at `commands.rs:325`, validates it, forks the worktree from it —
and then **discards it**. There is no `base_branch` field on `State`
(`state.rs` has `worktree_path:281` and `yes_ship:341`, nothing else relevant).

Every later entry point re-resolves from ambient config:

- `pipeline_launch::advance:1474` → `git_flow_for_project(project_root)`
- `pipeline_outcomes::run_checkout_hooks:1062` → same, feeding
  `HookContext.git_flow` → `hooks::merge_feature:181` →
  `GitFlow::merge_feature_into_develop` → `git checkout <trunk>; git merge --no-ff <branch>`
  (`git.rs:185-191`).

Failure scenario, entirely within documented usage:

1. `DEVFLOW_BASE_BRANCH=workspace/me devflow start 45` — worktree forks from
   `workspace/me`, note printed, monitor inherits the env.
2. The monitor dies (a documented condition — `devflow doctor` reports
   `Liveness::Stuck`). Operator opens a fresh shell without the export and runs
   `devflow resume --phase 45`, the documented recovery verb.
3. `git_flow_for_project` now resolves `develop`. Ship's `MergeFeature` hook
   merges `feature/phase-45` into `develop` instead of `workspace/me`, and
   `is_merged_into_develop` (`git.rs:197-208`) confirms success against the
   wrong branch, so nothing refuses.

This directly falsifies the contract asserted at `config.rs:104-107` ("both
resolve from this one value via `git_flow_for_project`, so they can never
disagree"). The guarantee holds within one process's environment only. The
phase's own precedent argues against the current shape: `yes_ship` **is**
persisted to `State` at `commands.rs:288`, for exactly this reason ("the
persisted authorization exists before the detached monitor that will later
consult it is ever spawned" — `commands.rs:278-284`).

**Fix.** Persist the resolved base on `State` alongside `yes_ship`, and have
the post-`start` sites prefer it:

```rust
// state.rs
/// The integration trunk resolved at `start` (45-01/D-01). `None` for states
/// written before this field existed; those fall back to the resolver.
pub base_branch: Option<String>,

// commands.rs::start, next to `state.yes_ship = ...`
state.base_branch = Some(resolved_base.value.clone());

// pipeline_launch::advance / pipeline_outcomes::run_checkout_hooks
let git_flow = match state.base_branch.as_deref() {
    Some(base) => GitFlowConfig { develop: base.to_string(), ..Default::default() },
    None => devflow_core::config::git_flow_for_project(project_root),
};
```

Regression test: write a `State` with `base_branch: Some("workspace/example")`,
clear `DEVFLOW_BASE_BRANCH` and write no `devflow.toml`, and assert
`merge_feature` targets `workspace/example`. Negative control: the same state
with `base_branch: None` must target `develop`.

---

### CR-03: the unattended decision policy is missing from the Claude/OpenCode loop-back Code prompt

**File:** `crates/devflow-core/src/prompt.rs:396-399` vs `crates/devflow-core/src/prompt.rs:466-476`; dispatch at `crates/devflow-cli/src/pipeline_gate.rs:198-203`

**Issue.** `CODE_STAGE_POLICY` reaches two renderers asymmetrically once a
`FixType` is present:

- `render_claude_style` (`prompt.rs:396-399`): `Code { fix: Some(_) }` →
  `fix_prompt` (`prompt.rs:567-580`), which contains **no policy** for any
  variant, including `FullExecute`.
- `render_workflow_style` (`prompt.rs:466-476`): `Some(FixType::FullExecute) | None`
  share one arm and **do** carry `CODE_STAGE_POLICY`.

`FixType::FullExecute` is a real production dispatch, not a theoretical one —
it is what a mid-arc phase loops back with (`pipeline_outcomes.rs:394`,
`pipeline_outcomes.rs:3443-3444`), and `prepare_loop_back_to_code` renders it
through the agent's own driver at `pipeline_gate.rs:198-203`.

So a Claude/OpenCode unattended run that fails Validate and loops back into a
full `/gsd-execute-phase {N} --auto` re-run gets **no** unattended-decision
policy at exactly the stage where GSD's decision checkpoints occur — while a
Codex/Pi run in the identical situation does. That contradicts the phase's own
stated contract, which the test at `prompt.rs:781-812` writes down as "only
full-execute Code prompts may carry the shared policy": the Claude-style
`FullExecute` prompt *is* a full-execute Code prompt (`prompt.rs:573-575`
renders the same `/gsd-execute-phase {phase}` command) and does not carry it.

The tests cannot catch this. `code_policy_is_identical_across_both_renderers`
(`prompt.rs:761-776`) exercises `fix: None` only, and
`code_policy_is_absent_from_prompts_that_must_not_carry_it` (`prompt.rs:781-812`)
exercises only the *workflow-style* `gaps_only`/`audit_fix` — the Claude-style
`FullExecute` prompt is asserted by neither.

**Fix.** Append the shared policy to `fix_prompt`'s `FullExecute` arm so both
renderers agree:

```rust
FixType::FullExecute => format!(
    "Validation reported issues. Run the fix command for this loop:\n\n    {command}\n\n\
     {CODE_STAGE_POLICY}\n\n{COMPLETION_PROTOCOL}"
),
```

and extend `code_policy_is_identical_across_both_renderers` to cover
`fix: Some(FixType::FullExecute)` on both renderers, keeping `GapsOnly` /
`AuditFix` in the absence test as the negative control.

---

### CR-04: `CODE_STAGE_POLICY` forbids the exact self-resolution `checkpoint_auto_decide_prompt` demands, in the same session

**File:** `crates/devflow-core/src/prompt.rs:86-89` vs `crates/devflow-core/src/prompt.rs:537-549`; injection at `crates/devflow-cli/src/pipeline_launch.rs:1095-1100`

**Issue.** The new policy says:

> "This authority does not extend to a `blocking-human` gate or a
> package-verification checkpoint. Those remain human-only: do not self-resolve
> or approve them; report them instead." (`prompt.rs:86-89`)

The pre-existing resume prompt says:

> "You previously stopped at a human-blocking checkpoint, but no human operator
> is available... DevFlow's policy is for you to resolve the checkpoint
> yourself, using your own best judgment, and continue the work."
> (`prompt.rs:539-544`)

These are the *same construct*: `agent_result.rs:615` defines
`HUMAN_GATE_VALUE = "blocking-human"`, and `relaunch_checkpoint_session` fires
only after `blocking_human_checkpoint_reported` confirms that literal token.

And they are delivered into the *same conversation*:
`relaunch_checkpoint_session` (`pipeline_launch.rs:1095-1100`) resumes the
exited session by `session_id` — its own doc comment says resuming "preserves
the original session's conversation context". `CODE_STAGE_POLICY` is still in
that context, stated as an absolute prohibition, when the contradicting
instruction arrives.

The doc comment at `prompt.rs:57-61` asserts the two are "complementary, not
duplicates". On the `blocking-human` clause that assertion is false, and
nothing in the code or the tests reconciles them: `code_policy_excludes_blocking_human_and_package_checkpoints`
(`prompt.rs:755-765`) pins the prohibition, `checkpoint_auto_decide_prompt_states_no_operator_judgment_and_record_reasoning`
(`prompt.rs:1008-1023`) pins the grant, and no test compares them.

The consequence, if the agent honours the earlier and more specific rule, is
that the resumed session re-reports the checkpoint instead of resolving it —
which is the run stalling on exactly the recovery path 999.94/AUTO-03 exists to
unstick, with `checkpoint_resumes` incrementing each round.

**What this does not establish:** I have proved the textual contradiction and
the shared session, not a measured stall. Model instruction-priority is not
determined by reading source. But an unattended-mode contract that depends on
an undocumented recency heuristic is not a contract.

**Fix.** Make the resume prompt explicitly supersede the standing policy, and
pin the relationship with a test:

```rust
pub fn checkpoint_auto_decide_prompt(phase: PhaseId) -> String {
    format!(
        "This is phase {phase} of a headless DevFlow run. ... \
         This instruction SUPERSEDES the earlier `Unattended decision checkpoints` \
         policy in this conversation, which withheld authority over `blocking-human` \
         gates: DevFlow has now confirmed no operator is available, so that \
         authority is granted for this checkpoint. ..."
    )
}
```

plus a test asserting the resume prompt names the same `blocking-human` token
the Code policy withholds, and states supersession — with a negative control
that the Code policy alone does not contain the supersession language.

---

## Warnings

### WR-01: `ensure_base_is_a_local_branch` guards only `start`; every other trunk consumer accepts an unvalidated commit-ish

**File:** `crates/devflow-cli/src/commands.rs:147-171`, sole production call site at `crates/devflow-cli/src/commands.rs:330-333`

**Issue.** `validate_base_branch` (`config.rs:325-343`) rejects `main`, blank,
and `-`-prefixed values only. The spelling bypass (`origin/main`,
`refs/heads/main`, `HEAD`, a bare SHA) is closed by
`ensure_base_is_a_local_branch` — which lives in the CLI crate and is called
from exactly one place. Confirmed by grep: `commands.rs:332` is the only
non-test occurrence.

`git_flow_for_project` (`config.rs:404-418`) therefore hands
`develop: "refs/heads/main"` to `hooks::merge_feature`, `commands::cleanup`,
`monitor::enumerate_phase_commits` and `ship_evidence::collect` whenever a
run reaches them without having passed through `start` in the same environment
(the CR-02 scenario, or a `devflow.toml` edited mid-run). `merge_feature_into_develop`
then runs `git checkout refs/heads/main`, which **detaches HEAD**, merges onto
the detached commit, and the post-merge ancestry check (`hooks.rs:210-222`)
correctly refuses — leaving the main checkout detached with an orphaned merge
commit. It fails closed on the merge, but leaves the repository in a state the
operator did not ask for and is not told how to recover from.

The doc comment at `config.rs:399-403` acknowledges the shape ("That fallback
is not a hole only because `commands::start` refuses on the same `Err`") but
the guard that closes the *spelling* bypass is not part of that `Err` — it is a
separate CLI-layer check `git_flow_for_project` never runs.

**Fix.** Move the local-branch requirement into `config::validate_base_branch`
so it travels with every resolution, taking the repo root as a parameter, or
add a cheap ref-shape refusal to the pure validator (`refs/`, `HEAD`, a
40-hex-digit string, and any value containing `/` whose first segment names a
configured remote) so `git_flow_for_project` cannot yield one.

### WR-02: the `..` rejection in `affects_compiled_binary` contradicts the documented fail direction and is unreachable

**File:** `crates/devflow-cli/src/staleness.rs:267-269` (rule) vs `crates/devflow-cli/src/staleness.rs:232-241` (doc)

**Issue.** The doc block states the predicate "fails toward Stale everywhere it
is uncertain". The `..` branch does the opposite: it returns `false` (= not
build-affecting = toward Fresh) for any path with a `..` segment. That is safe
for an *escaping* path (`crates/../foo.rs`), which is what the test at
`staleness.rs:685-688` asserts — but not for a *re-entrant* one:
`crates/devflow-core/../devflow-cli/src/main.rs` normalizes to a genuine
workspace member and is classified not-build-affecting, i.e. a real stale build
reads Fresh.

That is not exploitable, because neither producer emits `..`. Verified
directly, with the `..` deliberately injected into the pathspec:

```
$ git status --porcelain -- crates/devflow-core/../devflow-core/src/lib.rs
 M crates/devflow-core/src/lib.rs      # normalized
$ git diff --name-only
crates/devflow-core/src/lib.rs         # normalized
```

So the branch is dead code justified by a comment claiming it is load-bearing
("without this it would classify as a workspace member while escaping the
workspace entirely" — `staleness.rs:261-263`), and its only reachable effect
would be in the unsafe direction. Note that `porcelain_tracked_path`'s doc
(`staleness.rs:192-198`) explicitly *rejects* the sibling `./`-stripping branch
on precisely the "no test could reach it in the failing direction" ground; the
two decisions are inconsistent with each other.

**Fix.** Either delete the branch and record the producer-normalization
argument where the `./` rejection is recorded, or — if it is kept as
defence-in-depth — invert it to `return true` (fail toward Stale, matching the
stated direction) and update the test's expectation and cost string.

### WR-03: `WORKSPACE_MEMBER_PREFIX` hardcodes `crates/` while the member list is already parsed

**File:** `crates/devflow-cli/src/staleness.rs:24`, `crates/devflow-cli/src/staleness.rs:280`

**Issue.** `is_self_dogfood_workspace` (`staleness.rs:328-367`) already reads
the real `members = [...]` array out of the root manifest, then throws it away;
`affects_compiled_binary`'s scoped branch re-derives scope from a hardcoded
`"crates/"` string. Add a workspace member outside `crates/` — a `tools/`,
`xtask/` or `fuzz/` crate — and `is_self_dogfood_workspace` still returns
`true` (it only requires the two known members to be *present*), so the hard
block stays armed while every source file in the new member silently stops
counting as a build input. That is the exact "narrowing that lost a reachable
true positive" the doc block at `staleness.rs:238-241` says would be worse than
the false positive being fixed.

**Fix.** Return the parsed member paths from `is_self_dogfood_workspace` (or a
sibling `workspace_member_prefixes`) and match against those, falling back to
`crates/` only when parsing fails. Test with a fixture whose `members` includes
`"tools/xtask"` and assert `tools/xtask/src/main.rs` is build-affecting, with
`vendor/tools/xtask/src/main.rs` as the negative control.

### WR-04: a comment containing the word "members" silently degrades the D-18 hard block to a warning

**File:** `crates/devflow-cli/src/staleness.rs:337-354`

**Issue.** WR-05's fix anchors on the first occurrence of `members` not
preceded by an identifier character — which excludes `default-members`, but not
prose. A root `Cargo.toml` such as:

```toml
[workspace]
# keep these members sorted
resolver = "2"
members = ["crates/devflow-core", "crates/devflow-cli"]
```

anchors on the comment's `members`, then takes `rest.find('[')` — which lands
on the `[` of the *members array two lines later* only by luck of ordering; put
the comment above a `[workspace.package]` header and the scan reads
`workspace.package` as the array body, `has_member` fails, and
`is_self_dogfood_workspace` returns `false`. `staleness_outcome`
(`staleness.rs:381-389`) then downgrades `Block` to `Warn` — the safety gate
disappears with no error, no warning, and every existing test still green
(their fixtures all put `members = [...]` first, which is the same blind spot
WR-05 was written to fix).

DevFlow's own manifest is currently safe (`Cargo.toml:3` is the first
occurrence), so this is a latent trap rather than a live defect.

**Fix.** Require the anchor to be followed by optional whitespace and `=`
before accepting it, and skip `#`-comment lines entirely:

```rust
let members_start = contents
    .lines()
    .scan(0usize, |off, line| { let s = *off; *off += line.len() + 1; Some((s, line)) })
    .find(|(_, line)| {
        let t = line.trim_start();
        !t.starts_with('#') && t.starts_with("members") && t[7..].trim_start().starts_with('=')
    })
    .map(|(offset, _)| offset)?;
```

Test with the commented fixture above as the RED case and the existing
`default-members` fixture retained as the regression control.

### WR-05: the fail-open warning's emission site is unpinned; only the string builder is tested

**File:** `crates/devflow-cli/src/preflight.rs:566-570`, test at `crates/devflow-cli/src/preflight.rs:3288-3311`

**Issue.** Round-2 finding 2 asked for a RED test that the `Undeterminable`
arm's `fail-open` warning is actually emitted. The shipped fix extracts
`undeterminable_currency_warning` and asserts its *text*
(`preflight.rs:3288-3311`), which is a real improvement over the plan — but the
call site is still unpinned. Deleting `println!("{}", undeterminable_currency_warning(base));`
at `preflight.rs:569` leaves both the disposition test
(`ensure_base_ref_current_fails_open_for_a_local_only_planning_branch`,
which asserts only `is_ok()`) and the text test green. The function's own doc
comment (`preflight.rs:506-511`) states the limitation accurately, which is
good practice, but does not close it.

A mechanical backstop probably exists — `undeterminable_currency_warning` is
`pub(crate)` with no other production caller, so `dead_code` under
`-D warnings` should fire on the non-test build. I did not run that experiment,
so treat it as plausible, not established; and it would catch deletion of the
call, not a rewording that drops "fail-open".

**Fix.** Give `ensure_base_ref_current` the same injectable-writer treatment
`unattended_launch_check_reporting_to` already uses in this file
(`preflight.rs:1140-1144`) — an inner `ensure_base_ref_current_reporting_to(
project_root, base, &mut dyn Write)` — and assert on the bytes, with a
`Current` base as the negative control (must emit nothing).

### WR-06: `--dry-run` returns before base resolution, so it rehearses neither the refusal nor the trunk note

**File:** `crates/devflow-cli/src/commands.rs:307-310` vs `crates/devflow-cli/src/commands.rs:325-346`

**Issue.** `if dry_run { print_dry_run(&state); return Ok(()); }` sits above the
entire 45-01 block. An operator who misconfigures `base_branch = "main"` and
prudently rehearses with `devflow start --dry-run` gets a clean plan and exit 0;
the refusal only appears on the real run. Nor does dry-run print the
"note: base branch is `X` (from Y)" line at `commands.rs:342-345`, which is
described as a compensating control against a silent trunk redirect — the one
mode where an operator is explicitly inspecting what will happen shows them the
least.

**Fix.** Move the resolution + validation + note above the `dry_run` early
return, and include the resolved base and its source in `print_dry_run`'s
output. Test: `start(.., dry_run = true, ..)` with `base_branch = "main"` must
return `Err`.

### WR-07: `DEVFLOW_BASE_BRANCH=develop` converts the documented fresh-clone fall-open into a hard refusal

**File:** `crates/devflow-cli/src/commands.rs:330-333`

**Issue.** `ensure_base_is_a_local_branch` is gated on
`resolved_base.source != BaseBranchSource::Default`. Its own doc comment
(`commands.rs:139-145`) explains the scoping: "a fresh clone can legitimately
have `develop` only as `origin/develop` with no local branch, a case that falls
open today". But the gate keys on *provenance*, not on *value*. An operator who
exports `DEVFLOW_BASE_BRANCH=develop` — a semantically null redirect, and a
plausible thing to put in a shell profile or CI env after reading
`OPERATIONS.md:117` — flips `source` to `Env` and turns that documented
fall-open into a hard refusal on any clone whose default branch is `main`.

**Fix.** Gate on the value as well as the source:

```rust
if resolved_base.source != config::BaseBranchSource::Default && base != DEVELOP {
    ensure_base_is_a_local_branch(project_root, base)?;
}
```

Test: `DEVFLOW_BASE_BRANCH=develop` on a repo with only `origin/develop` must
not refuse; `DEVFLOW_BASE_BRANCH=workspace/absent` on the same repo must refuse
(negative control).

### WR-08: `porcelain_tracked_path` mis-splits filenames containing `" -> "` and does not unescape quoted paths

**File:** `crates/devflow-cli/src/staleness.rs:199-206`

**Issue.** Two narrower gaps in the new parser:

1. `path.rsplit(" -> ").next()` is applied to *every* line, not only `R`/`C`
   status codes. A tracked file legitimately named
   `crates/devflow-core/src/a -> b.rs` (git does not quote it — a plain space
   is printable ASCII) yields `b.rs`, which loses the `crates/` prefix and, in
   scoped mode, classifies as **not** build-affecting. A real workspace source
   change reads Fresh.
2. `trim_matches('"')` strips the quotes git adds under the default
   `core.quotePath=true` but does not undo the backslash escaping inside them,
   so a non-ASCII path arrives as `crates/.../caf\303\251.rs`. Harmless for the
   current predicate (prefix and `.rs` suffix both survive) but a latent
   mismatch for any future exact-name comparison.

Neither is likely; both fail toward Fresh, which is the direction this module
says it never fails toward.

**Fix.** Split on `" -> "` only when the status code is `R` or `C`
(`line[..2].contains(['R','C'])`), and either unescape the quoted form or
document explicitly that only prefix/suffix comparisons are safe on the
returned value.

---

## Info

### IN-01: dead `#[allow(clippy::too_many_arguments)]` on a four-argument function

**File:** `crates/devflow-cli/src/commands.rs:84`

The attribute sits above `phase_artifact_on_base`'s doc comment (unusual
ordering — attributes conventionally follow doc comments) and the function
takes four parameters, well under clippy's threshold of seven. It appears to be
a stale suppression intended for `start` (which carries its own at
`commands.rs:255`). Phase 45 added a parameter to this exact function, so the
suppression is now actively counterproductive: it will hide the lint if the
signature keeps growing.

**Fix:** delete it.

### IN-02: `merged_into_develop:` status label still names `develop`

**File:** `crates/devflow-cli/src/commands.rs:1132`

The only surviving operator-facing string that hardcodes the trunk name. The
value it prints is now computed against the *configured* base
(`ship_evidence.rs:158-165`), so the label is a false statement on a configured
project. Cosmetic but in the same class round-2 finding 4 flagged.

**Fix:** `println!("merged_into_base: {}", evidence.merged_into_develop);` or
interpolate the resolved base into the label.

### IN-03: duplicated comment block in `carried_phase_failures`

**File:** `crates/devflow-cli/src/commands.rs:243-246`

```rust
// Genuine zero: a phase's first start, or one whose completion already
// cleared the file.
// Genuine zero: a phase's first start, or one whose completion already
// cleared the file.
```

Pre-existing (not introduced by this phase), noted rather than acted on per the
repo's surgical-changes rule.

### IN-04: the F5 fix (`commands::reference`) ships with no test

**File:** `crates/devflow-cli/src/commands.rs:657`

Every other 45-01 call-site conversion landed with a paired test carrying a
negative control (`commands.rs:5540`, `parallel.rs:206`, `hooks.rs:427`,
`monitor.rs:1508`, `git.rs:853`). `reference` — the finding only one of the two
external lanes caught — did not. It is a one-line change, but the asymmetry
means a future revert of that line is silent.

**Fix:** assert `reference(root, None, false)` creates the snapshot from the
configured base, with a `develop`-configured repo as the negative control.

---

_Reviewed: 2026-09-02T12:42:26Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_

---

## Disposition (2026-09-02)

Operator decision: fix the two trunk-resolution blockers in this phase, defer the two prompt
blockers to the backlog.

| Finding | Disposition | Evidence |
|---|---|---|
| CR-01 | **Fixed** — `d90cea3` | `cleanup_merged_never_sweeps_the_builtin_trunks_under_a_configured_base`, with an ordinary-merged-branch negative control so the test cannot pass against a sweep that deletes nothing |
| CR-02 | **Fixed** — `0bdcba4` | `checkout_hooks_merge_into_the_persisted_base_not_the_ambient_default` (RED first: the work merged into `develop`), plus `git_flow_for_run_prefers_the_persisted_base_over_ambient_config` and `base_branch_round_trips_and_tolerates_states_predating_the_field` |
| CR-03 | **Deferred** — backlog 999.115 | — |
| CR-04 | **Deferred** — backlog 999.116 | — |
| WR-01 … IN-04 | Not addressed this session | — |

**Gate after the fixes:** `cargo test --workspace` 1229 passed / 0 FAILED (baseline before the
fixes was 1225; the four new tests account for the difference exactly),
`cargo clippy --workspace --all-targets -- -D warnings` exit 0 with 0 errors and 0 warnings,
`cargo fmt --all --check` exit 0.

**What that gate does not establish.** The suite is intermittently flaky for reasons unrelated to
these fixes — see backlog 999.114, reproduced on the pre-fix commit `eb3c6af` as a negative
control — so a single green run is weaker evidence here than it looks; 2 of 3 `--workspace` runs
on the fixed branch were clean and the third failed on the same victim the pre-fix baseline hit.
The CR-02 test drives `run_checkout_hooks` directly; it does not exercise a real `devflow resume`
end to end, so it establishes that the persisted base wins where the merge is performed, not that
every path from a live monitor death reaches that code with the field populated.
