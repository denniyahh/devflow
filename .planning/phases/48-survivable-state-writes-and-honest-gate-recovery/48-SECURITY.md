---
phase: "48"
slug: "survivable-state-writes-and-honest-gate-recovery"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-09-19"
---

# Phase 48 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| planning docs → downstream agents | Later plans, the verifier and Phase 49 read these criteria as the acceptance contract | acceptance criteria and requirement IDs (integrity) |
| agent-written PLAN.md → DevFlow resume decision | Agents may write plan files during Code (verify.rs:118-121); the parser's verdict decides whether auto-decide arms | checkpoint declarations parsed from untrusted plan text |
| cargo test process → spawned agent CLI | A test that resolves a real `pi`/`opencode`/`claude` through the developer's `PATH` runs it with real credentials (T-33-09/T-33-17) | developer PATH, real agent credentials |
| agent-written PLAN.md during Code → resume auto-decide | An agent can add or rewrite a `blocking-human` task after preflight | human-only checkpoint declarations |
| state file across binaries | Older binaries write state without the field | persisted checkpoint approval (serde default) |
| test threads to process environment | A global environment mutation affects unrelated concurrent process spawns | process-global environment variables |
| concurrent CLI verbs / monitor → `.devflow/` files | Several processes write the same gate and state paths | gate responses, state JSON, temp files |
| monitor process → `devflow advance` | A detached script from possibly another binary version invokes `advance` | stage argument, lock wait |
| operator shell verbs ↔ detached monitor/advance | Separate processes write one phase's state | phase state JSON under the phase lock |
| lock-file identity to command decisions | A stale or recycled pid can look alive without a start-time comparison | pid + process start time |
| operator recovery verbs → a possibly live phase | `recover --clean` must acquire the phase lock before it can delete state or gate artifacts | state and gate artifacts deleted by `recover --clean` |
| human gate response → persisted checkpoint approval | The response decides whether a later resume may auto-decide an agent-written plan | approval decision → recorded checkpoint set |
| operator verbs to gate response files | A response can decide a later gate if the original waiter is gone | gate answers that decide later gates |
| documented test rule to future contributors | An inaccurate rule can reintroduce global PATH mutation or hide its evidence limits | test-isolation rules (TESTING.md) |

---

## Threat Register

Register authored at plan time: 60 threats across the 19 `<threat_model>` blocks (49 in plans 01-17; 11 in gap plans 48-18 and 48-19, added 2026-09-23). Historical evidence
retains its recorded SHA; the blocking entries were refreshed by the 2026-09-20 formal re-audit.

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-48-01-01 | Tampering | ROADMAP.md whole-file rewrite | medium | mitigate | Scoped `Edit` only; acceptance checks no hunk lands outside the four named regions — **Evidence:** commit 7789c67: ROADMAP.md hunks only in the four named regions; phase-wide ROADMAP diff 59+/45− (no whole-file rewrite) | closed |
| T-48-01-02 | Repudiation | Requirement ID reuse (GATE-01/02 already mean #184/#170) | low | mitigate | New IDs CHKPT-01/02, TEST-01; Future Requirements rows verified unchanged — **Evidence:** REQUIREMENTS.md:84,91,98 (CHKPT-01/02, TEST-01); GATE-01/02 rows byte-identical to 59b9e3e | closed |
| T-48-02-01 | Elevation of Privilege | resume predicate whole-file match | high | mitigate | Shared line-start-anchored parser; RED prose-marker test (Task 1) — **Evidence:** verify.rs:131-135 (resume predicate filters declarations), :336-344 (line-start anchor); RED test verify.rs:758 | closed |
| T-48-02-02 | Elevation of Privilege | fence tracking fails open (toggle desync hides a real declaration) | high | mitigate | Same-char, run-length close rule plus R-3 unclosed-fence rescan; unterminated-fence negative test (Task 2) — **Evidence:** verify.rs:306-317 (`fence_closes`), :217-223 (unclosed-fence rescan); tests verify.rs:846/865/885/905 | closed |
| T-48-02-03 | Tampering | malformed plan text (CRLF, odd fences) | low | mitigate | Normalization and CRLF test (Task 3) — **Evidence:** verify.rs:183, :274-278 (CRLF normalized); test verify.rs:1052 | closed |
| T-48-03-01 | Elevation of Privilege | converted driver tests | medium | mitigate | Child `PATH` is only the stub directory; never the parent's `PATH`; `run_test_in_child` refuses a non-directory — **Evidence:** devflow-core test_support.rs:159-171 (PATH set on the child `Command` only); stub-or-empty PATH in pi.rs/opencode.rs | closed |
| T-48-03-02 | Denial of Service | vacuous child run reported green | medium | mitigate | Four-way guard plus the should-panic misspelled-name control — **Evidence:** devflow-core test_support.rs:183-220 (`1 passed` + non-zero filtered); `should_panic` control :314 | closed |
| T-48-03-03 | Tampering | concurrent PATH race corrupting unrelated tests | medium | mitigate | No process-global PATH mutation remains in devflow-core tests (Task 2 zero gates) — **Evidence:** no `set_var`/`remove_var` of PATH in devflow-core (base had 3+3) | closed |
| T-48-04-01 | Elevation of Privilege | converted outcome tests | medium | mitigate | Child PATH is an agent-free directory; `prepend_path` of the developer PATH is replaced — **Evidence:** pipeline_outcomes.rs: all 41 `run_test_in_child` calls use `agent_free_git_only_path_dir()` (cli test_support.rs:282-294) | closed |
| T-48-04-02 | Denial of Service | a conversion that silently runs nothing | medium | mitigate | Guard on every child; `#[test]` count ≥ base; module passed count ≥ baseline — **Evidence:** 46/46 child runs guarded; `#[test]` 69 ≥ 68 at c0fb193 (passed count is a SUMMARY claim) | closed |
| T-48-05-01 | Elevation of Privilege | abort-fixture tests (999.80) | medium | mitigate | Child with agent-free PATH; helpers assert child mode — **Evidence:** pipeline_launch.rs:1876-1888 (`enter_agent_free_child!`); fixture child-mode assert :3817-3820 | closed |
| T-48-05-02 | Denial of Service | vacuous child | medium | mitigate | Guard per child; test count ≥ base; contract tests run by exact name — **Evidence:** 21/21 child runs guarded; contract tests pipeline_launch.rs:3081/3117/3150/4741 | closed |
| T-48-06-01 | Elevation of Privilege | abort-fixture tests (999.80) | medium | mitigate | Child with agent-free PATH; helper child-mode assertion — **Evidence:** preflight.rs:1468-1480, pipeline_gate.rs:713-725 (`enter_path_isolated_child!`); staleness.rs:1192-1198 | closed |
| T-48-06-02 | Denial of Service | vacuous child | medium | mitigate | Guards; test counts ≥ base — **Evidence:** guard inside both macros and staleness.rs:1198; `#[test]` counts at or above base | closed |
| T-48-07-01 | Elevation of Privilege | agent adds a checkpoint to self-route into auto-decide | high | mitigate | Fresh-scan compare against the recorded set; new or changed parks at a human gate (Tasks 1-2) — **Evidence:** pipeline_launch.rs:1741-1762 (fresh scan → unapproved → gate); tests :3925, :4462, control :4582; state.rs:767 | closed |
| T-48-07-02 | Elevation of Privilege | loop-back Code evaluation silently widens the set | high | mitigate | Record only from `Pending`; `Unrecorded` is never promoted by later Code evaluation. The only production recorder sites are Code preflight paths, and the Supervise-refusal regression test adds an agent declaration after approval and leaves it unapproved — **Evidence:** formal GSD re-audit 2026-09-20, `preflight.rs` Pending-only recorder and Code-stage callers | closed |
| T-48-07-03 | Tampering | old state file loads with a permissive default | medium | mitigate | serde default is `Unrecorded`, which gates — **Evidence:** state.rs:427-428 (`serde(default)`), :456 (`Unrecorded` default), :487; tests state.rs:703, pipeline_launch.rs:4499 | closed |
| T-48-07-04 | Denial of Service | re-scan gate fires on every resume after approval | low | mitigate | Approval records the set (R-8 test) — **Evidence:** pipeline_launch.rs:1771-1780 (48-15 records on Advance before relaunch); test :4170, control :4260 | closed |
| T-48-08-01 | Elevation of Privilege | fixture reworded or parser changed → LoopBack → real `claude` spawn | medium | mitigate | Child with agent-free PATH; helpers panic outside a child; should-panic control — **Evidence:** devflow-core test_support.rs:154-172; first-statement asserts pipeline_outcomes.rs:1443/4660/4706; `should_panic` :1651-1659 | closed |
| T-48-09-01 | Tampering | future PATH mutation | medium | mitigate | Clippy guard plus three-spelling negative control — **Evidence:** clippy.toml:19-22 (`disallowed-methods`); clippy `-D warnings` in scripts/check.sh:40, ci.yml:75 | closed |
| T-48-09-02 | Repudiation | unreasoned lint suppression | low | mitigate | Only reasoned expect attributes allowed; gate rejects allow attributes — **Evidence:** holds at HEAD (0 `allow`, 21/21 reasoned `expect`) but no standing gate; expected `allow_attributes`/`allow_attributes_without_reason = "deny"` in `[workspace.lints.clippy]` | open — below high threshold (non-blocking) |
| T-48-09-03 | Denial of Service | retained non-PATH mutation | low | accept | Deferred by D-09; ENV_MUTEX and reasons remain visible — **Evidence:** accepted — see Accepted Risks Log (48-CONTEXT.md D-09 :217-221; TESTING.md:93) | closed |
| T-48-10-01 | Tampering | approval silently replaces a rejection | high | mitigate | Hard-link exclusive publish; two-publisher test — **Evidence:** gates.rs:387-417 (`publish_response_exclusive`, hard link, `AlreadyResponded`); test gates.rs:659 | closed |
| T-48-10-02 | Tampering | pre-planted symlink at a predictable temp path | medium | mitigate | Unpredictable name created with `create_new` (never follows) — **Evidence:** workflow.rs:204/219 (`File::create_new`, O_EXCL); temp name is predictable, protection rests on O_EXCL | closed |
| T-48-10-03 | Denial of Service | torn response skipped forever | medium | mitigate | Response becomes visible only as a complete linked file — **Evidence:** gates.rs:396-404 (write then link); poll reads only the linked path :276-279 | closed |
| T-48-10-04 | Denial of Service | hard links unsupported | low | accept | Loud `GateError::Io`; no silent fallback — **Evidence:** accepted — see Accepted Risks Log (48-CONTEXT.md:337; gates.rs:384-386, :412-414) | closed |
| T-48-11-01 | Tampering | queued advance evaluates a moved-on stage | high | mitigate | R-2 stage binding with `advance_failed` refusal — **Evidence:** pipeline_launch.rs:1596-1612 (stage mismatch → `advance_failed`); test :3012 (state bytes unchanged) | closed |
| T-48-11-02 | Denial of Service | advance waits forever | medium | mitigate | 10-minute bound + `advance_failed` event — **Evidence:** pipeline_launch.rs:55 (`ADVANCE_LOCK_WAIT` 600 s); lock.rs:75-95; test :2993 | closed |
| T-48-11-03 | Repudiation | refusal invisible (/dev/null) | medium | mitigate | Every refusal emits an event with phase, reason and holder/stages — **Evidence:** pipeline_launch.rs:1572-1581, :1598-1607 (reason, pids, stages); test :3006 | closed |
| T-48-11-04 | Tampering | old script without `--stage` | low | accept | Unbound path proceeds and records `advance_stage_unbound` (F-8) — **Evidence:** accepted — see Accepted Risks Log (48-RESEARCH.md:374 F-8; main.rs:127) | closed |
| T-48-12-01 | Tampering | start's monitor_pid save overwrites advance's transition (D-01 hazard a) | high | mitigate | start holds the lock; advance queues (48-11) — **Evidence:** commands.rs:360-369 (guard held through start); pipeline_launch.rs:1067-1068, :1569; start_lock_e2e.rs:113-169 | closed |
| T-48-12-02 | Tampering | stop re-creates state for an aborted phase (D-01 hazard b) | high | mitigate | R-5 lock-then-load; no state write while a live process holds the lock after the gate path answered — **Evidence:** commands.rs:2036-2072 (lock, then load, then save) | closed |
| T-48-12-03 | Denial of Service | stop leaves a signalled phase un-stopped | medium | mitigate | F-1 bounded retry; explicit message when the holder survives — **Evidence:** commands.rs:2036-2040 (bounded retry), :2050-2054 ("not marked"); stop_e2e.rs:107-174 | closed |
| T-48-12-04 | Spoofing | signalling a recycled pid or the stale `monitor_pid` | medium | mitigate | T-23-51/T-23-52 unchanged, now pinned by `stop_never_signals_the_recorded_monitor_pid` and `stop_refuses_to_signal_a_lock_holder_whose_start_time_does_not_match` — **Evidence:** commands.rs:1951-2021 (never reads `monitor_pid`; start-time match); stop_e2e.rs:176-231 | closed |
| T-48-12-05 | Tampering | stale response decides a fresh run's gate | medium | mitigate | start cleans leftovers under its lock (R-1) — **Evidence (re-audited 2026-09-22 at ab37d90):** lock commands.rs:360-369, then all five stages' gate cleanup :370-380 (gates.rs:302-335); `start_lock_e2e::start_clears_leftover_gate_files_after_taking_the_lock` (:200), which fails when the cleanup loop is removed. Caveat: an I/O error deleting a leftover only warns and proceeds (:377-379), so that leftover survives into the fresh run | closed |
| T-48-13-01 | Spoofing | recycled pid classified live | high | mitigate | Start-time mismatch test and Recycled status — **Evidence:** lock.rs:343-347 (start-time mismatch → `Recycled`), :737-750 | closed |
| T-48-13-02 | Tampering | doctor mutates a lock while observing it | medium | mitigate | Read-only parser and empty-lock preservation control — **Evidence:** lock.rs:320-332, :359-369 (read-only parser), :700-720 | closed |
| T-48-13-03 | Denial of Service | legacy waiter treated as absent | medium | mitigate | Unconfirmable remains may_be_waiting — **Evidence:** lock.rs:57-62 (`Unconfirmable` may be waiting), :752-762, :778-785 | closed |
| T-48-14-01 | Denial of Service | cleanup strands a live waiter until its timeout | high | mitigate | CX-1 / PI-S2 acquisition guard holds the phase lock across cleanup; contended-lock controls prove state and gates are untouched — **Evidence (re-audited 2026-09-22 at ab37d90):** explicit `clean_phase_report` recover.rs:201-229 takes the lock (`?`, :205) before gate cleanup and `clear_state`. The implicit sweep `clean_report` recover.rs:109-166 takes the lock (:128-137, `fb2c4f8`) before gate cleanup (`854bbce`) and `clear_state`, and skips a contended phase. Tests: `recover::tests::clean_phase_returns_without_cleanup_while_the_per_phase_lock_is_contended`, `..._for_a_recycled_pid_lock`, and `clean_keeps_a_stale_phase_whose_lock_is_held`, which covers state and gate bytes since `f73990f`; moving gate cleanup above the lock fails it. Also `recover_clean_e2e.rs`. The 2026-09-19/20 closure covered only the explicit path: the implicit sweep deleted live state without the lock until `fb2c4f8` | closed |
| T-48-14-02 | Repudiation | doctor prescribes a repair that does nothing | medium | mitigate | Shared `no_waiter_repair`; orphan-gate check yields to the new finding — **Evidence:** commands.rs:3353-3367 (`no_waiter_repair`), :3371-3374; tests commands.rs:6367-6459 | closed |
| T-48-14-03 | Tampering | doctor mutates state | low | mitigate | PI-1 read-only holder status plus report-only unchanged (T-18-02) — **Evidence:** commands.rs:3526 (read-only `holder_status`); test commands.rs:6804 | closed |
| T-48-15-01 | Elevation of Privilege | re-scan approval | high | mitigate | Only re-scan `Advance` records the freshly scanned declaration set; `LoopBack` preserves the prior set. A rejected Auto re-scan parks for Supervise repair without auto-deciding, and the resumed phase retains the rejection's prior approval set — **Evidence:** formal GSD re-audit 2026-09-20, `pipeline_launch.rs` re-scan action handling and Auto-park regression test | closed |
| T-48-15-02 | Tampering | Code preflight records from project root | high | mitigate | Worktree fixture proves recording and comparison use the same execution root — **Evidence:** preflight.rs:1311-1321 (execution root) matches pipeline_launch.rs:1717/1741; decoy-root tests preflight.rs:2582, :2684 | closed |
| T-48-15-03 | Repudiation | LoopBack silently accepts changed plans | medium | mitigate | LoopBack keeps approval unchanged and names unapproved plan files — **Evidence:** pipeline_launch.rs:1795-1803; preflight.rs:1422-1427; tests pipeline_launch.rs:4275-4393 | closed |
| T-48-16-01 | Tampering | stale response decides a later non-Ship gate | high | mitigate | NoHolder and Recycled write no response — **Evidence (re-audited 2026-09-22 at ab37d90):** holder checked before writing at commands.rs:1419 (`gate_respond`), :1557 + :1736 (sweep), :1926 (`stop`), with `stop`'s stale-gate check at :1914-1923. Tests: `commands::tests::gate_respond_with_a_recycled_lock_pid_at_a_non_ship_gate_writes_nothing`, `stop_via_gate_with_no_waiter_at_a_non_ship_gate_writes_nothing`, `gate_sweep_leaves_a_no_waiter_non_ship_gate_alone`, live control `stop_via_gate_with_a_live_waiter_writes_the_rejection`; `gate_wedge_e2e.rs:94`. **Residual accepted as AR-48-04:** the write-time check does not cover an answer that outlives its waiter | closed (residual: AR-48-04) |
| T-48-16-02 | Repudiation | command claims a waiter without evidence | medium | mitigate | A waiter claim now requires the observed and current lock records to have the same live `(pid, start-time)` identity; a successor holder fails closed — **Evidence:** commands.rs `answered_gate_contention_message` and its same-identity/successor negative-control matrix, refreshed 2026-09-20 | closed |
| T-48-16-03 | Spoofing | recycled pid treated as a waiter | high | mitigate | Recycled holders receive no response at every gate, including Ship; Ship's stored-response exception is limited to `NoHolder` — **Evidence (re-audited 2026-09-22 at ab37d90; every production response writer enumerated):** `gate_respond` commands.rs:1414-1424 (Ship exception `NoHolder` only since `f1accee`); `gate sweep` :1556-1567 with `gate_sweep_may_reap` :1736-1738 (Live only); `stop_via_gate` :1924-1936 (Live, or Ship with `NoHolder`); the `--yes-ship` auto-response pipeline_gate.rs:414-422 is written by the waiter itself. Tests: `commands::tests::gate_respond_with_a_recycled_lock_pid_at_the_ship_gate_writes_nothing` (fails if `f1accee` is reverted), `..._at_a_non_ship_gate_writes_nothing`, `stop_at_a_ship_gate_with_a_recycled_holder_claims_no_waiter`; NoHolder control `gate_approve_with_no_waiter_at_the_ship_gate_writes_and_names_ship`. The 2026-09-20 closure was false for `gate approve|reject` at Ship until `f1accee` | closed |
| T-48-17-01 | Tampering | stale test guidance | low | mitigate | Documentation uses the helper and lint names verified by Task 1 — **Evidence:** .planning/codebase/TESTING.md:69, :84, :91; named helpers exist; clippy.toml `disallowed-methods` | closed |
| T-48-17-02 | Repudiation | timing result presented as reliability proof | low | mitigate | SUMMARY records one-run and concurrent-load limits — **Evidence:** 48-17-SUMMARY.md:131-133 (Evidence Limits — the SUMMARY is the mitigation) | closed |
| T-48-18-01 | Repudiation | criterion 4 evidence: a pin that names `start` but drives another process | medium | mitigate | Both start arms assert the lock holder pid is the `start` child — **Evidence:** gate_wedge_e2e.rs:435-439, :549-553; advance arms renamed (:247, :325); lock-drop mutant fails (48-18-SUMMARY, mutant_exit=101) | closed |
| T-48-18-02 | Denial of Service | a parked `devflow start` leaked by a panicking assertion | low | mitigate | `ReapOnDrop` owns the file's only `start` spawn — **Evidence:** gate_wedge_e2e.rs:169-178, :188-193 | closed |
| T-48-18-03 | Tampering | commands.rs left mutated after the Task 2 gate | medium | mitigate | Checked backup + EXIT-trap restore + post-restore `git diff --quiet` — **Evidence:** 48-18-PLAN.md:219; commands.rs clean at HEAD, mutant string absent from history | closed |
| T-48-18-04 | Repudiation | pickup_ms read as a wedge measurement | low | accept | See AR-48-05 — **Evidence:** 48-18-SUMMARY.md:143, :195; gates.rs:293, :308 | closed |
| T-48-19-01 | Tampering | `start` overwriting a live run's state and wiping its gates | high | mitigate | Guard under lock-NN before cleanup, git and `save_state` — **Evidence:** commands.rs:360 lock, :370 guard, :378 cleanup, :541 git, :693 save; byte-identity arms start_lock_e2e.rs:403-417, :472-479. Residual deferred (not accepted): 999.139 | closed |
| T-48-19-02 | Elevation of Privilege | a second agent launched against a live phase worktree | high | mitigate | Same guard; arms assert no lock, branch or workflow_started — **Evidence:** start_lock_e2e.rs:480-503. Residual deferred (not accepted): 999.140 (`resume`) | closed |
| T-48-19-03 | Tampering | `--force` used as a bypass | medium | mitigate | Guard takes no `force`; call is unconditional — **Evidence:** commands.rs:2426, :370; agent arm runs `--force` (start_lock_e2e.rs:536) | closed |
| T-48-19-04 | Denial of Service | a recycled pid makes a dead run look live (false refusal) | low | accept | See AR-48-06 | closed |
| T-48-19-05 | Tampering | writes in the agent-exit → advance lock-free window | medium | accept | See AR-48-07 | closed |
| T-48-19-06 | Elevation of Privilege | operator follows the refusal's repair and launches a second agent | high | mitigate | Corrected text (20cc64c) pinned sentence by sentence in `bb639f5` — **Evidence:** commands.rs:2466-2487; start_lock_e2e.rs fragment list; an attack-text mutant passes the pre-`bb639f5` test and fails the new one on a new fragment (two mutants). Residual: operator-approved no-state carve-out (pinned by the phase-98 test) | closed |
| T-48-19-07 | Denial of Service | operator signals an unrelated process named by a recycled pid | medium | mitigate | `ps -ww -o pid=,lstart=,args= -p <pid>` with identity claims matching the spawn argv (20cc64c) — **Evidence:** commands.rs:2457, :2471-2477; monitor.rs:573-598, :600-607; tests pin the fragments and forbid `ps -p <pid>` | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

AR-48-01 to AR-48-03 are plan-time `accept` dispositions, recorded with the rationale the auditors
located. AR-48-04 is a residual on a closed threat, accepted by operator decision after the
2026-09-22 code review. No open threat has been accepted.

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-48-04 | T-48-16-01 (residual) | Gate answers are not bound to a gate incarnation, so an answer that outlives its waiter decides the next gate at the same phase and stage on its first poll (gates.rs:276-279). The write-time holder check cannot see three paths: (a) a waiter killed within the ≤60 s poll backoff after an answer is written, then `resume`, which relaunches without clearing it (pinned by `pipeline_launch::tests::resume_relaunches_without_consuming_a_pending_gate_answer` and its control); (b) a late `Gates::respond`, with no re-check after its hard-link publish (gates.rs:195-201 → :404), landing after a consume-and-cleanup; (c) the check-to-write window in the three CLI writers (commands.rs:1414→1434, :1556→1578, :1912→1937). (a) was confirmed through its pinned tests; (b) and (c) were reasoned from source, not reproduced. 48-CONTEXT D-05's claim that (3)+(4) close this hazard is superseded. Fix path: 999.130's gate-waiter registration | Operator decision 2026-09-22 (record as a Phase 48 limit; fold into backlog 999.130) | 2026-09-22 |
| AR-48-01 | T-48-09-03 | The 21 non-PATH environment mutations stay process-global under `ENV_MUTEX`, each justified in-source; per-`Command` scoping cannot reach an in-process reader (48-CONTEXT.md D-09 :217-221; TESTING.md:93) | Plan-time disposition, 48-09-PLAN (D-09) | 2026-09-19 |
| AR-48-02 | T-48-10-04 | Filesystems without hard links fail with `GateError::Io`; no rename fallback, so no silent overwrite (48-CONTEXT.md:337; gates.rs:384-386, :412-414) | Plan-time disposition, 48-10-PLAN | 2026-09-19 |
| AR-48-03 | T-48-11-04 | An older monitor script without `--stage` falls back to an unbound advance (48-RESEARCH.md:374 F-8; main.rs:127; pipeline_launch.rs:1613-1618) | Plan-time disposition, 48-11-PLAN | 2026-09-19 |

| AR-48-05 | T-48-18-04 | `pickup_ms` in the self-resolving arms measures the gate poll's first 1 s backoff step, not a wedge; the SUMMARY says so | Plan-time disposition, 48-18-PLAN | 2026-09-23 |
| AR-48-06 | T-48-19-04 | Pid liveness, not identity: a recycled pid gives a fail-closed false refusal; the refusal directs `ps -ww -o … args=` and `recover --clean` then `start` | Plan-time disposition, 48-19-PLAN (records the operator's Option A, 2026-09-22; the operator's wording appears only in planner/executor-written files) | 2026-09-23 |
| AR-48-07 | T-48-19-05 | The agent-exit → advance lock-free window is not closed by the guard | Operator-deferred to backlog 999.136 (ROADMAP.md, 2026-09-22) | 2026-09-23 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-09-19 | 49 | 45 | 4 (2 blocking: T-48-15-01, T-48-16-03; 2 non-blocking: T-48-16-02, T-48-09-02) | gsd-security-auditor ×3 (plans 01-06 / 07-11 / 12-17, split for context size) + orchestrator re-check |
| 2026-09-19 (round 2, 76867df) | 9 re-checked | 7 | 2 (T-48-15-01 high — Supervise Code path; T-48-16-02 medium — Live-arm race) | gsd-security-auditor (State A re-audit) |
| 2026-09-19 (round 3, 25aa87c) | 7 re-checked | 5 | 2 (T-48-15-01 high — Auto truncation path; T-48-07-02 high — regressed by 25aa87c) | gsd-security-auditor (State A re-audit) |
| 2026-09-19 (round 4, d3beb19) | — | — | — | gsd-security-auditor **DID NOT RUN** — terminated by a session rate limit (HTTP 429) before any verdict; must be re-run |
| 2026-09-19 (external, d3beb19) | rule + diff | — | 6 out-of-register findings (A–F below) | codex (gpt-5.6-terra, high) + agy (gemini-3.8-flash-high) on a `git archive` snapshot; both completed (~8 min); snapshot unmodified (139/139 hashes) |
| 2026-09-20 (round 5, 3aca386) | 2 re-checked | 2 | 0 blocking (1 medium threat remains non-blocking) | gsd-security-auditor, State A, ASVS L1; operator selected Verify all open threats |
| 2026-09-22 (round 6, ab37d90) | 4 re-checked (T-48-16-03, T-48-14-01, T-48-16-01, T-48-12-05) | 4 | 0 blocking (T-48-09-02 still non-blocking) | gsd-security-auditor, State A, ASVS L1, operator-requested after the 2026-09-22 code review; orchestrator added the T-48-14-01 gate-half test (`f73990f`) and AR-48-04 |
| 2026-09-23 (round 7, 60da104 → bb639f5) | 11 new (gap plans 48-18, 48-19) | 11 | 0 blocking (T-48-19-06 was open until `bb639f5`) | gsd-security-auditor, State A, ASVS L1; operator selected Verify all open threats; orchestrator added the missing refusal fragments and ran the two-way mutant control |

### Audit 2026-09-19 — notes

- **Depth:** ASVS L1 — each mitigation was located in code at HEAD and each cited test was read for the
  property it asserts. Auditors ran one test (`commands::tests::stop_at_a_ship_gate_with_a_recycled_holder_claims_no_waiter`,
  `1 passed`, printing the contradictory lines). Pass status of every other cited test rests on SUMMARY
  claims, not this audit.
- **T-48-15-01 classification:** the plans 12-17 auditor marked it CLOSED and reported the any-stage
  recording path as out-of-register; the plans 07-11 auditor independently found the same path and
  recommended judging it under T-48-15-01. The orchestrator confirmed it in code (`preflight.rs`
  Advance arm and `record_checkpoint_set_for_code_evaluation` both lack a stage check; the refusal text
  names no plan) and classified it OPEN, since it is exactly this threat's elevation path.
- **T-48-16-03:** confirmed by the orchestrator in code — `lock::acquire` checks liveness only, so a
  recycled pid reads as contention, and the answered-gate branch then claims a waiter.
- **Operator decision (2026-09-19):** fix the blocking threats inline, test-first, then re-audit.
- **Rounds 2-3 — fixes closed instances, not the class.** 76867df added a Code-stage check; round 2 found
  the Supervise Code path. 25aa87c added an Auto-mode check; round 3 found the Auto 300-character
  truncation path and that 25aa87c had **regressed T-48-07-02** by leaving `Pending` in place on a
  Supervise approval. Both round-3 findings were confirmed in code by the orchestrator.
- **Operator decision (2026-09-19, after round 3):** option 1 — a class-level rule: record only from
  `Pending` (the pre-agent set), at Code, in every mode; once recorded, only the re-scan gate widens the
  set. Implemented in d3beb19. Adversarial review by codex and agy on the rule and the diff, plus a
  round-4 audit, before closing.
- **External review of d3beb19 (codex + agy):** both lanes tried to break the `Pending`-only rule and
  could not: codex traced fresh/retry launches through `run_preflight` and found only the recorder and
  the re-scan gate as writers; agy traced every Code entry point (`start`, `transition`, loop-back,
  `resume`, stage-failure retry, ambiguous-outcome relaunch) and non-Claude drivers. This supports
  T-48-15-01 / T-48-07-02 closing; the later round-5 auditor confirmed both closed.
- **Round 5 re-audit (2026-09-20):** the formal State A auditor returned `SECURED` for T-48-07-02 and
  T-48-15-01. It verified that the Pending-only recorder is called only at Code preflight and that the
  re-scan Advance arm is the only post-agent writer; Auto rejection preserves the old approval through
  the parked Supervise resume. `threats_open: 0` is therefore correct at the configured `high` threshold.
- **Out-of-register findings (external review, 2026-09-19)** — operator dispositions:
  - **A (high, both lanes)** Supervise mode auto-decided a planner-declared human-only checkpoint with no
    human involved. **Closed by D-03b:** only Auto resumes an unchanged, recorded checkpoint; Supervise
    records a re-scan approval then opens a separate human Code gate. The scope is now explicit in source
    policy text and tests.
  - **B (high, both lanes)** `stop` could return success with state unmarked after an answered gate's holder
    changed or could not be confirmed. **Closed:** it now claims a waiter only when the observed and current
    holder share the same `(pid, start-time)` identity; every other result fails closed.
  - **C (high, agy)** `stop` could claim an existing response would end a phase when it was not an abort.
    **Closed:** only a live holder plus an existing abort response is attributed to `stop`; all other actions
    fall through without an outcome claim.
  - **D (medium, agy)** mixed `blocking-human` and `human-action` declarations could reach the wrong
    auto-decision predicate. **Disposition: backlog 999.127** — scoped outside Phase 48.
  - **E (medium, codex, reproduced)** `gate approve/reject` claimed a workflow outcome with no confirmed
    live holder. **Closed:** response persistence is described without claiming pickup.
  - **F (medium, codex, reproduced)** `gate sweep` claimed it reaped an unconfirmable-holder phase.
    **Closed:** sweep reaps only confirmed live holders and its dry-run has a live-holder positive control.
  - B, C, E, F were one class: *no recovery command states an outcome unless a confirmed live holder will
    act on it*. The class is closed by the post-review recovery fixes and their holder-matrix tests.
- **Outside the register (not counted):** `recover --clean` prints "cleaned up workflow state" after
  refusing on a contended lock (commands.rs:2417-2421); 48-07-SUMMARY cites a test name that does not
  exist (the real pin is `preflight::tests::code_preflight_does_not_record_an_unrecorded_set`); stale
  `NeutralPath` doc comment at cli test_support.rs:296-322; bare-`\r` plan files read as one line and fail
  open at preflight; child-mode checks test `DEVFLOW_CHILD_TEST` presence only; T-48-17-01's mitigation
  document lives in the mapper-owned `.planning/codebase/`.

### Audit 2026-09-22 — notes (round 6)

- **Trigger.** A codex + agy production-code review and a fix-round review
  (`.planning/reviews/48-code-review-2026-09-22/`) showed that two CLOSED high threats had rested on
  incomplete evidence:
  - **T-48-16-03:** `gate approve|reject` wrote a Ship response for a Recycled holder. Fixed in `f1accee`.
  - **T-48-14-01:** the implicit `recover --clean` sweep deleted live state without the lock. Fixed in
    `fb2c4f8`; gate cleanup was added under the lock in `854bbce`.

  Both are the pattern recorded in round 3: the evidence covered one instance of a class.
- **Verdict.** The round-6 auditor enumerated every production gate-response writer and both recover
  cleanup paths, and returned SECURED on the declared mitigations. It ran mutation checks against a
  `git archive` snapshot:
  - reverting `f1accee` failed the recycled-Ship test for the intended reason;
  - removing `start`'s cleanup loop failed `start_clears_leftover_gate_files_after_taking_the_lock`;
  - moving the sweep's gate cleanup above the lock passed all 32 sweep tests.

  The last one was a test gap. The orchestrator closed it in `f73990f` and confirmed that the same
  reorder now fails `clean_keeps_a_stale_phase_whose_lock_is_held` on the gate-file assertion.
- **T-48-16-01 residual.** The planned write-time check is present, but it does not cover answers that
  outlive their waiter; recorded as AR-48-04 on the operator's decision.
- **Non-blocking caveats, not counted:**
  - T-48-12-05 fails open on an I/O error while deleting a leftover gate file.
  - No test drives `stop` at a non-Ship gate with a Recycled holder.
  - The real `gate sweep` is never driven with a Recycled holder; only its predicate is tested.

  Both untested paths allow Live holders only.
- **Resolved from the round-1 "outside the register" list:** `recover --clean` no longer prints
  "cleaned up" after a refusal (`9a33c70`) or for a phase with nothing on disk (`d36936e`).
- **Evidence limits:**
  - Every test ran once.
  - Four hand-picked mutations are not a mutation sweep.
  - The late-responder and check-to-write races were reasoned from source.
  - At L1, the lock discipline of in-process `Gates::cleanup` callers rests on D-05's text-search claim.

---

### Audit 2026-09-23 — notes (round 7)

- **T-48-19-06 was open at 60da104.** The register said the tests pinned the repair guidance. They did
  not pin the monitor-first ordering, the recycled-pid condition on the recover path, or the "do neither
  while live" ban. An attack-text mutant ("if a named pid is live, run `recover --clean` then `start`")
  passed both refusal arms. `bb639f5` pins these sentences plus check-before-signal and do-not-signal-on-pid-alone.
  The orchestrator reran the attack mutant against the old and new tests:
  - old test: 1 passed;
  - new test: failed on "send SIGTERM to the confirmed monitor first";
  - a second mutant, which keeps that sentence: failed on the recycled-pid condition.

  The "do neither" fragment is masked behind the earlier failures and has no mutant of its own.
- **Residuals deferred, not accepted:** 999.139 (a state file that won't load hides a live monitor,
  T-48-19-01) and 999.140 (`resume` has no live-run check, T-48-19-02). Neither is in the Accepted
  Risks Log, because neither has an operator ruling.
- **Evidence limits:**
  - Tests ran once each.
  - Nothing sends a signal end to end to check the refusal guidance; the identity claims were read against the spawn code.
  - The auditor's runs used a scratch `DEVFLOW_CACHE_DIR`. The test suite itself still writes the operator's real cache (999.141).

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** pending
