---
phase: 27
slug: scrub-redirecting-git-environment-from-production-calls
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-30
---

# Phase 27 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by `/gsd-plan-phase 27` from `27-RESEARCH.md` § Validation Architecture.

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
`cargo test --workspace` for these; it does not terminate in bounded time under a hostile
`GIT_DIR` (RESEARCH.md Pitfall 2 / Open Question #2). Use the two scoped invocations:

```
GIT_DIR=<throwaway-repo>/.git cargo test -p devflow-core --features test-support
GIT_DIR=<throwaway-repo>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes
```

Baselines measured live at HEAD `6350798` on 2026-07-30: **54 failed / 352 passed** and
**44 failed / 139 passed** respectively — **98 failures total**, superseding the stale
"37 failing tests" figure carried in ROADMAP.md and 27-CONTEXT.md. Target after
migration: **0 failed** on both.

---

## Sampling Rate

- **After every task commit:** the quick-run command scoped to the module just migrated
- **After every plan wave:** `cargo test --workspace` in a normal environment — confirms no regression in the ordinary case
- **Phase gate (before `/gsd-verify-work`):** both hostile-`GIT_DIR` commands above, run to a clean `test result:` line — a timeout is **not** a pass
- **Max feedback latency:** ~240 seconds (full workspace); seconds for per-module runs

---

## Per-Task Verification Map

> Task IDs do not exist until plans are written. `/gsd-execute-phase` and
> `/gsd-validate-phase` fill this table; the decision-keyed map below is the
> planning-time contract it must satisfy.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | D-01 | T-27-01 | Every constructed `Command` has all 17 redirecting vars marked for removal; no bypass parameter exists | unit | `cargo test -p devflow-core --features test-support --lib` (new test in `git.rs`) | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | D-03 | T-27-02 | A constructed process resolves the caller-supplied root even with a hostile `GIT_DIR` set | unit/integration | `cargo test -p devflow-core --features test-support --lib` (new test in `git.rs`) | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | D-03 | T-27-02 | Previously-failing core tests pass under a hostile `GIT_DIR` | regression | `GIT_DIR=<throwaway>/.git cargo test -p devflow-core --features test-support` → 0 failed | ✅ | ⬜ pending |
| TBD | TBD | TBD | D-03 | T-27-02 | Previously-failing cli tests pass under a hostile `GIT_DIR` | regression | `GIT_DIR=<throwaway>/.git cargo test -p devflow --bin devflow -- --skip pipeline_gate --skip pipeline_outcomes` → 0 failed | ✅ | ⬜ pending |
| TBD | TBD | TBD | D-02 | — | `build.rs` is untouched by this phase | plan-time check | `git diff --stat develop..HEAD -- crates/devflow-cli/build.rs` is empty | n/a | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] New unit test in `crates/devflow-core/src/git.rs`'s existing `#[cfg(test)] mod tests` — asserts every redirecting variable is marked for removal on the new constructor's output, mirroring `test_support.rs:196-214`
- [ ] New unit test proving a spawned process resolves the caller's root, not a hostile `GIT_DIR`'s target (`tempfile` is already available)
- [ ] A variable-list drift test mirroring `test_support::local_env_vars_match_git`, so the production list stays honest against the installed `git rev-parse --local-env-vars`
- [ ] No framework install needed — `cargo test` is fully functional in this environment

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Root cause of `pipeline_gate` / `pipeline_outcomes` not terminating under a hostile `GIT_DIR` | RESEARCH Open Question #2 | Not diagnosed during research; the `--skip` workaround makes the acceptance signal measurable but does not explain the hang | Run each module alone under a hostile `GIT_DIR`, then together; if the migration does not resolve it, file a backlog entry rather than widening this phase |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 240s
- [ ] Both hostile-`GIT_DIR` commands reach a `test result:` line with 0 failed (not a timeout)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
