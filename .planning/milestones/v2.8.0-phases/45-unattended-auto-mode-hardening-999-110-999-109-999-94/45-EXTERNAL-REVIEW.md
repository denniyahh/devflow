---
phase: 45-unattended-auto-mode-hardening-999-110-999-109-999-94
reviewed: 2026-09-02
round: 3 (post-execution adversarial code review)
lanes:
  - name: codex
    model: gpt-5.6-terra
    effort: high
    status: completed
    findings: 6
    citations: 6
  - name: agy
    model: gemini-3.1-pro-high
    effort: high
    status: completed
    findings: 6
    citations: 6
  - name: opencode/deepseek
    status: not_run
    reason: >-
      Not attempted. Operator asked for codex and agy specifically. The lane is
      recorded here so its absence is never read as a clean pass.
verified_by: claude (every finding re-checked against source before action)
fixed: 8
deferred: 2
---

# Phase 45: External Adversarial Code Review (round 3)

Two external lanes, both completed, both source-grounded. Every finding below was
re-read against the shipped source before it was acted on; nothing was taken on the
reviewer's word.

**Complementarity held again.** The two lanes overlapped on exactly one finding
(`.cargo/config.toml`). Codex found the whole persisted-base blast radius; agy found
the config-parse fail-open and the shell defect. Either lane alone would have shipped
the other's blockers.

## Disposition

| # | Lane | Finding | Severity | Disposition |
|---|---|---|---|---|
| CR-45-01 | codex | `preflight_interactivity_check` re-resolves ambient config while holding the run's `State` | BLOCKER | **Fixed** |
| CR-45-06 | codex | `enumerate_phase_commits` uses ambient config for idle-timeout evidence | BLOCKER | **Fixed** |
| CR-45-07 | claude | `ship_evidence::collect` — same class, at a site neither lane enumerated | BLOCKER | **Fixed** |
| — | agy | `load_config`'s fail-soft TOML parse defeats `base_branch`'s documented fail-hard contract | BLOCKER | **Fixed** |
| CR-45-02 | codex | `git_flow_for_run` trusts a persisted base with no validation | WARNING | **Fixed** |
| CR-45-03 | codex | `start`'s `--no-worktree` arm re-resolves after persisting | WARNING | **Fixed** |
| CR-45-05 | codex + agy | `.cargo/config.toml` is not treated as build-affecting | WARNING | **Fixed** (scoped branch only) |
| — | agy | `cut-pr-branch.sh` word-splits `$FORBIDDEN_UNMERGED` | WARNING | **Fixed** |
| CR-45-04 | codex | scoped staleness accepts any `crates/**`, not the declared members | WARNING | **Won't fix** — filed as 999.117 |
| — | agy | `write_state_atomic` uses a fixed `.tmp` name (TOCTOU) | BLOCKER | **Deferred** — filed as 999.118 |
| — | agy | CR-04 prompt contradiction | WARNING | Already deferred as 999.116; agy independently corroborates |

## The class codex found, and the instance it missed

CR-02 persisted `State::base_branch` and threaded it into exactly two call sites
(`pipeline_launch`, `pipeline_outcomes`). Codex's contribution is showing that was the
**instance, not the class**: three more sites held the run's `State` — or could load it —
and re-resolved from ambient configuration anyway. `DEVFLOW_BASE_BRANCH` lives in the
environment of whichever shell ran `devflow start`, so every one of them was wrong
under the documented `devflow resume` recovery path.

Codex named two (`preflight`, `monitor`). Enumerating the rest during verification
turned up a third it missed — `ship_evidence::collect`, which already loads the `State`
for `stage`, threw the base away, and then asked `is_merged_into_develop` against the
wrong trunk. That is a false negative in the record that gates Ship.

## Won't fix: CR-45-04

The predicate accepts any `crates/**` path rather than the two declared workspace
members. Codex is factually right. It is not being changed because the error direction
is **safe**: an extra `crates/` path is a false *positive*, which hard-blocks a run that
could have proceeded. Narrowing to a parsed member list makes the failure direction
unsafe — a newly added crate would be silently dropped from the staleness check until
someone remembered to re-run the parse — and `is_self_dogfood_workspace` already
requires the member list to be exactly those two paths for the scoped rule to engage at
all. Trading a safe false positive for an unsafe false negative is the wrong trade
(T-45-09, the Phase 16 false-evidence incident).

**Filed as backlog 999.117** so the reasoning survives, including the condition that
would reverse it: this workspace gaining a third member, or a non-member directory
under `crates/`.

## Deferred: `write_state_atomic`

`crates/devflow-core/src/workflow.rs:189` writes through a fixed `path.with_extension("tmp")`,
so two processes saving state for the same phase collide. The mechanism is real. It is
**not phase-45 code** — `workflow.rs` is untouched by this branch — and expanding an
adversarial review into unrelated pre-existing code is a scope decision for the
operator, not for the reviewer. **Filed as backlog 999.118** (ROADMAP.md), with the
two losing interleavings, the unreachable-repro caveat, and a fix shape.

## Verification

Every fix carries a regression test, and every test was checked in the failing
direction by reverting its fix and confirming a real `test result: FAILED` — not merely
a not-pass:

| Test | Reverted fix produced |
|---|---|
| `base_branch_refuses_an_unparseable_config_rather_than_defaulting` | FAILED |
| `git_flow_for_run_refuses_a_persisted_base_that_start_would_have_rejected` | FAILED |
| `preflight_interactivity_check_probes_the_runs_persisted_base` | FAILED |
| `enumerate_phase_commits_prefers_the_runs_persisted_base` | FAILED |
| `collect_resolves_merge_evidence_against_the_runs_persisted_base` | FAILED |
| `affects_compiled_binary_in_workspace_scope_accepts_root_cargo_config` | FAILED |

The shell fix was proven directly: the old `for f in $FORBIDDEN_UNMERGED` splits
`.planning/my plan.md` into `.planning/my` and `plan.md`; the `while IFS= read -r`
replacement preserves it.

`cargo test --workspace`: **1235 passed, 0 failed**, three consecutive runs, exit 0 each
time. `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check`
clean.

**What that does not establish.** Three green runs is a weak reliability bound, not a
guarantee — and it matters here because an earlier run in this same session failed 7
tests at once (`spawn_with_timeout_kills_a_hung_child`, the `gates::notify_hook_*` pair,
and four `cleanup_merged_*` tests), all through the same process-spawn `NotFound` victim
class already filed as **999.114** (the CLI test binary blanks process-global `PATH`, so
whichever concurrent test spawns a program dies). That flake is pre-existing and
reproduced on the pre-fix commit `eb3c6af`; it is not attributable to these changes. But
it does mean a green suite on this repo is not yet a stable signal, and 999.114 should
be fixed before anyone reads run-to-run greenness as evidence of anything.

None of these fixes were exercised against a live unattended run. They are verified at
unit level only.
