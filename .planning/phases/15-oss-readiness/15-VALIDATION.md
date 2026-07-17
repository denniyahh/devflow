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
