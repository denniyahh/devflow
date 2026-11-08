---
phase: "51"
slug: "rate-limit-agent-failover"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-11-07"
---

# Phase 51 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

Collected from the `<threat_model>` blocks of 51-01..51-07 and 51-09..51-14 (there is no 51-08).

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| agent capture → classifier | agent-written stdout decides `AgentStatus::RateLimited`, which now selects an agent handoff at Code | untrusted agent stream-json |
| agent capture → idle-timeout verdict | an agent-written final line decides between quota denial, torn, and silent | untrusted agent stream-json |
| agent capture → reason / gate context / notify hook env | agent-written `result` fields flow into operator-facing text and the hook's environment | untrusted, bounded text |
| operator CLI → persisted run config | `--fallback-agent` decides which binary a detached supervisor may launch later, unattended | `AgentKind` (closed set) |
| project `devflow.toml` → persisted run config | a key in a tracked project file decides which binary a detached supervisor may launch | `AgentKind` (closed set) |
| state file → supervisor | `fallback_agent` read from `state-NN.json` chooses which binary the supervisor launches | operator-owned local state |
| persisted run state → next agent's instruction | `code_fix` decides which workflow an unattended agent runs | `FixType` |
| recorded launch evidence → next launch | `state.canary` / `session_id` recorded for one agent gate the next launch | launch evidence |
| supervisor advance → unattended relaunch | a failed relaunch inside the supervisor has no operator watching | run control flow |
| supervisor environment → agent environment | the notify command may carry a credential and must not reach an agent | operator secrets |
| operator-edited state file → gate hook | an on-disk edit of `gate_notify_cmd` decides what command runs at a gate | shell command |
| test environment → release binary | a test seam reaching a release build would let an env var disable or fake liveness confirmation | env vars |
| test process → spawned `devflow` | an e2e child must not reach the operator's real home, cache or environment | host state |
| repository source → test verdict | source scans must not pass by matching nothing | source text |
| operator docs → operator decisions | an operator arms an unattended fallback on what the docs promise | documentation |

---

## Threat Register

Evidence was checked at ASVS L1 by two `gsd-security-auditor` passes (51-01..06 and 51-07, 51-09..14). The orchestrator re-read the cited lines for every high-severity row and for the accepted risks. Line numbers are at HEAD `7d8460e`.

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-51-01 | Spoofing | failover trigger in `advance_with` | medium | mitigate | Only non-test caller of `fail_over_on_rate_limit` is the `AgentStatus::RateLimited` arm (`pipeline_launch.rs:4135`) | closed |
| T-51-02 | Denial of service | `fail_over_on_rate_limit` | medium | mitigate | `fallback_consumed = true` in the handoff commit (`pipeline_launch.rs:3176`); consumed fallback refused (`:3066`); second rate limit takes the cron arm bounded by `MAX_INFRA_FAILURES` | closed |
| T-51-03 | Repudiation | handoff and refusal | low | mitigate | `agent_handoff` (`pipeline_launch.rs:3185-3197`) and `agent_handoff_refused` (`:3273-3283`) events; test `a_refused_handoff_takes_the_cron_arm_and_names_the_refusal` | closed |
| T-51-04 | Tampering | `fallback_agent` in the state file | low | accept | See Accepted Risks Log (AR-51-01) | closed |
| T-51-05 | Tampering | `doc_check::the_removed_sequentagent_handoff_stays_dead` | medium | mitigate | Planted positives, decoys and non-vacuity asserts (`doc_check.rs:1591-1722`); test run: 1 passed | closed |
| T-51-06 | Tampering | `idle_timeout_cause` → reason → gate context | medium | mitigate | `last_top_level_result` selection, ≤3 errors × 160 chars, control chars replaced, clause cap `MAX_IDLE_TIMEOUT_CLAUSE_CHARS = 200` (`agent_result.rs:2324-2486`) | closed |
| T-51-07 | Denial of service | idle-timeout record size and parse | low | mitigate | Same bounds applied on write (`monitor.rs:2774`); key-set pin `the_on_disk_idle_timeout_record_key_set_is_pinned` | closed |
| T-51-08 | Information disclosure | cause text in the notify hook context | low | accept | See Accepted Risks Log (AR-51-02) | closed |
| T-51-09 | Elevation of privilege | auto-mode fallback launch | high | mitigate | `validate_fallback_agent` → `fallback_launch_shape_refusal` (`preflight.rs:1093-1105`) at start (`commands.rs:420`) and resume (`pipeline_launch.rs:3539`); full bundle again at handoff (`:3150`) | closed |
| T-51-10 | Tampering | `--fallback-agent` value | low | mitigate | clap `Option<AgentKind>` (`main.rs:127/229`); tests `start_accepts_a_fallback_agent_flag`, `resume_accepts_a_fallback_agent_flag` | closed |
| T-51-11 | Repudiation | arming a fallback | low | mitigate | `print_fallback_notice` (`preflight.rs:1125`) from start and resume; persisted at `state.rs:482` | closed |
| T-51-12 | Spoofing | `capture_rate_limit_denial` / `detect_claude_stream_rate_limit` | high | mitigate | `is_top_level` enforced for turn boundary and event selection (`agent_result.rs:1451-1478, 1745-1770`); denial list `["rejected"]` (`:1692`); torn line classifies `IdleTimeout`, never `RateLimited` | closed |
| T-51-14 | Elevation of privilege | `canary_gate` after a handoff | high | mitigate | Handoff commit clears `canary` and `session_id` (`pipeline_launch.rs:3182-3183`); Confirmed→Absent control in `a_handoff_reruns_the_delivery_canary_for_the_new_agent` | closed |
| T-51-15 | Denial of service | `fail_over_on_rate_limit` relaunch failure | high | mitigate | Non-`NotOwner` error emits `agent_handoff_relaunch_failed` and takes `handle_infra_outcome` (`pipeline_launch.rs:3243-3269`); test `a_handoff_whose_relaunch_fails_ends_at_a_gate` | closed |
| T-51-16 | Denial of service | refusal and failure loops | low | mitigate | `infra_failures` bumped and checked against `MAX_INFRA_FAILURES` (`pipeline_outcomes.rs:50-53, 66, 97-101`) | closed |
| T-51-17 | Tampering | `State.code_fix` lifetime | medium | mitigate | Set only in `prepare_loop_back_to_code` (`pipeline_gate.rs:205`), cleared at `:86`/`:116`; test `a_transition_clears_the_persisted_code_fix` | closed |
| T-51-18 | Tampering | renderer mismatch across agents | medium | mitigate | Only `Option<FixType>` persisted (`state.rs:507`); rendered via `driver_for(state.agent)` (`pipeline_launch.rs:166-167`); argv test and single-renderer scan | closed |
| T-51-21 | Tampering | gate entry write (IN-10) | medium | mitigate | Test `gate_entry_over_an_unparseable_state_does_not_rewrite_it` (`pipeline_gate.rs:2756`) | closed |
| T-51-22 | Tampering | notify command at gate entry (WR-12) | low | mitigate | Test `gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded` (`pipeline_gate.rs:2671`) | closed |
| T-51-23 | Information disclosure | notify command reaching a fallback agent | medium | mitigate | Relaunch → `spawn_monitor` → `strip_notify_env` (`monitor.rs:649`); sites pinned by `strip_notify_env_doc_names_exactly_its_call_sites` | closed |
| T-51-24 | Elevation of privilege | `DEVFLOW_TEST_WITHHOLD_PUBLISH_PROBE` seam | high | mitigate | Seam and call under `cfg(any(test, feature = "test-support"))` (`monitor.rs:666-670, 2457`); `test-support` enabled only under `[dev-dependencies]` (devflow-cli `Cargo.toml:29`, devflow-core `Cargo.toml:38`); compiled-out scan test with planted decoy | closed |
| T-51-25 | Repudiation | criterion 3 evidence | medium | mitigate | `supv01_evidence_violations` (`pipeline_launch.rs:17272`); `the_supv01_evidence_check_rejects_a_stub` run: 1 passed | closed |
| T-51-26 | Repudiation | OPERATIONS.md failover description | low | mitigate | Limits stated (`OPERATIONS.md:31-32, 289-325`); test `operations_md_documents_the_fallback_agent_and_its_events` | closed |
| T-51-29 | Tampering | `failover_e2e.rs` children | medium | mitigate | Only spawn via `common::hermetic_devflow` (`failover_e2e.rs:268`); `FakeBin`; covered by `test_home_isolation` and `hermetic_spawn_sites` walks | closed |
| T-51-30 | Denial of service | supervisors left by a failing e2e | low | mitigate | `LaunchedRunReaper` in all three tests; `common::wait_for` bounded at 120 s | closed |
| T-51-33 | Tampering | failover at an unintended stage | medium | mitigate | `failover_target` checks `Stage::Code` (`pipeline_launch.rs:3066`); test `failover_target_selects_the_fallback_only_at_code` run: 1 passed | closed |
| T-51-34 | Elevation of privilege | key-supplied fallback in `--mode auto` | high | mitigate | Key merged with flag then `validate_fallback_agent` (`commands.rs:410-420`); bundle re-run at handoff | closed |
| T-51-35 | Tampering | `fallback_agent` in `devflow.toml` | low | mitigate | Closed serde `AgentKind`; tests `config_key_with_an_invalid_value_refuses_start` and resolver test (`config.rs:1068`) | closed |
| T-51-36 | Repudiation | standing default fallback | low | mitigate | Source notice (`commands.rs:433-438`); test `config_key_alone_arms_the_fallback_and_names_its_source`; persisted in state | closed |
| T-51-37 | Elevation of privilege | `DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE` seam | high | mitigate | `#[cfg(not(test))]` passes real `supervisor::probe`, `#[cfg(test)]` the seam (`pipeline_launch.rs:2387-2390`); compiled-out scan test with decoy | closed |
| T-51-38 | Repudiation | criterion 3 conclusion | medium | mitigate | `the_supv01_evidence_is_complete` (`pipeline_launch.rs:17943`) run with `--run-ignored only`: 1 passed | closed |
| T-51-39 | Tampering | `FailoverRelaunchGuard` exemption | medium | mitigate | RAII guard (`pipeline_launch.rs:2098-2121`), one non-test engage site pinned by scan; fence helpers byte-identical to merge base; test `a_failover_superseded_mid_relaunch_writes_nothing` | closed |
| T-51-40 | Tampering | unfenced `write_cron_instructions` after the cron arm's commit (pre-existing) | low | accept | See Accepted Risks Log (AR-51-03) | closed |
| T-51-41 | Denial of service | agent group of an unpublished successor | low | mitigate | Positive control `agent_group_was_live` and `agent_group_ended` check in the failover publish-wait test (`pipeline_launch.rs:16876-16947`) | closed |
| T-51-SC | Tampering | cargo installs | high | mitigate | `Cargo.lock` and all `Cargo.toml` files unchanged against the merge base with develop (`git diff --stat` empty; control diff over `crates/devflow-core/src` non-empty); 51-11 phase gate ran `cargo deny` and `cargo machete` clean | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-51-01 | T-51-04 | The state file is operator-owned local state. `fallback_agent` is typed `Option<AgentKind>` (`state.rs:482`), so a hand edit can only name a supported driver, which still passes preflight, binary and health checks before launch. | Plan 51-01 threat model (disposition `accept`); premise checked 2026-11-07 | 2026-11-07 |
| AR-51-02 | T-51-08 | The hook already receives the failure reason; the cause adds at most a 200-character clause (`MAX_IDLE_TIMEOUT_CLAUSE_CHARS`, `agent_result.rs:2347`), and `run_notify_command` passes context as an environment variable, not interpolated into the command (`gates.rs:1546-1552`). | Plan 51-03 threat model (disposition `accept`); premise checked 2026-11-07 | 2026-11-07 |
| AR-51-03 | T-51-40 | Pre-existing window, not created by Phase 51: a superseded supervisor can still write `cron-instructions-NN.json` between the owned commit and the plain write. Phase 51 narrowed its own claim to exclude it. Tracked as backlog 999.185 in ROADMAP.md with fix shape "wrap the write in `fenced`". | Plan 51-06 threat model (disposition `accept`); backlog entry checked 2026-11-07 | 2026-11-07 |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-11-07 | 35 | 35 | 0 | gsd-secure-phase (2 × gsd-security-auditor, orchestrator spot-check of all high-severity rows) |

### Audit notes (non-blocking)

- T-51-24's plan text says the CLI's `[dev-dependencies]` is the only place `test-support` is enabled; devflow-core's own `[dev-dependencies]` (`Cargo.toml:38`) enables it too. Both are dev-only, so release builds are not exposed. Plan wording only.
- Indirect test coverage: no test refuses a fallback at `resume` in `--mode auto` (T-51-09), and none refuses a `devflow.toml`-supplied fallback in `--mode auto` (T-51-34). Both reach the same tested `validate_fallback_agent` call as the start-flag path.
- Tests confirmed to exist but not run in this audit: the supervisor-level T-51-15 and T-51-39 tests. Four named tests were run, each 1 passed.
- The SUPV01 evidence checker confirms cited lines resolve, not what they say; the auditor compared 17 cited locations by hand.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-11-07

## Security Audit 2026-11-08

| Metric | Count |
|---|---|
| Threats found | 35 |
| Closed | 35 |
| Open | 0 |
