---
phase: 25
slug: end-to-end-dogfood-blockers
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-27
---

# Phase 25 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Derived from `25-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` harness via `cargo test` — no external test framework; `tempfile` is the only `dev-dependencies` addition in either crate |
| **Config file** | none — `scripts/check.sh` is the project's canonical local/CI runner wrapper |
| **Quick run command** | `cargo test --workspace <module-path filter>` (e.g. `cargo test --workspace version::`) |
| **Full suite command** | `scripts/check.sh` (= `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`) |
| **Estimated runtime** | ~180 seconds full suite (618 tests / 17 binaries at last measurement); targeted module filters run in seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace <module just touched>`
- **After every plan wave:** Run `scripts/check.sh` — the same chain CI runs (`.github/workflows/ci.yml`)
- **Before `/gsd-verify-work`:** Full `scripts/check.sh` green
- **Max feedback latency:** ~180 seconds (full suite); targeted filters well under 30s

**25e exception — local-green is explicitly insufficient.** 999.47's confirmed mechanism (the fork/exec cmdline-inheritance window) only reproduces reliably inside the pinned CI container; local probes at 4000 spawns never observed it. The 25e retarget's success can therefore only be confirmed by CI-on-branch stability across several pushes, not by a single local green run. This matches the project's own established precedent (`19-RESEARCH.md`: *"Verification must be CI-on-branch — local-green is explicitly insufficient"*).

---

## Per-Task Verification Map

> Task IDs are assigned by the planner. This map is keyed by unit until plans exist; the planner must expand it to per-task rows.

| Unit | Requirement | Behavior | Test Type | Automated Command | File Exists | Status |
|------|-------------|----------|-----------|-------------------|-------------|--------|
| 25a | 999.51 / DEN-76 | Base ref currency is enforced — the "heading present but code stale" case is closed, not just "heading absent" | unit | `cargo test --workspace start_reachability` (new, alongside the existing `phase_reachability_on_base` suite in `preflight.rs`) | ❌ W0 | ⬜ pending |
| 25b | 999.48 / DEN-73 | `enforce_build_staleness` runs at `start`, not on every stage transition | unit | `cargo test --workspace enforce_build_staleness` — plus an ADDED assertion that a mid-run stage transition does NOT re-invoke the check | ✅ existing / ❌ W0 new assertion | ⬜ pending |
| 25c | 999.49 / DEN-74 | `compute_version` derives from (reachable semver baseline, classified bump); refuses on unreachable-highest-tag; floors to patch on no-bump and on unrecognised type | unit | `cargo test --workspace version::` (full rewrite of the existing suite) | ✅ existing, needs rewrite | ⬜ pending |
| 25c | 999.49 / DEN-74 | Major-bump preflight gate fires and never auto-ships | unit + integration | `cargo test --workspace preflight_major_bump` (new) | ❌ W0 | ⬜ pending |
| 25c | 999.49 / DEN-74 | `pipeline_gate.rs`'s finalization-retry fixture still predicts the correct next tag under the new algorithm | integration | `cargo test --workspace finalization_retry_gate_never_auto_approves` (existing, needs rewrite — **the previously-unflagged consumer at `pipeline_gate.rs:809-840` independently re-derives the OLD algorithm**) | ✅ existing, needs rewrite | ⬜ pending |
| 25d | 999.44 / DEN-68 | `TERM`→`KILL` escalation with a bounded wait clears a `SIGTERM`-ignoring child, and death is verified rather than assumed | unit | `cargo test --workspace terminate_and_verify` (new) | ❌ W0 | ⬜ pending |
| 25d | 999.44 / DEN-68 | Registry-independent discovery finds a process whose root directory no longer exists; wrapper and child are reaped together | integration | new test spawning a real child under a since-deleted temp root | ❌ W0 | ⬜ pending |
| 25e | 999.47 / DEN-72 | Retargeted tests assert the `(pid, starttime)` identity guard, not `looks_like_devflow_process` | unit | `cargo test --workspace looks_like_devflow_process_is_false_for_a_non_devflow_process` and `stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check` (both existing, rewritten in place) | ✅ existing, needs rewrite | ⬜ pending |
| 999.38 | folded into 25b | `ahead_build_from_descendant_commit_warns_instead_of_blocking` no longer races concurrent git-shelling tests | unit (flake-class) | `cargo test --workspace ahead_build_from_descendant_commit_warns_instead_of_blocking` — **flake reproduction requires a full-suite concurrent run, not this single-test invocation** | ✅ existing | ⬜ pending |
| 25f | CONTRIBUTING drift + D-16 | CONTRIBUTING.md step 5 matches the actual signing procedure; ROADMAP's Phase 25 Acceptance paragraph matches D-15 | manual-only (docs) | N/A | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] New tests for 25a's chosen option (whichever of D-17's three the planner selects) — **no existing test covers base-ref currency at all**
- [ ] New tests for the 25c major-bump preflight gate — no existing coverage
- [ ] New tests for 25d's `terminate_and_verify` — `agent::terminate` today has only single-`SIGTERM` tests, no escalation-with-verification coverage
- [ ] New test for 25d's registry-independent discovery — nothing today spawns a child, deletes its root out from under it, then asserts discovery still finds it
- [ ] New assertion for 25b that a mid-run stage transition does not re-invoke the staleness check
- [ ] No framework install needed — `cargo test --workspace` already covers the workspace

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| CONTRIBUTING.md release step 5 directs the reader to `git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" tag -s …`, and the stale `tag.gpgsign=false` warning is removed | 25f | Prose correctness against a real procedure — no assertion can prove the instructions are followable | Read step 5 against the tracked `.gitconfig` added in PR #38; confirm `tag.gpgsign=true` is set there and that the documented command names the release signing key, not the agent's |
| ROADMAP's Phase 25 "Acceptance" paragraph reflects CONTEXT.md D-15 (acceptance run decoupled from phase closure) | 25f / D-16 | Doc consistency; a verifier reading the un-amended paragraph would mark a correctly-completed phase unmet | Confirm the paragraph no longer requires `devflow evidence --require-shipped` to exit 0 as a closure condition |
| `PROJECT.md` Constraints and `ROADMAP.md:36` no longer ban commit-message-based versioning | 25c / D-06 | Same class — a planner or verifier reading the un-amended constraint would treat the whole 25c design as a violation | Confirm both locations are updated in the same commit range as the 25c implementation |
| 25e flake is actually gone | 25e | Only reproduces in the pinned CI container | Observe CI-on-branch green across several pushes; a single local green run does not close this |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 180s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
