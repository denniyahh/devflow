---
phase: 29
slug: release-cut-executor-observe-then-act-within-the-repo-s-rules
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: false
wave_0_complete: true
created: 2026-07-31
validated: 2026-07-31
---

# Phase 29 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by `/gsd-plan-phase 29` from `29-RESEARCH.md` § Validation Architecture.
> Audited by `/gsd-validate-phase 29` on 2026-07-31, **mid-arc**: wave 1 of 6
> executed (29-01 only). Every map row below was re-run live — no row's status is
> carried over from an executing agent's self-report.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` — workspace-standard `#[test]` / `#[cfg(test)] mod tests`, integration tests under `crates/*/tests/`. Direct precedent for this command family: `crates/devflow-cli/tests/release_check.rs` |
| **Config file** | none — conventions live in `CONTRIBUTING.md` § Testing and `.claude/skills/ai-change-acceptance/rules/change-acceptance.md` |
| **Quick run command** | `cargo test -p devflow-core --features test-support --lib <filter>` / `cargo test -p devflow --test <name>` — see the two traps below |
| **Full suite command** | `cargo test --workspace`, plus `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — all three must be clean (`ai-change-acceptance` requirement 4) |
| **Estimated runtime** | ~40 seconds full workspace; <10 seconds package-scoped |

### Command traps confirmed during this audit

1. **`-p devflow-core` needs `--features test-support`.** Without it, the
   pre-existing `tests/devflow_dir_gitignore.rs` and `tests/monitor_e2e.rs`
   integration binaries fail to compile (`devflow_core::test_support` is behind
   `#[cfg(any(test, feature = "test-support"))]`, and package-scoped runs skip
   Cargo's workspace feature unification). Pre-existing, unrelated to Phase 29 —
   recorded in `29-01-SUMMARY.md` § Issues Encountered. Adding `--lib` also
   sidesteps it.
2. **libtest OR-matches multiple filters.** The seeded command
   `cargo test -p devflow-core release_observe -- tag` returns **48 passed**, not
   the 9 tag tests — the trailing `tag` is a second filter OR'd with the first, so
   it sweeps in all of `version::tests::*…tag…`. Every command in the map below has
   been narrowed to a single fully-qualified filter and its count asserted. This is
   the same family of false-signal as the `--exact` bare-name trap: **assert on the
   `N passed` count, never on exit 0.**

---

## Sampling Rate

- **After every task commit:** `cargo test -p devflow-core --features test-support --lib <new_module_name>` (or `-p devflow --test <name>` for CLI-side tasks)
- **After every plan wave:** `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --check`
- **Before `/gsd-verify-work`:** full suite green, **plus** one real read-only `devflow release status` run against this repo's actual state (safe — 29a is pure observation)
- **Max feedback latency:** 90 seconds

**Escalation rule carried from Phase 26:** a green suite is necessary but not
sufficient here. Phase 26 scored 11/11 verification with 763 passing tests while
carrying twelve Criticals, every one found by a human reading code and none by a
test. For 29c's irreversible steps, adversarial review is the primary gate; the
suite is the backstop.

---

## Per-Task Verification Map

The phase's planned arc is 7 plans across 6 waves. **Wave 1 (29-01) is executed;
waves 2–6 (29-02 … 29-07) are not.** The map is therefore split: Part A is
delivered behavior, re-verified live; Part B is behavior whose code does not exist
yet. Part B rows are **implementation gaps, not validation gaps** — see the
disposition note below.

### Part A — delivered behavior (29-01, wave 1)

Every command below was executed during this audit on 2026-07-31. Counts are the
asserted signal.

| # | Unit | Behavior | Automated Command | Result |
|---|------|----------|-------------------|--------|
| A1 | 29a | Tag-ref classification: annotated vs lightweight vs absent, from real `git ls-remote` output shapes | `cargo test -p devflow-core --features test-support --lib release_observe::tests::classify_tag_refs` | ✅ 4 passed |
| A2 | 29a | Signed-tag classification: present / absent-unsigned / absent-lightweight / **undetermined→Unreachable with the reason carried** | `… --lib release_observe::tests::classify_signed_tag` | ✅ 5 passed |
| A3 | 29a | crates.io HTTP classification: 200→Present, 404→Absent, 000 and all other codes→Unreachable naming the code | `… --lib release_observe::tests::classify_http_status` | ✅ 4 passed |
| A4 | 29a | Multi-crate combination: two-present→Present, present+absent→Absent, **any-unreachable dominates**, empty slice→Unreachable | `… --lib release_observe::tests::combine_crate` | ✅ 4 passed |
| A5 | 29a | The `unreachable ≠ absent` invariant itself, asserted directly | `… --lib release_observe::tests::unreachable_is_never_absent` | ✅ 1 passed (within A-block) |
| A6 | 29a | CLI end-to-end: no-remote→Unreachable-not-Absent, reachable-but-absent-tag→warn, summary line names version + count | `cargo test -p devflow --test release_status` | ✅ 4 passed, 2 ignored |
| A7 | 29a | `devflow release --check` (the pre-existing 20d preflight) is byte-for-byte unchanged | `cargo test -p devflow --test release_check` | ✅ 10 passed |
| A8 | 29b | Version-bump / changelog helper suites still green and untouched by this phase | `… --lib version` / `… --lib hooks` | ✅ 53 + 16 passed |
| A9 | 29a | **Live** oracles against real infrastructure: this repo's real signed `origin` tags, and the real crates.io registry | `cargo test -p devflow --test release_status -- --ignored` | ✅ 2 passed |
| A10 | — | Full workspace gate | `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --check` | ✅ 0 failed / clean / clean |

**Note on A1–A2's test type.** The seeded map called this row *"unit (real git
fixture, no network)"*. As delivered it is a pure string classifier over injected
`git ls-remote` output (A1–A2) **plus** real-git-fixture integration tests in
`crates/devflow-cli/tests/release_status.rs` (A6). The behavior is covered; the
seeded test-type label was imprecise, not the coverage.

### Part B — blocked on missing implementation

Each row's target module or symbol was checked by hand during this audit and is
genuinely absent. These are not fillable by test generation.

| # | Unit | Behavior | Blocked by | Absence verified |
|---|------|----------|-----------|------------------|
| B1 | 29a | Six-question observer — all six release questions answered, each three-valued | 29-02 (wave 2) | `release_status` wires **2 of 6** (`crates/devflow-cli/src/commands.rs:2288-2297`); `ReleaseStep::ALL` declares six variants but `VersionBumped`, `ChangelogWritten`, `ReleasePrMerged`, `SyncMerged` have no oracle |
| B2 | 29b | Merge-method policy: selects `merge` for sync-back even when `squash` is allowed; refuses loudly when the required method is absent from the discovered set | 29-03 (wave 2) | `crates/devflow-core/src/release_policy.rs` absent; `resolve_merge_method`, `discover_allowed_merge_methods`, `MergeIntent` → **0 matches** repo-wide |
| B3 | 29b | `yes_release` authorization resolved from flag OR `devflow.toml` OR env, defaulting false, never consumed | 29-03 (wave 2) | `fn yes_release` → **0 matches** |
| B4 | 29b | Executor walks the six-step set in order, observe-then-act; Unreachable stops the walk; open PR reported in flight with no duplicate; refuses without a mandate without prompting | 29-04 (wave 3) | `crates/devflow-core/src/release_execute.rs` and `crates/devflow-cli/tests/release_cut.rs` both absent |
| B5 | 29b | Version bump + changelog written via the **existing** `version::write_version` / `changelog_sections` / `ship::prepend_changelog` chain, in a scratch worktree removed on every exit path | 29-05 (wave 4) | `release_execute.rs` absent. (A8 proves those helpers still pass; nothing yet *reuses* them.) |
| B6 | 29b | Sync-back reproduces every `scripts/sync-main-to-develop.sh` check — ancestor short-circuit, `-X ours`, byte-identical tree verification gating the push | 29-06 (wave 5) | `release_execute.rs` absent |
| B7 | 29c | Signed-tag command form matches `CONTRIBUTING.md` exactly (argv order, `-c user.signingkey=`, `-s`, message form) | 29-07 (wave 6) | `crates/devflow-core/src/release_publish.rs` absent |
| B8 | 29c | `publish_order()` consumed in the exact returned order — never re-sorted, never a hardcoded crate name | 29-07 (wave 6) | `release_publish.rs` absent; no `publish_order_respected` test exists. (`release_observe.rs:359` already consumes `publish_order` correctly for *observation*; the ordering-on-publish property is what is missing.) |
| B9 | 29c | IN-01 regression: a pre-existing local unsigned `v{version}` tag from `hooks_after_ship` is detected and classified **before** `git tag -s` runs | 29-07 (wave 6) | `release_publish.rs` absent; no `tag_namespace_collision` test exists |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky · ⛔ blocked on implementation*

**Disposition — why the nyquist auditor was not spawned.** Every Part B row targets
a module that does not exist. `gsd-nyquist-auditor` is constrained never to modify
implementation files, so it cannot make such a test compile; and in Rust a single
non-compiling test target fails the entire workspace build, `scripts/check.sh`, and
the pre-push hook. Generating those tests would convert an honest "not built yet"
into a broken build. The auditor spawn was therefore skipped deliberately, not
silently — this paragraph is the record. Part B doubles as ready-made acceptance
criteria for waves 2–6.

---

## Wave 0 Requirements

- [x] `crates/devflow-core/src/release_observe.rs` — **created** by 29-01, 18 unit tests
- [ ] `crates/devflow-core/src/release_publish.rs` — blocked on 29-07
- [x] Test-module home for the `gh` / `curl` invocation boundary — `#[cfg(test)] mod tests` alongside `release_observe`, matching `git.rs` / `hooks.rs` / `preflight.rs`
- [x] Integration-test home with real git fixtures — `crates/devflow-cli/tests/release_status.rs` (`init_repo`/`write_workspace_fixture` helpers)
- [ ] Shared fixture helper reproducing the `hooks_after_ship`-style local unsigned `v{version}` tag for the IN-01 collision regression — blocked on 29-07
- [x] Framework install: **none** — `cargo test` already configured workspace-wide

---

## Manual-Only Verifications

| Behavior | Unit | Why Manual | Status |
|----------|------|------------|--------|
| crates.io `/api/v1/crates/{name}/{version}` 200/404 semantics still hold | 29a | External API contract; can rot silently | **Satisfied and now automated** — `crates_published_live_smoke` (`#[ignore]`) re-run live during this audit, passing against the real registry |
| Signed-tag verification via `gh api .../git/tags/<sha>` returns a usable `.verification` field | 29a | GitHub server-side contract; the tag-object-vs-peeled-sha distinction is invisible to a fake | **Satisfied and now automated** — `signed_tag_live_smoke` (`#[ignore]`) re-run live during this audit, passing against this repo's real `origin` |
| `gh pr merge --auto <method>` actually waits for green checks and then merges with the requested method | 29b | GitHub server-side behavior; faking it removes the property under test | **Not yet performable** — blocked on 29-05/29-06 (no executor exists to run the merge step) |
| Real `git tag -s` → `git push origin vX.Y.Z` → `cargo publish` core-then-cli, including interaction with `scripts/hooks/pre-push` | 29c | Genuinely irreversible against the live world | **Not yet performable** — blocked on 29-07. `checkpoint:human-verify` gate; line-by-line review before the run, not a test |

> The two "not yet performable" rows are recorded here because they are
> manual-**by design**, but they are **not** claims that a human has verified them.
> Nothing in Part B has been verified by any means.

---

## Validation Sign-Off

- [x] All **delivered** tasks have `<automated>` verify — 29-01's every coverage claim re-run live, none taken on trust
- [x] Sampling continuity: no 3 consecutive delivered tasks without automated verify
- [x] Wave 0 covers all ❌ references **for the delivered wave**; the rest await their own waves
- [x] No watch-mode flags
- [x] Feedback latency < 90s (full workspace ~40s measured)
- [x] Every `unreachable ≠ absent` arm has an explicit negative test — A2, A4, A5, A6
- [ ] `nyquist_compliant: true` — **not set.** 9 of 19 mapped behaviors have no automated verification because their implementation does not exist yet.

**Approval:** PARTIAL — validated against what wave 1 delivered.

---

## Validation Audit 2026-07-31

| Metric | Count |
|--------|-------|
| Map rows audited | 19 |
| Covered (Part A, re-run live) | 10 |
| Blocked on missing implementation (Part B) | 9 |
| Gaps found that were *fillable* | 0 |
| Resolved by auditor | 0 (auditor deliberately not spawned — see Disposition) |
| Escalated to operator | 9 |

**Audit method.** Every Part A command was executed, not read. Every Part B
absence was confirmed by `ls`/`rg` for the specific module and symbol, not
inferred from the missing SUMMARY files. Two seeded commands were corrected for
false-signal (multi-filter OR-matching; `-p devflow-core` needing
`--features test-support`).

**Outcome.** Phase 29 is mid-arc — wave 1 of 6. The end state
`status: validated` + `nyquist_compliant: false` is the designed PARTIAL
representation per audit-milestone §5.5. Setting `nyquist_compliant: true`
because zero gaps were *fillable* would overclaim coverage.

**Operator action.** Part B closes by finishing the arc, not by generating tests:
run `/gsd-execute-phase 29` (waves 2→6, plans 29-02 … 29-07, all present and
unexecuted), then re-run `/gsd-validate-phase 29`. Precedent: on Phase 26 the same
Part B split was written mid-arc, the operator finished the arc, and re-running
validate-phase returned **zero** gaps — every Part B row came back green because
each plan brought tests for the exact rows Part B had named.

> Note for the re-audit: **match rows on behavior, not on the seeded filter
> string.** Phase 26's Part B rows were seeded as `git::tests::…` and landed in a
> new `release` module as `release::tests::…`; a covered row then reads as a
> missing test. Confirm every name via `cargo test -- --list` first.

---

## Fix-Loop Attempt 2026-07-31 — `/gsd-execute-phase 29 --gaps-only`

DevFlow routed the PARTIAL verdict above into its gap-closure fix loop. **The
command matches zero plans and is a no-op here.** Recorded so the next iteration
does not re-derive it.

`--gaps-only` selects plans carrying `gap_closure: true` in frontmatter
(`execute-phase.md` § filtering). Applying that filter to Phase 29:

| Plan | `has_summary` | `gap_closure` | Selected |
|------|---------------|---------------|----------|
| 29-01 | ✅ (executed) | — | no — already complete |
| 29-02 … 29-07 | ❌ | **absent** | no — not gap-closure plans |

`rg gap_closure .planning/phases/29-*/` → **0 matches**. Selection set is empty →
the orchestrator's documented behavior is "No matching incomplete plans" → exit.

**Why the precondition is unmet.** The gap-closure cycle is
`/gsd-plan-phase 29 --gaps` (reads `29-VERIFICATION.md`) → mints `gap_closure: true`
plans → `--gaps-only` executes them. Phase 29 has **no `29-VERIFICATION.md`** —
`/gsd-verify-work` has never run, because the phase is mid-arc at wave 1 of 6.
What produced the PARTIAL was `/gsd-validate-phase`, and all 9 of its findings are
Part B rows meaning *"the implementation does not exist yet"* — not a
gap-closure-shaped defect. Minting gap plans for them would duplicate
29-02 … 29-07, which already exist and already carry tests for those exact rows.

**Re-verified live during this fix-loop attempt (not carried over):**

- `cargo test --workspace` → **0 failed** across every target. No regression is
  hiding behind the PARTIAL; the verdict is purely missing implementation.
- `crates/devflow-core/src/` contains `release_observe.rs` only —
  `release_policy.rs`, `release_execute.rs`, `release_publish.rs` still absent.
- `resolve_merge_method` / `discover_allowed_merge_methods` / `fn yes_release` →
  **0 matches** repo-wide. `crates/devflow-cli/tests/` has `release_check.rs` and
  `release_status.rs` only; `release_cut.rs` absent.

Part B is unchanged and remains closable only by finishing the arc.

**Escalation — operator decision required.** Two routes, and this loop cannot
pick between them:

1. **Finish the arc** — `/gsd-execute-phase 29` (no flag) runs waves 2→6, plans
   29-02 … 29-07. This is the route § Operator action names, and Phase 26's
   precedent says re-running `/gsd-validate-phase 29` afterward returns zero
   gaps. Note 29-07 is `autonomous: false` and carries a
   `checkpoint:human-verify` gate over genuinely irreversible steps (`git tag -s`,
   `cargo publish`) — it will not run unattended.
2. **Close Phase 29 as PARTIAL** at wave 1, and re-scope waves 2–6.

Route 1 is a six-wave build, not a fix; running it from inside a fix loop would
be a scope expansion this loop was not authorized to make. Hence: **failed**, with
the arc left intact and nothing fabricated.
