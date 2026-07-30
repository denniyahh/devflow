---
phase: 27
slug: scrub-redirecting-git-environment-from-production-calls
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-07-30
---

# Phase 27 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by `/gsd-plan-phase 27` from `27-RESEARCH.md` § Validation Architecture.
> Filled in by `27-06` (wave 3, the phase's own acceptance plan) against this
> worktree's HEAD (base `f539012f8656d37e41627c6015cf9bc4db509051`, all of
> `27-01`…`27-05` merged).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` (`#[test]`), no separate runner |
| **Config file** | none — the `test-support` feature gate in `crates/devflow-core/Cargo.toml` is the closest analogue |
| **Quick run command** | `cargo test -p devflow-core --features test-support --lib <module>::` (core) / `cargo test -p devflow --bin devflow -- <module>::` (cli) |
| **Full suite command** | `cargo test --workspace` (normal, non-hostile environment) |
| **Estimated runtime** | ~90–240 seconds for the full workspace; per-module quick runs are seconds |

**Acceptance-specific commands (hostile `GIT_DIR` — the D-03 signal).** Do **not** use
`cargo test --workspace` for these; RESEARCH.md documented it as non-terminating under a
hostile `GIT_DIR` pre-migration. Use the two scoped invocations:

```
GIT_DIR=<throwaway-repo>/.git cargo test -p devflow-core --features test-support
GIT_DIR=<throwaway-repo>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes
```

### Baseline of record — the stale `37` figure is superseded

**ROADMAP.md § "Phase 27" and `27-CONTEXT.md` D-03 both cite `37` failing tests. That
figure is stale** (measured under an earlier phase's HEAD) and is superseded here by
a directly re-measured figure.

Re-measuring against the phase's own pre-migration base commit (`6350798`, the
`git merge-base HEAD develop` of this worktree) from inside this dedicated,
isolated worktree was judged **impractical**: checking out a different commit
in a worktree whose lifecycle this agent does not own risks violating the
worktree-isolation invariant this execution runs under (no `git checkout` to a
foreign ref, no detached-HEAD excursions). Per this plan's own § "Recorded
corrections" instruction, the baseline of record is instead **cited from
`27-RESEARCH.md`**, dated and attributed:

> `GIT_DIR=<throwaway>/.git cargo test -p devflow-core --features test-support`
> → **54 failed / 352 passed** (clean run, 4.08s)
> `GIT_DIR=<throwaway>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes`
> → **44 failed / 139 passed** (clean run, 9.49s)
> **Combined: 98 failures**, measured live at HEAD `6350798` on 2026-07-30
> (`27-RESEARCH.md` § Summary and § The 37-Failing-Test Acceptance Signal).

Target after migration: **0 failed** on both. **Achieved — see § Post-Migration
Acceptance Run below.**

---

## Sampling Rate

- **After every task commit:** the quick-run command scoped to the module just migrated
- **After every plan wave:** `cargo test --workspace` in a normal environment — confirms no regression in the ordinary case
- **Phase gate (before `/gsd-verify-work`):** both hostile-`GIT_DIR` commands above, run to a clean `test result:` line — a timeout is **not** a pass
- **Max feedback latency:** ~240 seconds (full workspace); seconds for per-module runs. Confirmed within budget: the slowest single target in the post-migration `cargo test --workspace` run (below) finished in 52.79s.

---

## Post-Migration Acceptance Run (27-06, this plan)

All commands below were run live in this worktree, against HEAD (base
`f539012f8656d37e41627c6015cf9bc4db509051`, `27-01`…`27-05` merged), with a
fresh `mktemp -d && git init -q` throwaway repository supplying `GIT_DIR`.

### Hostile-`GIT_DIR` acceptance — devflow-core

```
$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support
test result: ok. 411 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.94s
```

First attempt showed 1 collateral failure
(`agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape`,
"exec visibility timed out") — re-run in isolation passed in 0.01s, and the
full suite re-run cleanly at 411/0. Classified as a resource-contention flake
in this sandboxed environment (the same class `27-05-SUMMARY.md` documented
for `cargo test --workspace`'s default parallelism), not a regression tied to
the hostile `GIT_DIR` — the failing test has no git dependency at all (it is a
process-liveness/exec-visibility timing check).

### Hostile-`GIT_DIR` acceptance — devflow-cli

```
$ GIT_DIR=<hostile>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes
test result: ok. 188 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out; finished in 9.61s
```

**Both acceptance commands reach a literal `test result:` line with 0 failed. Neither timed out.**

### Re-verified deferred checks from 27-01 and 27-05

Both plans documented `<verify>` checks that could not pass at their own
plan's scope boundary, precisely because they required later plans' files to
also be migrated. Re-run now that all of `27-02`…`27-05` are merged:

```
$ GIT_DIR=<hostile>/.git cargo test -p devflow-core --features test-support --lib git::tests::
test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 373 filtered out; finished in 1.33s
```
(27-01-SUMMARY.md Deviation 2 — deferred at the time because 7 of `git.rs`'s 9
sites were still owned by `27-02`. Now green.)

```
$ GIT_DIR=<hostile>/.git cargo test -p devflow --bin devflow -- preflight::
test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 194 filtered out; finished in 4.74s
```
(27-05-SUMMARY.md Deviations 2 and 3 — deferred at the time because
`preflight_major_bump_check`/`preflight_interactivity_check` reach into
`version.rs` (27-03) and `commands.rs` (27-04), unmigrated in 27-05's own
isolated worktree. Now green, exactly as 27-05's own "Next Phase Readiness"
section predicted.)

### RESEARCH Open Question #2 residual — `pipeline_gate` / `pipeline_outcomes`

```
$ GIT_DIR=<hostile>/.git timeout 300 cargo test -p devflow --bin devflow -- pipeline_gate pipeline_outcomes
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 188 filtered out; finished in 4.60s
```

**Outcome: completed clean.** Wall-clock elapsed ~16s (well under the 300s
bound; two `date +%s` timestamps 16 seconds apart bracketing the run). This
was previously the single reproducible non-termination in the whole phase —
RESEARCH.md recorded it not finishing within 180–480s bounds even at reduced
parallelism, pre-migration. Post-migration it finishes in under 5 seconds of
test time. This is recorded as a **finding**, per the plan's own instruction
— the phase does not pass or fail on it — but it is the "plausible bonus
effect of the migration" `27-RESEARCH.md` Open Question #2 predicted: these
modules call through `GitFlow` methods in `git.rs`, one of the migrated
files, and the pathological slowdown (plausibly retry/lock contention against
a foreign, unrelated repository under the old unscrubbed code) no longer
occurs once the constructor resolves the caller's own small local repo
instead.

### Ordinary-environment gates

```
$ cargo test --workspace
21 test binaries, all `test result: ok.`, 0 failed anywhere (slowest single
target: devflow-core lib, 411 passed, 6.56s; slowest overall target 52.79s)

$ cargo clippy --workspace --all-targets -- -D warnings
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.86s — 0 warnings

$ cargo fmt --check
   (no output — clean)
```

### D-02 boundary check

```
$ git diff --stat $(git merge-base HEAD develop)..HEAD -- crates/devflow-cli/build.rs
(no output — build.rs byte-identical across the entire phase diff)
```
`git merge-base HEAD develop` resolved to `6350798e0f368a209d58485da259c2bd3402d611` —
the same commit `27-RESEARCH.md` cites as its measurement HEAD, confirming this
is genuinely the phase base commit, not a drifted reference.

### Spec-less probe fallback

spec-less probe fallback skipped: phase has no requirement IDs to probe (visible skip).

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 27-01-T1 | 27-01 | 1 | D-01 | T-27-01 | `hermetic_command`'s output has all 17 redirecting vars marked for removal; no bypass parameter exists | unit | `cargo test -p devflow-core --features test-support --lib git::tests::git_command_marks_every_redirecting_var_for_removal` | ✅ | ✅ green |
| 27-01-T1 | 27-01 | 1 | D-03 | T-27-02 | A `git_command`-constructed process resolves the caller-supplied root even with a hostile `GIT_DIR` set | unit/integration | `cargo test -p devflow-core --features test-support --lib git::tests::hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir` | ✅ | ✅ green |
| 27-02-T1/T2 | 27-02 | 2 | D-01, D-03 | T-27-03 (GitFlow/worktree chokepoints) | `git.rs`'s 9 sites (4 method wrappers + 3 free-standing + 2 from 27-01) and `worktree.rs`'s 2 sites (the `run` chokepoint + independently-migrated `list`) route through `git_command` | unit | `GIT_DIR=<hostile> cargo test -p devflow-core --features test-support --lib git::` → 38 passed, 0 failed; `worktree::` → 9 passed, 0 failed | ✅ | ✅ green |
| 27-03-T1/T2 | 27-03 | 2 | D-01, D-03 | T-27-04, T-27-08 | `version.rs`'s 10 sites and `agent_result.rs`'s 3 sites route through `git_command` | unit | `GIT_DIR=<hostile> cargo test -p devflow-core --features test-support --lib version::` → 47 passed, 0 failed; `agent_result::` → 72 passed, 0 failed | ✅ | ✅ green |
| 27-04-T1/T2 | 27-04 | 2 | D-01, D-02, D-03 | T-27-08 (staleness) + RESEARCH Open Question #1 (indirect `sh → cargo → build.rs` chain) | `staleness.rs`'s remaining 2 sites, `commands.rs`'s 3 sites, and `test_cmd`'s `sh -c "cargo …"` spawn route through `git_command`/`hermetic_command` | unit/integration | `GIT_DIR=<hostile> cargo test -p devflow --bin devflow -- staleness::` → 43 passed, 0 failed; `commands:: --skip pipeline_gate --skip pipeline_outcomes` → 102 passed, 0 failed | ✅ | ✅ green |
| 27-05-T1/T2 | 27-05 | 2 | D-01, D-03 | T-27-01 (`fast_forward_base_ref`'s `update-ref`, the module's only write) | `preflight.rs`'s 11 sites, including 2 closure-embedded sites and the phase's only write, route through `git_command` | unit | `GIT_DIR=<hostile> cargo test -p devflow --bin devflow -- preflight::` → 41 passed, 0 failed (confirmed at 27-06/wave 3, once `version.rs`/`commands.rs` were also merged) | ✅ | ✅ green |
| 27-06-T1 | 27-06 | 3 | D-01 | T-27-15 | Workspace-wide comment-filtered sweep confirms zero unscrubbed direct git constructions; full spawn-edge census enumerates every production `Command::new(...)`, closes or escalates RESEARCH Assumption A2 | other | `rg --no-heading -n 'Command::new\("git"\)' crates/devflow-core/src crates/devflow-cli/src \| rg -v ':\s*(//\|///\|//!)' \| wc -l` → 0; `27-SPAWN-CENSUS.md` produced | ✅ | ✅ green (A2 explicitly stated **OPEN**, escalated with a proposed backlog entry — see `27-SPAWN-CENSUS.md`) |
| 27-06-T2 | 27-06 | 3 | D-03 | T-27-02, T-27-16 | Previously-failing devflow-core tests pass under a hostile `GIT_DIR` (full crate) | regression | `GIT_DIR=<throwaway>/.git cargo test -p devflow-core --features test-support` → 0 failed | ✅ | ✅ green (411 passed) |
| 27-06-T2 | 27-06 | 3 | D-03 | T-27-02, T-27-16 | Previously-failing devflow-cli tests pass under a hostile `GIT_DIR` (scoped) | regression | `GIT_DIR=<throwaway>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes` → 0 failed | ✅ | ✅ green (188 passed) |
| 27-06-T2 | 27-06 | 3 | D-02 | T-27-05 | `build.rs` is untouched by this phase | plan-time check | `git diff --stat $(git merge-base HEAD develop)..HEAD -- crates/devflow-cli/build.rs` is empty | n/a | ✅ green |
| 27-06-T2 | 27-06 | 3 | — (T-27-17, finding) | T-27-17 | `pipeline_gate`/`pipeline_outcomes` under a hostile `GIT_DIR`, bounded 300s | finding, not a gate | `GIT_DIR=<hostile> timeout 300 cargo test -p devflow --bin devflow -- pipeline_gate pipeline_outcomes` | ✅ | ✅ green — completed clean, 47 passed / 0 failed in ~16s wall (not blocking; recorded as a finding) |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] New unit test in `crates/devflow-core/src/git.rs`'s existing `#[cfg(test)] mod tests` — asserts every redirecting variable is marked for removal on the new constructor's output, mirroring `test_support.rs:196-214`. **Present:** `git_command_marks_every_redirecting_var_for_removal` (`git.rs:1750`).
- [x] New unit test proving a spawned process resolves the caller's root, not a hostile `GIT_DIR`'s target. **Present:** `hermetic_command_resolves_caller_root_even_under_a_hostile_git_dir` (`git.rs:1669`).
- [x] A variable-list drift test mirroring `test_support::local_env_vars_match_git`, so the production list stays honest against the installed `git rev-parse --local-env-vars`. **Present:** `local_env_vars_match_git` (`git.rs:1784`).
- [x] No framework install needed — `cargo test` is fully functional in this environment. Confirmed throughout this phase's execution.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions | Outcome |
|----------|-------------|------------|--------------------|---------|
| Root cause of `pipeline_gate` / `pipeline_outcomes` not terminating under a hostile `GIT_DIR` | RESEARCH Open Question #2 | Not diagnosed during research; the `--skip` workaround made the acceptance signal measurable but did not explain the hang | Run each module alone under a hostile `GIT_DIR`, then together; if the migration does not resolve it, file a backlog entry rather than widening this phase | **Resolved as a bonus effect of the migration, not manually diagnosed.** Re-measured post-migration (27-06): completes clean, 47 passed / 0 failed in ~16s wall, well under the 300s bound. No backlog entry needed for this specific residual — see § Post-Migration Acceptance Run above. |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 240s (slowest single target 52.79s, well within budget)
- [x] Both hostile-`GIT_DIR` commands reach a `test result:` line with 0 failed (not a timeout) — devflow-core 411/0, devflow-cli 188/0
- [x] `nyquist_compliant: true` set in frontmatter

**Note on scope:** this sign-off covers the D-01/D-02/D-03 mechanism-proof
acceptance this phase's `<verification>`/`<success_criteria>` define. It does
**not** claim RESEARCH Assumption A2 (the exhaustiveness-beyond-the-41-sites
question) is closed — `27-SPAWN-CENSUS.md` states that explicitly as **OPEN**,
with 5 unmitigated spawn edges named, evidenced, and escalated as a proposed
backlog entry rather than fixed in this plan (which modifies no source under
`crates/`, per its own declared scope).

**Approval:** pending — `/gsd-validate-phase` owns advancing `status` from
`draft` to `validated`, not this plan.
