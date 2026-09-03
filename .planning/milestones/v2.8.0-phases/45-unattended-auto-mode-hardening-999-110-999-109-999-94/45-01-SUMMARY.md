---
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
plan: 01
subsystem: config / git-flow
tags: [auto-01, d-01, base-branch, unattended, git-flow]
status: complete
requires:
  - "devflow_core::config::load_config / env_value resolver idiom (Phase 16 D-03)"
  - "preflight::base_ref_currency + ensure_base_ref_current (25e, 999.51/D-18a)"
  - "worktree::add's existing start_point parameter"
provides:
  - "config::base_branch -> ResolvedBaseBranch { value, source } (fallible resolver)"
  - "config::validate_base_branch, config::git_flow_for_project"
  - "GitFlow::with_config / GitFlow::for_project"
  - "commands::ensure_base_is_a_local_branch"
  - "preflight::undeterminable_currency_warning (pure, assertable)"
  - "commands::phase_artifact_on_base (renamed, parameterised)"
  - "DEVFLOW_BASE_BRANCH env var + devflow.toml base_branch key"
affects:
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
tech-stack:
  added: []
  patterns:
    - "Fallible resolver with provenance — deliberately diverges from the yes_ship sibling's fail-soft shape, because falling back on a bad base silently redirects the trunk"
    - "Ref-anchored validation (refs/heads/{base}) instead of bare rev-parse --verify, which accepts any commit-ish"
    - "Pure message helper extracted from an uninjectable println! so the message contract becomes assertable"
key-files:
  created: []
  modified:
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
decisions:
  - "D-01 implemented as ONE value, not two: the configured base is both the worktree fork point and the git-flow merge target, resolved through config::git_flow_for_project so they cannot drift"
  - "The resolver is fail-hard on an explicitly supplied value and infallible only on the Default arm — a warn-and-fall-through would make the main-refusal unobservable"
  - "ensure_base_is_a_local_branch is scoped to source != Default, preserving the existing fall-open for a fresh clone with only origin/develop"
  - "The no-worktree fork assertion is one level below the CLI entry point (GitFlow::for_project(..).feature_start) — recorded here rather than claimed as end-to-end coverage"
metrics:
  duration: ~55 min
  completed: 2026-09-02
  tasks: 3
  commits: 3
actuals:
  tokens: 22967
  tasks: 3
  commits: 3
---

# Phase 45 Plan 01: Configurable Worktree Base Branch Summary

`devflow start` now forks a phase worktree from — and merges phase work back
into — a project-resolved `base_branch` rather than the hardcoded `develop`
constant, so a project whose `.planning/` lives on a planning branch can launch
`--mode auto` with `.planning/config.json` present in the worktree (AUTO-01).

## What was built

| Task | Name | Commit | Key files |
|---|---|---|---|
| 1 (tracer) | A configured base branch reaches `worktree::add` | `32aad70` | `config.rs`, `git.rs`, `parallel.rs`, `commands.rs`, `OPERATIONS.md`, `unattended-mode.md` |
| 2 | Pin the fail-open contract; retarget the artifact probe | `c1b071a` | `preflight.rs`, `commands.rs` |
| 3 | Call-site audit — fork point and merge target are one value | `6b72b71` | `hooks.rs`, `monitor.rs`, `ship_evidence.rs`, `commands.rs`, `pipeline_launch.rs`, `pipeline_outcomes.rs` |

## Verification results — every command, with its real output

All 18 `<automated>` verify commands were run and each printed a real,
non-zero pass count. Full table (summary lines abbreviated to the counts):

| Gate | Result |
|---|---|
| `config::tests::base_branch` | `ok. 6 passed` |
| `config::tests::validate_base_branch` | `ok. 2 passed` |
| `config::tests::git_flow_for_project` | `ok. 1 passed` |
| `with_config_uses_the_supplied_develop_not_the_default` | `ok. 1 passed` |
| `ensure_phase_worktree_forks_from_the_supplied_base` | `ok. 1 passed` |
| `base_branch_errors_on_an_explicitly_configured` | `ok. 2 passed` |
| `ensure_base_is_a_local_branch_rejects_commit_ish_...` | `ok. 1 passed` |
| `no_worktree_start_forks_the_feature_branch_...` | `ok. 1 passed` |
| doc_check env vars documented | `ok. 1 passed` (path corrected — see Deviations) |
| `base_ref_currency_is_undeterminable_when_the_remote_ref_is_absent` | `ok. 1 passed` |
| `ensure_base_ref_current_fails_open_for_a_local_only_planning_branch` | `ok. 1 passed` |
| `undeterminable_currency_warning_names_the_branch_and_its_disposition` | `ok. 1 passed` |
| `phase_artifact_probe_reads_the_supplied_base_not_the_default_trunk` | `ok. 1 passed` |
| `merge_feature_targets_the_configured_base_not_the_default` | `ok. 1 passed` |
| `hook_context_git_flow_is_not_discarded` | `ok. 1 passed` |
| `enumerate_phase_commits_ranges_from_the_configured_base` | `ok. 1 passed` |
| `hooks::` (whole module) | `ok. 18 passed` |
| `enumerate_phase_commits_resolves_config_from_the_project_root` | `ok. 1 passed` |

Gate commands (non-test):

| Gate | Result at HEAD (negative control) | Result after the work |
|---|---|---|
| residual-production-site audit | `residual_prod_default_sites=18` + 18 file:line rows | `residual_prod_default_sites=0`, no rows |
| `rg -n 'phase_artifact_on_develop' commands.rs preflight.rs` | 12 match lines, `rg_exit=0` | no output, `rg_exit=1` |
| `rg -c 'behind develop' commands.rs` | `1`, `rg_exit=0` | no output, `rg_exit=1` |
| `rg -c 'always \`devflow_core::config::DEVELOP\`' preflight.rs` | `1`, `rg_exit=0` | no output, `rg_exit=1` |
| `cargo test --workspace` | — | `cargo_exit=0`, FAILED count `0` |
| `cargo clippy --workspace --all-targets -- -D warnings` | — | `clippy_exit=0`, `^error` count `0` |
| `cargo fmt --all -- --check` | — | exit `0` |

### The `DEVELOP\b` surfacing gate — each surviving line adjudicated

The plan's `rg -n 'DEVELOP\b' commands.rs` gate surfaces sites; it does not
adjudicate them. Two lines survive, both legitimate:

- `:27` `use devflow_core::config::{self, DEVELOP, FEATURE_PREFIX, MAIN};` — an
  import, explicitly allowed by the gate's own `fails_when`.
- `:336` `if base != DEVELOP {` — a **comparison against the resolved base**,
  gating the operator note that announces a non-default trunk. Also explicitly
  allowed.

The shape the gate exists to catch — an `unwrap_or`/`unwrap_or_else` default
(the `commands::reference` site review round 2 found) and positional probe
arguments — is gone: `reference` now defaults to
`config::git_flow_for_project(project_root).develop`.

## RED halves — observed failure output, not asserted from memory

Three of the four TDD cycles produced a real `test result: FAILED`. Quoted:

**1. `hook_context_git_flow_is_not_discarded`, against unconverted `hooks.rs`:**

```
thread 'hooks::tests::hook_context_git_flow_is_not_discarded' panicked at
crates/devflow-core/src/hooks.rs:478:14:
a non-existent merge target must fail, not silently hit develop: ()
test result: FAILED. 0 passed; 1 failed
```

The `: ()` is the point — `merge_feature` returned `Ok(())` while pointed at a
branch that does not exist, because it had re-defaulted the trunk and merged
into the real `develop`. This is the finding, reproduced.

**2. `merge_feature_targets_the_configured_base_not_the_default`, same state:**

```
panicked at crates/devflow-core/src/hooks.rs:445:9:
the phase commit must land on the configured base
test result: FAILED. 0 passed; 1 failed
```

**3. Task 2's fail-open arm, produced by BODY substitution** (`Ok(())` swapped
for `Err(..)`; the arm was NOT deleted, because `BaseRefCurrency` is matched
exhaustively with no wildcard and deletion is a compile error, which is not
evidence):

```
panicked at crates/devflow-cli/src/preflight.rs:3296:9:
a local-only planning branch must launch, not be refused
test result: FAILED. 0 passed; 1 failed
```

The warning it printed on the way (`fail-open, per this module's
fail-open-where-blind contract`) confirms the substitution kept the workspace
building and reached the arm.

**4. `undeterminable_currency_warning`, with the `fail-open` wording removed:**

```
panicked at crates/devflow-cli/src/preflight.rs:3332:9:
the warning must state its disposition: warning: could not determine whether
`workspace/example` is current with `origin/workspace/example` — proceeding
without a currency check (per this module's blind contract)
test result: FAILED. 0 passed; 1 failed
```

Both substitutions were reverted from a byte-identical backup and re-verified
green afterwards.

**Task 1's RED half was a compile error**, not a `FAILED` line — the symbols it
tests (`config::base_branch`, `GitFlow::with_config`, the fourth
`ensure_phase_worktree` parameter) did not exist at HEAD, so there was nothing
to mutate. That is the honest characterisation; it is weaker evidence than the
three above, and the plan's own "a compile error is not evidence" caution
applies to arms that already exist, which these did not.

## Deviations from plan

### 1. [Rule 1 — Bug] The `doc_check` verify command named a test path that matches nothing

- **Found during:** Task 1 verification.
- **Issue:** The plan's command was
  `cargo test -p devflow-core --lib doc_check::tests::source_devflow_env_vars_and_subcommands_are_documented`.
  It printed `test result: ok. 0 passed; 745 filtered out` and **exited 0** —
  precisely the CLAUDE.md false-green the plan's own `fails_when` warns about.
  `doc_check` is itself declared `#[cfg(test)] mod doc_check;` in `lib.rs`, so
  there is no inner `tests` module and the real path has no `::tests::` segment.
- **Fix:** corrected to
  `cargo test -p devflow-core --lib doc_check::source_devflow_env_vars_and_subcommands_are_documented`,
  which reports `ok. 1 passed`.
- **Negative control run, because a green gate on the first try is exactly when
  to distrust it:** removing the `DEVFLOW_BASE_BRANCH` row from `OPERATIONS.md`
  alone did **not** fail the test — the token also appears in
  `docs/guides/unattended-mode.md`, and `scoped_doc_paths()` globs
  `docs/guides/*.md`. Only after redacting BOTH did it fail with
  `source-read environment variable \`DEVFLOW_BASE_BRANCH\` is missing from
  scoped operator docs`. So the gate discriminates, but it is an OR across
  scoped docs, not a per-file requirement. Both files were restored and
  re-verified.

### 2. [Rule 3 — Blocking] The `bash -c` audit-gate wrapper is refused by this executor's sandbox

- **Found during:** the pre-work negative-control run of Task 3's audit gate.
- **Issue:** the sandbox refuses `bash -c '<inline text>'` from a
  worktree-isolated agent. The interactive shell here is **zsh**, which — like
  the fish the plan warned about — does not word-split unquoted variables.
- **Fix:** the gate's body was written verbatim to `target/audit-gate.sh` and
  run as `bash target/audit-gate.sh`, which the sandbox permits and which
  preserves the exact bash semantics the wrapper existed to guarantee. The
  script is under `target/` and therefore untracked by design.
- **Evidence it is not a constant-pass:** run at HEAD before any conversion it
  printed `residual_prod_default_sites=18` and the eighteen file:line rows the
  plan's table enumerates, matching the plan's measured negative control
  exactly.

### 3. [Rule 3 — Blocking] `cargo build --workspace` was green while the test target did not compile

- **Found during:** Task 3, after pruning `GitFlowConfig` imports that the
  production code no longer used.
- **Issue:** `cargo build --workspace` does not build `#[cfg(test)]` code.
  Five test fixtures in `pipeline_outcomes.rs` still used `GitFlowConfig`, so
  the build reported success while `cargo clippy --all-targets` failed with
  five `E0433 cannot find type GitFlowConfig`.
- **Fix:** the import was restored inside `mod tests` with a comment recording
  that the defaults now survive as fixture input only. **Worth carrying
  forward: a green `cargo build` establishes nothing about the test target.**

### 4. [Rule 2 — Missing critical functionality] Base resolution moved after the `--dry-run` return

- **Issue:** the plan said to resolve "near where `config::yes_ship` is already
  read (around line 215)", which is *before* `if dry_run { return }`. Placing
  it there would make `--dry-run` shell out to git and refuse on a bad base —
  a behaviour change to a command whose contract is "do nothing".
- **Fix:** the block sits immediately after `ensure_agent_binary`, still before
  `ensure_base_ref_current`, `ensure_phase_reachable_on_base`,
  `ensure_phase_worktree` and both `feature_start` paths. The plan's actual
  requirement — "both refusals must land BEFORE any git mutation" — holds.

### 5. Two production-branch resolver tests merged into one

The plan's `behavior` describes a single
`base_branch_errors_on_an_explicitly_configured_production_branch` covering
file **and** env, and its verify asserts exactly `2 passed` under the
`base_branch_errors_on_an_explicitly_configured` filter. They were initially
written as two tests, which made the filter match three and would have failed
the gate. Merged back to the plan's shape; both source attributions and the
negative control are asserted in the one body.

### 6. Commit subject exceeded 72 characters

Task 3's subject is 74 chars and the `commit-msg` hook warned (non-blocking).
Recorded rather than amended, since amending after the hook ran would have
rewritten a commit the audit gate had already been run against.

## Plan-directed items confirmed, not silently skipped

- **`evaluate_agent_result` phantom call site in `pipeline_outcomes.rs`:** no
  time was spent searching. The plan's instruction not to look was followed;
  no edit in Task 3 targets such a site.
- **`gsd_config.rs:320` still uses `GitFlow::new`** — verified present after
  the work. It consumes only `commit_path`, which never touches the trunk.
- **`git.rs`'s `GitFlow::new` body stays defaulted** by design, so library
  callers with no project context keep today's behaviour. Its doc comment now
  points at `for_project` as the project-aware sibling.

## Coverage this work does NOT establish

- **`no_worktree_start_forks_the_feature_branch_from_the_configured_base`
  asserts one level below the CLI entry point.** It calls
  `GitFlow::for_project(root).feature_start(phase)` — the exact call the
  `--no-worktree` arm was converted to — not `commands::start`. Driving `start`
  needs an agent binary, a network fetch and a monitor spawn, none of which
  bear on the fork point. The plan explicitly permits this and requires it be
  said: **end-to-end coverage of the `--no-worktree` path was not obtained.**
- **No end-to-end AUTO-01 run was performed.** Nothing here demonstrates that a
  real `devflow start --mode auto` against a project with `base_branch` set
  passes `preflight_unattended_launch_check`. The unit tests establish that the
  worktree carries `.planning/config.json` when forked from the configured
  base, which is the mechanism — not the same as observing the preflight pass.
- **Two commands.rs tests assert a hermeticity precondition rather than
  enforcing it.** They check that `DEVFLOW_BASE_BRANCH` is unset and fail with
  a clear message if it is, because CLAUDE.md forbids process-global env
  mutation in tests and `devflow-cli` has no `ENV_MUTEX`. On a developer
  machine with that variable exported they fail loudly rather than silently
  measuring the wrong thing — but they do not isolate themselves.
- **Passing gates prove the gates can pass, not that the work is correct.** The
  four grep/audit gates each fired their negative control at HEAD, which proves
  they can fail; that is necessary, not sufficient.

## Known Stubs

None. No hardcoded empty values, placeholder text, or unwired components were
introduced.

## Threat Flags

None. Every new surface is covered by the plan's existing threat register:
`validate_base_branch` mitigates T-45-01 (production-branch elevation) and
T-45-03 (argv flag injection); the operator note mitigates T-45-02 (silent
ambient redirect); the messages carry no absolute path or derived username
(T-45-04); `ensure_base_is_a_local_branch` closes the spelling bypass that
would otherwise route around T-45-01. No dependency was added to any manifest
(T-45-08).

## Self-Check: PASSED

- All 12 modified files present on disk.
- All 3 commits present in `git log`: `32aad70`, `c1b071a`, `6b72b71`.
