---
phase: 35
slug: loop-termination-and-baseline-correctness-999-77-999-78-999
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-06
---

# Phase 35 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by `/gsd-plan-phase` from `35-RESEARCH.md` § "Validation Architecture".
> The Per-Task Verification Map is deliberately unfilled at plan time — it is populated once
> PLAN.md task IDs exist.

**This phase's subject is the discipline this document enforces.** Every criterion here exists
because a test passed (or would pass) against both the buggy and the fixed code. A proxy
measurement is not a weak result in Phase 35 — it is the defect under repair. Treat every green
below as suspect until its named negative control has been run and seen to fail.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (standard Rust harness), workspace crates `devflow-core` and `devflow` |
| **Config file** | none dedicated — `scripts/check.sh` is the single definition of "green": `fmt`, `clippy --all-targets -- -D warnings`, `test` |
| **Quick run command** | `cargo test -p devflow-core --lib <module>::` or `cargo test -p devflow --lib <module>::` |
| **Full suite command** | `scripts/check.sh all` (host) / `scripts/check-in-container.sh all` (pinned CI image) |
| **Estimated runtime** | **unmeasured** — measure at Wave 0 and record here rather than assume |

**Package-name trap (CLAUDE.md).** devflow-core's package is `devflow-core`; **devflow-cli's
package is `devflow`**, not `devflow-cli` (`crates/devflow-cli/Cargo.toml:2`). `cargo test --exact
<name>` **exits 0 when the name matches nothing** — assert on a real `N passed` line with a
non-zero `filtered out` count. Never trust the exit code alone, and never trust a pipeline's exit
code (it is the last command's).

**`PATH`-mutating tests are serialized, not parallel.** Criteria 1 and 6 both install a `PATH`
guard. `test_support::env_lock()` (`crates/devflow-cli/src/test_support.rs:94`) is the mutex;
`NeutralPath` (`:327`) is the existing RAII precedent. A test that mutates `PATH` without holding
the guard corrupts unrelated tests non-deterministically — the failure will not point at the
offender.

---

## Sampling Rate

- **After every task commit:** targeted `cargo test -p <package> --lib <module>::` for the module
  touched, asserting a real `N passed` count
- **After every plan wave:** `scripts/check.sh all` (fmt + clippy + full suite)
- **Before `/gsd-verify-work`:** full suite green **and** criterion 4's performed revert
  demonstration recorded as evidence in the phase SUMMARY — `cargo test` does not reach it, so it
  needs explicit manual sign-off
- **Max feedback latency:** unmeasured — see Test Infrastructure

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| *(pending — filled once PLAN.md task IDs exist)* | | | | | | | | | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Requirement → verification, ahead of task IDs

| Req / Criterion | Behavior | Test Type | Command or artifact | Exists? |
|---|---|---|---|---|
| HARDEN-01 · c1 | A `None` measurement does not overwrite the persisted `consecutive_failures` baseline; the streak accumulates across failure→success-with-unchanged-count | unit (multi-cycle sequence) | new test driving `handle_validate_outcome` twice with `NoGitPath` installed for cycle 1 | ❌ W0 |
| HARDEN-01 · c1 doc | `pipeline_outcomes.rs`'s doc comment no longer promises a guarantee the code lacks | diff review | assert the phase diff edits the identified doc comment | N/A |
| HARDEN-02 · c2 counter | A loop committing trivial `.planning/` artifacts every cycle still reaches a bound | unit | new test over the never-reset per-phase total + its ceiling | ❌ W0 |
| HARDEN-02 · c2 message | Supervise-mode gate message reports the cumulative total, not the streak — and the two read as **different numbers** at the 2nd vs 5th gate | unit | assertion on the gate message string across ≥2 gates | ❌ W0 |
| HARDEN-02 · c2 `--force` | Counter's behaviour across a `--force` restart is **stated and tested**, or explicitly documented as accepted-not-tested | unit or recorded decision | depends on the option the plan picks — see Open Risk below | ❌ W0 |
| HARDEN-03 · c3 | A stale `{N}-VERIFICATION.md` dispatches `FullExecute`; a fresh one dispatches `GapsOnly` | unit (two-direction) | new test asserting **both** directions | ❌ W0 |
| HARDEN-04 · c4 | Worktree-mode `GateReview` auto-decide reads `execution_root`, not `project_root` | integration | extend `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records` (`pipeline_launch.rs:2302`) with a worktree PLAN + D-05's decoy PLAN under `project_root` | ✅ base exists, extend |
| HARDEN-04 · c4 mechanical | `assert!(!phase_has_blocking_human_checkpoint(project_root, phase))` — the re-running control | integration | same test, D-06's shape | ❌ W0 |
| HARDEN-04 · c4 revert | The revert is **performed** and the new test **watched to fail** | manual demonstration | one-time; evidence recorded in SUMMARY | ❌ W0 (artifact, not a test) |
| HARDEN-05 · c5 | `release --check` reports `Viable`/`NotViable` from a real `ssh-keygen -Y sign` probe | unit | new probe tests in `git.rs` / `release_check.rs` — positive, negative, block-then-recover | ❌ W0 |
| HARDEN-05 · c5 deletion | `classify_ssh_add_status` / `SigningStatus` / `inline_key_fingerprint` removed with their tests | compile + diff review | workspace builds; `rg` finds no surviving reference | ❌ W0 |
| HARDEN-07 · c6 | `evaluate_layer2` returns `Ok(None)` for `exit_code = 0` + `Stage::Code` + unrunnable `git` | unit | new test in `agent_result.rs` with `NoGitPath` installed | ❌ W0 |

---

## Mandatory Negative Controls

This repo's stated discipline: **every measurement includes a case that must produce the opposite
result. If both cases agree, the measurement is broken — not the subject.** These are not optional
and each is individually named, because an unnamed control is one nobody notices went missing.

| # | Measurement | Required opposite-result case | Source |
|---|---|---|---|
| NC-1 | The harness itself blocks `git` | A probe-level assertion that `Command::new("git")` returns `Err` **with** `NoGitPath` installed and `Ok` **without** it. Without this, a green regression test can mean "the shim never took effect". Write it **first**. | RESEARCH §A |
| NC-2 | `None` preserves the `consecutive_failures` baseline | Revert to the unconditional `Some(current)` write with the old lossy `None`→`0` mapping — the same multi-cycle test must then show the streak **reset to 1** | RESEARCH §A/§B |
| NC-3 | The 999.77 sequence discriminates | A **single-cycle** test passes against both the buggy and fixed code. It is a proxy and must not be accepted as coverage for c1 | ROADMAP 999.77 + CONTEXT.md |
| NC-4 | Unrunnable ≠ measured-zero | A `git` shim that **runs and exits non-zero** yields `Some(0)` (real observation), **not** `None`. A test built on a failing shim rather than an absent `git` exercises the wrong path entirely | A-06 + RESEARCH §A |
| NC-5 | The 999.78 ceiling actually bounds | Remove the ceiling check (or the increment) — the trivial-commit loop must no longer gate | RESEARCH §C |
| NC-6 | The gate message reports the total | Revert the format string to interpolate only `consecutive_failures` — the "total ≠ streak at the 2nd vs 5th gate" assertion must fail | RESEARCH §C |
| NC-7 | Staleness detection is two-directional | A rule marking everything stale forever passes the stale case but **must fail** the fresh case. A one-direction test cannot catch this silent permanent regression | A-12 + RESEARCH §D |
| NC-8 | `:1070` passes `execution_root` | **Perform** the revert to `project_root`, run the test, confirm it fails, revert the revert. The mechanical D-06 assertion is a re-running control and does **not** substitute for the performed revert; neither substitutes for the other | D-05/D-06 + criterion 4 |
| NC-9 | The signing probe is not vacuously true | Remove the private key file (leave only the `.pub`) — `Viable` must flip to `NotViable` | RESEARCH §F |
| NC-10 | `SSH_ASKPASS_REQUIRE=never` is what prevents the hang | Omit the env var against the **same** encrypted-key fixture — the run must visibly exceed the test's timeout budget, proving the env var and not the timeout alone (nor the fixture) is load-bearing | D-01 + RESEARCH §F |
| NC-11 | The 999.87 case is the unrunnable one | The existing `evaluate_layer2_exit_zero_no_commits_is_failed` (`agent_result.rs:6668`) covers ordinary `commits == 0` and correctly asserts `Failed`. Extending **it** would be a proxy — c6's discriminating case is `exit_code = 0` + `Stage::Code` + unrunnable `git` | ROADMAP 999.87 |

**Why NC-1 is load-bearing and not ceremony.** Criteria 1 and 6 both assert on behaviour that only
occurs when `git` cannot be executed. If the guard silently fails to take effect — wrong `PATH`
ordering, a guard dropped early, an absolute-path `git` invocation — every assertion still runs and
every one of them passes for the wrong reason. NC-1 is the only thing standing between that and a
green suite over an unfixed defect.

---

## What the evidence does NOT establish

Carried as a standing obligation on the phase summary:

- **Criterion 1's test says nothing about the `Some(0)` path.** A `git` that runs and reports a
  genuinely absent branch is a different, already-correct path. Green here is not evidence about it.
- **Criterion 4 proves root-selection correctness for a plain directory standing in for a
  worktree.** D-05 deliberately uses `create_dir_all`, not a real `git worktree add` — nothing here
  establishes git-worktree-specific semantics.
- **Criterion 5's fixtures are n=1 on one host, one OpenSSH build, one key type (ed25519).**
  CONTEXT.md states this outright: the operator's measurements "fix the shape of the design; they
  are not a claim about behaviour across OpenSSH versions or key types, and the phase's own tests
  should not cite them as coverage."
- **Nothing here establishes how often Layer 2 is the deciding layer in production.** The code path
  was read and verified; frequency was never measured. The 999.87 and 999.77 backlog entries both
  record this as "Not established."
- **A green suite does not establish that the 999.78 bound survives a `--force` restart** unless the
  option chosen for the Open Risk below is itself tested. `State::new` zeroes every counter
  unconditionally.

---

## Open Risk Carried Into Planning

**The 999.78 counter's lifetime is per-RUN, but the bound is specified as per-PHASE.** `State::new`
(`state.rs:263-272`) zeroes every counter and `start()` calls it unconditionally on every run,
`--force` included (CONTEXT.md A-11; independently re-confirmed by research at `commands.rs:124`).
A naive `State`-field implementation therefore resets on `--force`, and a bound that resets on
restart does not bound the unattended case D-07 exists for.

CONTEXT.md A-11's instruction is explicit: **the planner must state the counter's persistence
explicitly**, and the reset event must be a real event (phase completion / operator approval at the
ceiling gate), not "whenever a new process starts". If it cannot outlive `State`, that is a finding
to **escalate**, not to paper over.

Whichever option the plan selects, this document requires that the choice be named and either
tested or explicitly recorded as accepted-not-tested. Silence here is a validation gap.

---

## Wave 0 Requirements

- [ ] **One `NoGitPath` RAII guard per crate** (empty-directory `PATH`, mirroring `NeutralPath` at
      `crates/devflow-cli/src/test_support.rs:327`, each holding its own crate's single `PATH`
      mutex). Prerequisite for **both** criterion 1's and criterion 6's tests.
      - `crates/devflow-cli/src/test_support.rs` — beside `NeutralPath`, under `env_lock()` (`:94`)
      - `crates/devflow-core/src/test_support.rs` — plus that crate's **first** `PATH` mutex,
        `#[cfg(test)]`-only
      **Why not one shared guard:** `crates/devflow-core/src/lib.rs:79` gates the module with
      `#[cfg(any(test, feature = "test-support"))]` and `tempfile` is a dev-dependency, so a guard
      shared across the crate boundary would not compile for devflow-cli. The `#[cfg(test)]` gate on
      devflow-core's copy makes it unreachable from devflow-cli, so no test binary can ever hold two
      guards under two different mutexes — the `PATH` race is prevented structurally, not by
      discipline.
- [ ] NC-1's harness-sanity control, **in each crate** — `Command::new("git")` returns `Err` with
      the guard installed and `Ok` without it, and `PATH` is byte-identical to its pre-guard value
      after the guard drops. Write **before** the regression tests; retain if cheap.
- [ ] **Flake risk to watch:** `NoGitPath` is the first guard in this workspace that makes `git`
      unresolvable process-wide, and devflow-core has had zero `PATH` mutations until now. Sibling
      `git`-shelling tests can fail spuriously inside a guarded window. Mitigation is scope
      minimisation, enforced as an acceptance criterion.
- [ ] `crates/devflow-core/src/agent_result.rs` — the 999.87 `evaluate_layer2`-unrunnable-`git`
      test; and the `Option<u32>` change to `phase_commit_count` (`:1841`) that forces both
      consumers open.
- [ ] The 999.77 multi-cycle test — module owner to be decided by the plan (`pipeline_outcomes.rs`
      if it drives `handle_validate_outcome`; `agent_result.rs` if it drives the lower-level
      `phase_commit_count` / progress-check pair).
- [ ] `crates/devflow-core/src/state.rs` — new field(s) for 999.78's counter and 999.79's
      staleness fingerprint, each with a serde round-trip pair (present + absent-defaults)
      mirroring `last_validate_failure_commit_count`'s existing pair (`state.rs:415-447`).
- [ ] `crates/devflow-cli/src/pipeline_launch.rs` — the extended 999.84 test on the `:2302` base.
- [ ] `crates/devflow-core/src/git.rs` — the rewritten `check_ssh_signing_viability` and its
      probe-based fixtures (positive / negative / block-then-recover).
- [ ] `crates/devflow-cli/tests/release_check.rs` — rewrite the two `ssh_add_absent`-named tests to
      exercise "`ssh-keygen` absent" instead; `ssh-add` leaves the probe entirely under D-04.
- [ ] Measure and record the full-suite runtime in Test Infrastructure.

*Framework install: none — `cargo test` is already configured in this workspace.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `execution_root` reaches `phase_has_blocking_human_checkpoint`, proven by a performed revert | HARDEN-04 c4 | "This test fails when the fix is reverted" is a property of a mutation, not of the committed tree; no committed test can assert it about itself | The binding is `pipeline_launch.rs:1068` (`let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);`); the call the ROADMAP criterion cites is `:1070` (`&& verify::phase_has_blocking_human_checkpoint(execution_root, phase)`). Revert **either** — bind `execution_root = project_root`, or pass `project_root` at the call — run the new test, record the failure output, revert the revert, re-run and record the pass. Both halves go in the SUMMARY. |
| Doc comment no longer over-promises | HARDEN-01 c1 | Prose accuracy; no assertion can judge whether a comment matches behaviour | Read the identified `pipeline_outcomes.rs` doc comment against the post-change code and confirm the guarantee claimed is the guarantee delivered |
| Public-API removal recorded for release | HARDEN-05 c5 / D-04 | `CHANGELOG.md` + crate-doc accuracy is editorial | Confirm the removal of `classify_ssh_add_status` and `SigningStatus` is recorded; version stays `v2.5.0` per D-08; version set in two places; `devflow-core` publishes before `devflow-cli` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or a Wave 0 dependency
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all ❌ references above
- [ ] No watch-mode flags
- [ ] Every measurement in "Mandatory Negative Controls" has its opposite-result case present, run,
      and **seen to fail** — not asserted from reading the fix
- [ ] NC-1 passed before any criterion-1 or criterion-6 result was believed
- [ ] The 999.78 `--force` persistence choice is stated in a PLAN.md and either tested or recorded
      as accepted-not-tested
- [ ] Full-suite runtime measured and recorded (not assumed)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
