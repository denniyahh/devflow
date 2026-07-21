---
phase: 19
slug: release-integrity-main-rs-decomposition
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-07-21
---

# Phase 19 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` (no external harness) |
| **Config file** | none — implicit via `Cargo.toml` / `rust-toolchain.toml` |
| **Quick run command** | `cargo test -p devflow <test_name>` — **NOTE:** `--lib` does not work on this binary-only crate (confirmed dead end, STATE.md 18-01 decision entry); use the bare form |
| **Full suite command** | `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~90 seconds (`build_provenance.rs` nested-build tests dominate) |

**Baseline before this phase's own new tests** — measured live 2026-07-21 via
`cargo test -p devflow-core -- --list` and `cargo test -p devflow -- --list`:
296 devflow-core lib + 2 `monitor_e2e` + 106 devflow-cli unit + ~20 devflow-cli
integration ≈ **424 tests workspace-wide**.

---

## Sampling Rate

- **After every task commit:** `cargo test -p devflow <affected_test_or_module>` (fast, targeted)
- **After every plan wave:** `cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite green **on a CI run against the branch**, not just local-green — **D-11 is explicit**: 19i hit 2/2 in CI after passing locally most of the time. CI triggers automatically on push.
- **Max feedback latency:** 90 seconds local; CI adds one push cycle.

⚠ **A false-green trap applies to every command in this file.** `cargo test --exact`
with a bare test name matches nothing and still exits 0. Assert on the reported
pass count (e.g. `1 passed`), never on exit status alone. The package is
`devflow`, not `devflow-cli`.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 19-01-xx | 01 | 1 | 19a-WR01 | T-19-01 | A newly created `.devflow/` is self-ignoring, so agent stdout and operator paths can never enter a downstream user's git history | integration | new test asserting `.devflow/.gitignore` contains `*` after `ensure_devflow_dir` runs, **plus** a live scratch-repo reproduction of the `17-REVIEW.md` scenario (`git add . && git commit` no longer sweeps `.devflow/*`) | ❌ W0 | ⬜ pending |
| 19-01-xx | 01 | 1 | 19a-WR01-coverage | T-19-01 | Every code path that writes under `.devflow/` produces the `.gitignore` — the next new writer cannot silently reopen the hole | integration | new coverage test exercising each of the 7 converted `create_dir_all` sites, **including the `sequentagent`/`parallel` path that never calls `save_state`** | ❌ W0 | ⬜ pending |
| 19-01-xx | 01 | 1 | 19a-WR02 | T-19-02 | `events.jsonl`'s `exe_path` carries only a filename — no absolute path, no home directory, no OS username | unit | update existing `workflow_started_payload_carries_build_provenance` (`main.rs:6867`); assert `!payload["exe_path"].as_str().unwrap_or("").contains('/')` | ✅ update existing | ⬜ pending |
| 19-02-xx | 02 | 1 | 19b | — | `commit_path` on byte-identical content is a genuine no-op, so a release tag can never sit on an empty commit | unit | new test in `crates/devflow-core/src/git.rs` `mod tests`, **RED-then-GREEN**: call `commit_path` twice with identical content, assert `git rev-list --count HEAD` unchanged after the second | ❌ W0 | ⬜ pending |
| 19-03..06-xx | 03–06 | 2–3 | 19c–19f | — | Zero behavioral change | **equivalence proof, not a new test** | per-function `diff` procedure + `cargo test --workspace -- --list` name-set diff + full-suite pass-count identity against the 424-test baseline | n/a — structural proof | ⬜ pending |
| 19-07-xx | 07 | any | 19g | — | The reviewer rejects a test that only asserts constants, reproduces the production algorithm, or compares a call with itself | manual/process | dogfood the contract on itself: re-run `/gsd-code-review` against a deliberately non-compliant test-only diff and confirm it is flagged | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Task IDs are provisional** — plan-phase seeds this file before plans exist. The
planner assigns real IDs; validate-phase reconciles.

---

## Wave 0 Requirements

- [ ] New test for the `.gitignore` write, in `ensure_devflow_dir`'s home module — covers 19a-WR01
- [ ] New coverage test spanning all 7 `.devflow/`-creating paths — covers 19a-WR01-coverage. **Must include the `sequentagent`/`parallel` path** (`run_agent_blocking`, `main.rs:2417`), which uses *"synthetic, never-persisted state"* and never calls `save_state` — the reason a `save_state` chokepoint was rejected
- [ ] Update `workflow_started_payload_carries_build_provenance` (`main.rs:6867`) — covers 19a-WR02
- [ ] New test in `crates/devflow-core/src/git.rs` for `commit_path`'s no-op-on-identical-content — covers 19b. **No existing test pins this behavior** (verified), so it must be RED first
- [ ] New `.claude/skills/<name>/SKILL.md` + `rules/*.md` — covers 19g; no project skill directory exists today

---

## Equivalence-Proof Procedure (19c–19f)

The split's correctness claim is **absence of behavioral change**, which a passing
suite alone does not establish — a pure move that silently drops a test also
passes. Three independent checks, all required:

1. **Symbol-level diff.** For every moved function, `diff` the before/after body.
   Only leading-whitespace and `use`-path changes are permitted. Any other delta
   is a behavioral change and must be split into its own commit outside this phase.
2. **Test name-set identity.** `cargo test --workspace -- --list` before and after
   must produce the *same set* of test names. A shrunk set means tests were lost
   in the move; a grown set means logic was added.
3. **Pass-count identity.** Full-suite pass count matches the 424 baseline plus
   exactly the new tests this phase adds, enumerated above.

**Plus D-11: all three must hold on a CI run against the branch.** Local-green is
explicitly insufficient — this is the failure class (19i, GAP-2, DEN-29) with the
worst track record on this project.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| The reviewer actually enforces the five D-19 requirements | 19g | The subject under test is an LLM review pass, not a deterministic function; no automated assertion can prove judgment quality | Construct a diff containing a test that only asserts a constant. Run `/gsd-code-review`. Confirm it is flagged, and that the rationale cites the contract rather than generic style. |
| `.devflow/` no longer reaches a downstream user's git history | 19a-WR01 | The end-to-end threat crosses a repository boundary — the automated test proves the `.gitignore` exists; only a real scratch repo proves the sweep stops | In a fresh scratch repo with **no** `.devflow/` entry in `.gitignore`: run a phase, then `git add . && git commit`, then `git log -1 --name-only`. No `.devflow/` paths may appear. This is the exact reproduction from `17-REVIEW.md`. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] Equivalence-proof procedure run and green **in CI**, not only locally (D-11)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
