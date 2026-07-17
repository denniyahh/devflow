---
phase: 15
slug: oss-readiness
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-07-17
---

# Phase 15 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Scope: 15b (OSS Packaging) only — 15a (dogfood enablement) already shipped and validated live.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (workspace: devflow-core + devflow-cli), plus the existing `--help` snapshot integration test |
| **Config file** | none — no `pytest.ini`/`jest.config`; test discovery is Cargo's standard `tests/` convention |
| **Quick run command** | `cargo test -p devflow --test help_snapshot` |
| **Full suite command** | `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` |
| **Estimated runtime** | ~60 seconds (full workspace suite; not re-measured this session) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p devflow --test help_snapshot` (fast CLI/doc-drift guard) plus a manual read-through of the specific doc file touched against its named source file(s)
- **After every plan wave:** Run `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check`
- **Before `/gsd-verify-work`:** Full suite green, `cargo publish --dry-run -p devflow-core` clean, devcontainer build check green (if built)
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 15-01-T1 | 01 | 1 | 15b | — | README matches real CLI surface (gate/logs, per-phase state, no bare `state.json`) | integration | `cargo test -p devflow --test help_snapshot && rg -q 'OPERATIONS\.md' README.md && rg -q 'state-' README.md && ! rg -q 'state\.json' README.md` | ✅ | ✅ |
| 15-01-T2 | 01 | 1 | 15b | T-15-01-SC (medium) | SECURITY.md points to real evidence files, not the phantom `audit.log` | unit | `! rg -q 'audit\.log' SECURITY.md && rg -q 'events\.jsonl' SECURITY.md && rg -q 'state-' SECURITY.md` | ✅ | ✅ |
| 15-01-T3 | 01 | 1 | 15b | — | DEPENDENCIES.md free of decoy-config/phantom-command references | unit | `! rg -q '\.devflow\.yaml' DEPENDENCIES.md && ! rg -q 'devflow confirm' DEPENDENCIES.md && rg -q '1\.2\.0' DEPENDENCIES.md` | ✅ | ✅ |
| 15-REVIEW-CR-01 | — | — | 15b | T-15-01-SC (medium) | `.gitignore` covers all DevFlow runtime-state paths SECURITY.md/ARCHITECTURE.md/README.md claim are ignored (`.devflow/state.json`, `.devflow/events.jsonl`, `.devflow/gates/`) — regression guard for CR-01 (15-REVIEW.md), the leaked-telemetry incident where a `.gitignore` rewrite silently dropped this coverage | integration | `cargo test -p devflow --test gitignore_coverage` | ✅ | ✅ |
| 15-02-T1 | 02 | 1 | 15b | — | ARCHITECTURE.md matches current Stage/hooks/lock/events model, no dead 8-step machine | unit | `! rg -q 'Branching|Executing|Docsing' ARCHITECTURE.md && ! rg -q '\.devflow\.yaml' ARCHITECTURE.md && ! rg -q 'rejectpr' ARCHITECTURE.md && rg -q 'events\.jsonl' ARCHITECTURE.md && rg -q 'Define' ARCHITECTURE.md` | ✅ | ✅ |
| 15-02-T2 | 02 | 1 | 15b | — | quickstart.md free of phantom `init`/config commands | unit | `! rg -q 'devflow init' docs/guides/quickstart.md && ! rg -q '\.devflow\.yaml' docs/guides/quickstart.md && rg -q 'devflow start --phase' docs/guides/quickstart.md` | ✅ | ✅ |
| 15-02-T3 | 02 | 1 | 15b | — | configuration.md reflects no-config-file reality | unit | `! rg -q '\.devflow\.yaml' docs/guides/configuration.md && rg -q 'DEVFLOW_GATE_NOTIFY_CMD' docs/guides/configuration.md && rg -q 'OPERATIONS\.md' docs/guides/configuration.md` | ✅ | ✅ |
| 15-03-T1 | 03 | 1 | 15b | — | CONTRIBUTING.md documents required checks + devcontainer path | unit | `rg -q 'devcontainer' CONTRIBUTING.md && rg -q 'cargo clippy' CONTRIBUTING.md && rg -q 'cargo fmt' CONTRIBUTING.md && test -f CODE_OF_CONDUCT.md` | ✅ | ✅ |
| 15-03-T2 | 03 | 1 | 15b | T-15-03-SC (high) | Devcontainer base image pinned, not `:latest` | unit | `test -f .devcontainer/devcontainer.json && ! rg -q ':latest' .devcontainer/devcontainer.json && rg -q 'mcr.microsoft.com/devcontainers/rust' .devcontainer/devcontainer.json && rg -q 'rustup component add clippy rustfmt' .devcontainer/devcontainer.json` | ❌→delivered this wave (Wave 0 gap closed) | ✅ |
| 15-03-T3 | 03 | 1 | 15b | — | Container-parity CI job builds/tests/lints inside the devcontainer | integration | `test -f .github/workflows/devcontainer.yml && rg -q 'cargo build' .github/workflows/devcontainer.yml && rg -q 'cargo test' .github/workflows/devcontainer.yml && rg -q 'cargo clippy' .github/workflows/devcontainer.yml && rg -q 'devcontainer' .github/workflows/devcontainer.yml` | ❌→delivered this wave (Wave 0 gap closed) | ✅ (static structure only — see Manual-Only for live-CI caveat) |
| 15-04-T1 | 04 | 1 | 15b | T-15-04-SC (medium) | LICENSE-APACHE backs the declared dual license with canonical text, not adapted MIT | unit | `test -f LICENSE-APACHE && rg -q 'Apache License' LICENSE-APACHE && rg -q 'Version 2.0' LICENSE-APACHE && ! rg -q 'Permission is hereby granted, free of charge' LICENSE-APACHE && rg -q 'MIT OR Apache-2.0' Cargo.toml` | ✅ | ✅ |
| 15-04-T2 | 04 | 1 | 15b | — | Both crates package/publish cleanly (dry-run) | integration | `cargo publish --dry-run -p devflow-core && cargo package --workspace` | ✅ | ✅ |
| 15-05-T1 (manual) | 05 | 2 | 15b | T-15-05-SC (high) | Operator holds crates.io token outside repo (`cargo login` / CI secret, never committed) | manual | N/A — see Manual-Only Verifications | manual only | ✅ done — operator ran `cargo login` + real `cargo publish` directly (15-05-SUMMARY.md); no token in any repo-tracked file |
| 15-05-T2 | 05 | 2 | 15b | — | `devflow-core` live on registry post-publish | integration | `cd "$(mktemp -d)" && cargo new _probe --bin >/dev/null 2>&1 && cd _probe && cargo add devflow-core@1.2.0 --dry-run` | ✅ | ✅ re-run live 2026-07-17 — resolves `devflow-core v1.2.0` from crates.io index |
| 15-05-T3 | 05 | 2 | 15b | — | `devflow` (CLI) live on registry post-publish, after core | integration | `cd "$(mktemp -d)" && cargo new _probe2 --bin >/dev/null 2>&1 && cd _probe2 && cargo add devflow@1.2.0 --dry-run` | ✅ | ✅ re-run live 2026-07-17 — resolves `devflow v1.2.0` from crates.io index |
| 15-REVIEW2-WR-01 | — | — | 15b | 15-REVIEW.md WR-01 (this round) | `RUST_LOG` honored regardless of `DEVFLOW_LOG_FORMAT` (json or plain) — regression guard for the WR-01 bug fixed in commit `8672172`, where the json branch hardcoded `LevelFilter::INFO` and never consulted `RUST_LOG` | integration | `cargo test -p devflow --test log_format_env` | ✅ | ✅ |
| 15-REVIEW2-CR-01 | — | — | 15b | 15-REVIEW.md CR-01 (this round) | `.github/workflows/devcontainer.yml`'s `runCmd` fails fast — `set -e` is the first command line, preceding every `cargo` invocation — regression guard for the CR-01 bug fixed in commit `3918792`, where a failing command earlier in the unguarded `bash -c` script could be masked as green by a later passing command | integration | `cargo test -p devflow --test devcontainer_ci_failfast` | ✅ | ✅ |
| 15-REVIEW3-CR-01 | — | — | 15b | 15-REVIEW.md CR-01 (third round) | `devflow` defaults to INFO-level logging (not ERROR-only) when `RUST_LOG` is unset, on both the plain-text and JSON branches — regression guard for the CR-01 bug fixed in commit `50db857`, where the bare `EnvFilter::from_default_env()` silently defaulted to ERROR-only, contradicting the documented "default: info" behavior | integration | `cargo test -p devflow --test log_format_env` | ✅ | ✅ |
| 15-REVIEW3-CR-02 | — | — | 15b | 15-REVIEW.md CR-02 (third round) | Tracing log output reaches **stderr**, never stdout, on both the plain-text and JSON branches — regression guard for the CR-02 bug fixed in commit `55be573`, where `tracing_subscriber::fmt()` never called `.with_writer(std::io::stderr)` on either branch, so logs defaulted to stdout, contradicting `lib.rs`'s documented stderr contract | integration | `cargo test -p devflow --test log_format_env` | ✅ | ✅ |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] A devcontainer build/test check — delivered by 15-03-T3 (`.github/workflows/devcontainer.yml`, container-parity CI running `cargo build && cargo test && cargo clippy`).

*No other Wave 0 gap: the existing `--help` snapshot test (15a) already covers CLI-surface drift; doc-prose accuracy is inherently a read-and-compare activity, not something to force into a new automated test.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Actual `cargo publish` (not dry-run) | 15b — crates.io publish | Requires a crates.io API token held by the operator; not automatable/scriptable | **Done 2026-07-17.** Operator ran `cargo login` then `cargo publish -p devflow-core` (17:39:23Z) followed by `cargo publish -p devflow` (17:40:31Z), leaf-first, directly — after two automated Code-stage attempts both false-positived without actually publishing (see 15-05-SUMMARY.md Decisions Made). Confirmed live via `curl .../api/v1/crates/{devflow-core,devflow}` (both `1.2.0`, `yanked: false`) |
| CI badge / PR gate status rendering | 15b — CI badge | GitHub badge rendering can only be confirmed by viewing the rendered README on GitHub | View README on GitHub after merge; confirm badge renders and links to the real workflow |
| **CR-02 (15-REVIEW.md, third round) — RESOLVED**: `docs/guides/configuration.md:33` and `crates/devflow-core/src/lib.rs:9,34` both claim tracing log output goes to **stderr**; it actually went to **stdout** on both the plain-text and JSON branches — `main.rs`'s `tracing_subscriber::fmt()` builder never called `.with_writer(std::io::stderr)`. | 15b — doc/code accuracy | Was an unresolved implementation defect, not a test-coverage gap. | **Done — commit `55be573`** ("fix(cli): route tracing log output to stderr (CR-02)"). Added `.with_writer(std::io::stderr)` to both branches; docs were already correct (stated intent), so no doc change needed. Updated `log_format_env.rs`'s three existing tests (previously asserting log lines on stdout, per the code's actual pre-fix behavior) to assert log lines land on stderr and are absent from stdout. See Per-Task Map row `15-REVIEW3-CR-02` below. |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (15-05-T1 is the sole manual checkpoint)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (15-03-T3 closes the devcontainer CI gap)
- [x] No watch-mode flags
- [x] Feedback latency < 90s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** mapped — plan-checker verified all tasks carry automated verify or a correctly-typed manual checkpoint (`## VERIFICATION PASSED`); ready for execution

---

## Validation Audit 2026-07-17

| Metric | Count |
|--------|-------|
| Gaps found | 8 |
| Resolved | 8 |
| Escalated | 0 |

**What was audited:** Plans 15-01 through 15-04 (all complete, each with a green SUMMARY.md). Every automated command in the Per-Task Verification Map was re-run against the current tree.

**Root cause of all 8 gaps:** every flagged Automated Command used the idiom `test "$(rg -c PATTERN FILE)" = "0"` to assert zero matches. Under this environment's ripgrep (15.2.0), `rg -c` on zero matches prints nothing and exits 1 — not the literal string `0` — so the comparison evaluated `"" = "0"` (false) even when the underlying file was correct. Confirmed directly (`rg -c 'zzz_no_match' README.md` → exit 1, empty stdout) and cross-checked every flagged file with a direct `rg -n PATTERN FILE` presence check: zero real matches in all 8 cases — the docs themselves were never wrong, only the check script. This same quirk was already independently discovered and worked around ad hoc in three SUMMARY.md files (15-01, 15-02, 15-04) during execution, but the fix hadn't been propagated back into this file.

**Fix applied:** replaced `test "$(rg -c PATTERN FILE)" = "0"` with `! rg -q PATTERN FILE` (checks the zero-match condition via exit code, correct under any ripgrep version) in the Automated Command column for 15-01-T1, 15-01-T2, 15-01-T3, 15-02-T1, 15-02-T2, 15-02-T3, 15-03-T2, 15-04-T1. Every corrected command was re-run end-to-end and confirmed green.

**Bonus fix (same file, discovered while verifying 15-01-T1):** the Automated Command and the Test Infrastructure "Quick run command" both referenced a nonexistent Cargo package name (`devflow-cli`); the CLI crate's real package name is `devflow` (`crates/devflow-cli/Cargo.toml` declares `name = "devflow"`). Corrected both references to `cargo test -p devflow --test help_snapshot`.

**Additionally re-verified (no ripgrep-quirk bug, just stale ⬜ placeholders):** 15-03-T1, 15-03-T3, and 15-04-T2 were run as-written and confirmed green; Status column updated from `⬜ pending` to `✅`.

**Not touched — plan not yet executed:** 15-05-T1/T2/T3 remain `⬜ pending`. Plan 15-05 (crates.io publish) is gated behind a `checkpoint:human-action` task requiring the operator's crates.io API token (see 15-05-PLAN.md Task 1) and has not run yet per `.planning/STATE.md` (`stopped_at: Completed 15-04-PLAN.md`). This is not a Nyquist coverage gap — 15-05-T1 is correctly typed as the sole manual checkpoint, and T2/T3's automated registry-resolution commands are already correctly designed — the wave is simply not yet executed. Phase 15 is not fully complete; re-run `/gsd-validate-phase 15` after 15-05 executes if its commands need auditing.

**No implementation files were modified.** Only `15-VALIDATION.md`'s own command text and status cells changed.

---

## Validation Audit 2026-07-17 (re-confirmation)

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

**What was audited:** Independent re-run of all 11 Automated Commands for 15-01 through 15-04 (the corrected commands from the prior audit above), plus a re-check of `.planning/STATE.md` and the phase directory for a 15-05-SUMMARY.md.

**Result:** All 11 commands still pass, including `cargo publish --dry-run -p devflow-core && cargo package --workspace` (15-04-T2, re-run live). No drift since the prior audit. 15-05 still has no SUMMARY.md and `.planning/STATE.md` still reports `stopped_at: Completed 15-04-PLAN.md` — 15-05 remains unexecuted, unchanged from the previous audit's finding. No new gaps; nothing to fix.

---

## Validation Audit 2026-07-17 (15-05 closure + post-review-fix re-verification)

| Metric | Count |
|--------|-------|
| Gaps found | 1 |
| Resolved | 1 |
| Escalated | 0 |

**What was audited:** All 14 Automated Commands across 15-01 through 15-05 (re-run live), plus the three post-review fix commits (`5a8cbad` WR-01, `e7a35b7` WR-02, `d021e3a` CR-01) that landed after the prior re-confirmation audit. `.planning/STATE.md` is stale (still shows `stopped_at: Completed 15-04-PLAN.md`) — the phase's actual state was read from git history (`47db27d` "mark 15-05 complete", `15-05-SUMMARY.md`, `15-VERIFICATION.md` status: passed, 20/20) and `15-REVIEW.md`, not from STATE.md.

**Gap found:** 15-05-T1/T2/T3 in the Per-Task Verification Map still read `⬜ pending — plan 15-05 not yet executed`, but plan 15-05 completed on 2026-07-17 (`devflow-core` published 17:39:23Z, `devflow` published 17:40:31Z, both confirmed live via `curl .../api/v1/crates/*` and `cargo add --dry-run` resolving from the live registry index). This was a stale-status gap in this file, not a missing-coverage gap — 15-05-T2/T3's automated commands were already correctly designed; they simply hadn't been re-run since publish.

**Fix applied:** Re-ran 15-05-T2's and 15-05-T3's automated commands live (`cargo add devflow-core@1.2.0 --dry-run` / `cargo add devflow@1.2.0 --dry-run` in scratch crates) — both resolve cleanly from the crates.io index. Updated their Status cells to ✅ and 15-05-T1 (manual) to ✅ done, with the Manual-Only Verifications table's publish row updated to record the actual completion (operator ran `cargo login` + real `cargo publish`, leaf-first, after two automated Code-stage attempts false-positived — see `15-05-SUMMARY.md` Decisions Made).

**Post-review-fix re-verification (no VALIDATION.md gap — spot-check only):** `15-REVIEW.md` (2026-07-17T18:12:51Z) flagged CR-01 (critical: `.gitignore` didn't cover per-phase runtime files, leaking real telemetry into git), WR-01 (devcontainer CI step missing `cargo fmt --check`), and WR-02 (devcontainer named volumes not scoped per worktree, collision risk). All three are fixed in commits `d021e3a`, `5a8cbad`, `e7a35b7` respectively. Re-ran 15-03-T2 and 15-03-T3's existing Automated Commands after these fixes: both still pass, and `rg -q 'cargo fmt' .github/workflows/devcontainer.yml` confirms WR-01's fix is present. IN-01 (info: devcontainer lacks `gh` CLI) was left unfixed — info-severity, no Per-Task Map row references it, not a Nyquist gap. No `15-REVIEW-FIX.md` was written for these three fixes (unlike Phase 13/14's pattern of a dedicated fix doc); noting this as a documentation-hygiene gap outside Nyquist validation's scope, not something this audit corrects.

**Also re-confirmed:** 15-01 through 15-04's 11 Automated Commands (unchanged since the prior re-confirmation audit) all still pass, including `cargo publish --dry-run -p devflow-core && cargo package --workspace` (15-04-T2).

**No implementation files were modified.** Only `15-VALIDATION.md`'s own status cells and audit trail changed.

---

## Validation Audit 2026-07-17 (gsd-nyquist-auditor — CR-01 automated coverage gap)

| Metric | Count |
|--------|-------|
| Gaps found | 1 |
| Resolved | 1 |
| Escalated | 0 |

**What was audited:** The single outstanding MISSING gap flagged for adversarial coverage: no automated test asserted `.gitignore` actually covers `.devflow/state.json`, `.devflow/events.jsonl`, and `.devflow/gates/` — the exact three paths that regressed once already (CR-01 in `15-REVIEW.md`: commit `d021e3a` fixed a leaked-telemetry incident by rewriting `.gitignore`'s `.devflow/` patterns, but the rewrite silently dropped these three paths; a second code review caught it, fixed again in `9b2fac4`). Prior "coverage" (15-01-T2) only grepped SECURITY.md's prose, not the actual `.gitignore` patterns — exactly how the regression slipped through undetected.

**Gap found:** `no_test_file` — no integration test exercised `git check-ignore` against these paths.

**Test written:** `crates/devflow-cli/tests/gitignore_coverage.rs`, following this repo's existing `Command`-shelling convention from `crates/devflow-cli/tests/help_snapshot.rs`. Runs `git check-ignore -v .devflow/state.json .devflow/events.jsonl .devflow/gates/probe.json` and asserts exit success (all three paths matched). Pure pattern matching — no fixture files created or cleaned up.

**Debug iteration (1/3):** First run failed — `git check-ignore` exited 1 with empty stdout/stderr. Root cause: `cargo test` runs test binaries with cwd set to the crate directory (`crates/devflow-cli/`), not the repo root, and `.gitignore`'s `.devflow/state.json` pattern is anchored (no leading `**/`), so it does not match when git is invoked from a subdirectory two levels deep. Confirmed manually: running the identical `git check-ignore` command from repo root exits 0 with all three patterns matching; running from `crates/devflow-cli/` exits 1. This was a test-harness bug, not an implementation bug — `.gitignore` itself is correct (confirmed via the repo-root run). Fixed by adding `current_dir(repo_root())` to the `Command`, resolving `CARGO_MANIFEST_DIR/../..` via `canonicalize()`. Re-ran: test passes.

**Resolution:** FILLED. `cargo test -p devflow --test gitignore_coverage` passes and would fail if `.gitignore` ever again drops coverage of these three paths, closing the automated-guard gap that let CR-01 regress silently the first time.

**No implementation files were modified.** `.gitignore` was read-only reference (already correct per `9b2fac4`); only the new test file and this file's own audit trail changed.

---

## Validation Audit 2026-07-17 (second review round — WR-01/CR-01 coverage gap)

| Metric | Count |
|--------|-------|
| Gaps found | 2 |
| Resolved | 2 |
| Escalated | 0 |

**What was audited:** `.planning/STATE.md` and prior audit entries in this file only covered plans 15-01 through 15-05 and the first review round (`15-REVIEW.md` findings CR-01/WR-01/WR-02 fixed in `d021e3a`/`5a8cbad`/`e7a35b7`). Git history showed a *second* review round landed after that: `2f09710` rewrote `15-REVIEW.md` in place (new timestamp `2026-07-17T18:50:51Z`) with two new findings, both already fixed before this audit ran: CR-01 (`.github/workflows/devcontainer.yml`'s `runCmd` had no `set -e`, so `bash -c` would report the exit code of only the *last* command — a failing `cargo test`/`cargo clippy` earlier in the block could go green) fixed in commit `3918792`; WR-01 (`RUST_LOG` silently ignored whenever `DEVFLOW_LOG_FORMAT=json`, because the json branch called `tracing_subscriber::fmt().json().init()` with a hardcoded `LevelFilter::INFO` while the plain-text branch honored `RUST_LOG`) fixed in commit `8672172`. Neither fix had a Per-Task Map row or any test coverage — confirmed via `rg` that no test file referenced `DEVFLOW_LOG_FORMAT`, `RUST_LOG`, `set -e`, or `devcontainer.yml` before this audit.

**Gaps found:** Both `no_test_file` (MISSING) — genuine coverage gaps, not stale-status issues like the prior two audit rounds. `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` were all green going in; the gap was purely the absence of regression guards for the two review findings.

**Test written (WR-01):** `crates/devflow-cli/tests/log_format_env.rs`. Drives the built `devflow` binary against a project with a legacy `.devflow/state.json`, which makes `workflow::migrate_legacy_state` emit a deterministic `debug!("migrated legacy state.json to ...")` line. Two tests: (1) `DEVFLOW_LOG_FORMAT=json RUST_LOG=debug` asserts the DEBUG line reaches stdout (`tracing_subscriber::fmt()` writes to stdout by default — confirmed via the test's own discovery, not an assumption); (2) `DEVFLOW_LOG_FORMAT=json` with `RUST_LOG` unset asserts the DEBUG line is absent. Together these prove `RUST_LOG` is actually consulted on the json branch, not just that the code compiles.

**Test written (CR-01):** `crates/devflow-cli/tests/devcontainer_ci_failfast.rs`. Line-parses the `runCmd: |` block scalar in `.github/workflows/devcontainer.yml` (no YAML dependency) and asserts `set -e` is literally the first command line and precedes every `cargo` invocation — not a substring grep, which the auditor correctly identified as insufficient (a grep for `set -e` anywhere in the file would still pass if a future edit moved it below a `cargo` line, reintroducing the exact bug). Verified as a real regression guard by temporarily reordering `set -e` after `cargo build --workspace` in the working tree — the test failed as expected — then restoring the file (confirmed via `git status --porcelain` showing the workflow file untouched afterward).

**Debug iterations:** `log_format_env.rs` took 1 iteration — initial version used a nonexistent `--project` flag and asserted on stderr instead of stdout; both corrected. `devcontainer_ci_failfast.rs` passed on first run.

**Verification performed independently by the orchestrator (not just the auditor's self-report):** re-ran both new tests (`cargo test -p devflow --test log_format_env` — 2 passed; `cargo test -p devflow --test devcontainer_ci_failfast` — 1 passed), `cargo clippy --workspace -- -D warnings` (clean), `cargo fmt --check` (clean), and `git status --porcelain` (only the two new test files untracked — no implementation files modified). Also read both test files in full to confirm they exercise real behavior (actual binary invocation / actual file-position assertion) rather than shallow presence checks.

**Resolution:** FILLED. Per-Task Verification Map rows `15-REVIEW2-WR-01` and `15-REVIEW2-CR-01` added, both ✅.

**No implementation files were modified.** Only the two new test files and this file's own audit trail changed.

---

## Validation Audit 2026-07-17 (third review round — CR-01 log-level default coverage gap; CR-02 escalated)

| Metric | Count |
|--------|-------|
| Gaps found | 1 |
| Resolved | 1 |
| Escalated | 1 |

**What was audited:** Git history showed a *third* review round landed after the last audit above: `2108161` rewrote `15-REVIEW.md` in place (new timestamp `2026-07-17T19:08:30Z`) re-reviewing the same four files, with the prior round's CR-01/WR-01 confirmed fixed and three new findings raised: CR-01 (`configuration.md`/`lib.rs` claimed `RUST_LOG` defaults to `info`, but `main.rs` built the filter via bare `EnvFilter::from_default_env()`, which defaults to `ERROR`-only when unset), CR-02 (same docs claim log output goes to stderr; it actually goes to stdout — no `.with_writer(...)` override anywhere in `main.rs`), and WR-01 (quickstart.md's "Build from source" section ran `cargo install devflow`, which installs from the registry, not from a local clone). Two follow-up commits landed before this audit ran: `50db857` fixed CR-01 (switched to `try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))` on both branches) and `0f82caa` fixed WR-01 (quickstart now does `git clone` + `cd` + `cargo install --path crates/devflow-cli`, matching `scripts/install.sh`'s existing fallback). CR-02 has no fix commit — confirmed via `git status` (clean) and direct inspection of `main.rs` (neither branch calls `.with_writer(std::io::stderr)`).

**Gap found:** `no_test_file` (MISSING) for the CR-01 fix. The existing `log_format_env.rs` test `rust_log_default_suppresses_debug_under_json_log_format` only asserted DEBUG-level output is absent by default — that was true both before and after the fix (default changed from ERROR to INFO, and INFO still suppresses DEBUG), so it provided zero regression coverage for the actual behavior change. A revert of `50db857` back to the bare `from_default_env()` would leave the whole suite green.

**Gap resolved:** `gsd-nyquist-auditor` extended `crates/devflow-cli/tests/log_format_env.rs` with `rust_log_unset_still_shows_info_level_logs_by_default`, which shells `devflow gate approve` against a project with a single open gate (a real, unconditional `info!("gate response written for phase 15 ship: approved=true")` call in `Gates::respond`) with `RUST_LOG`/`DEVFLOW_LOG_FORMAT` both unset, and asserts the INFO-level line reaches stdout. The auditor adversarially verified real coverage: temporarily reverted `50db857` in a scratch working-tree state, confirmed the new test fails with the expected message, then `git checkout --` restored `main.rs` before returning. Added Per-Task Map row `15-REVIEW3-CR-01`.

**Independently re-verified by the orchestrator (not just the auditor's self-report):** re-ran `cargo test -p devflow --test log_format_env` (3 passed), `cargo test --workspace` (full suite green), `cargo clippy --workspace -- -D warnings` (clean), `cargo fmt --check` (clean), and read the new test's diff in full to confirm it exercises real binary output rather than a shallow presence check.

**WR-01 (quickstart.md fix):** No test created — per this file's own Wave 0 convention ("doc-prose accuracy is inherently a read-and-compare activity, not something to force into a new automated test"), and manually confirmed correct via `git show 0f82caa`.

**CR-02 escalated, not resolved — requires a follow-up code or doc fix, not a Nyquist test:** `configuration.md`/`lib.rs` still claim stderr; `main.rs` still writes to stdout on both branches. This is a live, unfixed implementation/doc mismatch (an operator following `lib.rs`'s own worked example — `DEVFLOW_LOG_FORMAT=json RUST_LOG=info devflow status 2>log.json` — gets an empty `log.json`). Nyquist validation does not modify implementation files and will not write a test that encodes the current stdout behavior as "correct," since that would validate the bug rather than guard against it. Added to Manual-Only Verifications as an open item pointing back to `15-REVIEW.md` CR-02 for the actual fix. **This phase has an outstanding gap that Nyquist validation cannot close — recommend a follow-up `/code-review` or direct fix before treating Phase 15 as fully clean.**

**No implementation files were modified by this audit.** Only `crates/devflow-cli/tests/log_format_env.rs` (new test) and this file's own Per-Task Map row, Manual-Only table, and audit trail changed.

---

## Validation Audit 2026-07-17 (CR-02 direct fix — escalation closed)

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 1 (escalated item, not a Nyquist gap) |
| Escalated | 0 |

**Context:** `/gsd-execute-phase 15 --gaps-only` was run as the loop's fix command for the CR-02 escalation above. `gsd-tools query phase-plan-index 15` confirmed zero plans with `gap_closure: true` and zero incomplete plans (all of 15-01..15-05 already carry a SUMMARY.md) — there was no gap-closure plan for `--gaps-only` to execute, consistent with the prior audit's own conclusion that CR-02 is "not a Nyquist gap" and needs "a follow-up `/code-review` or direct fix" rather than a gap-closure plan cycle.

**Direct fix applied (commit `55be573`):** Read `main.rs`'s tracing setup (lines 285-300) and confirmed neither branch called `.with_writer(...)` — `tracing_subscriber::fmt()` defaults to stdout. Cross-checked `lib.rs:9` ("log output goes to **stderr** so stdout remains available for agent output") and the worked example at `lib.rs:37` (`DEVFLOW_LOG_FORMAT=json RUST_LOG=info devflow status 2>log.json`, which only makes sense if logs go to stderr) — both treat stderr as the deliberate, documented design intent, not an error to be reconciled by changing the docs. Grepped all non-test `stdout` usages across `crates/` and confirmed none of them depend on DevFlow's own tracing macros landing on DevFlow's own stdout (all real stdout-capture call sites capture *other* spawned agent subprocesses; the CLI's own user-facing output goes through direct `println!`/`stdout().write_all()`, unrelated to the tracing subscriber). This confirmed the code, not the docs, was wrong, and that the fix was safe and isolated.

Added `.with_writer(std::io::stderr)` to both the plain-text and JSON `tracing_subscriber::fmt()` branches in `main.rs`.

**Test update:** `log_format_env.rs`'s three existing tests previously asserted log lines on stdout, with an explicit comment ("tracing output... share stdout — assert against stdout, not stderr") documenting the pre-fix bug as the expected behavior. Updated `run_status()` to return `(stdout, stderr)` instead of stdout alone, and updated all three tests to assert log lines land on stderr AND assert their absence from stdout (catching a regression in either direction). Added Per-Task Map row `15-REVIEW3-CR-02`.

**Verification:** `cargo test -p devflow --test log_format_env` (3 passed), `cargo test --workspace` (full suite green, 220+ tests), `cargo clippy --workspace -- -D warnings` (clean), `cargo fmt --check` (clean).

**No republish:** This fix lands after the 2026-07-17 crates.io publish (15-05) without a version bump or republish, consistent with how the same round's CR-01/WR-01 fixes (`50db857`, `0f82caa`) and the second round's fixes (`d021e3a`, `5a8cbad`, `e7a35b7`) were handled — none of those triggered a republish either. Version bumping is owned by DevFlow's Ship-stage tooling on the next real release, not by validation fixes.

**No other implementation files were modified.** Only `crates/devflow-cli/src/main.rs`, `crates/devflow-cli/tests/log_format_env.rs`, and this file's own Manual-Only table, Per-Task Map row, and audit trail changed.
