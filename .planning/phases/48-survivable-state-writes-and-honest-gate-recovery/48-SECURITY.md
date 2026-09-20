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

Register authored at plan time: 49 threats across the 17 `<threat_model>` blocks. Historical evidence
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
| T-48-12-05 | Tampering | stale response decides a fresh run's gate | medium | mitigate | start cleans leftovers under its lock (R-1) — **Evidence:** commands.rs:370-380; gates.rs:302-332; start_lock_e2e.rs:199-251 | closed |
| T-48-13-01 | Spoofing | recycled pid classified live | high | mitigate | Start-time mismatch test and Recycled status — **Evidence:** lock.rs:343-347 (start-time mismatch → `Recycled`), :737-750 | closed |
| T-48-13-02 | Tampering | doctor mutates a lock while observing it | medium | mitigate | Read-only parser and empty-lock preservation control — **Evidence:** lock.rs:320-332, :359-369 (read-only parser), :700-720 | closed |
| T-48-13-03 | Denial of Service | legacy waiter treated as absent | medium | mitigate | Unconfirmable remains may_be_waiting — **Evidence:** lock.rs:57-62 (`Unconfirmable` may be waiting), :752-762, :778-785 | closed |
| T-48-14-01 | Denial of Service | cleanup strands a live waiter until its timeout | high | mitigate | CX-1 / PI-S2 acquisition guard holds the phase lock across cleanup; contended-lock controls prove state and gates are untouched — **Evidence:** recover.rs:137-145, :155-168 (lock before cleanup); recover.rs:570-637; gate_wedge_e2e.rs:123-167 | closed |
| T-48-14-02 | Repudiation | doctor prescribes a repair that does nothing | medium | mitigate | Shared `no_waiter_repair`; orphan-gate check yields to the new finding — **Evidence:** commands.rs:3353-3367 (`no_waiter_repair`), :3371-3374; tests commands.rs:6367-6459 | closed |
| T-48-14-03 | Tampering | doctor mutates state | low | mitigate | PI-1 read-only holder status plus report-only unchanged (T-18-02) — **Evidence:** commands.rs:3526 (read-only `holder_status`); test commands.rs:6804 | closed |
| T-48-15-01 | Elevation of Privilege | re-scan approval | high | mitigate | Only re-scan `Advance` records the freshly scanned declaration set; `LoopBack` preserves the prior set. A rejected Auto re-scan parks for Supervise repair without auto-deciding, and the resumed phase retains the rejection's prior approval set — **Evidence:** formal GSD re-audit 2026-09-20, `pipeline_launch.rs` re-scan action handling and Auto-park regression test | closed |
| T-48-15-02 | Tampering | Code preflight records from project root | high | mitigate | Worktree fixture proves recording and comparison use the same execution root — **Evidence:** preflight.rs:1311-1321 (execution root) matches pipeline_launch.rs:1717/1741; decoy-root tests preflight.rs:2582, :2684 | closed |
| T-48-15-03 | Repudiation | LoopBack silently accepts changed plans | medium | mitigate | LoopBack keeps approval unchanged and names unapproved plan files — **Evidence:** pipeline_launch.rs:1795-1803; preflight.rs:1422-1427; tests pipeline_launch.rs:4275-4393 | closed |
| T-48-16-01 | Tampering | stale response decides a later non-Ship gate | high | mitigate | NoHolder and Recycled write no response — **Evidence:** commands.rs:1414-1420, 1548-1559 + 1728-1730, 1903-1911 (status checked before writing); tests commands.rs:4157/4184/4225; gate_wedge_e2e.rs:94-110 | closed |
| T-48-16-02 | Repudiation | command claims a waiter without evidence | medium | mitigate | A waiter claim now requires the observed and current lock records to have the same live `(pid, start-time)` identity; a successor holder fails closed — **Evidence:** commands.rs `answered_gate_contention_message` and its same-identity/successor negative-control matrix, refreshed 2026-09-20 | closed |
| T-48-16-03 | Spoofing | recycled pid treated as a waiter | high | mitigate | Recycled holders receive no response at every gate, including Ship; Ship's stored-response exception is limited to `NoHolder` — **Evidence:** commands.rs Ship recycled/no-holder counterpart tests, refreshed 2026-09-20 | closed |
| T-48-17-01 | Tampering | stale test guidance | low | mitigate | Documentation uses the helper and lint names verified by Task 1 — **Evidence:** .planning/codebase/TESTING.md:69, :84, :91; named helpers exist; clippy.toml `disallowed-methods` | closed |
| T-48-17-02 | Repudiation | timing result presented as reliability proof | low | mitigate | SUMMARY records one-run and concurrent-load limits — **Evidence:** 48-17-SUMMARY.md:131-133 (Evidence Limits — the SUMMARY is the mitigation) | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

These three are plan-time `accept` dispositions, recorded here with the rationale the auditors
located. No open threat has been accepted at audit time.

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-48-01 | T-48-09-03 | The 21 non-PATH environment mutations stay process-global under `ENV_MUTEX`, each justified in-source; per-`Command` scoping cannot reach an in-process reader (48-CONTEXT.md D-09 :217-221; TESTING.md:93) | Plan-time disposition, 48-09-PLAN (D-09) | 2026-09-19 |
| AR-48-02 | T-48-10-04 | Filesystems without hard links fail with `GateError::Io`; no rename fallback, so no silent overwrite (48-CONTEXT.md:337; gates.rs:384-386, :412-414) | Plan-time disposition, 48-10-PLAN | 2026-09-19 |
| AR-48-03 | T-48-11-04 | An older monitor script without `--stage` falls back to an unbound advance (48-RESEARCH.md:374 F-8; main.rs:127; pipeline_launch.rs:1613-1618) | Plan-time disposition, 48-11-PLAN | 2026-09-19 |

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

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** pending
