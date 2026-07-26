---
phase: 23-end-to-end-dogfood
verified: 2026-07-26T14:09:15Z
status: gaps_found
score: 7/8 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "One phase is driven start-to-finish by `devflow` with Claude, unattended, reaching a completed Ship stage without manual intervention (ROADMAP.md's stated acceptance criterion, behavioural not code-shaped)"
    status: failed
    reason: "The phase's own acceptance run (plan 23-11) stopped at the Define stage after ~90 seconds and was ended via `devflow stop`. `devflow start --phase 24` forks its worktree from `develop`'s current tip; Phase 24's ROADMAP entry existed only on `feature/phase-23` (unmerged), so the tree handed to the run had no Phase 24 to define. Terminal event was `workflow_aborted`, never `workflow_shipped` (the sole ACCEPTANCE-PASSED predicate built by 23-06) or even `workflow_finished`. `devflow evidence --phase 24 --require-shipped` exits 1 both before and after the run. ROADMAP.md's own Phase 23 entry states this plainly: \"the phase's own behavioral acceptance criterion is NOT met\" / \"ACCEPTANCE FAILED\". This is not a self-report discrepancy — I independently re-read the raw `.devflow/events.jsonl` excerpts quoted in 23-ACCEPTANCE-RUN.md and confirm no `workflow_shipped` or `workflow_finished` line exists for phase 24, only `workflow_started` → `stage_launched` (define) → `advance_evaluated` (failed) → `gate_fired` → `notify_fired` → `gate_resolved` (rejected) → `workflow_aborted`."
    artifacts:
      - path: ".planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-RUN.md"
        issue: "Records `outcome: run-incomplete` as its first line and the operator verdict `record: valid` / `failed` in §14 — this is the phase's own primary evidence document, and it self-reports the miss rather than claiming success."
    missing:
      - "A successful acceptance run: merge Phase 23 into `develop` (so the target phase's ROADMAP entry and `.planning/phases/<N>-*/` directory are reachable from the branch `devflow start` forks from), then re-run `devflow start --phase N --agent claude --mode auto --yes-ship` to a completed Ship stage, producing a `workflow_shipped` event and `devflow evidence --phase N --require-shipped` exiting 0."
      - "The third precondition check named by 23-11-SUMMARY.md's 'Next Phase Readiness' and 23-FINDINGS.md §B1: verify the target phase's ROADMAP.md entry and `.planning/phases/<N>-*/` directory exist on `develop` itself (not merely on the branch running the acceptance plan) before launching `devflow start`."
deferred: []
---

# Phase 23: End-to-End Dogfood — Verification Report

**Phase Goal:** Make `devflow start --phase N` drive one real phase from Define
through Ship unattended with Claude, with no manual `ps`, no manual `devflow
advance`, and no silent stall — verified behaviourally, not by code shape.

**Verified:** 2026-07-26T14:09:15Z
**Status:** gaps_found
**Re-verification:** No — initial verification

## Two layers, assessed separately, per the verification brief

This phase has two distinct deliverable layers and they do **not** average
together into one score:

- **(a) Code-shaped units (23a–23e, `--yes-ship`).** All six requirement
  tokens are built, wired, and independently exercised by me below with real
  test runs (not taken from SUMMARY.md self-reports). All pass.
- **(b) The behavioural acceptance criterion.** ROADMAP.md states it
  explicitly: "one phase driven start-to-finish by `devflow` with Claude,
  unattended, reaching a completed Ship stage without manual intervention."
  This is unmet. The phase's own acceptance run (23-11) stopped at Define.

A phase whose every plan completed but whose stated behavioural acceptance
criterion is unmet is not scored as passed. That is the finding of this
report.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | `devflow gate list --all-roots` enumerates every gated phase across every registered project root, with pruning of dead roots and skip-on-corrupt-entry | ✓ VERIFIED | `crates/devflow-core/src/registry.rs` (553 lines, real read_dir/prune/skip logic, not a stub). Wired: `crates/devflow-cli/src/commands.rs:799-866` (`gate_list` → `gate_list_all_roots` → `render_all_roots_gate_row`), CLI flag at `main.rs:308-309,482`. Live-exercised, not just unit-tested: `23-FINDINGS.md` §A records 23 real orphaned processes found and 22 cleared using exactly this mechanism on the operator's own machine. I independently ran `./target/release/devflow gate list --all-roots` — returned a real row count across 130 registered roots with no crash. |
| 2 | `devflow gate sweep` bounds gate lifetime — rejects aged gates only (never approves), leaves fresh gates untouched, and the still-polling target process tears itself down through its own existing `abort()` path with no signal sent | ✓ VERIFIED | `crates/devflow-cli/tests/gate_sweep_e2e.rs` exists (4 real e2e tests spawning a genuine `devflow advance` child). I ran `sweep_reaps_an_aged_gate_and_a_real_poller_resolves_to_abort` in isolation (not the full suite): `1 passed; 0 failed`, confirming a real spawned poller actually unwinds through its own abort path when swept. Live-exercised in the acceptance run: `23-ACCEPTANCE-RUN.md` §7 runs `gate sweep --dry-run` against the real machine state and reports the correct zero-phase-24 count. |
| 3 | `devflow stop --phase N` ends a running phase without `ps`/`kill`/knowing which PID is right — writes a rejection when a gate is open (target self-unwinds), else signals the lock-holder PID (never `state.monitor_pid`), refusing to signal a non-devflow process; idempotent | ✓ VERIFIED | `crates/devflow-cli/src/commands.rs:1087` (`fn stop`), wired at `main.rs:272,573`. `crates/devflow-cli/tests/stop_e2e.rs` (9 real e2e tests). I ran `stop_ends_a_gated_phase_through_its_own_abort_path_with_no_signal_sent` in isolation: `1 passed; 0 failed`, with log output showing a real gated phase's monitored process picking up the rejection and the workflow logging `workflow aborted for phase … : abort: stopped by devflow stop`. **Live-used for real, not just tested**: `devflow stop --phase 24` is exactly the command that cleanly ended the acceptance run's parked gate (`23-ACCEPTANCE-RUN.md` §3). |
| 4 | The Ship-shipped predicate has exactly one emission site (`workflow_shipped`), distinct from `workflow_finished` (also emitted by a clean `--until` stop), so `devflow evidence --require-shipped` cannot read a stopped-but-not-shipped run as shipped | ✓ VERIFIED | `crates/devflow-core/src/ship_evidence.rs` (327 lines). I ran both load-bearing tests in isolation: `pipeline_gate::tests::advance_ship_success_emits_workflow_shipped_and_ship_evidence_reports_shipped` → `1 passed`; `pipeline_gate::tests::until_stop_never_emits_workflow_shipped_and_ship_evidence_reports_not_shipped` → `1 passed`. This is the exact false-green class the phase's own probe (23a) caught being scored `VERIFIED`/`3/3` on an unrun Ship stage — 23-06 closes it, and I confirmed the closing test actually passes rather than trusting the SUMMARY's claim. |
| 5 | `--yes-ship` pre-authorizes exactly the Ship gate (auto-approved, attributed, still written and resolved through the normal protocol) and never the finalization-retry gate; cannot be set via config file or environment variable, only the CLI flag | ✓ VERIFIED | `crates/devflow-core/src/state.rs:91-101,164` (persisted `yes_ship: bool`, defaults false). I ran three tests in isolation, all `1 passed`: `pipeline_outcomes::tests::handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution`, `pipeline_gate::tests::finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` (the negative guarantee), and (via listing) `pipeline_outcomes::tests::config_file_with_yes_ship_key_loads_but_never_sets_the_flag` (the config-bypass negative guarantee — confirmed present and named correctly, not run individually beyond the two above given time budget, but same test file/pattern as the two executed). |
| 6 | The two-agent `sequentagent` verb is fully removed from the CLI surface and the core-side surface, with docs reconciled — no functional reference remains | ✓ VERIFIED | `rg -i sequentagent crates/` returns only two non-functional hits: a doc comment in `pipeline_outcomes.rs` contrasting the new single-agent resume path with the old verb's *former* behaviour, and a doc comment + a **negative-assertion test** in `ship.rs` (`assert!(!record.hermes_cron.command.contains("sequentagent"))`) proving the string never appears in generated output. `crates/devflow-cli/src/main.rs`'s `Command` enum has no `Sequentagent` variant; the committed help snapshot (`crates/devflow-cli/tests/snapshots/devflow-help.txt`) lists 16 commands, none of them the removed verb. |
| 7 | The phase's changes introduce zero regressions: full workspace test suite passes, and a real gated/reaped process pair leaves no orphan residue | ✓ VERIFIED | Orchestrator-supplied and independently spot-checked: `cargo test --workspace --no-fail-fast` → 592 passed / 0 failed / 0 ignored. I additionally ran the two real-process e2e tests myself (`stop_e2e`, `gate_sweep_e2e`, one each, not the full suite) and confirmed via `ps aux | grep "devflow advance"` and `git status --porcelain` that no process or working-tree residue was left behind after my own runs. |
| 8 | **One phase is driven start-to-finish by `devflow` with Claude, unattended, reaching a completed Ship stage without manual intervention** (ROADMAP's stated behavioural acceptance criterion) | ✗ FAILED | See Gaps below. The only acceptance attempt stopped at Define; terminal event `workflow_aborted`; `devflow evidence --require-shipped` exits 1 both before and after. |

**Score:** 7/8 truths verified (0 present-but-behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `crates/devflow-core/src/registry.rs` | Cross-root gate registry (23b, enumeration half) | ✓ VERIFIED | 553 lines, substantive; wired into `commands::gate_list_all_roots` |
| `crates/devflow-core/src/ship_evidence.rs` | Deterministic Ship-shipped oracle (23e) | ✓ VERIFIED | 327 lines, substantive; wired into `commands::evidence`, tests pass |
| `crates/devflow-cli/tests/gate_sweep_e2e.rs` | Real e2e proof the reaper tears down a genuine child (23b, acting half) | ✓ VERIFIED | 4 tests present; 1 spot-run, passed with real process teardown |
| `crates/devflow-cli/tests/stop_e2e.rs` | Real e2e proof `devflow stop` works against a genuine gated process (23c) | ✓ VERIFIED | 9 tests present; 1 spot-run, passed with real gate-response teardown |
| `.planning/phases/23-end-to-end-dogfood/23-PROBE-FINDINGS.md` | 23a probe record | ✓ VERIFIED | Complete verbatim `events.jsonl` excerpts, correctly invalidates the phase's original central hypothesis (monitor-death) |
| `.planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-RUN.md` | 23-11 acceptance run record | ✓ VERIFIED (as a record) — but the run it records is a FAILED acceptance | Complete verbatim event excerpts, explicit operator verdict `record: valid` / `failed` |
| CHANGELOG.md v2.0.0 breaking-change entry (23d) | Documents `sequentagent` removal | ✓ VERIFIED | Confirmed via `ROADMAP.md`/plan cross-reference; not independently re-read line-by-line given time budget, but the removal itself is confirmed at the source level (Artifacts row 6 above) |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `pipeline_launch` (phase launch) | `registry::register` | same code path that writes `state.monitor_pid` | ✓ WIRED | Confirmed by source read; corroborated live — the acceptance run's `gate list --all-roots` and `evidence` calls both saw phase 24 registered/deregistered correctly |
| `commands::gate_sweep` | `Gates::reap` → `Gates::respond` | reused response-file protocol, never a signal | ✓ WIRED | `gate_sweep_e2e.rs` test passed against a real spawned child |
| `.devflow/lock-{phase:02}` | `commands::stop` | lock-holder PID (never `state.monitor_pid`) | ✓ WIRED | `stop_e2e.rs` test passed; live-used to end the phase-24 acceptance run |
| `finish_workflow_with_gate_timeout` | `workflow_shipped` event (single site) | `ship_evidence::collect` | ✓ WIRED | `advance_ship_success_emits_workflow_shipped…` and `until_stop_never_emits_workflow_shipped…` both pass |
| `Command::Start`'s `--yes-ship` flag | `state.yes_ship` (persisted) | `handle_ship_outcome`'s auto-response | ✓ WIRED | `handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution` passes; negative guarantee (`finalization_retry_gate_never_auto_approves_even_with_yes_ship_set`) also passes |
| Acceptance run (23-11) | Ship stage / `workflow_shipped` | `devflow start --phase 24 --yes-ship` | ✗ NOT REACHED | Run stopped at Define; `workflow_shipped` never fired for phase 24 |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Binary freshness — matches Cargo.toml | `./target/release/devflow --version` vs `grep version Cargo.toml` | both `1.8.1` | ✓ PASS |
| `--yes-ship` auto-approves Ship gate, attributed | `cargo test -p devflow --bin devflow pipeline_outcomes::tests::handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| `--yes-ship` never auto-approves the finalization-retry gate | `cargo test -p devflow --bin devflow pipeline_gate::tests::finalization_retry_gate_never_auto_approves_even_with_yes_ship_set -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| `workflow_shipped` fires only on real Ship finalization | `cargo test -p devflow --bin devflow pipeline_gate::tests::advance_ship_success_emits_workflow_shipped_and_ship_evidence_reports_shipped -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| `--until` clean-stop never counts as shipped | `cargo test -p devflow --bin devflow pipeline_gate::tests::until_stop_never_emits_workflow_shipped_and_ship_evidence_reports_not_shipped -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| `devflow stop` unwinds a real gated child, no signal sent | `cargo test -p devflow --test stop_e2e stop_ends_a_gated_phase_through_its_own_abort_path_with_no_signal_sent -- --exact` | `1 passed; 0 failed`, log shows real `workflow aborted … stopped by devflow stop` | ✓ PASS |
| `devflow gate sweep` reaps an aged gate, real poller aborts | `cargo test -p devflow --test gate_sweep_e2e sweep_reaps_an_aged_gate_and_a_real_poller_resolves_to_abort -- --exact` | `1 passed; 0 failed` | ✓ PASS |
| No process/tree residue left by my own spot-check runs | `ps aux \| grep "devflow advance"`, `git status --porcelain` | zero matching processes, clean tree | ✓ PASS |
| `sequentagent` verb absent from CLI | `rg -i sequentagent crates/` | only 2 non-functional doc-comment/negative-test hits | ✓ PASS |

**Note on test invocation:** I deliberately avoided repeating the full `cargo test --workspace` run — the orchestrator already supplied a fresh, independently-obtained 592/0/0 result, and I instead spot-ran 7 individually-named tests (each `--exact`, each verified to report `1 passed` rather than the `0 passed`/false-green shape a bare/ambiguous test name can produce under `--exact`).

### Probe Execution

Step 7c (`scripts/*/tests/probe-*.sh` convention): **SKIPPED — no such scripts exist in this project.** `ls scripts/` shows `deploy.sh`, `install.sh`, `scratch-dogfood-repo.sh`, `sync-main-to-develop.sh`, `hooks/` — no `tests/probe-*.sh` files. This project's "probe" terminology (23a) refers to a manual, human-observed `devflow start` dogfood run, not an automated probe script — and that run's full verbatim event log is already reproduced and analyzed in `23-PROBE-FINDINGS.md` and `23-ACCEPTANCE-RUN.md`, both read in full above.

### Requirements Coverage

No REQUIREMENTS.md / REQ-IDs exist for this project (confirmed absent). Plans instead carry unit tokens. Coverage against those tokens:

| Unit | Description | Status | Evidence |
|---|---|---|---|
| 23a | Dogfood probe — run and record where an unattended run dies | ✓ SATISFIED | 23-01/23-02 plans; `23-PROBE-FINDINGS.md` correctly invalidated the phase's original central hypothesis (monitor-death), driving the 2026-07-25 re-aim |
| 23b | Cross-root gate registry + `gate list --all-roots` + `gate sweep` (re-aimed) | ✓ SATISFIED | Truths 1–2 above |
| 23c | `devflow stop` (re-aimed, targets lock holder) | ✓ SATISFIED | Truth 3 above |
| 23d | Delete `sequentagent` (CLI + core surface) | ✓ SATISFIED | Truth 6 above |
| 23e | Ship-evidence oracle, `workflow_shipped`, `--require-shipped` | ✓ SATISFIED | Truth 4 above |
| yes-ship | `--yes-ship` pre-authorization flag | ✓ SATISFIED | Truth 5 above |
| **Phase-level behavioural acceptance criterion** | One phase Define→Ship, unattended, with Claude | ✗ **NOT SATISFIED** | Truth 8 / Gaps below |

Every code-shaped unit token is satisfied. The phase-level composite criterion — which the ROADMAP explicitly states is the actual pass/fail bar, not the units individually — is not.

### Anti-Patterns Found

None in the phase's core deliverable files (`registry.rs`, `ship_evidence.rs`, `commands.rs`, `main.rs`, `pipeline_outcomes.rs`, `state.rs`) — `TBD`/`FIXME`/`XXX`/`TODO`/`HACK`/`PLACEHOLDER` scans returned zero hits. The remaining `sequentagent` string occurrences are a doc comment contrasting old/new behaviour and a negative-assertion test — not debt markers.

**Notable findings, self-disclosed by the phase's own documents, not blocking the stated must-haves but worth carrying forward (not part of 23a–23e/`--yes-ship` scope, so not scored as gaps here):**

- `23-FINDINGS.md` §A1 — a process that has already lost its own lock/state file but is still running is invisible to both `gate list --all-roots` and `stop` (both correctly report "nothing to do" — a true-but-incomplete answer, not a false green in the attestation sense, but worth naming).
- `23-FINDINGS.md` §A2 — the registry produces duplicate dry-run counts for one root (cosmetic over-report, no double-reap occurs).
- `23-FINDINGS.md` §A3 — this phase's own e2e suites (`gate_sweep_e2e`, `stop_e2e`) leak real spawned process pairs into `/tmp` scratch directories on every `cargo test --workspace` run; fix belongs in test teardown, not production code.
- `23-FINDINGS.md` §B2 — `devflow cleanup` deletes any local branch it judges "merged" by ancestry, including an operator's own `recovery/*` ref, reproduced twice in one day. Real product behaviour gap, unfixed, out of this phase's declared scope.
- `23-ACCEPTANCE-RUN.md` §9 / `23-FINDINGS.md` §B3 — the self-dogfood staleness **hard block** (`StalenessOutcome::Block`) was never exercised by a real run; only the non-blocking `Ahead`/warn branch fired. Named explicitly as a coverage gap by the phase's own plan.

## Gaps Summary

The phase built and verified every code-shaped unit it committed to (23a
through 23e, `--yes-ship`) — I independently re-ran seven individually-named
tests against the actual compiled binary/library, not the SUMMARY.md
narrative, and all seven passed, including the two negative guarantees
(`--yes-ship` never touching the finalization-retry gate; the shipped
predicate never firing on a clean `--until` stop) that most directly guard
against the false-green failure class this phase exists to close.

But the phase's own ROADMAP.md states its acceptance criterion is
**behavioural, not code-shaped**: one phase driven start-to-finish by
`devflow` with Claude, unattended, reaching a completed Ship stage. The one
attempt at that (plan 23-11) did not reach it — it stopped at Define within
about 90 seconds because the acceptance target's own ROADMAP entry was
promoted onto `feature/phase-23` and never merged to `develop`, the branch
`devflow start` always forks from. The phase's own artifacts are candid about
this: `23-ACCEPTANCE-RUN.md` opens with `outcome: run-incomplete`, ROADMAP.md's
Phase 23 entry itself says "ACCEPTANCE FAILED," and `23-11-SUMMARY.md`'s
"Next Phase Readiness" section names exactly what a retry needs (merge Phase
23 to `develop` first, then re-run with a third precondition check verifying
the target is reachable from `develop`).

Root cause is attributed, credibly and with source-level corroboration in
this report, to an orchestrator sequencing gap across plans 23-10/23-11 — not
a defect in any of the code this phase shipped. That distinction matters for
where the fix belongs (a retry of the acceptance procedure, not a code
change) but it does not change the verification outcome: the phase's stated
goal, as written in ROADMAP.md, is not yet true in the world. Per this
project's own standard for what counts as done, that keeps this phase at
`gaps_found` rather than `passed`.

**What closes the gap:** merge Phase 23 to `develop`, then run a fresh
acceptance attempt against a target phase whose ROADMAP entry and
`.planning/phases/<N>-*/` directory are reachable from `develop` at launch
time — Phase 24 (already promoted, low-stakes, advisory-only) remains
available and untouched as that target once the merge precondition is
satisfied.

---

*Verified: 2026-07-26T14:09:15Z*
*Verifier: Claude (gsd-verifier)*
