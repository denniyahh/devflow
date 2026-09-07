---
phase: "46"
slug: "ci-load-shape-and-operator-input-validation"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: true
created: "2026-09-04"
---

# Phase 46 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by plan-phase from `46-RESEARCH.md` § Validation Architecture.
> The Per-Task Verification Map is intentionally unfilled — task IDs do not exist until the
> planner has written the plans. `/gsd-validate-phase` fills it.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `libtest` via `cargo test` — no third-party test framework in this workspace |
| **Config file** | none — workspace members in `Cargo.toml`; unit tests in `#[cfg(test)] mod tests`, integration tests in `crates/devflow-cli/tests/*.rs` |
| **Quick run command** | `cargo test -p devflow --bin devflow <name>` (unit) · `cargo test -p devflow --test <file_stem>` (integration) |
| **Full suite command** | `scripts/check.sh all` locally · `scripts/check-in-container.sh all` for CI parity |
| **Estimated runtime** | quick ~10-40s · full container run 2-9min cold |

**Two command traps this repo has already paid for — both are hard errors here, not style:**

- `cargo test -p devflow --lib` verifies **nothing**. `devflow` is binary-only (no `src/lib.rs`);
  cargo exits non-zero with `error: no library targets found in package 'devflow'` before running a
  single test. Use `-p devflow --bin devflow`. `-p devflow-core --lib` *is* valid.
- `cargo test --exact <name>` exits **0** when the name matches nothing. Assert on a real
  `1 passed` **and** a non-zero `filtered out` count.

---

## Sampling Rate

- **After every task commit:** the narrow command for the file touched — e.g.
  `cargo test -p devflow --bin devflow ensure_base_is_a_local_branch`, or
  `cargo test -p devflow --test ci_parity_guards`
- **After every plan wave:** `cargo test --workspace --no-fail-fast` (i.e. `scripts/check.sh test`)
- **Before `/gsd-verify-work`:** `scripts/check-in-container.sh all` green — this is what
  `git push` runs anyway via `scripts/hooks/pre-push:220`
- **Max feedback latency:** ~40s for the per-task commands

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| *(unfilled — plans not yet written; `/gsd-validate-phase` populates this)* | | | | | | | | | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Requirement → target file (established by research; no Wave 0 needed)

| Req | Test type | Automated command | Target file |
|---|---|---|---|
| VALID-01 | unit | `cargo test -p devflow --bin devflow ensure_base_is_a_local_branch_rejects_commit_ish_that_is_not_a_local_branch` | `crates/devflow-cli/src/commands.rs:5459` — **extend** per D-10 |
| VALID-02 | integration | `cargo test -p devflow --test stop_e2e` | `crates/devflow-cli/tests/stop_e2e.rs` — **add** cases |
| INFRA-01 (automatable half) | integration (YAML assertion) | `cargo test -p devflow --test ci_parity_guards` | `crates/devflow-cli/tests/ci_parity_guards.rs` — **add** guards |
| INFRA-01 (acceptance half) | **manual / external** | `gh pr checks <PR>` **unfiltered**, then read the new job's own row | not a test file — see Manual-Only below |

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements.* All three target files already exist and
are already wired into `cargo test`. No framework install, no new test file, no fixture scaffolding.
Every change is an extension of an existing test or an addition to an existing integration file.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| The new CI job actually ran and was green on the PR's current `HEAD_SHA` | INFRA-01 | Nothing in-tree can prove a GitHub job ran. `cargo test` can only prove the workflow file has the required *shape*; whether the job executed is external state. | Run `gh pr checks <PR>` **unfiltered** and read the new job's own row. Do **not** use `--required`: D-05 keeps the job out of the 4 required contexts, so `--required` would omit it and report green without ever looking. Separately confirm `gh pr view <PR> --json headRefOid` matches the run's commit. `gh run list` is **not** acceptable evidence — branch run history does not establish PR check status. |

---

## Failing-Direction Requirement (repo-specific, non-negotiable)

Every guard added to `ci_parity_guards.rs` must have a **demonstrated** failing direction: mutate
the YAML, run the test, capture a real `test result: FAILED`, restore. A guard whose failing
direction was never observed has not been shown to discriminate. This repo has shipped a
non-discriminating gate before (`rg -c <pat> | rg '^0$'`, which exits 1 on both a green and a red
suite), documented it, and still left it in place across five plans — documenting a broken gate is
not fixing it.

Two related traps for any counting assertion in these guards:

- `rg -c <pat> | rg '^0$'` is a constant-fail, not a zero-check. Print the count and the command's
  own exit code on separate lines and assert on those.
- A grep over source counts comment prose. Strip comments before counting.

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references *(n/a — no Wave 0 gaps)*
- [ ] No watch-mode flags
- [ ] Feedback latency < 40s
- [ ] Every `ci_parity_guards.rs` guard has an observed `test result: FAILED`
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
