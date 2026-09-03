# Adversarial review — DevFlow Phase 45 PLAN set (round 2)

You are reviewing THREE execution plans before they are handed to autonomous executor
agents that will write Rust code against them. Be adversarial. Your job is to find what
is WRONG, not to summarise or praise.

## Repository
DevFlow — a Rust workspace (`crates/devflow-core`, `crates/devflow-cli`) that orchestrates
AI coding agents through a phase lifecycle. Read the repo directly; you have it in cwd.

## Phase goal
Make `--mode auto` launchable and safe out of the box by (1) fixing worktree base detection
for `.planning/`, (2) scoping staleness detection to workspace crates, (3) enforcing
merit-based decision-checkpoint resolution.

Requirements: AUTO-01 (999.110), AUTO-02 (999.109), DECN-01 (999.94).

## Facts already established — do NOT spend effort re-deriving these
These were verified directly against source this session. Attack them only if you can show
evidence they are FALSE:

1. Every `develop`-consuming `GitFlow` method already reads `self.config.develop` (14 reads
   in `crates/devflow-core/src/git.rs`). The ONLY hardcoding is `GitFlow::new` (git.rs:119),
   the sole constructor, which sets `config: GitFlowConfig::default()`. No `with_config`
   exists. Every literal `"develop"` in git.rs is at line >=817, inside the test module.
2. `rg -c <pat> | rg '^0$'` is a constant-fail pipeline: `rg -c` prints nothing and exits 1
   on zero matches. Verified in BOTH directions (green and red suites both exit 1). Phase 45
   plans deliberately avoid it.
3. `.planning/config.json` exists on branch `workspace/denniyahh` but NOT on `develop`. This
   is the substance of AUTO-01.

## What I want you to attack

**A. Executability.** These plans are executed by an agent with no human present. For each
`<automated>` verify command: would it actually run in this repo from the repo root? Does the
`<fails_when>` name a signal that the command can actually emit? Flag any command that is a
constant-pass or constant-fail. Cite `file:line`.

**B. The `workspace_scoped` design in 45-02.** `affects_compiled_binary` is being narrowed to
Cargo workspace members. A flag `workspace_scoped: bool` is threaded from the self-dogfood
check so the narrowing applies ONLY inside DevFlow's own workspace. Is that threading correct
and complete? Does `enforce_build_staleness` compute staleness before the self-dogfood check
(which would mean the predicate runs against other projects)? Are the negative controls real
(paths that MUST return false)?

**C. DECN-01's honesty in 45-03.** The fix adds prompt policy in `crates/devflow-core/src/prompt.rs`
telling the agent to evaluate decision-checkpoint options on merit. But GSD's own
`execute-phase.md` (outside this repo) hardcodes "take the first option". So the plan adds a
COMPETING instruction it cannot delete. Is the plan honest about this? Is the prompt block
added to BOTH `code_stage_prompt` and `workflow_code_prompt` (they are reportedly duplicated,
serving Claude/OpenCode and Codex/Pi respectively)? Divergence there would give the two agent
families different unattended semantics.

**D. AUTO-01 blast radius in 45-01.** Making the base branch configurable touches worktree
creation, preflight base-ref reachability/currency, and GitFlow. What call sites does the plan
MISS? Specifically check: does it handle a configured base branch that has NO `origin/` tracking
ref (a local-only personal branch)? Does adding a new `DEVFLOW_*` env var require a same-commit
docs edit to satisfy the `doc_check` test `source_devflow_env_vars_and_subcommands_are_documented`?

**E. Cross-plan conflicts.** All three plans are wave 1 and claimed to have zero
`files_modified` overlap, so they run in PARALLEL in separate worktrees and get merged.
Verify that claim against their frontmatter. If two plans touch the same file, parallel
execution will produce a merge conflict or silently drop code.

**F. Anything else that would waste an executor's time or ship a defect.**

## Output format — required
For each finding:

```
### [CONFIRMED|SUSPECTED] <one-line title>
**Where:** <file:line>
**Evidence:** <what you actually read/ran that shows it>
**Impact:** <what breaks, concretely>
**Fix:** <smallest correct change>
```

End with a `## Verdict` section: APPROVE / APPROVE WITH CHANGES / REJECT, plus a count of
how many distinct `file:line` locations you cited. Do not pad. A finding you cannot ground
in a specific line is worth less than no finding — mark those SUSPECTED and say what you
could not check.

---
# THE PLANS UNDER REVIEW

## ===== 45-01-PLAN.md =====

---
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/ship_evidence.rs
  - crates/devflow-cli/src/parallel.rs
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/preflight.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - OPERATIONS.md
  - docs/guides/unattended-mode.md
autonomous: true
requirements: [AUTO-01]

estimate:
  tokens: 60000
  raw_tokens: 120000
  tasks: 3
  confidence: med

must_haves:
  truths:
    - "`devflow start --phase N` creates `.worktrees/phase-NN` forked from the configured base branch, not from `develop`, when `base_branch` is set in `devflow.toml` or `DEVFLOW_BASE_BRANCH` is exported (D-01 / AUTO-01)."
    - "A phase worktree forked from a configured base branch that carries `.planning/config.json` contains that file, so `preflight_unattended_launch_check`'s `unattended_config_condition` passes without operator intervention (AUTO-01)."
    - "With no `base_branch` key, no `DEVFLOW_BASE_BRANCH`, and no `devflow.toml` at all, every resolved value is byte-identical to today's `develop` behaviour — zero regression for existing projects."
    - "When the configured base branch has no `origin/<base>` tracking ref, `ensure_base_ref_current` returns `Ok(())` after printing a warning containing the words `fail-open`, rather than refusing the launch (REVIEWS risk 1.2, verified fail-open contract at preflight.rs:540-547)."
    - "A resolved base branch equal to the value of `config::MAIN` is refused before any git mutation, with an operator-facing message that names the branch and contains no absolute filesystem path."
    - "The branch a phase worktree forks FROM and the branch `merge_feature_into_develop` merges INTO are the same resolved value — `GitFlow`'s develop-consuming methods read the project-resolved config, not `GitFlowConfig::default()` (REVIEWS finding 1 substance)."
    - "`HookContext.git_flow` is no longer discarded: `merge_feature`, `branch_create` and `branch_cleanup` construct their `GitFlow` from `ctx.git_flow` rather than re-defaulting it."
  artifacts:
    - crates/devflow-core/src/config.rs
    - crates/devflow-core/src/git.rs
    - crates/devflow-cli/src/parallel.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-cli/src/preflight.rs
    - crates/devflow-core/src/hooks.rs
    - OPERATIONS.md
    - docs/guides/unattended-mode.md
  key_links:
    - "`config::base_branch(project_root)` -> `config::git_flow_for_project(project_root)` -> `GitFlow::for_project(project_root)`: one resolver feeds both the worktree start point and the git-flow integration target, so they can never disagree."
    - "`commands::start` -> `ensure_base_ref_current(root, &base)` / `ensure_phase_reachable_on_base(root, phase, &base)` -> `ensure_phase_worktree(root, phase, force, &base)` -> `worktree::add(.., start_point = &base)`: the branch the guards inspect is the branch the run forks from."
    - "`pipeline_outcomes.rs:1056-1064` -> `HookContext.git_flow` -> `hooks::merge_feature` -> `GitFlow::with_config`: the only production `HookContext` construction site is the single point where the resolved config reaches every checkout hook."
    - "`env_value(\"DEVFLOW_BASE_BRANCH\")` written as a string literal -> `doc_check::source_read_env_vars` -> `OPERATIONS.md` env table: a const-mediated read would be invisible to the scanner and pass green while undocumented."
  prohibitions:
    - statement: "The base-branch knob must never become a path by which an unattended run forks from or merges into the production branch. A resolved base equal to `config::MAIN` is refused, and no ambient environment value may silently redirect a phase's integration target onto `main`."
    - statement: "A configured base branch name supplied by the operator may be echoed back to the operator, but DevFlow must never derive, infer, or synthesize a username or an absolute host path into any preflight message or persisted `events.jsonl` payload as a consequence of this change (WR-02 / 999.10 class)."
---

<objective>
Make the branch a phase worktree forks from — and merges back into — a resolved
configuration value rather than the hardcoded `develop` constant, so a project whose
`.planning/` lives on a personal tracking branch (e.g. `workspace/denniyahh`) can launch
`--mode auto` out of the box with `.planning/config.json` present in the worktree.

Purpose: `preflight_unattended_launch_check`'s `unattended_config_condition` reads
`.planning/config.json` from the WORKTREE (`preflight.rs:1116`, 999.76). A worktree forked
from `develop` does not carry a `.planning/` that only exists on the planning branch, so the
unattended check refuses every launch. That is AUTO-01 / 999.110.

Output: a `base_branch` config field + `DEVFLOW_BASE_BRANCH` resolver, a config-taking
`GitFlow` constructor, threading through `commands::start` / `preflight` / `parallel`, the
production call-site audit that keeps fork-point and merge-target identical, and the
same-plan operator-docs edit the `doc_check` env-var test requires.

This implements D-01 as locked in 45-CONTEXT.md ("configurable base branch resolution in
DevFlow config ... with `develop` as the default ... used when creating a phase worktree or
checking base ref reachability/currency"), at D-01's own **costly** reversibility rating.
</objective>

<execution_context>
@~/.claude/gsd-core/workflows/execute-plan.md
@~/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@CLAUDE.md
@.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-CONTEXT.md
@.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-PATTERNS.md
@.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-REVIEWS.md
</context>

## Artifacts this phase produces

New symbols and keys introduced by THIS plan (excluded from drift verification — they do
not exist at HEAD):

| Kind | Name | Location |
|---|---|---|
| struct field | `DevflowConfig.base_branch: Option<String>` | `crates/devflow-core/src/config.rs` |
| function | `config::base_branch(project_root: &Path) -> String` | `crates/devflow-core/src/config.rs` |
| function | `config::git_flow_for_project(project_root: &Path) -> GitFlowConfig` | `crates/devflow-core/src/config.rs` |
| function | `config::validate_base_branch(value: &str) -> Result<(), String>` | `crates/devflow-core/src/config.rs` |
| constructor | `GitFlow::with_config(root, config: GitFlowConfig) -> Self` | `crates/devflow-core/src/git.rs` |
| constructor | `GitFlow::for_project(root) -> Self` | `crates/devflow-core/src/git.rs` |
| env var | `DEVFLOW_BASE_BRANCH` | `crates/devflow-core/src/config.rs`, `OPERATIONS.md` |
| config key | `base_branch` (top-level key in `devflow.toml`) | `OPERATIONS.md`, `docs/guides/unattended-mode.md` |
| fn parameter | `base: &str` added to `parallel::ensure_phase_worktree` | `crates/devflow-cli/src/parallel.rs` |

## Review incorporation (REVIEWS.md finding 1)

**Incorporated, with a correction to the finding's premise.**

- **Premise checked and found FALSE.** REVIEWS claims "the downstream GitFlow lifecycle
  ... remains hardcoded to `develop`". It is not. All eight develop-consuming `GitFlow`
  methods already read `self.config.develop` (`git.rs:133, 142, 161, 177, 200, 209, 234,
  317-318, 397`); production code in `git.rs` contains no `develop` string literal (every
  literal is at line >= 817, inside the test module). The hardcoding lives in exactly one
  place: `GitFlow::new`'s body (`git.rs:119-124`), which sets `GitFlowConfig::default()`.
  There is no `with_config` constructor at HEAD — verified absent. So the work is **one new
  constructor plus a call-site audit**, not the lifecycle rewrite the finding assumed.
- **Substance incorporated (finding 1, Planner Action bullets 1-2).** The finding's real
  question — is `base_branch` a worktree start point only, or the whole trunk? — is answered
  explicitly in Task 1: it is the **whole trunk for this project's DevFlow phase lifecycle**.
  Fork point and merge target resolve from the same value. The alternative (a separate
  `worktree_start_ref` with `develop` still the merge target) is REJECTED here because it
  produces exactly finding 1's risk 1.1: a feature branch forked from a personal branch,
  merged into `develop`, dragging unrelated history in. Task 3 does the call-site audit that
  makes the single-value design real.
- **Risk 1.2 (no `origin/` ref) incorporated as a REGRESSION TEST, not new handling.**
  Verified at source: `base_ref_currency` returns `Undeterminable` when `origin/{base}` fails
  `rev-parse --verify` (`preflight.rs:383-390`), and `ensure_base_ref_current` maps
  `Undeterminable` to `Ok(())` with a printed `fail-open` warning (`preflight.rs:540-547`).
  The behaviour the finding asks for already exists; Task 2 pins it so a future change cannot
  silently convert a local-only planning branch into a hard refusal.
- **`main` guard added beyond the finding.** Making the trunk configurable creates a new way
  to point a phase run at `main`. Task 1 refuses that outright.

## Flagged assumption (spec-less probe fallback, §C)

This phase has no SPEC.md, so the deterministic edge probe ran against REQUIREMENTS.md text.
AUTO-01's row came back `{"category":"unclassified","status":"unresolved"}`. Per the
protocol an `unclassified` row is NEVER auto-resolved with a backstop and NEVER silently
dropped — it is surfaced here as a flagged assumption:

> **FLAGGED (AUTO-01, unclassified/unresolved):** the probe could not classify what edge
> category AUTO-01 belongs to, so no probe-derived acceptance criterion was authored into
> `must_haves.truths` from it. The `truths` in this plan were derived goal-backward from
> the ROADMAP success criterion and from 45-PATTERNS.md's verified source reading, not from
> the probe. If AUTO-01 has an edge the probe would have found under a proper
> classification, this plan does not cover it. **Reviewer: check this before accepting.**

No-silent-drop accounting for this plan: 1 probe-surfaced item = 0 authored into `truths`
from the probe + 1 surfaced as a flagged assumption.

**Canon-referral breadcrumb (§B):** "a base branch name shaped like a git flag (leading `-`)
reaching an argv position" was recalled and is canon argument-injection, so it is DROPPED
from `must_haves.prohibitions` per the canon-referral rule. The validation is still
implemented (Task 1) as an ordinary acceptance criterion.

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1: End-to-end — a configured base branch reaches worktree::add</name>

  <read_first>
    - crates/devflow-core/src/config.rs (whole file — the `yes_ship` field at :74-98, the resolver at :210-229, `load_config` at :138-158, `env_value` at :265-267, the `ENV_MUTEX`/`EnvOverride` test guard at :272-292, and the `claude_legacy_launch` doc comment at :246-250 explaining why the env-var name MUST be a string literal)
    - crates/devflow-core/src/git.rs lines 95-135 (the `GitFlow` struct, its `config` field, and `new`'s body + doc comment)
    - crates/devflow-cli/src/parallel.rs lines 1-45 (`ensure_phase_worktree` in full)
    - crates/devflow-cli/src/commands.rs lines 198-350 (the `start` entry point: the `yes_ship` config-combining idiom at :215-232, `ensure_base_ref_current` at :270, `ensure_phase_reachable_on_base` at :280, the `divergence_from_develop` arm at :321-332, `ensure_phase_worktree` at :335)
    - crates/devflow-core/src/worktree.rs lines 60-83 (`add`, already parameterised on `start_point`)
    - crates/devflow-core/src/doc_check.rs lines 8-13, 55-67, 210-228, 407-428 (SCOPED_DOCS, scoped_doc_paths incl. `docs/guides/*.md`, `source_read_env_vars`, and the test that will go red without the docs edit)
    - OPERATIONS.md lines 108-125 (the existing `DEVFLOW_*` env-var table — `DEVFLOW_YES_SHIP` at :117 is the row to copy)
    - .planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-PATTERNS.md (sections D-01a through D-01e)
  </read_first>

  <files>
    crates/devflow-core/src/config.rs,
    crates/devflow-core/src/git.rs,
    crates/devflow-cli/src/parallel.rs,
    crates/devflow-cli/src/commands.rs,
    OPERATIONS.md,
    docs/guides/unattended-mode.md
  </files>

  <behavior>
    Write these tests FIRST and watch them fail before implementing.

    In `crates/devflow-core/src/config.rs` tests (serialize on the existing `ENV_MUTEX`,
    use the existing `EnvOverride` RAII guard for every env-mutating case):
    - `base_branch_defaults_to_develop_with_no_config`: an empty tempdir (no `devflow.toml`)
      resolves to the value of `DEVELOP`. NEGATIVE CONTROL for the whole change: if this
      breaks, every existing project silently changed trunk.
    - `base_branch_reads_devflow_toml`: a `devflow.toml` containing a `base_branch` key set
      to `workspace/example` resolves to `workspace/example`.
    - `base_branch_env_beats_file`: with the same file present AND `DEVFLOW_BASE_BRANCH` set
      to `other/branch`, resolves to `other/branch`.
    - `base_branch_empty_env_falls_through_to_file`: `DEVFLOW_BASE_BRANCH` set to the empty
      string is ignored (this is `env_value`'s documented empty-string filter), so the file
      value wins.
    - `git_flow_for_project_replaces_develop_only`: `git_flow_for_project` on a tempdir whose
      `devflow.toml` sets `base_branch` to `workspace/example` returns a `GitFlowConfig` whose
      `develop` is `workspace/example`, whose `main` still equals `MAIN`, and whose
      `feature_prefix` still equals `FEATURE_PREFIX`. NEGATIVE CONTROL: `main` and
      `feature_prefix` must NOT move.
    - `validate_base_branch_refuses_main`: passing the value of `MAIN` returns `Err`, and the
      error string contains neither a `/` followed by `home` nor the current user's name.
    - `validate_base_branch_refuses_flag_shaped_and_blank`: a value beginning with `-` returns
      `Err`; a value that is empty or all whitespace returns `Err`; `workspace/example`
      returns `Ok`. NEGATIVE CONTROL: the `Ok` case is what proves the validator is not
      rejecting everything.

    In `crates/devflow-core/src/git.rs` tests (copy the temp-repo fixture shape used by the
    `cleanup_merged` tests at :1119-1249):
    - `with_config_uses_the_supplied_develop_not_the_default`: build a repo with `main`,
      `develop` and a third branch; construct via `with_config` with `develop` set to the
      third branch; assert a develop-relative operation resolves against the third branch.
      NEGATIVE CONTROL: the same operation via `GitFlow::new` on the same repo must resolve
      against `develop`, proving the constructor is what changed the answer.

    In `crates/devflow-cli/src/parallel.rs` tests:
    - `ensure_phase_worktree_forks_from_the_supplied_base`: a repo with `develop` and a
      `workspace/example` branch carrying a file that is absent on `develop`; calling
      `ensure_phase_worktree` with `workspace/example` produces a worktree containing that
      file. NEGATIVE CONTROL: the same call with `develop` produces a worktree where the file
      is ABSENT. Both directions must be asserted in the same test or the test proves nothing.
  </behavior>

  <action>
    Per D-01, add `base_branch: Option<String>` to `DevflowConfig`, defaulting to `None` in the
    `Default` impl. Document on the field that `None` means the built-in `DEVELOP` constant and that this
    key is the project's DevFlow integration trunk, not merely a worktree start point.

    Add `config::base_branch(project_root: &Path) -> String` following the exact shape of the
    existing `yes_ship` resolver: read `env_value("DEVFLOW_BASE_BRANCH")` with the variable name
    written as a **string literal** (a const-mediated read is invisible to
    `doc_check::source_read_env_vars` and would pass green while undocumented — the failure
    recorded in `claude_legacy_launch`'s doc comment), then `load_config(project_root).base_branch`,
    then `DEVELOP.to_string()`. Because the value is a `String`, there is no `parse()` arm; a
    resolved value that fails `validate_base_branch` must `tracing::warn!` and fall through to
    the next source rather than panicking, matching `load_config`'s fail-soft contract.

    Add `config::validate_base_branch(value: &str) -> Result<(), String>` rejecting: a value
    equal to `MAIN`; a value that is empty or entirely whitespace; a value whose first byte is
    `-`. The `MAIN` rejection message must name the offending branch and the reason (forking and
    merging phase work on the production branch bypasses the release path), and must contain no
    absolute filesystem path and no host username, per the WR-02 / 999.10 convention documented
    at `preflight.rs:218-224`.

    Add `config::git_flow_for_project(project_root: &Path) -> GitFlowConfig` returning a
    `GitFlowConfig` whose `main` is `MAIN`, `feature_prefix` is `FEATURE_PREFIX`, and `develop`
    is `base_branch(project_root)`. This is the single place the trunk substitution happens.

    In `git.rs`, add `pub fn with_config(root: impl AsRef<Path>, config: GitFlowConfig) -> Self`
    and `pub fn for_project(root: impl AsRef<Path>) -> Self` (the latter delegating to
    `with_config` with `config::git_flow_for_project`). Leave `new` behaviourally unchanged so
    library callers with no project context keep the defaults, and update `new`'s doc comment,
    which currently asserts it uses the hardcoded git-flow constants without mentioning that a
    project-resolved sibling now exists.

    In `parallel.rs`, add a `base: &str` parameter to `ensure_phase_worktree` and pass it as
    `worktree::add`'s `start_point` in place of the imported `DEVELOP` constant. Switch the
    `delete_branch` call in the `force` arm to `GitFlow::for_project(project_root)` so the
    branch-protection list it consults matches the resolved trunk. Remove the now-unused
    `DEVELOP` import if nothing else in the file uses it.

    In `commands.rs::start`, resolve the base once near where `config::yes_ship` is already read
    (around line 215), call `config::validate_base_branch` on it and return a `CliError::Message`
    on `Err`, then substitute the resolved value for the `DEVELOP` argument at both
    `ensure_base_ref_current` and `ensure_phase_reachable_on_base`, and pass it into
    `ensure_phase_worktree`. When the resolved value differs from `DEVELOP`, print an
    operator-visible note in the same shape as the existing `yes_ship` note at :215-232, naming
    the resolved branch and its source (environment or config file). Switch the
    `divergence_from_develop` construction at :321 to `GitFlow::for_project`, and rewrite the two
    operator strings in that arm so they name the resolved branch via interpolation instead of
    the literal trunk name.

    Documentation, in this SAME task because `doc_check`'s
    `source_devflow_env_vars_and_subcommands_are_documented` test goes red the moment the
    `env_value` literal lands: add a `DEVFLOW_BASE_BRANCH` row to the OPERATIONS.md env-var
    table (same columns as the `DEVFLOW_YES_SHIP` row: name, default, description naming the
    `base_branch` key it overrides), and add a short subsection to
    `docs/guides/unattended-mode.md` stating that a project whose planning artifacts live on a
    branch other than the default trunk must set `base_branch`, that the value is the branch
    phase worktrees fork from AND merge back into, and that `main` is refused.
  </action>

  <verify>
    <automated>cargo test -p devflow-core --lib config::tests::base_branch 2>&1 | rg 'test result: ok. [1-9][0-9]* passed'</automated>
    <fails_when>non-zero exit, or no line matching "test result: ok. &lt;n&gt; passed" with n >= 1 (rg prints nothing and exits 1). A filter that matches no test prints "0 passed" and is therefore also a failure — this is the CLAUDE.md false-green trap.</fails_when>

    <automated>cargo test -p devflow-core --lib config::tests::validate_base_branch 2>&1 | rg 'test result: ok. [1-9][0-9]* passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line, or no "test result: ok." line at all</fails_when>

    <automated>cargo test -p devflow-core --lib config::tests::git_flow_for_project 2>&1 | rg 'test result: ok. [1-9][0-9]* passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow-core --lib with_config_uses_the_supplied_develop_not_the_default 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or the summary reads "0 passed" (the name matched nothing — `--exact`-style filters exit 0 on a no-match, per CLAUDE.md)</fails_when>

    <automated>cargo test -p devflow --bin devflow ensure_phase_worktree_forks_from_the_supplied_base 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed", or stderr contains "no library targets found" (which would mean `--lib` was used against the binary-only `devflow` package — CLAUDE.md)</fails_when>

    <automated>cargo test -p devflow-core --lib doc_check::tests::source_devflow_env_vars_and_subcommands_are_documented 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or output contains "source-read environment variable `DEVFLOW_BASE_BRANCH` is missing from scoped operator docs", or "0 passed"</fails_when>
  </verify>

  <acceptance_criteria>
    - `crates/devflow-core/src/config.rs` contains a `base_branch` field on `DevflowConfig` whose `Default` value is `None`.
    - `crates/devflow-core/src/config.rs` contains the exact substring `env_value("DEVFLOW_BASE_BRANCH")` written as a string literal (not via a const), so `doc_check::source_read_env_vars` sees it.
    - `OPERATIONS.md` contains the token `DEVFLOW_BASE_BRANCH`.
    - `docs/guides/unattended-mode.md` contains the token `base_branch`.
    - `crates/devflow-core/src/git.rs` declares both `pub fn with_config` and `pub fn for_project`.
    - `crates/devflow-cli/src/parallel.rs`'s `ensure_phase_worktree` signature has four parameters, the last being `base: &str`, and `worktree::add`'s fifth argument at that call site is that parameter.
    - `config::base_branch` on a tempdir with no `devflow.toml` and no `DEVFLOW_BASE_BRANCH` returns a value equal to `DEVELOP` (regression guard: existing projects unchanged).
    - `config::git_flow_for_project` on a project configured with a non-default base returns `main == MAIN` and `feature_prefix == FEATURE_PREFIX` unchanged, and only `develop` moved.
    - `config::validate_base_branch(MAIN)` returns `Err`; `validate_base_branch("")`, `validate_base_branch("   ")` and `validate_base_branch("--upload-pack=x")` return `Err`; `validate_base_branch("workspace/example")` returns `Ok`.
    - `ensure_phase_worktree` called with a base branch carrying a file absent on `develop` produces a worktree containing that file, AND the same call with `develop` produces a worktree without it (both directions asserted).
    - No operator-facing string added by this task contains a `/home/` or `/Users/` prefix or an OS username.
  </acceptance_criteria>

  <done>
    A `devflow.toml` with `base_branch = "workspace/example"` (or `DEVFLOW_BASE_BRANCH` exported)
    causes `devflow start` to fork `.worktrees/phase-NN` from that branch; with neither set,
    behaviour is byte-identical to today; `main` as a base is refused; and the workspace
    doc-check test is green because the env var is documented in the same commit.
  </done>

  <reversibility rating="costly">
    Adds a public config surface and two public constructors that downstream code will start
    depending on; removing the key later means an operator's committed `devflow.toml` silently
    changes meaning. Matches the operator's own D-01 rating in 45-CONTEXT.md. Not `one-way`,
    so no `checkpoint:decision` is inserted — deliberately, because this phase exists to make
    UNATTENDED runs work and a blocking-human checkpoint cannot receive an answer under
    DevFlow's one-shot launch model (999.57 / DEN-82).
  </reversibility>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Pin the fail-open contract for a base branch with no origin ref</name>

  <read_first>
    - crates/devflow-cli/src/preflight.rs lines 130-160 (`phase_reachability_on_base` and its stale NOTE at :142-145 claiming `base` is always `DEVELOP`)
    - crates/devflow-cli/src/preflight.rs lines 318-400 (`ORIGIN`, the `BaseRefCurrency` enum, and `base_ref_currency`'s `ref_exists` branch that returns `Undeterminable` at :383-390)
    - crates/devflow-cli/src/preflight.rs lines 520-575 (`ensure_base_ref_current`, including the `Undeterminable` fail-open arm at :540-547 and the `Diverged` refusal)
    - crates/devflow-cli/src/preflight.rs around line 2760 (the existing `currency_fixture` helper) and around line 2917 (`currency_behind_refuses_when_base_is_checked_out_in_another_worktree`, the closest existing test shape)
    - .planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-REVIEWS.md (finding 1, risk 1.2)
  </read_first>

  <files>crates/devflow-cli/src/preflight.rs</files>

  <behavior>
    Write these tests FIRST. They must FAIL if the fail-open arm is removed — verify that by
    temporarily deleting the `Undeterminable => Ok(())` arm and confirming the test reports a
    real failure, not a hang or a compile error that never runs.

    - `base_ref_currency_is_undeterminable_when_the_remote_ref_is_absent`: build a repo with a
      local branch that has no `origin/<name>` ref and no remote configured at all; assert
      `base_ref_currency` returns `BaseRefCurrency::Undeterminable`.
      NEGATIVE CONTROL, same test: a second branch that DOES have a matching `origin/` ref (set
      up via the existing `currency_fixture` shape) must return `Current`, not `Undeterminable`.
      Without that arm the test cannot distinguish "correctly detected absence" from "always
      returns Undeterminable".
    - `ensure_base_ref_current_fails_open_for_a_local_only_planning_branch`: the same
      remote-less branch passed to `ensure_base_ref_current` returns `Ok(())`. Capture the
      printed report through the module's existing injectable-writer idiom if one is reachable
      for this function; if it is not, assert the `Ok(())` disposition and add a comment stating
      that the warning text is asserted indirectly via the `Undeterminable` classification test
      above — do NOT claim to have asserted output that was never captured.
      NEGATIVE CONTROL, same test: a genuinely diverged base still returns `Err`, proving the
      function did not become unconditionally permissive.
  </behavior>

  <action>
    Add the two regression tests described in `behavior` to `preflight.rs`'s test module, reusing
    the `currency_fixture` and `run_git` helpers already in that module rather than writing a new
    git fixture. Follow CLAUDE.md's git-hermeticity rules for any new repo fixture: configure
    `user.email`, `user.name`, `commit.gpgsign=false` and `core.hooksPath=/dev/null` on the temp
    repo, and never mutate process-global environment.

    Rewrite the stale doc comment at `preflight.rs:142-145`. It currently asserts that `base` is
    always the `DEVELOP` constant at the one call site — after Task 1 that claim is false, and a
    stale doc claim is precisely the drift this repo's post-commit hook exists to catch. Replace
    it with a statement that `base` is the project-resolved value from `config::base_branch`, and
    a pointer to the `ORIGIN` constant's own note that the remote name remains non-configurable.

    Add a doc comment on `ensure_base_ref_current`'s `Undeterminable` arm recording, in one line,
    that this arm is what allows a local-only planning branch to launch, and naming the two tests
    above as its guards. State the fail direction and the reason, per this codebase's convention
    of commenting fail direction explicitly.
  </action>

  <verify>
    <automated>cargo test -p devflow --bin devflow base_ref_currency_is_undeterminable_when_the_remote_ref_is_absent 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary (the filter matched no test), or stderr contains "no library targets found"</fails_when>

    <automated>cargo test -p devflow --bin devflow ensure_base_ref_current_fails_open_for_a_local_only_planning_branch 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary</fails_when>

    <automated>rg -c 'always `devflow_core::config::DEVELOP`' crates/devflow-cli/src/preflight.rs; echo "rg_exit=$?"</automated>
    <fails_when>a printed count other than 0, or the absence of the line `rg_exit=1` (rg exits 1 when it finds nothing, which is the passing state here; exit 0 means the stale claim survives)</fails_when>
  </verify>

  <acceptance_criteria>
    - Both named tests exist in `crates/devflow-cli/src/preflight.rs` and each reports `1 passed`.
    - Each of the two tests contains BOTH a positive case and its stated negative control in the same test body — a test asserting only `Undeterminable`, or only `Ok(())`, does not satisfy this criterion.
    - Deleting the `BaseRefCurrency::Undeterminable => Ok(())` arm from `ensure_base_ref_current` causes `ensure_base_ref_current_fails_open_for_a_local_only_planning_branch` to print a real `test result: FAILED`. A hang, a timeout, or a compile error is NOT evidence the test catches the regression (CLAUDE.md: "a revert that hangs is not a revert that fails"). Record the observed failure output in the SUMMARY.
    - The phrase asserting that `base` is always the develop constant no longer appears in `preflight.rs`.
    - No new git fixture in this task mutates process-global environment variables.
  </acceptance_criteria>

  <done>
    A base branch with no `origin/` tracking ref classifies as `Undeterminable` and launches with
    a warning rather than a refusal, that behaviour is pinned by a test with a working negative
    control, and `preflight.rs` no longer claims the base is always `develop`.
  </done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: Call-site audit — fork point and merge target resolve to one value</name>

  <read_first>
    - crates/devflow-core/src/hooks.rs lines 36-50 (`HookContext`, including the `git_flow: GitFlowConfig` field that is currently read only for `feature_prefix`)
    - crates/devflow-core/src/hooks.rs lines 118-145 (`branch_create`, `branch_cleanup`) and lines 175-300 (`merge_feature` and the remaining `GitFlow::new` sites at :239, :278, :289)
    - crates/devflow-cli/src/pipeline_outcomes.rs lines 1050-1075 (the ONLY production `HookContext` construction, with `GitFlowConfig::default()` at :1056 feeding it at :1064)
    - crates/devflow-cli/src/pipeline_outcomes.rs line 599 (`phase_commit_count` with a defaulted config)
    - crates/devflow-cli/src/pipeline_launch.rs line 1471 (`GitFlowConfig::default()` feeding `evaluate_agent_result`)
    - crates/devflow-core/src/monitor.rs lines 1243-1258 (`enumerate_phase_commits`, which builds a `develop..feature` range from a defaulted config)
    - crates/devflow-core/src/ship_evidence.rs lines 155-165 (`GitFlow::new` at :158 and `GitFlowConfig::default().feature_prefix` at :161)
    - crates/devflow-cli/src/commands.rs lines 660-680 and 2040-2080 (the remaining `GitFlow::new` sites at :672, :2052, :2072)
    - crates/devflow-core/src/gsd_config.rs line 320 (`GitFlow::new` used only for `commit_path` — trunk-irrelevant, expected to stay as-is)
    - .planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-PATTERNS.md ("Every production site that must be re-pointed" table and the `HookContext` latent-inconsistency note)
  </read_first>

  <files>
    crates/devflow-core/src/hooks.rs,
    crates/devflow-core/src/monitor.rs,
    crates/devflow-core/src/ship_evidence.rs,
    crates/devflow-cli/src/commands.rs,
    crates/devflow-cli/src/pipeline_launch.rs,
    crates/devflow-cli/src/pipeline_outcomes.rs
  </files>

  <behavior>
    Write these tests FIRST.

    - `merge_feature_targets_the_configured_base_not_the_default`: a temp repo with `develop`, a
      `workspace/example` branch, and a `feature/phase-NN` branch forked from `workspace/example`
      with one commit; a `HookContext` whose `git_flow.develop` is `workspace/example`; running
      the merge hook lands the commit on `workspace/example`.
      NEGATIVE CONTROL, same test: `develop` must NOT contain that commit afterwards. Asserting
      only "the merge succeeded" would pass against today's code merging into `develop`, and is
      therefore worthless as a regression guard.
    - `hook_context_git_flow_is_not_discarded`: with a `HookContext` whose `git_flow.develop` is
      a branch that does not exist in the repo, the merge hook returns `Err` naming that branch.
      This is the direct proof the field is read rather than re-defaulted — under today's code
      the hook would silently succeed against the real `develop`.
    - `enumerate_phase_commits_ranges_from_the_configured_base`: commits reachable from the
      feature branch but not from the configured base are enumerated.
      NEGATIVE CONTROL, same test: a commit present on the configured base is NOT listed.
  </behavior>

  <action>
    Convert every production site that consumes the trunk from a defaulted config to the
    project-resolved one, so that the branch a worktree forks from in Task 1 is the same branch
    the lifecycle merges into. Named sites, each a substitution rather than a redesign:

    In `hooks.rs`: `branch_create` (:125), `branch_cleanup` (:132), `merge_feature` (:181), and
    the sites at :239, :278 and :289 construct their `GitFlow` from `ctx.git_flow` via
    `GitFlow::with_config(&ctx.project_root, ctx.git_flow.clone())` instead of `GitFlow::new`.
    This is the fix for the latent inconsistency 45-PATTERNS.md flagged: `HookContext.git_flow`
    exists and is currently read only for `feature_prefix`, so every git operation inside a hook
    silently re-defaults the trunk. Threading config into `HookContext` alone would be a no-op
    without this half.

    In `pipeline_outcomes.rs`: replace the `GitFlowConfig::default()` at :1056 that feeds the
    `HookContext` at :1064 with `config::git_flow_for_project(project_root)`. This is the single
    production construction site, so it is the one point where the resolved value reaches every
    checkout hook. Do the same for the defaulted config at :599 and :1056's sibling use in
    `evaluate_agent_result`.

    In `pipeline_launch.rs:1471` and `monitor.rs:1249`: replace `GitFlowConfig::default()` with
    the project-resolved config. `monitor.rs` builds a two-dot range from the trunk to the phase
    branch for idle-timeout commit enumeration; against a mismatched trunk that range either
    over- or under-reports the phase's commits, which is exactly the false-evidence shape the
    never-silent commit gate depends on.

    In `commands.rs` (:672, :2052, :2072) and `ship_evidence.rs` (:158, and the
    `GitFlowConfig::default().feature_prefix` read at :161): switch to `GitFlow::for_project` /
    `config::git_flow_for_project`.

    Leave `gsd_config.rs:320` on `GitFlow::new`: it uses only `commit_path`, which never consults
    the trunk. Record that exclusion in the SUMMARY with its reason rather than leaving a reader
    to wonder whether it was missed.

    After the conversions, sweep the touched files for operator-facing strings and doc comments
    that name the trunk as a literal word rather than interpolating the resolved value, and
    update them. A message that says the run is behind a branch it did not actually compare
    against is a false statement to the operator.
  </action>

  <verify>
    <automated>cargo test -p devflow-core --lib merge_feature_targets_the_configured_base_not_the_default 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line (the test name matched nothing)</fails_when>

    <automated>cargo test -p devflow-core --lib hook_context_git_flow_is_not_discarded 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow-core --lib enumerate_phase_commits_ranges_from_the_configured_base 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow-core --lib hooks:: 2>&1 | rg 'test result: ok. [1-9][0-9]* passed'</automated>
    <fails_when>non-zero exit, or "0 passed", or any line matching "^test result: FAILED"</fails_when>

    <automated>cargo test --workspace > target/45-01-suite.txt 2>&1; echo "cargo_exit=$?"; grep -c '^test result: FAILED' target/45-01-suite.txt</automated>
    <fails_when>the line `cargo_exit=0` is absent, or the trailing count printed by grep is any value other than 0. NOTE: do NOT use the Phase-44 form `... | rg -c '^test result: FAILED' | rg '^0$'` — verified this session that `rg -c` prints nothing and exits 1 on zero matches, so that pipeline can never pass on a green suite.</fails_when>

    <automated>cargo clippy --workspace --all-targets -- -D warnings 2>&1 | rg 'Finished|error'</automated>
    <fails_when>any line containing "error" appears in the output, or the "Finished" line is absent</fails_when>
  </verify>

  <acceptance_criteria>
    - `crates/devflow-core/src/hooks.rs` contains `GitFlow::with_config(&ctx.project_root, ctx.git_flow.clone())` at each of the six sites previously using `GitFlow::new(&ctx.project_root)` (lines 125, 132, 181, 239, 278, 289 at HEAD).
    - `crates/devflow-cli/src/pipeline_outcomes.rs`'s production `HookContext` construction populates `git_flow` from `config::git_flow_for_project`, not `GitFlowConfig::default()`.
    - `crates/devflow-core/src/monitor.rs`'s `enumerate_phase_commits` and `crates/devflow-cli/src/pipeline_launch.rs:1471` both resolve their `GitFlowConfig` from the project, not from `default()`.
    - `crates/devflow-core/src/gsd_config.rs:320` still uses `GitFlow::new`, and the SUMMARY states why (uses only `commit_path`; trunk-irrelevant).
    - `merge_feature_targets_the_configured_base_not_the_default` asserts BOTH that the configured base received the commit AND that `develop` did not. A single-direction assertion does not satisfy this criterion.
    - `hook_context_git_flow_is_not_discarded` fails against HEAD's code (where `ctx.git_flow` is re-defaulted). Record the observed pre-fix failure output in the SUMMARY as the RED half of the cycle.
    - `cargo test --workspace` reports `cargo_exit=0` and a `^test result: FAILED` count of 0.
    - `cargo clippy --workspace --all-targets -- -D warnings` prints a `Finished` line and no `error` line.
  </acceptance_criteria>

  <done>
    Every production consumer of the git-flow trunk resolves it from the project configuration,
    `HookContext.git_flow` is genuinely read rather than re-defaulted, the merge target provably
    equals the fork point, and the full workspace suite plus clippy are clean.
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| ambient environment → DevFlow config resolver | `DEVFLOW_BASE_BRANCH` is read from the process environment, which a CI runner, a parent shell, or a `.envrc` can set without the operator noticing |
| `devflow.toml` on disk → DevFlow config resolver | a committed config file changes trunk for every clone of the repository |
| resolved branch name → `git` argv | the base value is passed to `git worktree add` and `git merge-base` as a positional argument |
| resolved branch name → operator output + `.devflow/events.jsonl` | a personal branch name can carry a username into persisted artifacts |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-45-01 | Elevation of Privilege | `config::base_branch` → `worktree::add` / `merge_feature_into_develop` | high | mitigate | `validate_base_branch` refuses a value equal to `config::MAIN`, so the knob cannot be used to fork from or merge into the production branch (Task 1) |
| T-45-02 | Tampering | `env_value("DEVFLOW_BASE_BRANCH")` | medium | mitigate | Task 1 prints an operator-visible note whenever the resolved base differs from the default, naming the branch and its source (env vs file), so an ambient redirect is never silent |
| T-45-03 | Tampering | resolved branch name → `git` argv | medium | mitigate | `validate_base_branch` rejects a value whose first byte is `-`, closing the argv-flag-injection shape before the value reaches any `git` invocation (Task 1) |
| T-45-04 | Information Disclosure | preflight messages, `.devflow/events.jsonl` | medium | mitigate | Task 1 acceptance criteria forbid absolute paths and derived usernames in any new operator string; DevFlow echoes only the operator-supplied branch name and never derives one from the host (WR-02 / 999.10) |
| T-45-05 | Spoofing | `base_branch` naming a branch that does not exist | low | mitigate | `ensure_phase_reachable_on_base` already refuses before any git mutation; Task 1 routes the resolved value into that existing guard rather than bypassing it |
| T-45-06 | Repudiation | merge target diverging from fork point | high | mitigate | Task 3's call-site audit makes both resolve from `config::git_flow_for_project`, and `merge_feature_targets_the_configured_base_not_the_default` asserts both directions |
| T-45-07 | Denial of Service | a local-only planning branch failing the currency probe | medium | accept | The existing `Undeterminable` fail-open arm is the accepted disposition; Task 2 pins it with a test rather than adding new handling. Accepted because the alternative — refusing every launch from an unpushed branch — is the AUTO-01 defect itself |
| T-45-08 | Tampering | npm/pip/cargo installs | high | accept | This plan adds no new dependency to any manifest; no package-legitimacy gate is required. If an executor finds itself adding a crate, stop and escalate — the phase has no RESEARCH.md and therefore no Package Legitimacy Audit table |
</threat_model>

<verification>
- `cargo test --workspace` reports `cargo_exit=0` and zero `^test result: FAILED` lines.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` are clean.
- With no `devflow.toml` and no `DEVFLOW_BASE_BRANCH`, `config::base_branch` returns the value of `DEVELOP` — the zero-regression guard for every existing project.
- `doc_check::source_devflow_env_vars_and_subcommands_are_documented` passes, proving `DEVFLOW_BASE_BRANCH` is documented in a scoped operator doc.
- The RED half of Task 2's and Task 3's TDD cycles is recorded in the SUMMARY as observed failure output, not asserted from memory.
</verification>

<success_criteria>
AUTO-01 is satisfied: a project whose `.planning/` lives on a branch other than `develop`
can set `base_branch` (or export `DEVFLOW_BASE_BRANCH`), and `devflow start --phase N`
creates a worktree that contains `.planning/config.json`, so
`preflight_unattended_launch_check` passes without operator intervention. The branch forked
from and the branch merged into are the same resolved value. Default behaviour is unchanged.
</success_criteria>

<output>
Create `.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-01-SUMMARY.md` when done.
</output>

## ===== 45-02-PLAN.md =====

---
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
plan: 02
type: tdd
wave: 1
depends_on: []
files_modified:
  - crates/devflow-cli/src/staleness.rs
autonomous: true
requirements: [AUTO-02]

estimate:
  tokens: 25000
  raw_tokens: 50000
  tasks: 2
  confidence: med

must_haves:
  truths:
    - "A tracked change confined to `.planning/spikes/` — including `.planning/spikes/foo/Cargo.toml` and `.planning/spikes/foo/src/main.rs` — does not cause `enforce_build_staleness` to hard-block DevFlow's own workspace (D-02 / AUTO-02)."
    - "A tracked change to any path under `crates/` that ends in `.rs`, or whose final segment is one of `Cargo.toml`/`Cargo.lock`/`build.rs`/`rust-toolchain.toml`, still classifies as build-affecting."
    - "A change to a root build file — `Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml` matched by exact equality, no directory prefix — still classifies as build-affecting."
    - "`vendor/crates/devflow-core/src/lib.rs` and `crates-extras/foo/src/lib.rs` classify as NOT build-affecting: the rule is a `crates/` path-segment prefix, never a substring match."
    - "The existing regression guard `mixed_range_docs_and_source_is_stale` — a range touching both `.planning/x.md` and `crates/devflow-cli/src/main.rs` — still reads Stale."
    - "A project that is NOT DevFlow's own workspace keeps today's broad build-input rule, so narrowing the self-dogfood check does not silently delete the stale-build warning for every other Rust project DevFlow drives."
  artifacts:
    - crates/devflow-cli/src/staleness.rs
  key_links:
    - "`enforce_build_staleness` -> `is_self_dogfood_workspace(project_root)` -> `combined_staleness(.., workspace_scoped)` -> `ancestry_range_affects_build` / `tree_has_modified_build_inputs` -> `affects_compiled_binary(rel_path, workspace_scoped)`: the scope flag is computed once and threaded, so there is exactly one predicate and one rule."
    - "`git status --porcelain` line -> `porcelain_tracked_path` (strips status bytes, resolves `ORIG -> PATH` renames, strips quotes) -> `affects_compiled_binary`: path normalization happens before predicate evaluation, not inside it."
  prohibitions:
    - statement: "Narrowing this predicate must never make the staleness gate quieter than it is today for any project other than DevFlow's own workspace. Fixing DevFlow's spike directory must not silently disable a stale-build safety warning for every downstream Rust project that keeps its crates outside a `crates/` directory."
    - statement: "The narrowed predicate must never be permitted to turn a real workspace-source change into a Fresh verdict. The failure this gate exists to prevent is a stale binary reporting green against its own source (the Phase 16 false-evidence incident); a narrowing that loses a true positive is worse than the spike false positive it fixes."
---

<objective>
Stop `.planning/spikes/` and other non-workspace code from tripping DevFlow's self-dogfood
stale-build hard block, by scoping the build-input predicate to Cargo workspace member paths
(`crates/`) plus the four root build files.

Purpose: `affects_compiled_binary` currently matches ANY path ending in `.rs`, and matches a
build file at ANY depth via a `/{name}` suffix test. `.planning/spikes/foo/src/main.rs` and
`.planning/spikes/foo/Cargo.toml` therefore both read as build-affecting, and
`enforce_build_staleness` hard-blocks the pipeline on code that cannot possibly change the
compiled `devflow` binary. That is AUTO-02 / 999.109.

Output: a scoped predicate with the first direct unit tests it has ever had, carrying explicit
negative controls in both directions, plus an end-to-end fixture proving a spikes-only change
does not block.

This implements D-02 as locked in 45-CONTEXT.md ("scope `affects_compiled_binary` to Cargo
workspace member paths (`crates/*`) and root build files ... so non-workspace spikes never trip
the D-18 self-dogfood stale build block"), at D-02's own **reversible** rating. One deliberate
design choice inside D-02 is flagged below rather than taken silently.
</objective>

<execution_context>
@~/.claude/gsd-core/workflows/execute-plan.md
@~/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@CLAUDE.md
@.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-CONTEXT.md
@.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-PATTERNS.md
@.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-REVIEWS.md
</context>

## Artifacts this phase produces

New symbols introduced by THIS plan (excluded from drift verification — they do not exist at
HEAD):

| Kind | Name | Location |
|---|---|---|
| fn parameter | `workspace_scoped: bool` added to `affects_compiled_binary` | `crates/devflow-cli/src/staleness.rs` |
| fn parameter | `workspace_scoped: bool` added to `ancestry_range_affects_build` | `crates/devflow-cli/src/staleness.rs` |
| fn parameter | `workspace_scoped: bool` added to `tree_has_modified_build_inputs` | `crates/devflow-cli/src/staleness.rs` |
| fn parameter | `workspace_scoped: bool` added to `combined_staleness` and `embedded_commit_is_stale`'s helper chain | `crates/devflow-cli/src/staleness.rs` |
| const | `WORKSPACE_MEMBER_PREFIX: &str = "crates/"` | `crates/devflow-cli/src/staleness.rs` |

## Design decision inside D-02 — READ THIS BEFORE IMPLEMENTING

D-02 says: "Scope `affects_compiled_binary` to Cargo workspace member paths (`crates/*`) and
root build files. Any path outside `crates/` must return `false`."

Applied unconditionally, that has a collateral effect D-02 does not mention and this plan will
not ship silently. Verified against live source this session:

- `enforce_build_staleness` (`staleness.rs:324-333`) computes `combined_staleness` FIRST and
  `is_self_dogfood_workspace(project_root)` SECOND. The predicate therefore runs against
  **every** project DevFlow drives, not only DevFlow's own workspace. The self-dogfood flag only
  decides whether a Stale verdict Blocks or merely Warns (`staleness_outcome`).
- So an unconditional `crates/` rule would make a modified `src/main.rs` in an ordinary Rust
  project — the standard Cargo layout, where nothing lives under `crates/` — classify as
  not-build-affecting. That project silently loses its stale-build warning. DevFlow is published
  to crates.io and drives other people's repositories; deleting a safety warning for all of them
  to fix this repo's spike directory is not what AUTO-02 asks for.

**Decision:** thread a `workspace_scoped: bool` — sourced from the existing
`is_self_dogfood_workspace` — through the predicate chain. Inside DevFlow's own workspace the
narrowed `crates/` rule applies (AUTO-02 satisfied); everywhere else today's broad rule is
preserved byte-for-byte. This keeps ONE predicate with ONE rule, parameterized — it does not
fork or reimplement `affects_compiled_binary`, which the D-07 comment at `staleness.rs:104-109`
explicitly forbids.

**Rejected alternative:** apply the narrowed rule unconditionally (the most literal reading of
D-02). Rejected because of the collateral warning loss above. **Operator: if you want the
literal unconditional reading instead, say so and this becomes a one-line change** — drop the
parameter and hardcode `true`. This is flagged rather than decided silently.

## Review incorporation (REVIEWS.md finding 2)

- **"Must ensure exact matching for root build files and prefix matching for `crates/`" —
  INCORPORATED** as the literal predicate rule in Task 1, with `crates-extras/` and
  `vendor/crates/` as explicit negative controls proving prefix, not substring.
- **"Write explicit unit tests with negative controls" — INCORPORATED** as Task 2, which adds
  the FIRST direct unit tests `affects_compiled_binary` and `porcelain_tracked_path` have ever
  had. Verified at source: neither has any direct test today; both are exercised only
  indirectly through git-fixture tests.
- **"Standardize path normalization before predicate evaluation" — PARTIALLY INCORPORATED, with
  one sub-item explicitly REJECTED.** `porcelain_tracked_path` (`staleness.rs:166-173`) already
  handles status bytes, `ORIG -> PATH` renames, and quoting; Task 2 pins each of those with a
  direct test. The finding also asks to handle a leading `./` prefix. **Rejected:**
  `git status --porcelain` does not emit `./`-prefixed paths, and `git diff --name-only` (the
  other input, `staleness.rs:110-117`) does not either. Adding stripping for a prefix neither
  producer emits would be speculative code with an untestable branch. Task 2 records this in a
  comment beside the normalizer instead of implementing it.

## Flagged assumption (spec-less probe fallback, §C)

AUTO-02's probe row came back `{"category":"concurrency","status":"unresolved","probe":"If
interrupted or run in parallel, what is guaranteed?"}`. Surfaced, not silently dropped:

> **FLAGGED (AUTO-02, unresolved).** The probe classified AUTO-02 as a **concurrency** concern.
> That looks like a misclassification: `affects_compiled_binary` is a pure `&str -> bool`
> transform with no shared state, no I/O and no interruption point, so "if interrupted or run
> in parallel, what is guaranteed?" has no meaningful answer for it. This plan records that
> judgement rather than inventing a concurrency edge to satisfy the probe. **No concurrency
> acceptance criterion was authored from this row.** If a reviewer believes there IS a
> concurrency surface here — the nearest candidate is `enforce_build_staleness` reading
> `git status --porcelain` while another process mutates the worktree, which is pre-existing and
> out of AUTO-02's scope — raise it before accepting this plan.

No-silent-drop accounting for this plan: 1 probe-surfaced item = 0 authored into `truths` from
the probe + 1 surfaced as a flagged assumption.

**Canon-referral breadcrumb (§B):** "a crafted path escaping the workspace prefix via `..`
segments" was recalled and is canon path-traversal, so it is DROPPED from
`must_haves.prohibitions` per the canon-referral rule. Task 1 still adds `crates/../foo.rs` as
an ordinary negative-control test case.

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1: End-to-end — a spikes-only change no longer blocks the dogfood workspace</name>

  <read_first>
    - crates/devflow-cli/src/staleness.rs lines 30-95 (`embedded_commit_is_stale` and its two arms calling `ancestry_range_affects_build` at :59 and :84)
    - crates/devflow-cli/src/staleness.rs lines 95-200 (`ancestry_range_affects_build` with its `unwrap_or(true)` fail-toward-Stale posture, `run_git_stdout`, `tree_has_modified_build_inputs`, `porcelain_tracked_path`, and `affects_compiled_binary` in full)
    - crates/devflow-cli/src/staleness.rs lines 200-235 (`combined_staleness` and its documented truth table)
    - crates/devflow-cli/src/staleness.rs lines 235-280 (`is_self_dogfood_workspace` — the source of the scope flag)
    - crates/devflow-cli/src/staleness.rs lines 300-400 (`enforce_build_staleness`, note that `combined_staleness` runs BEFORE `is_self_dogfood_workspace` today, and note the WR-02 comment explaining why the emitted event must stay path-free)
    - crates/devflow-cli/src/staleness.rs lines 680-790 (`enforce_build_staleness_blocks_self_dogfood_behind_worktree_head` — the end-to-end fixture shape to copy)
    - crates/devflow-cli/src/staleness.rs lines 1537-1591 (`mixed_range_docs_and_source_is_stale` — the regression guard this change must not break, and the git-fixture boilerplate at :1541-1556)
    - .planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-PATTERNS.md (the D-02 section)
  </read_first>

  <files>crates/devflow-cli/src/staleness.rs</files>

  <behavior>
    Write these tests FIRST and observe them fail against HEAD before implementing.

    - `spikes_only_dirty_tree_does_not_block_self_dogfood`: build a temp repo that
      `is_self_dogfood_workspace` accepts (root `Cargo.toml` whose `members` array contains both
      exact member paths — copy the fixture at `staleness.rs:690-710`). Commit a clean baseline.
      Then modify tracked files `.planning/spikes/foo/Cargo.toml` and
      `.planning/spikes/foo/src/main.rs` only. Assert `enforce_build_staleness` returns `Ok`.
      NEGATIVE CONTROL, same test: modify `crates/devflow-cli/src/main.rs` in the same repo and
      assert `enforce_build_staleness` returns `Err`. Without this second half the test cannot
      distinguish "correctly ignored the spikes" from "the gate stopped working entirely" — and
      a gate that never blocks is a strictly worse outcome than the bug being fixed.

    - `non_dogfood_project_keeps_the_broad_build_input_rule`: a temp repo whose root
      `Cargo.toml` does NOT list DevFlow's member paths, with a modified tracked `src/main.rs`.
      Assert `tree_has_modified_build_inputs` with `workspace_scoped = false` returns
      `Some(true)`.
      NEGATIVE CONTROL, same test: the identical input with `workspace_scoped = true` returns
      `Some(false)`. The pair is what proves the flag is load-bearing rather than ignored.

    - `mixed_range_docs_and_source_is_stale` (EXISTING, `staleness.rs:1537`) must still pass
      byte-for-byte unmodified. Do not edit it. If it needs editing to pass, the new rule is
      wrong — stop and report rather than adjusting the guard to fit.
  </behavior>

  <action>
    Add a private `WORKSPACE_MEMBER_PREFIX` constant holding the workspace-member directory
    prefix used by this repo's root manifest, with a trailing separator so it is a path-segment
    prefix and not a substring.

    Give `affects_compiled_binary` a second parameter `workspace_scoped: bool` and split its body
    into two rules:
    - When `workspace_scoped` is false, keep today's behaviour exactly: any path ending in `.rs`,
      or equal to / suffixed by a separator plus one of the four build-affecting file names.
      Preserve this branch byte-for-byte so no non-DevFlow project changes behaviour.
    - When `workspace_scoped` is true, return true only if the path equals one of the four
      build-affecting names exactly (no separator anywhere in the path), OR the path starts with
      `WORKSPACE_MEMBER_PREFIX` AND either ends in `.rs` or has a final segment equal to one of
      the four names.

    Document on the function which direction it fails and why, matching this module's convention
    of stating fail direction explicitly in the doc comment. State plainly that the scoped branch
    trades a narrower true-positive surface for eliminating the `.planning/spikes/` false
    positive, and that the surface it gives up is limited to paths a `cargo build` of this
    workspace cannot reach.

    Thread `workspace_scoped` through the two direct callers — `ancestry_range_affects_build`
    (:110) and `tree_has_modified_build_inputs` (:143) — and through their callers
    `embedded_commit_is_stale` and `combined_staleness`, up to `enforce_build_staleness`.

    In `enforce_build_staleness`, move the `is_self_dogfood_workspace(project_root)` call ABOVE
    the `combined_staleness` call and pass its result as `workspace_scoped`. Add a comment
    recording that the ordering is now load-bearing, and that `is_self_dogfood_workspace` stays
    anchored on `project_root` rather than `execution_root` for the reason already documented at
    `staleness.rs:315-323` (Assumption A3) — the reorder must not quietly change which root it
    inspects.

    Leave `porcelain_tracked_path` unchanged; normalization already happens before the predicate
    and Task 2 pins that.
  </action>

  <verify>
    <automated>cargo test -p devflow --bin devflow spikes_only_dirty_tree_does_not_block_self_dogfood 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line (the filter matched no test — this exits 0 and is the CLAUDE.md false-green trap), or stderr contains "no library targets found" (meaning `--lib` was used against the binary-only `devflow` package)</fails_when>

    <automated>cargo test -p devflow --bin devflow non_dogfood_project_keeps_the_broad_build_input_rule 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow --bin devflow mixed_range_docs_and_source_is_stale 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow --bin devflow staleness:: 2>&1 | rg 'test result: ok. [1-9][0-9]* passed'</automated>
    <fails_when>non-zero exit, or "0 passed", or any line matching "^test result: FAILED"</fails_when>
  </verify>

  <acceptance_criteria>
    - `affects_compiled_binary` has two parameters, the second named `workspace_scoped: bool`.
    - `crates/devflow-cli/src/staleness.rs` declares a `WORKSPACE_MEMBER_PREFIX` constant whose value ends with a `/`.
    - In `enforce_build_staleness`, the `is_self_dogfood_workspace` call appears at a lower line number than the `combined_staleness` call, and its result is the value passed as `workspace_scoped`.
    - `spikes_only_dirty_tree_does_not_block_self_dogfood` asserts BOTH the `Ok` (spikes-only) and the `Err` (workspace source) outcome in the same test body. A test asserting only `Ok` does not satisfy this criterion.
    - `non_dogfood_project_keeps_the_broad_build_input_rule` asserts both `Some(true)` at `workspace_scoped = false` and `Some(false)` at `workspace_scoped = true`.
    - `mixed_range_docs_and_source_is_stale` passes with its body unmodified — `git diff` on the plan's commit shows no change to lines 1537-1591 of the pre-change file other than any mechanical call-signature update forced by the new parameter, and that update is noted in the SUMMARY.
    - Both new tests are observed FAILING against HEAD before the implementation lands, and the observed failure output is recorded in the SUMMARY as the RED half of the cycle. A test that could not be made to fail is not evidence.
    - Any new git fixture configures `user.email`, `user.name`, `commit.gpgsign=false` and `core.hooksPath=/dev/null` on the temp repo, and no test mutates process-global environment variables.
  </acceptance_criteria>

  <done>
    A tracked change confined to `.planning/spikes/` no longer hard-blocks a stage in DevFlow's
    own workspace, a change under `crates/` still does, and a non-DevFlow project's behaviour is
    unchanged.
  </done>

  <reversibility rating="reversible">
    A pure path-predicate change behind one boolean parameter; reverting is a one-commit
    revert with no data migration and no persisted state. Matches the operator's D-02 rating.
  </reversibility>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Direct unit tests for the predicate and the porcelain normalizer</name>

  <read_first>
    - crates/devflow-cli/src/staleness.rs lines 160-200 (`porcelain_tracked_path` and `affects_compiled_binary` as they stand after Task 1)
    - crates/devflow-cli/src/staleness.rs lines 470-533 (`is_self_dogfood_workspace_matches_both_member_paths_only` and `is_self_dogfood_workspace_requires_exact_member_paths_not_substrings` — the table-plus-negative-control test shape to copy, including the per-assert comment stating what going wrong would cost)
    - .planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-REVIEWS.md (finding 2, the negative-control list)
  </read_first>

  <files>crates/devflow-cli/src/staleness.rs</files>

  <behavior>
    `affects_compiled_binary` is a pure `&str -> bool`, so these tests need no tempdir — a
    straight table of `(input, expected)`.

    `affects_compiled_binary_in_workspace_scope_accepts_only_members_and_root_build_files`
    — TRUE cases: `crates/devflow-core/src/lib.rs`, `crates/devflow-cli/src/main.rs`,
    `crates/devflow-core/Cargo.toml`, `crates/devflow-cli/build.rs`, `Cargo.toml`, `Cargo.lock`,
    `build.rs`, `rust-toolchain.toml`.
    — FALSE cases (the negative controls; a suite with only TRUE cases cannot fail and proves
    nothing): `.planning/spikes/foo/Cargo.toml`, `.planning/spikes/foo/src/main.rs`,
    `.planning/45-PLAN.md`, `vendor/crates/devflow-core/src/lib.rs`,
    `crates-extras/foo/src/lib.rs`, `crates/../foo.rs`, `README.md`, `CHANGELOG.md`,
    `docs/guides/quickstart.md`, `scripts/hooks/pre-push`, `src/main.rs` (the standard Cargo
    layout — false in workspace scope by construction, and the reason Task 1's scope flag
    exists).
    Each FALSE assertion carries a message naming what the miss would cost, following the
    `is_self_dogfood_workspace` test convention.

    `affects_compiled_binary_unscoped_preserves_the_pre_phase_45_rule`
    — the same table evaluated with `workspace_scoped = false`, asserting the pre-change answers:
    `.planning/spikes/foo/src/main.rs`, `vendor/crates/devflow-core/src/lib.rs` and
    `src/main.rs` are TRUE here, while `.planning/45-PLAN.md`, `README.md` and
    `docs/guides/quickstart.md` remain FALSE. This is the zero-regression proof for every
    non-DevFlow project.

    `porcelain_tracked_path_normalizes_status_bytes_renames_and_quotes`
    — a modified entry yields the bare path; a staged-modified entry yields the bare path; a
    rename entry of the form `ORIG -> PATH` yields the destination, not the origin; a quoted
    path yields the unquoted value.
    NEGATIVE CONTROLS, same test: an untracked `??` line yields `None`, and a line shorter than
    four characters yields `None`. Without those the function could return `Some` for everything
    and still pass.
  </behavior>

  <action>
    Add the three tests to `staleness.rs`'s existing test module, placed adjacent to the
    `is_self_dogfood_workspace` tests so the predicate tests sit with their sibling. These are
    the first direct tests either function has ever had — verified at source that
    `affects_compiled_binary` and `porcelain_tracked_path` are today exercised only indirectly
    through git-fixture tests, so this is a new block, not an extension of an existing one.

    Add a short comment beside `porcelain_tracked_path` recording that a leading `./` prefix is
    deliberately NOT stripped because neither producer emits one — `git status --porcelain` and
    `git diff --name-only` both emit repo-relative paths without it. This closes REVIEWS finding
    2's `./` sub-item with a stated reason rather than speculative code, and stops a future
    reader from re-raising it.

    Do not add a test that asserts an outcome the code cannot produce. If any table entry above
    does not match the implementation from Task 1, the discrepancy is a finding to report, not a
    table entry to adjust.
  </action>

  <verify>
    <automated>cargo test -p devflow --bin devflow affects_compiled_binary 2>&1 | rg 'test result: ok. 2 passed'</automated>
    <fails_when>non-zero exit, or a passed count other than 2 (fewer means a test is missing or the name filter matched nothing; "0 passed" exits 0 and is the false-green trap CLAUDE.md warns about)</fails_when>

    <automated>cargo test -p devflow --bin devflow porcelain_tracked_path_normalizes_status_bytes_renames_and_quotes 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test --workspace > target/45-02-suite.txt 2>&1; echo "cargo_exit=$?"; grep -c '^test result: FAILED' target/45-02-suite.txt</automated>
    <fails_when>the line `cargo_exit=0` is absent, or the trailing count printed by grep is any value other than 0. Do NOT substitute the Phase-44 form `| rg -c '^test result: FAILED' | rg '^0$'` — verified this session that `rg -c` prints nothing and exits 1 on zero matches, so that pipeline can never pass on a green suite.</fails_when>

    <automated>cargo clippy --workspace --all-targets -- -D warnings 2>&1 | rg 'Finished|error'</automated>
    <fails_when>any line containing "error" appears in the output, or the "Finished" line is absent</fails_when>
  </verify>

  <acceptance_criteria>
    - `crates/devflow-cli/src/staleness.rs` contains a test named `affects_compiled_binary_in_workspace_scope_accepts_only_members_and_root_build_files` asserting at least 8 TRUE cases and at least 11 FALSE cases, with `.planning/spikes/foo/Cargo.toml` and `.planning/spikes/foo/src/main.rs` among the FALSE cases.
    - The FALSE set includes `vendor/crates/devflow-core/src/lib.rs` and `crates-extras/foo/src/lib.rs`, proving path-segment prefix matching rather than substring matching.
    - `crates/devflow-cli/src/staleness.rs` contains a test named `affects_compiled_binary_unscoped_preserves_the_pre_phase_45_rule` in which `src/main.rs`, `vendor/crates/devflow-core/src/lib.rs` and `.planning/spikes/foo/src/main.rs` all assert TRUE.
    - `porcelain_tracked_path_normalizes_status_bytes_renames_and_quotes` asserts a `None` result for at least one `??` line and one short line, alongside its `Some` cases.
    - Every FALSE assertion in the workspace-scope test carries an assertion message stating what a wrong answer would cost.
    - `cargo test --workspace` reports `cargo_exit=0` and a `^test result: FAILED` count of 0.
    - `cargo clippy --workspace --all-targets -- -D warnings` prints a `Finished` line and no `error` line.
  </acceptance_criteria>

  <done>
    `affects_compiled_binary` and `porcelain_tracked_path` have direct unit tests with negative
    controls in both directions, the pre-Phase-45 rule is pinned for non-DevFlow projects, and
    the `./`-prefix sub-item of REVIEWS finding 2 is closed in a source comment with its reason.
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| git worktree contents → staleness predicate | repo-relative paths from `git status --porcelain` and `git diff --name-only` are untrusted input to the predicate |
| staleness verdict → pipeline gate | the verdict decides whether a stage hard-blocks; a wrong Fresh lets a stale binary drive its own workspace |
| `enforce_build_staleness` message → `.devflow/events.jsonl` | the block message embeds an absolute execution path; the persisted event must not |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-45-09 | Spoofing | `affects_compiled_binary` narrowed rule | critical | mitigate | The narrowing must never turn a real workspace-source change into Fresh — the Phase 16 false-evidence class. Task 1's negative control asserts `Err` for a modified `crates/devflow-cli/src/main.rs` in the same test that asserts `Ok` for spikes-only; Task 2's TRUE table pins all eight build-affecting shapes |
| T-45-10 | Tampering | path prefix matching | high | mitigate | `WORKSPACE_MEMBER_PREFIX` includes a trailing separator so `crates-extras/` and `vendor/crates/` cannot satisfy it; both are explicit FALSE cases in Task 2, and `crates/../foo.rs` is a third |
| T-45-11 | Denial of Service | non-DevFlow projects losing the stale-build warning | medium | mitigate | The `workspace_scoped` flag preserves today's broad rule outside DevFlow's own workspace; `affects_compiled_binary_unscoped_preserves_the_pre_phase_45_rule` is the regression guard |
| T-45-12 | Repudiation | `is_self_dogfood_workspace` reorder in `enforce_build_staleness` | medium | mitigate | Task 1 requires the reordered call to keep its `project_root` anchor (Assumption A3, `staleness.rs:315-323`); a silent switch to `execution_root` would change which manifest decides the scope |
| T-45-13 | Information Disclosure | `self_dogfood_stale_blocked` event payload | low | accept | Pre-existing and already mitigated at `staleness.rs` by the WR-02 comment and a path-free payload. This plan changes no message and no payload; the disposition is accept-unchanged, not accept-unexamined |
| T-45-14 | Tampering | npm/pip/cargo installs | high | accept | This plan adds no dependency to any manifest, so no package-legitimacy gate applies. An executor that finds itself adding a crate must stop and escalate — this phase has no RESEARCH.md and therefore no Package Legitimacy Audit table |
</threat_model>

<verification>
- `cargo test --workspace` reports `cargo_exit=0` and zero `^test result: FAILED` lines.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` are clean.
- `mixed_range_docs_and_source_is_stale` passes with its assertions unmodified.
- Both Task 1 tests are recorded in the SUMMARY as observed-failing against HEAD before the fix — the RED half, quoted from real output rather than asserted from memory.
- The SUMMARY states explicitly whether the `workspace_scoped` design decision above was accepted or overruled.
</verification>

<success_criteria>
AUTO-02 is satisfied: `affects_compiled_binary` inspects only Cargo workspace members
(`crates/`) plus the four root build files when evaluating DevFlow's own workspace, so
`.planning/spikes/` and non-workspace crates never trip the D-18 hard block; every other
project keeps today's behaviour; and both directions are proven by direct unit tests with
negative controls.
</success_criteria>

<output>
Create `.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-02-SUMMARY.md` when done.
</output>

## ===== 45-03-PLAN.md =====

---
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
plan: 03
type: tdd
wave: 1
depends_on: []
files_modified:
  - crates/devflow-core/src/prompt.rs
autonomous: true
requirements: [DECN-01]

estimate:
  tokens: 22000
  raw_tokens: 44000
  tasks: 2
  confidence: med

must_haves:
  truths:
    - "The Code stage prompt delivered to Claude and OpenCode carries an explicit decision-checkpoint policy instructing the agent to evaluate all presented options on their merits rather than taking the first one (D-03 / DECN-01)."
    - "The Code stage prompt delivered to Codex and Pi carries the byte-identical policy, so the two agent families do not have different unattended semantics."
    - "The policy requires the agent to record its reasoning for the decision in its final message, so an unattended resolution is auditable after the fact."
    - "The policy explicitly excludes `blocking-human` gates and package-legitimacy checkpoints from self-resolution — it grants merit-based judgement on ordinary `decision` checkpoints only."
    - "The policy text and the advisory self-review text live in ONE shared constant referenced by both renderers, so the two copies cannot drift apart (they are duplicated verbatim at HEAD with no shared constant)."
    - "Prompts that are not the full-execute Code prompt — Validate, Ship, and the `GapsOnly` / `AuditFix` fix arms — do NOT carry the policy, proving the presence assertions discriminate between prompts rather than matching everything."
    - "No prompt reintroduces the tokens `AskUserQuestion` or `request_user_input`, which the existing pinning test forbids and an unattended run cannot answer."
  artifacts:
    - crates/devflow-core/src/prompt.rs
  key_links:
    - "`CODE_STAGE_POLICY` -> `code_stage_prompt` -> `render_claude_style` -> `stage_prompt`: the Claude/OpenCode delivery path."
    - "`CODE_STAGE_POLICY` -> `workflow_code_prompt` (the `FullExecute | None` arm) -> `render_workflow_style`: the Codex/Pi delivery path. One constant feeds both, which is what makes the two families' semantics provably identical."
    - "`COMPLETION_PROTOCOL` stays the LAST element of both prompts after the policy block is inserted — `checkpoint_auto_decide_prompt_terminates_with_completion_protocol` shows the existing `ends_with` contract this must not break."
  prohibitions:
    - statement: "The decision policy must never be phrased so that it authorizes the agent to self-resolve a `blocking-human` gate or a package-legitimacy checkpoint. Those carve-outs exist because no automated judgement is acceptable there; a policy that reads as blanket permission to decide converts a deliberate human gate into a silent auto-approval."
    - statement: "Requiring the agent to 'record its reasoning' must not become an invitation to manufacture a post-hoc justification for a decision it did not actually evaluate. The policy must ask for the comparison that produced the choice, not merely for a sentence explaining the choice."
---

<objective>
Give the Code stage an explicit unattended decision-checkpoint policy: when the agent reaches a
`decision` checkpoint with no operator available, resolve it by evaluating the presented options
on their merits and record the reasoning — rather than taking whichever option happens to be
listed first.

Purpose: DECN-01 / 999.94. Verified this session at
`~/.claude/gsd-core/workflows/execute-phase.md:1123`, the workflow DevFlow's Code prompt tells
the agent to run says, verbatim: *"**decision** → Auto-spawn continuation agent with
`{user_response}` = first option from checkpoint details."* Positional order is not merit.

Output: one shared policy constant delivered identically to both agent families, with tests that
pin its content and prove the assertions discriminate — plus a plainly stated limit on what those
tests establish. Implements D-03 exactly as locked in 45-CONTEXT.md: a dedicated policy
instruction layer in `code_stage_prompt`, extended to the sibling renderer so the two agent
families cannot diverge.
</objective>

<execution_context>
@~/.claude/gsd-core/workflows/execute-plan.md
@~/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@CLAUDE.md
@.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-CONTEXT.md
@.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-PATTERNS.md
@.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-REVIEWS.md
</context>

## What this plan does NOT establish — read before accepting it

D-03 locks the mechanism to a prompt-template change in `prompt.rs`. That mechanism has a
ceiling, and this plan states it rather than letting a green test suite imply more than it proves:

1. **A string-presence test proves the instruction was DELIVERED. It does not prove the agent
   OBEYED it.** Every test in this plan asserts on the rendered prompt text. None of them
   observes a model resolving a real checkpoint. There is no unit test that can close that gap.
2. **This does not change `execute-phase.md`.** That file lives in `~/.claude/gsd-core/`, outside
   this repository, and its `decision` branch still reads "first option from checkpoint details".
   This plan adds a competing instruction in the prompt that carries the workflow invocation; if
   the agent follows the workflow file's procedural step literally, the prompt policy loses.
   REVIEWS finding 3 named this risk and it is REAL, not hypothetical — it was verified at that
   file and line.
3. **Therefore DECN-01 is mitigated here, not closed.** The honest end state after this plan is
   "the unattended run is now instructed to decide on merit and to show its work," not
   "an unattended decision checkpoint provably never takes the first option."
   **Recommended follow-up, for the operator to accept or decline:** file a backlog item against
   `@opengsd/gsd-core` to make `execute-phase.md`'s `decision` branch merit-based rather than
   positional. That is upstream work and is out of this phase's scope; it is named here so the
   residual gap is recorded rather than forgotten.

## Review incorporation (REVIEWS.md finding 3)

- **"Clarify the boundary between `code_stage_prompt` policy and `checkpoint_auto_decide_prompt`"
  — INCORPORATED.** Verified at source: `checkpoint_auto_decide_prompt` (`prompt.rs:515-527`) is
  injected into a RESUMED session after DevFlow's own human-blocking gate found nobody to answer
  it. It is not reached during an ordinary one-shot Code stage. The two are complementary, not
  redundant: this plan's policy advises the agent DURING execution, that one handles DevFlow's own
  post-hoc gate resume. Task 1 records this boundary in a doc comment on the new constant so a
  future reader does not delete one as a duplicate of the other.
- **"Prompt instructions cannot override a hardcoded procedural step if GSD does not consult the
  LLM" — INCORPORATED as a stated limitation, not as a code change.** See item 2 above. The
  finding is correct and this plan cannot fix it from inside this repository; D-03 locks the
  mechanism to `prompt.rs`. Recorded with a recommended upstream follow-up rather than silently
  dropped.
- **"Add test fixtures verifying option ordering and decision rationale recording" —
  INCORPORATED WITH A CORRECTION.** Rendered-prompt tests cannot verify *option ordering
  behaviour*, because no option list exists at render time — the checkpoint's options are
  discovered by the agent at run time, inside GSD, not by DevFlow. What IS deterministically
  testable, and what Task 2 asserts, is that the delivered text forbids positional selection and
  requires the reasoning to be recorded. Claiming a prompt test verifies ordering behaviour would
  be exactly the proxy-measurement error this project's CLAUDE.md warns about.

## Deferred (recorded, not dropped)

An operator-facing paragraph describing this policy belongs in `docs/guides/unattended-mode.md`.
It is **deferred out of this plan** because 45-01 already modifies that file in the same wave,
and two same-wave plans may not share a `files_modified` entry. No `doc_check` test requires it
(the policy introduces no `DEVFLOW_*` env var and no CLI subcommand), so nothing goes red. Add it
in a later docs pass.

## Flagged assumption (spec-less probe fallback, §C)

DECN-01's probe row came back `{"category":"unclassified","status":"unresolved"}`. Per the
protocol an `unclassified` row is never auto-resolved with a backstop and never silently dropped:

> **FLAGGED (DECN-01, unclassified/unresolved):** the probe could not classify DECN-01's edge
> category, so no probe-derived acceptance criterion was authored into `must_haves.truths` from
> it. This is a plausible miss rather than a plausible absence — a requirement about resolving
> checkpoints without an operator has obvious edges (what happens when the options are
> indistinguishable on merit? when there are zero options? when the checkpoint is malformed?),
> and NONE of them are reachable from DevFlow's side, because DevFlow renders a prompt and never
> parses the checkpoint. **Reviewer: confirm you accept that those edges belong to GSD-core, not
> to this phase.**

No-silent-drop accounting for this plan: 1 probe-surfaced item = 0 authored into `truths` from
the probe + 1 surfaced as a flagged assumption.

**Canon-referral breadcrumb (§B):** "policy text becoming an injection vector if a checkpoint's
own content is interpolated into it" was recalled and is canon prompt-injection, so it is DROPPED
from `must_haves.prohibitions` per the canon-referral rule. It is also inapplicable here: the
constant is static text with no interpolation of external content.

## Artifacts this phase produces

New symbols introduced by THIS plan (excluded from drift verification — they do not exist at
HEAD):

| Kind | Name | Location |
|---|---|---|
| const | `CODE_STAGE_POLICY: &str` (shared advisory self-review + decision policy block) | `crates/devflow-core/src/prompt.rs` |

<tasks>

<task type="tracer" tdd="true">
  <name>Task 1: One shared policy constant reaches both agent families</name>

  <read_first>
    - crates/devflow-core/src/prompt.rs lines 40-66 (`AUTO_CHAIN_PRESERVING_FLAG`, the `COMPLETION_PROTOCOL` const, and `StageIntent`)
    - crates/devflow-core/src/prompt.rs lines 318-360 (`stage_prompt`, `stage_prompt_for_project`, and `code_stage_prompt` in full — including the "Advisory incremental self-review" block at :344-353 whose composition shape the new block copies)
    - crates/devflow-core/src/prompt.rs lines 360-405 (`render_claude_style` routing Code to `code_stage_prompt`, and `render_workflow_style` routing Code to `workflow_code_prompt`)
    - crates/devflow-core/src/prompt.rs lines 426-455 (`workflow_code_prompt` — note the advisory block is duplicated VERBATIM in the `FullExecute | None` arm at :437-450, and that the `AuditFix` and `GapsOnly` arms deliberately do not carry it)
    - crates/devflow-core/src/prompt.rs lines 505-530 (`checkpoint_auto_decide_prompt` — the closest existing prose model for this policy's wording, and a DIFFERENT code path that must not be conflated with it)
    - crates/devflow-core/src/prompt.rs lines 655-695 (`code_stage_prompt_is_unchanged_single_command_template` — the pinning test, including its `!contains("AskUserQuestion")` and `!contains("request_user_input")` assertions)
    - .planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-PATTERNS.md (the D-03 section, which enumerates both duplication sites and the test analogs)
  </read_first>

  <files>crates/devflow-core/src/prompt.rs</files>

  <behavior>
    Write these tests FIRST and observe them fail against HEAD before implementing.

    - `code_policy_is_identical_across_both_renderers`: render the Code stage through
      `stage_prompt(Stage::Code, phase)` (the Claude/OpenCode path) and through
      `render_workflow_style` with a `StageIntent::Code { fix: None }` and any workflow root (the
      Codex/Pi path). Assert both contain the shared policy constant as a substring, and assert
      the two policy substrings are the same value by comparing against the constant itself
      rather than against each other. This is the end-to-end assertion for this plan: it runs at
      the two PUBLIC entry points, not against the private per-stage builders, so a future
      refactor that stops routing through them is caught.

    - `code_policy_is_absent_from_prompts_that_must_not_carry_it`: assert the policy constant is
      NOT contained in `stage_prompt(Stage::Validate, phase)`, NOT in
      `stage_prompt(Stage::Ship, phase)`, NOT in the `GapsOnly` fix arm, and NOT in the
      `AuditFix` fix arm.
      THIS IS THE NEGATIVE CONTROL for the whole plan. Without it, a `contains` assertion that
      happened to match every prompt string would look exactly like a passing test. If both this
      test and the one above pass, the assertions discriminate; if this one fails, the presence
      assertions above are measuring nothing.

    - `both_code_prompts_still_end_with_the_completion_protocol`: assert
      `stage_prompt(Stage::Code, phase)` and the workflow-style Code prompt both `ends_with` the
      `COMPLETION_PROTOCOL` constant, matching the existing contract that
      `checkpoint_auto_decide_prompt_terminates_with_completion_protocol` pins for its sibling.
      Inserting a block in the wrong position is the obvious way to break this.
  </behavior>

  <action>
    Add a private `CODE_STAGE_POLICY: &str` constant to `prompt.rs`, placed beside
    `COMPLETION_PROTOCOL`, holding BOTH policy sections that the full-execute Code prompt carries:
    the existing "Advisory incremental self-review" section moved verbatim (do not reword it —
    the pinning test asserts its four angle strings), followed by a new
    "Unattended decision checkpoints" section.

    Write the new section's text to say, in prose, all of the following. Wording is yours; every
    item must be present:
    - When a `decision` checkpoint is reached and no operator is available, resolve it rather than
      pausing — this run is unattended and nobody is coming.
    - Choose by comparing the presented options against the phase's goal and constraints. Do not
      choose an option because of its position in the list. State explicitly that the GSD workflow
      being invoked describes selecting the first option, and that this instruction supersedes
      that for merit-based choices.
    - Where an option is explicitly marked as recommended, treat that marking as evidence and
      weigh it; it is not automatically decisive.
    - Record the comparison that produced the choice in the final message — which options were
      considered, and why the chosen one won — not merely a sentence asserting the choice. The
      final message is the only record of the decision.
    - This authority does NOT extend to a `blocking-human` gate or to a package-verification
      checkpoint. Those remain human-only; do not self-resolve them, do not approve them, and
      report them instead.
    - This must not pause execution or request human input, matching the sibling advisory
      section's closing sentence.

    Give the constant a doc comment recording the boundary REVIEWS finding 3 asked for: this
    policy advises the agent DURING one-shot Code execution, whereas `checkpoint_auto_decide_prompt`
    is injected into a RESUMED session after DevFlow's own human-blocking gate found no operator.
    They are complementary; neither is a duplicate of the other, and neither should be deleted as
    one.

    Rewrite `code_stage_prompt` to interpolate `CODE_STAGE_POLICY` in place of its inline advisory
    paragraph, keeping `COMPLETION_PROTOCOL` last. Rewrite `workflow_code_prompt`'s
    `FullExecute | None` arm the same way. After this, the two prompts share one source of truth
    and the verbatim duplication that exists at HEAD is gone. Leave the `AuditFix` and `GapsOnly`
    arms without the policy — they are not full executions and the negative-control test asserts
    their exclusion.

    Do not introduce the tokens `AskUserQuestion` or `request_user_input` anywhere in the new
    text; the existing pinning test forbids both, and an unattended run cannot answer either.
  </action>

  <verify>
    <automated>cargo test -p devflow-core --lib code_policy_is_identical_across_both_renderers 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line — a test-name filter matching nothing exits 0, which is the CLAUDE.md false-green trap</fails_when>

    <automated>cargo test -p devflow-core --lib code_policy_is_absent_from_prompts_that_must_not_carry_it 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow-core --lib both_code_prompts_still_end_with_the_completion_protocol 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow-core --lib prompt:: 2>&1 | rg 'test result: ok. [1-9][0-9]* passed'</automated>
    <fails_when>non-zero exit, or "0 passed", or any line matching "^test result: FAILED"</fails_when>
  </verify>

  <acceptance_criteria>
    - `crates/devflow-core/src/prompt.rs` declares a single `CODE_STAGE_POLICY` constant.
    - `code_stage_prompt` and `workflow_code_prompt`'s `FullExecute | None` arm each reference `CODE_STAGE_POLICY`, and neither contains an inline copy of the advisory self-review paragraph any more — the verbatim duplication present at HEAD is eliminated.
    - `stage_prompt(Stage::Code, ..)` and `render_workflow_style` on a `StageIntent::Code { fix: None }` both contain `CODE_STAGE_POLICY` as a substring.
    - `code_policy_is_absent_from_prompts_that_must_not_carry_it` passes, covering Validate, Ship, `GapsOnly` and `AuditFix`. If it does not pass, the presence assertions are non-discriminating and the task is not done regardless of the other tests being green.
    - Both Code prompts still satisfy `ends_with(COMPLETION_PROTOCOL)`.
    - The new policy text contains neither `AskUserQuestion` nor `request_user_input`.
    - `CODE_STAGE_POLICY`'s doc comment names `checkpoint_auto_decide_prompt` and states the boundary between the two.
    - The three new tests are observed FAILING against HEAD before the implementation lands, and the observed failure output is recorded in the SUMMARY as the RED half of the cycle.
  </acceptance_criteria>

  <done>
    One shared constant carries both the advisory self-review and the new decision policy; both
    the Claude/OpenCode and the Codex/Pi Code prompts deliver it identically; prompts that must
    not carry it provably do not; and the completion protocol is still last.
  </done>

  <reversibility rating="reversible">
    A prompt-template change behind one constant, with no persisted state and no public API
    change. Reverting is a one-commit revert. Matches the operator's D-03 rating.
  </reversibility>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Pin the policy's content and update the frozen pinning test</name>

  <read_first>
    - crates/devflow-core/src/prompt.rs lines 655-695 (`code_stage_prompt_is_unchanged_single_command_template` — the frozen pinning test, whose four angle assertions and two not-contains assertions must survive)
    - crates/devflow-core/src/prompt.rs lines 835-885 (the `checkpoint_auto_decide_prompt` test trio: determinism, terminator, and lowercased keyword assertions with a stated reason per assert — this is the shape to copy)
    - crates/devflow-core/src/prompt.rs (the `CODE_STAGE_POLICY` constant as written by Task 1)
  </read_first>

  <files>crates/devflow-core/src/prompt.rs</files>

  <behavior>
    - `code_policy_forbids_positional_option_selection`: lowercase the rendered Code prompt and
      assert it contains a token conveying "first option" together with a negation, and that it
      contains the word "merit" or "merits". Assert with a message stating what a miss costs: the
      GSD workflow's own `decision` branch instructs first-option selection, so a policy that does
      not explicitly contradict it adds nothing.
      NEGATIVE CONTROL, same test: assert the SAME lowercased assertions do NOT hold for
      `stage_prompt(Stage::Validate, ..)`. An assertion that passes on an unrelated prompt is
      measuring the English language, not this policy.

    - `code_policy_requires_the_reasoning_to_be_recorded`: lowercase the rendered Code prompt and
      assert it contains both "record" and "reasoning", mirroring the existing
      `checkpoint_auto_decide_prompt_states_no_operator_judgment_and_record_reasoning` assertion
      and its stated reason (the final message is the only record of what was decided).

    - `code_policy_excludes_blocking_human_and_package_checkpoints`: lowercase the rendered Code
      prompt and assert it contains a token for the human-blocking gate carve-out and a token for
      package verification. This is the safety assertion: without it, a policy granting
      merit-based judgement reads as blanket permission to auto-resolve gates that exist
      precisely because automated judgement is unacceptable there.

    - `code_stage_prompt_is_unchanged_single_command_template` (EXISTING): extend, do not weaken.
      Every assertion it makes today must still hold — the `/gsd-execute-phase 9` command, the
      `DEVFLOW_RESULT` token, the four self-review angles, and both not-contains assertions. Add
      an assertion that the decision policy is present. Do NOT delete an existing assertion to
      make the test pass; if one genuinely no longer applies, stop and report it rather than
      removing it.

    - `code_stage_prompt_is_deterministic`: two renders of the same stage and phase are equal,
      copying `checkpoint_auto_decide_prompt_is_deterministic`.
  </behavior>

  <action>
    Add the four new tests and extend the existing pinning test as described. Give every assertion
    an explanatory message following this module's convention — each assert in the
    `checkpoint_auto_decide_prompt` trio states why it exists, and a bare `assert!` with no message
    is the shape to avoid.

    Add a short comment block above the new tests recording, in one or two sentences, what this
    group of tests does NOT establish: they prove the instruction is present in the delivered
    prompt, not that any model obeys it, and they do not change the GSD workflow file whose own
    `decision` branch describes positional selection. This is the source-resident form of the
    limitation stated in this plan's objective, put where a future reader of the test file will
    actually see it before drawing a stronger conclusion than the tests support.

    Do not add a test asserting that a specific option gets chosen, or that options are reordered.
    No option list exists at render time — DevFlow renders a prompt and never parses a checkpoint
    — so such a test could only assert against a fabricated fixture and would prove nothing about
    the real path.
  </action>

  <verify>
    <automated>cargo test -p devflow-core --lib code_policy_forbids_positional_option_selection 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow-core --lib code_policy_requires_the_reasoning_to_be_recorded 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow-core --lib code_policy_excludes_blocking_human_and_package_checkpoints 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test -p devflow-core --lib code_stage_prompt_is_unchanged_single_command_template 2>&1 | rg 'test result: ok. 1 passed'</automated>
    <fails_when>non-zero exit, or "0 passed" in the summary line</fails_when>

    <automated>cargo test --workspace > target/45-03-suite.txt 2>&1; echo "cargo_exit=$?"; grep -c '^test result: FAILED' target/45-03-suite.txt</automated>
    <fails_when>the line `cargo_exit=0` is absent, or the trailing count printed by grep is any value other than 0. Do NOT substitute the Phase-44 form `| rg -c '^test result: FAILED' | rg '^0$'` — verified this session that `rg -c` prints nothing and exits 1 on zero matches, so that pipeline can never pass on a green suite.</fails_when>

    <automated>cargo clippy --workspace --all-targets -- -D warnings 2>&1 | rg 'Finished|error'</automated>
    <fails_when>any line containing "error" appears in the output, or the "Finished" line is absent</fails_when>
  </verify>

  <acceptance_criteria>
    - Four new tests exist with the names given in `behavior`, and each reports `1 passed`.
    - `code_policy_forbids_positional_option_selection` contains a negative control asserting its keyword conditions do NOT hold for a non-Code prompt.
    - `code_stage_prompt_is_unchanged_single_command_template` retains every assertion it has at HEAD — the `/gsd-execute-phase 9` substring, `DEVFLOW_RESULT`, all four self-review angles, `!contains("/gsd-code-review")`, `!contains("already exists")`, `!contains("AskUserQuestion")`, `!contains("request_user_input")` — plus a new assertion for the decision policy. `git diff` on this file must show no assertion deleted from that test.
    - Every new assertion carries an explanatory message; no bare `assert!` without a message is added.
    - A comment above the new test group states that these tests prove delivery of the instruction and not compliance with it, and that they do not modify the GSD workflow file.
    - `cargo test --workspace` reports `cargo_exit=0` and a `^test result: FAILED` count of 0.
    - `cargo clippy --workspace --all-targets -- -D warnings` prints a `Finished` line and no `error` line.
  </acceptance_criteria>

  <done>
    The policy's required content — no positional selection, merit-based comparison, recorded
    reasoning, and the human-gate carve-out — is pinned by tests with a working negative control;
    the frozen pinning test is extended rather than weakened; and the limits of what these tests
    establish are recorded in the source next to them.
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| DevFlow prompt → agent process | the rendered prompt is the only channel by which DevFlow constrains an unattended agent's behaviour |
| GSD workflow file → agent process | `execute-phase.md`, outside this repository, is a second and competing instruction source reaching the same agent |
| agent's final message → decision audit trail | the recorded rationale is the ONLY record of what an unattended run decided and why |

## STRIDE Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation Plan |
|-----------|----------|-----------|----------|-------------|-----------------|
| T-45-15 | Elevation of Privilege | decision policy text | critical | mitigate | The policy explicitly excludes `blocking-human` gates and package-verification checkpoints from self-resolution; `code_policy_excludes_blocking_human_and_package_checkpoints` (Task 2) pins that exclusion. Without it, granting merit-based judgement reads as permission to auto-approve gates that exist because automated judgement is unacceptable |
| T-45-16 | Repudiation | unattended decision with no recorded rationale | high | mitigate | The policy requires the comparison that produced the choice — options considered and why the winner won — in the final message; `code_policy_requires_the_reasoning_to_be_recorded` pins it |
| T-45-17 | Spoofing | prompt policy losing to the workflow file's positional-selection step | high | accept | Cannot be mitigated from this repository: `execute-phase.md` is a GSD-core artifact and D-03 locks this phase's mechanism to `prompt.rs`. Accepted with the residual gap stated in the objective and an upstream follow-up recommended. This is an ACCEPT with a named owner, not an unexamined one |
| T-45-18 | Tampering | the two duplicated prompt blocks drifting apart | medium | mitigate | Task 1 collapses them into one `CODE_STAGE_POLICY` constant referenced by both renderers, and `code_policy_is_identical_across_both_renderers` asserts both paths carry it |
| T-45-19 | Denial of Service | the policy reintroducing an interactive-question token | medium | mitigate | The existing pinning test's `!contains("AskUserQuestion")` and `!contains("request_user_input")` assertions are preserved and explicitly protected by a Task 2 acceptance criterion forbidding assertion deletion |
| T-45-20 | Tampering | npm/pip/cargo installs | high | accept | This plan adds no dependency to any manifest, so no package-legitimacy gate applies. An executor that finds itself adding a crate must stop and escalate — this phase has no RESEARCH.md and therefore no Package Legitimacy Audit table |
</threat_model>

<verification>
- `cargo test --workspace` reports `cargo_exit=0` and zero `^test result: FAILED` lines.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` are clean.
- `code_policy_is_absent_from_prompts_that_must_not_carry_it` passes — without it the presence assertions are non-discriminating and every other green test in this plan is uninformative.
- The RED half of Task 1's cycle is recorded in the SUMMARY as observed failure output.
- The SUMMARY restates the limitation from this plan's objective: these tests prove delivery, not compliance, and DECN-01 is mitigated rather than closed.
</verification>

<success_criteria>
DECN-01 is mitigated to the ceiling D-03's locked mechanism allows: both agent families receive
an identical, explicitly non-positional, merit-based decision-checkpoint policy that requires
recorded reasoning and carves out human-only gates; the two previously-duplicated prompt blocks
share one constant; and the residual gap — that GSD's own workflow file still describes
first-option selection — is recorded rather than papered over.
</success_criteria>

<output>
Create `.planning/phases/45-unattended-auto-mode-hardening-999-110-999-109-999-94/45-03-SUMMARY.md` when done.
</output>

## ===== 45-CONTEXT.md (locked operator decisions) =====
# Phase 45: Unattended Auto-Mode Hardening (999.110 + 999.109 + 999.94) - Context

**Gathered:** 2026-09-01
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase hardens `--mode auto` execution so that unattended phase runs can launch cleanly out of the box and run safely. Specifically:
1. Resolving the worktree base branch / start point from configuration (e.g. `base_branch` or `planning_branch` with fallback to `develop`) so that `.planning/` and `.planning/config.json` are present in freshly created phase worktrees (AUTO-01 / 999.110).
2. Scoping the self-dogfood staleness check's `affects_compiled_binary` predicate to Cargo workspace members (`crates/*`) plus root build files, ignoring `.planning/spikes/` and other non-workspace code (AUTO-02 / 999.109).
3. Establishing a prompt policy layer in the Code stage instructing the agent to evaluate `decision` checkpoints on merit and record its reasoning rather than blindly taking the first option (DECN-01 / 999.94).

</domain>

<decisions>
## Implementation Decisions

### Worktree Base Branch Resolution (AUTO-01 / 999.110)
- **D-01:** Add configurable base branch resolution in DevFlow config (`git.base_branch` or `git.planning_branch`) with `develop` as the default. When creating a phase worktree or checking base ref reachability/currency, use the configured base branch so personal tracking branches like `workspace/denniyahh` carrying `.planning/` can be targeted cleanly. — **Reversibility:** costly — changes configuration fields and worktree creation call sites across CLI commands and preflight.

### Workspace Member Scoping for Staleness Check (AUTO-02 / 999.109)
- **D-02:** Scope `affects_compiled_binary` to Cargo workspace member paths (`crates/*`) and root build files (`Cargo.toml`, `Cargo.lock`, `build.rs`, `rust-toolchain.toml`). Any path outside `crates/` (e.g. `.planning/spikes/*`) must return `false` so non-workspace spikes never trip the D-18 self-dogfood stale build block. — **Reversibility:** reversible — pure path predicate update in `staleness.rs`.

### Unattended Decision Checkpoint Policy (DECN-01 / 999.94)
- **D-03:** Add a dedicated policy instruction layer to `code_stage_prompt` in `prompt.rs`. When unattended, the agent is instructed to resolve any `decision` checkpoint by evaluating all presented options on their merits, respecting explicit recommended markings where present, and recording the rationale in the final response. — **Reversibility:** reversible — prompt template enhancement in `prompt.rs`.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Worktree & Preflight
- `crates/devflow-core/src/worktree.rs` — Worktree management and `add` start-point execution
- `crates/devflow-cli/src/commands.rs` — Pre-start checks, worktree creation wiring, and `ensure_phase_worktree`
- `crates/devflow-cli/src/parallel.rs` — `ensure_phase_worktree` implementation for parallel/standard phase worktree setup
- `crates/devflow-cli/src/preflight.rs` — Base ref currency, reachability, and `unattended_config_condition`

### Staleness Check
- `crates/devflow-cli/src/staleness.rs` — Build staleness detection and `affects_compiled_binary` definition

### Stage Prompts & Checkpoint Resolution
- `crates/devflow-core/src/prompt.rs` — `code_stage_prompt` and `checkpoint_auto_decide_prompt` contracts

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `devflow_core::worktree::add`: Takes `start_point` parameter for `git worktree add -b <branch> <path> <start_point>`.
- `devflow_core::config`: Configuration structures for git and workflow settings.
- `staleness::porcelain_tracked_path`: Extracts repository-relative tracked path from git status output.

### Established Patterns
- All preflight failure messages avoid embedding absolute host filesystem paths or usernames (WR-02 / 999.10).
- Staleness checks fail toward `Stale` on unexpected git errors to maintain safety.
- Stage prompts append `{COMPLETION_PROTOCOL}` to enforce structured outcome reporting.

### Integration Points
- `crates/devflow-cli/src/commands.rs`: `start` entry point where base branch reachability and currency are checked.
- `crates/devflow-cli/src/parallel.rs`: `ensure_phase_worktree` where `worktree::add` is called.
- `crates/devflow-cli/src/staleness.rs`: `affects_compiled_binary` called by `ancestry_range_affects_build` and `tree_has_modified_build_inputs`.
- `crates/devflow-core/src/prompt.rs`: `code_stage_prompt` used by `stage_prompt`.

</code_context>

<specifics>
## Specific Ideas

- Ensure unit tests cover `.planning/spikes/foo/Cargo.toml` and `.planning/spikes/foo/src/main.rs` confirming `affects_compiled_binary` evaluates to `false`.
- Ensure tests verify that worktree creation honors configured base branch instead of hardcoding `develop`.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 45-Unattended Auto-Mode Hardening (999.110 + 999.109 + 999.94)*
*Context gathered: 2026-09-01*
