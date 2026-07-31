---
phase: 29
slug: release-cut-executor-observe-then-act-within-the-repo-s-rules
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-31
---

# Phase 29 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by `/gsd-plan-phase 29` from `29-RESEARCH.md` § Validation Architecture.
> The Per-Task Verification Map is populated once PLAN.md task IDs exist.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` — workspace-standard `#[test]` / `#[cfg(test)] mod tests`, integration tests under `crates/*/tests/`. Direct precedent for this command family: `crates/devflow-cli/tests/release_check.rs` |
| **Config file** | none — conventions live in `CONTRIBUTING.md` § Testing and `.claude/skills/ai-change-acceptance/rules/change-acceptance.md` |
| **Quick run command** | `cargo test -p devflow-core <module_name>` / `cargo test -p devflow <name>` — package-scoped and name-scoped. **Never `cargo test --exact` with a bare name**: it matches nothing and still exits 0 (proven false-green trap; assert on `N passed`) |
| **Full suite command** | `cargo test --workspace`, plus `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — all three must be clean (`ai-change-acceptance` requirement 4) |
| **Estimated runtime** | ~90 seconds full workspace; <10 seconds package-scoped |

---

## Sampling Rate

- **After every task commit:** `cargo test -p devflow-core <new_module_name>` (or `-p devflow` for CLI-side tasks)
- **After every plan wave:** `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --check`
- **Before `/gsd-verify-work`:** full suite green, **plus** one real read-only `devflow release status` run against this repo's actual state (safe — 29a is pure observation)
- **Max feedback latency:** 90 seconds

**Escalation rule carried from Phase 26:** a green suite is necessary but not sufficient here. Phase 26 scored 11/11 verification with 763 passing tests while carrying twelve Criticals, every one found by a human reading code and none by a test. For 29c's irreversible steps, adversarial review is the primary gate; the suite is the backstop.

---

## Per-Task Verification Map

*Task IDs do not exist until plans are written. The unit-level map below is the contract each plan's tasks must satisfy; validate-phase expands it to `29-NN-NN` rows.*

| Unit | Behavior | Test Type | Automated Command | File Exists |
|------|----------|-----------|-------------------|-------------|
| 29a | Six-question observer returns correct state for each of Present / Absent / **Unreachable** per oracle | unit (oracle results injected) | `cargo test -p devflow-core release_observe` | ❌ W0 |
| 29a | Signed-tag presence distinguishes lightweight vs annotated vs signed via fixture-repo tag objects | unit (real git fixture, no network) | `cargo test -p devflow-core release_observe -- tag` | ❌ W0 |
| 29a | crates.io status classification: 200→Published, 404→NotPublished, timeout/DNS-fail→Unreachable | unit (fake status injected at the classification boundary) | `cargo test -p devflow-core release_observe -- publish_state` | ❌ W0 |
| 29b | Merge-method policy selects `merge` for sync-back intent even when `squash` is also allowed; refuses loudly when the preferred method is absent from the allowed set | unit (pure fn, no I/O) | `cargo test -p devflow-core release_publish -- merge_method` | ❌ W0 |
| 29b | Version bump / changelog reuse existing tested functions unchanged | existing suites | `cargo test -p devflow-core version hooks` | ✅ existing |
| 29b | PR create / PR merge against a real GitHub PR | **live-remote, not hermetic** | manual UAT — see below | N/A |
| 29c | Signed-tag command form matches `CONTRIBUTING.md` exactly (argv order, `-c user.signingkey=`, `-s`, message form) | unit (argv assertion, no execution) | `cargo test -p devflow-core release_publish -- tag_command_form` | ❌ W0 |
| 29c | `publish_order()` is consumed in the exact returned order — never re-sorted, never hardcoded | unit (consumer-order assertion) | `cargo test -p devflow-core release_publish -- publish_order_respected` | ❌ W0 |
| 29c | A pre-existing local unsigned `v{version}` tag from `hooks_after_ship` is detected before `git tag -s` is attempted (IN-01 regression) | unit (real git fixture reproducing the collision) | `cargo test -p devflow-core release_publish -- tag_namespace_collision` | ❌ W0 |
| 29c | Real `git tag -s` / `git push` / `cargo publish` against the live remote and registry | **irreversible, not hermetic** | manual UAT — see below | N/A |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/devflow-core/src/release_observe.rs` (or equivalent) — new module, no existing test file to extend
- [ ] `crates/devflow-core/src/release_publish.rs` (or equivalent) — new module
- [ ] Shared fixture helper reproducing the `hooks_after_ship`-style local unsigned `v{version}` tag for the IN-01 collision regression — extend `git.rs`'s existing `init_repo()` / `flow()` helpers rather than duplicating them
- [ ] Test-module home for the `gh` / `curl` invocation boundary — `#[cfg(test)] mod tests` alongside each new module, matching `git.rs` / `hooks.rs` / `preflight.rs`
- [ ] Framework install: **none** — `cargo test` is already configured workspace-wide

---

## Manual-Only Verifications

| Behavior | Unit | Why Manual | Test Instructions |
|----------|------|------------|-------------------|
| `gh pr merge --auto <method>` actually waits for green checks and then merges with the requested method | 29b | GitHub server-side behavior; faking it removes the property under test | Open a real PR on this repo, run the executor's merge step, confirm the merge commit's method matches the requested one (ancestry preserved for `merge`) |
| crates.io `/api/v1/crates/{name}/{version}` 200/404 semantics still hold | 29a | External API contract; verified live 2026-07-31, can rot silently | `#[ignore]`-gated live smoke test, run manually |
| Real `git tag -s` → `git push origin vX.Y.Z` → `cargo publish` core-then-cli, including interaction with `scripts/hooks/pre-push` | 29c | Genuinely irreversible against the live world | `checkpoint:human-verify` gate; line-by-line review before the run, not a test |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or a Wave 0 dependency
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all ❌ references above
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] Every `unreachable ≠ absent` arm has an explicit negative test
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
