---
phase: 20
slug: release-correctness-operator-control
status: verified
threats_open: 0
asvs_level: 1
created: 2026-07-23
---

# Phase 20 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| release automation → workspace Cargo.toml | `VersionBump` rewrites a file `cargo publish` trusts as the source of truth for dependency versions (20a) | version strings |
| operator CLI → another operator's in-progress worktree | `cleanup --force` can force-remove a worktree an alive monitor is still writing into (20b) | filesystem state |
| operator CLI arg → pipeline state machine | `--until <stage>` is free-form operator input selecting a stop point (20c) | stage identifier |
| release preflight → local signing key state | `release --check` reads `git config` and probes `ssh-add`/`gpg` state and key files (20d) | signing-key fingerprint |
| operator CLI → terminal Ship transition | `devflow ship --force` can drive a phase through the terminal, irreversible after-ship hook batch (20e) | workflow state, gate response |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-20-01a | Tampering | `write_version` inline-table pass (20a) | high | mitigate | Rewrite anchored strictly to `path=`/`version=` tokens on local-path entries only; `write_version_leaves_third_party_version_only_dep_untouched` + `write_version_rewrites_self_pin_regardless_of_key_order` regression tests | closed |
| T-20-01b | Denial of Service | `write_version` on malformed/empty section | low | mitigate | `write_version_no_ops_on_missing_workspace_dependencies_section` proves no panic, clean no-op | closed |
| T-20-SC-01 | Tampering | package installs (20a) | low | accept | Zero new external dependencies — no install task in this plan | closed |
| T-20-02 | Tampering | `cleanup --force` worktree removal (20b) | high | mitigate | Fail-closed liveness gate: refuses on live agent (any monitor state, incl. Unknown/Stuck) OR active monitor; extended in the post-review fix pass (CR-02) to also refuse on `state.stopped` (a `--until`-parked phase). `cleanup_force_refuses_on_live_agent_unknown_monitor` + `cleanup_force_refuses_on_dead_monitor_live_agent` regression tests | closed |
| T-20-02b | Denial of Service | transient `Directory not empty` race on dead-phase removal | medium | mitigate | Bounded-backoff retry around `worktree::remove` for Stuck/Unknown phases | closed |
| T-20-SC-02 | Tampering | package installs (20b) | low | accept | Zero new external dependencies | closed |
| T-20-03 | Tampering (input) | `--until <stage>` parsing (20c) | low | accept | Reuses existing `Stage: FromStr` parser (rejects unknown stages); `--until ship` additionally rejected as a semantic no-op | closed |
| T-20-03b | Denial of Service | orphaned monitor on stop (20c) | medium | mitigate | Stop path emits `workflow_finished`, clears `monitor_pid`/`gate_pending`, never calls `launch_stage`; `start_until_plan_halts_cleanly` + `reconcile_phase_ignores_dead_monitor_when_stopped` regression tests | closed |
| T-20-SC-03 | Tampering | package installs (20c) | low | accept | Zero new external dependencies | closed |
| T-20-04 | Information Disclosure | signing-viability check output (20d) | high | mitigate | Reports ONLY boolean viability + public-key fingerprint, never private-key bytes or filesystem path; `release_check_signing_output_leaks_no_key_material_or_path` regression test. **Independently re-verified live** during this phase's UAT (2026-07-23) across all 4 signing states (correct key / no agent / empty agent / unrelated key) against the real operator setup — every output showed only a `SHA256:…` fingerprint | closed |
| T-20-04b | Denial of Service | absent `ssh-add`/`gpg` tooling (20d) | low | mitigate | Fail-soft: emits "cannot verify signing viability — <tool> not found" instead of panicking | closed |
| T-20-04c | Tampering | check writing state / drifting to executor (20d) | medium | mitigate | `release_check` strictly read-only and network-independent — no `save_state`/`Gates::respond`/`git tag`/publish/`git fetch` calls. **Independently re-verified**: code review confirmed no mutating git subcommand anywhere in the check chain; this session's live `devflow release --check` runs never touched `.git/FETCH_HEAD` or tracking refs | closed |
| T-20-SC-04 | Tampering | package installs (20d) | low | accept | Zero new external dependencies | closed |
| T-20-05 | Elevation of Privilege | `--force` scope (20e) | high | mitigate | Scoped to `state.stage == Stage::Ship` ONLY (D-02); `ship_override_refuses_when_not_at_ship_stage` regression test asserts refusal on every non-Ship stage with `--force` true/false | closed |
| T-20-05b | Tampering | after-ship batch integrity (20e) | high | mitigate | `finish_workflow` reused verbatim — inherits the existing fail-closed contract (failed Merge stops the batch, preserves state, refuses `workflow_finished`); no reimplementation to drift | closed |
| T-20-05d | Tampering | race / double-run of terminal hooks (20e) | high | mitigate | `ship_override` acquires the same per-phase lock the live advance path holds (fail-closed on `Contended`); refuses when an ack file already sits alongside the response. `ship_override_refuses_when_lock_contended` + `ship_override_refuses_when_response_already_acked` regression tests. Post-review fix (WR-02) bounds the foreground wait to 60s (`DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`) without weakening the fail-closed contract — an unresolved gate still errors out, just fast | closed |
| T-20-05c | Repudiation | terminal effect not recorded (20e) | low | accept | `finish_workflow` already emits `workflow_finished`; the second trigger reuses it unchanged | closed |
| T-20-SC-05 | Tampering | package installs (20e) | low | accept | Zero new external dependencies | closed |

*Status: open · closed · open — below `high` threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `workflow.security_block_on` (high) count toward `threats_open`*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|--------------|------|
| AR-20-01 | T-20-SC-01..05 | Phase introduces zero new external dependencies across all 5 plans (RESEARCH § Package Legitimacy Audit) — no npm/pip/cargo install task exists anywhere in Phase 20 | Claude (gsd-secure-phase, L1 short-circuit) | 2026-07-23 |
| AR-20-02 | T-20-03 | `--until <stage>` reuses the pre-existing `Stage: FromStr` parser (stage.rs) — no new free-form parsing surface introduced | Claude (gsd-secure-phase, L1 short-circuit) | 2026-07-23 |
| AR-20-03 | T-20-05c | Repudiation risk unchanged from the pre-existing `finish_workflow` audit trail — `ship --phase` is a second trigger of the same already-audited terminal path, not a new unaudited path | Claude (gsd-secure-phase, L1 short-circuit) | 2026-07-23 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-07-23 | 18 | 18 | 0 | Claude (gsd-secure-phase, State B — built from PLAN.md threat models + SUMMARY.md threat-flag scan, L1 short-circuit: `threats_open=0 AND register_authored_at_plan_time=true AND asvs_level==1`) |

**Verification method:** All 5 PLAN.md files (20-01..20-05) carried formal `<threat_model>` blocks authored at plan time (`register_authored_at_plan_time: true`). All 11 distinct named regression tests referenced in threat mitigations were grep-confirmed present in source (`crates/devflow-core/src/version.rs`, `crates/devflow-cli/tests/phase7_cli.rs`, `crates/devflow-cli/src/commands.rs`, `crates/devflow-cli/tests/release_check.rs`, `crates/devflow-cli/src/pipeline_gate.rs`). No SUMMARY.md flagged additional threats during implementation. The post-review fix pass (CR-01, CR-02, WR-01/02/03) strengthened two existing mitigations (T-20-02's stopped-phase gap; T-20-05d's bounded foreground wait) without weakening any fail-closed contract — confirmed by re-running the full workspace gate (491 tests / 0 failed) and by PR #20's independent CI runs. T-20-04 and T-20-04c were additionally re-verified live against the operator's real signing setup during this phase's UAT (not just grep-confirmed).

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-07-23
