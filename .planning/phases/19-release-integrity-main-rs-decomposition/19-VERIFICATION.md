---
phase: 19-release-integrity-main-rs-decomposition
verified: 2026-07-22T10:28:47-04:00
status: passed
score: 7/7
behavior_unverified: 0
overrides_applied: 1
overrides:
  - must_have: "All three parts of the equivalence proof hold on a CI run against the branch"
    reason: "The existing CI workflow has no symbol-diff or test-name-set steps and Plan 19-11 prohibited changing it. Both scripts passed locally on the exact pushed SHA, while three independent CI attempts on that SHA ran the repository's test, Clippy, and format jobs successfully. The known coverage limitation was presented explicitly at the blocking checkpoint."
    accepted_by: "user"
    accepted_at: "2026-07-22T10:28:47-04:00"
gaps: []
human_verification: []
---

# Phase 19: Release Integrity + `main.rs` Decomposition Verification

## Verdict

**PASSED.** The phase goal is achieved in the live codebase. Both release-integrity defects are closed, `main.rs` is a thin crate root backed by flat sibling modules, the shared CLI environment lock is preserved, and the AI change acceptance contract exists and has completed its human-approved dogfood checkpoint.

The verification does not treat SUMMARY claims as implementation evidence. It directly inspected the production files, ran the focused release-integrity tests, checked the module and mutex wiring, reconciled the durable baseline against the pushed SHA, and confirmed three GitHub Actions attempts on that SHA.

## Observable Truths

| # | Truth | Status | Direct evidence |
|---:|---|---|---|
| 1 | 19a: DevFlow runtime artifacts do not enter an ordinary downstream commit, and provenance no longer emits an absolute executable path. | VERIFIED | `workflow::ensure_devflow_dir` writes `*\n`; all seven production constructors call it; `cargo test -p devflow-core --test devflow_dir_gitignore` passed 2/2; `workflow_started_payload_carries_build_provenance` passed 1/1; the approved scratch reproduction committed no `.devflow/` path in full and marker-only cases while `git add -f` succeeded. |
| 2 | 19b: `commit_path` is idempotent on unchanged content and does not create an empty release commit. | VERIFIED | `cargo test -p devflow-core commit_path` passed all four matching tests, including repeat-content, clean-path, nonexistent-path, and scoped-staging behavior. `commit_all` remains separate and unchanged in scope. |
| 3 | 19c: The split has a durable equivalence baseline and one shared CLI environment mutex. | VERIFIED | Baseline SHA and both 438-entry name files are tracked. Exactly one CLI `static ENV_MUTEX` exists in `test_support.rs`; all 18 CLI `.lock()` sites resolve through that module. |
| 4 | 19d: Tests and pipeline behavior survived the mechanical extraction. | VERIFIED | Final trailing-name set is 438/438 with an empty diff; all 11 per-target pass counts match the committed baseline; launch/outcomes/gate modules retain direct cross-module calls and their focused tests were exercised by the full suite and CI. |
| 5 | 19e: The pipeline bottleneck is decomposed without behavioral change. | VERIFIED | `pipeline_launch.rs`, `pipeline_outcomes.rs`, and `pipeline_gate.rs` are substantive modules with direct imports/calls; 26 moved pipeline functions reconcile with zero unexplained hunks. |
| 6 | 19f: `main.rs` is a thin crate root and the codebase documentation matches the final layout. | VERIFIED | `main.rs` is 478 lines and declares all eight production siblings. Its production responsibilities are Clap types, `CliError`, dispatch, `main`, `run`, and `project_root`. `STRUCTURE.md`, `TESTING.md`, and ROADMAP reflect the live layout and testing invariants. |
| 7 | 19g: The repository contains and documents an AI change acceptance contract that was exercised through review. | VERIFIED | `.claude/skills/ai-change-acceptance/` contains a 58-line skill plus both rule files; `CONTRIBUTING.md` places `AI Change Acceptance` between PR Process and Cutting a Release. Plan 19-05's blocking dogfood checkpoint was human-approved with its isolated-review citation gap retained as a non-blocking finding. |

## Equivalence Proof

Baseline: `f35d6c1ec34fc3fbc5e4c4477d98e16f4355d04f`. Pushed implementation SHA: `aa9587355d51de51737703be4878a77c4ff747d1`.

| Proof | Result |
|---|---|
| Symbol reconciliation | 18 functions from 19-07 + 26 from 19-08 + 56 from 19-09 = 100. Independent count: 103 baseline top-level functions minus three remaining = 100. Zero unexplained hunks. |
| Test name set | 438 live, 438 baseline, empty C-sorted diff, `NAMESET-IDENTICAL`. |
| Per-target counts | `106/3/4/1/1/3/10/306/2/2/0` on both sides; 438 total, zero failures. |

The plan's literal `rg '::tests::'` command is invalid because it drops top-level tests and retains Cargo's suffix. Verification used the corrected all-`: test` extraction documented in `19-11-SUMMARY.md`.

## Artifacts and Wiring

| Artifact or link | Status | Evidence |
|---|---|---|
| `.devflow/.gitignore` constructor | VERIFIED | `ensure_devflow_dir` uses create-new semantics and writes `*\n`; seven production call sites replace raw directory creation. |
| Constructor -> downstream Git behavior | VERIFIED | Focused integration test and real scratch repository both show ordinary `git add .` excludes `.devflow/`. |
| `commit_path` -> Git no-op handling | VERIFIED | `commit_path` omits `--allow-empty`, routes no-change output through the scoped combined-output helper, and passes four focused tests. |
| `main.rs` -> sibling modules | VERIFIED | Eight `mod` declarations exist and dispatch imports `advance`, `resume`, `parallel`, `sequentagent`, and command handlers from their owners. |
| Pipeline module cycle | VERIFIED | Launch calls preflight/outcomes/gate functions directly; gate transitions call launch; module documentation records the intentional Rust module cycle. |
| CLI tests -> shared mutex | VERIFIED | One `crate::test_support::ENV_MUTEX`, 18 lock sites, D-04 invariant documented. Core's two statics remain in separate test binaries. |
| Acceptance contract -> contributor/review surface | VERIFIED | Skill and rules are tracked; CONTRIBUTING names the contract and review enforcement. |

`gsd-tools verify.artifacts` and `verify.key-links` reported no failures, but these plans encode artifact/link entries as strings rather than structured path objects, so the tool reported zero structured checks. The table above is the required direct Level 1-3 inspection rather than treating that zero-count result as evidence.

## Behavioral Checks

```text
cargo test -p devflow-core --test devflow_dir_gitignore
  2 passed; 0 failed

cargo test -p devflow-core commit_path
  4 passed; 0 failed

cargo test -p devflow workflow_started_payload_carries_build_provenance
  1 passed; 0 failed
```

The full workspace suite was run once for the final phase proof and again by the pre-push hook: 438 tests, zero failures, with every target count matching the baseline. Three CI attempts on the same pushed SHA also passed:

- https://github.com/denniyahh/devflow/actions/runs/29927890337/attempts/1
- https://github.com/denniyahh/devflow/actions/runs/29927890337/attempts/2
- https://github.com/denniyahh/devflow/actions/runs/29927890337/attempts/3

CI's literal commands remain `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check`.

## ENV_MUTEX Disposition

`ENV_MUTEX preserved — no finding`.

Plans 19-06 through 19-09 record 15 consecutive package-test runs after successive extraction stages. Final local runs and all three shared-runner attempts were stable. The CLI contains one mutex static and 18 lock sites. No count changed and no env-sensitive test failed.

## Anti-Pattern Scan

The Phase 19 Rust source set contains no `TBD`, `FIXME`, or `XXX` debt marker. No placeholder module, empty test module, unrestricted public export, new dependency, or source edit from the verification-only plan was found.

Known non-blocking findings remain recorded rather than hidden:

- CI does not execute the symbol/name scripts. The explicit override above records the user-approved disposition. **Tracked as backlog item 999.22** (filed 2026-07-22).
- ~~GitHub annotates `actions/checkout@v4` for Node.js 20 deprecation~~ — **resolved after this verification was written** by commit `a12a617`, which bumped all three workflows to `actions/checkout@v7` (verified to exist: v7.0.1) and made the CI test job's workspace scope explicit (`cargo test --workspace`). CI is green on the resulting HEAD.
- Plan 19-05 found that an isolated reviewer applies the contract's judgment but does not cite the project contract unless dispatched to load it. **Tracked as backlog item 999.21** (filed 2026-07-22).

## Post-Verification Addendum (2026-07-22, independent review)

This verification was written at pushed SHA `aa95873`. Three further commits landed
afterward (`ef3ac49`, `e1a33e6` docs/tracking; `a12a617`, `1e2ddbb` CI config). An
independent review at HEAD `1e2ddbb` re-confirmed the phase's core claims from source
rather than from SUMMARY prose:

| Re-check | Method | Result |
|---|---|---|
| Pure-move equivalence | Symbol set of baseline `f35d6c1:main.rs` vs union of all nine current CLI modules | 231 vs 231, zero lost, zero added |
| Body-level equivalence | Normalized line-multiset diff (visibility keywords and imports stripped) | Only 63 differing lines, all rustfmt re-wraps of signatures pushed past 100 chars by `pub(crate)`, plus 9 expected `#[cfg(test)]` attrs. Zero logic changes |
| 19a chokepoint completeness | Production-only `create_dir_all` scan across the workspace | Exactly one production call site, inside `ensure_devflow_dir` itself — no bypass |
| `ENV_MUTEX` singularity | Static and lock-site census in `devflow-cli` | One `pub(crate) static` in `test_support.rs`, 18 lock sites across 5 modules, all resolving to it |
| Gates at HEAD | `cargo build` / `clippy --workspace --all-targets -D warnings` / `fmt --check` / `test --workspace` | All clean; 438 passed, 0 failed |
| CI at HEAD | GitHub Actions runs `29934143450` (CI) and `29934146203` (Devcontainer) on `1e2ddbb` | Both success |

One correction to the record: the CI change from `cargo test` to `cargo test --workspace`
is explicitness, not a coverage fix — the root manifest is virtual, so both forms already
collect the same 438 tests (measured both ways). The commit message overstates it slightly;
no behavior is affected.

Note that `devflow_dir` remains duplicated (`workflow.rs:34` public, `agent_result.rs:872`
private). Both only compute a path and neither creates a directory, so the 19a chokepoint is
unaffected; this is cosmetic duplication, not a defect.

## Requirement Coverage

This phase intentionally has no formal `REQUIREMENTS.md`; ROADMAP and CONTEXT define units 19a through 19g. All seven units have direct evidence above and a `landed` verdict in `19-11-SUMMARY.md`. No requirement is orphaned, assumed, partial, or blank.

## Human Verification

None pending. Plan 19-11's downstream reproduction and `ENV_MUTEX` disposition were the phase's blocking human checkpoint, and the user approved it on 2026-07-22 after the CI limitation and full roll-call were presented.

## Conclusion

Phase 19 achieves its stated goal with one narrow, explicit verification override. There are no actionable gaps and no remaining human-verification items. The phase may be marked complete.
