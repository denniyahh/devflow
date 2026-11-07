---
phase: 51-rate-limit-agent-failover
plan: 13
subsystem: pipeline
tags: [rust, supervisor, liveness, failover, test-seam, evidence]

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "51-10: the trace, the first two experiments, run_handoff_child_with_stubs_and_env, group_recording_script, the resume fixtures and supv01_evidence_violations; 51-06: FailoverRelaunchGuard and the routing of a failed failover relaunch to the gate (R2-D1); 51-01: the handoff harness with a second agent stub"
provides:
  - "Test-only seam DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE=<agent>:<stage>:<marker-path> (successor_probe_for, #[cfg(test)]) that answers publish_record's post-save probe Gone, with its compiled-out pin"
  - "successor_runs_on takes its probe as a parameter (private fn; release builds pass devflow_core::supervisor::probe)"
  - "Six D-12 experiment tests: the post-save probe (failover and ordinary), the pending-record lock probe (failover and ordinary), and the cron arm's two resume lock probes"
  - "Complete mode of the SUPV-01 evidence checker (five further headings) and the_supv01_evidence_is_complete"
  - "51-SUPV01-EVIDENCE.md completed: every trace row a test or a gap, the per-arm conclusion, the limits, D-13 and the backlog findings"
affects: [51-11]

actuals:
  tokens: 21186
  tasks: 3
  commits: 5
plan_head_before: 21512e7ccd3e8e7fc60904204328101d50d9398d
plan_head_after: 5b7562c54ccec1ed8bd6b71d4444e299a3ec00ba

tech-stack:
  added: []
  patterns:
    - "A test seam that substitutes a probe's answer at the probe (a closure parameter), so the production match arms still decide in both builds"
    - "A planted on-disk record whose lock path cannot be probed, as a seam-free intervention for a lock read"

key-files:
  created:
    - .planning/phases/51-rate-limit-agent-failover/51-13-SUMMARY.md
  modified:
    - crates/devflow-cli/src/pipeline_launch.rs
    - doc-check-allowlist.toml
    - .planning/phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md

key-decisions:
  - "The post-save seam answers Gone (R2-8). The error arm returns the healthy branch's true, so it is recorded in the evidence file as settled by reading (pipeline_launch.rs:2555-2560), with no test"
  - "The pending-record tests also assert launch_refused_over_live_pending naming the planted launch in both runs. That ties each refusal to read (3) and not to some other refusal. It is an event commit_pending_record emits on both paths, not a cause string from the ordinary path (R2-9)"
  - "The intervention-removed runs were temporary tests, deleted before each commit, as in 51-10. The resume tests carry their baselines inside the test"
  - "D-13 found one natural rate limit in Phase 51's own dogfood run. It took the cron arm on a build that predates the failover code, so it is evidence for the cron arm only"
  - "No new backlog finding. The ordinary pending-record refusal is filed as a second reproduction under 51-06 finding 2 / 51-10's extension, not as a duplicate"

patterns-established:
  - "Before relying on a planted unprobeable path, assert that the probe errors on it (non-vacuity)"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "A Gone post-save answer for the failover relaunch records the run stopped with NEXT_STOPPED_BEFORE_SAVE_REASON while the Codex successor is still held; an ordinary Plan to Code launch disposes identically"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::failover_with_the_post_save_probe_answering_gone_records_the_run_stopped"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::an_ordinary_transition_with_the_post_save_probe_answering_gone_records_the_run_stopped"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage"
        status: pass
    human_judgment: false
  - id: D2
    description: "The post-save seam is compiled out of non-test builds, and its variable is allowlisted"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_successor_probe_seam_is_compiled_out_of_non_test_builds"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::source_devflow_env_vars_and_subcommands_are_documented"
        status: pass
    human_judgment: false
  - id: D3
    description: "Over a pending record whose lock cannot be probed, the failover relaunch is refused by commit_pending_record and ends at the Code gate with no cron arm; an ordinary Code launch is refused the same way"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::failover_over_an_unprobeable_pending_record_ends_at_a_gate"
        status: pass
      - kind: e2e
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::an_ordinary_transition_over_an_unprobeable_pending_record_is_refused_the_same_way"
        status: pass
    human_judgment: false
  - id: D4
    description: "A cron-arm resume and an ordinary resume refuse alike when the recorded lock or the pending lock cannot be probed; lock-missing baselines resume"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_cron_resume_and_an_ordinary_resume_refuse_alike_when_the_recorded_lock_cannot_be_probed"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::a_cron_resume_and_an_ordinary_resume_refuse_alike_when_the_pending_lock_cannot_be_probed"
        status: pass
    human_judgment: false
  - id: D5
    description: "The evidence file is complete and checked row by row in complete mode; the checker rejects planted stubs, including a fabricated text carrying the new headings"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-cli/src/pipeline_launch.rs#pipeline_launch::tests::the_supv01_evidence_check_rejects_a_stub"
        status: pass
      - kind: unit
        ref: "cargo nextest run -p devflow --bin devflow --run-ignored only --no-tests=fail -E 'test(=pipeline_launch::tests::the_supv01_evidence_is_complete)'"
        status: pass
    human_judgment: false
  - id: D6
    description: "The per-arm conclusion follows from the six experiments, and the cited lines hold the reads named"
    requirement: "SUPV-02"
    verification: []
    human_judgment: true
    rationale: "The checker proves citations exist, not that a cited line holds the read or that the conclusion's wording follows from the outcomes; that is a reading, re-done by hand at 5559cd2"

duration: 20min
completed: 2026-11-07
status: complete
---

# Phase 51 Plan 13: SUPV-01 Post-Save, Pending-Lock and Resume-Lock Experiments Summary

**Every liveness read the two rate-limit arms make has now been withheld with an ordinary control, or recorded as a gap with its reason. Each one disposes failover and an ordinary launch, or a cron resume and an ordinary resume, identically. The only difference is what follows a failed in-supervisor launch: failover opens the never-silent gate, and an ordinary transition is left ungated. The evidence file is checked in complete mode by a named Rust test.**

## Needs attention

- **The ordinary path's ungated launch failure was reproduced a second time.** In the pending-record control, the ordinary Plan → Code transition is refused. The run is then left at Code with no supervisor, not stopped, with no gate and nothing scheduled. This time a cause event exists (`launch_refused_over_live_pending`), but nothing prompts the operator. I added it to the existing 51-06 / 51-10 backlog finding rather than filing a duplicate.
- **D-13: Phase 51's own dogfood run hit a real rate limit**, at Code on 2026-11-06T16:27:56Z. It took the cron arm, and the resume on 2026-11-07 consumed it. That build (`46064c6`) predates the failover code, so this is cron-arm evidence only.

## Performance

- **Duration:** 20 min
- **Started:** 2026-11-07T23:38:38Z
- **Completed:** 2026-11-07T23:58:17Z
- **Tasks:** 3
- **Files modified:** 3 (plus this SUMMARY)

## Accomplishments
- **The seam (T-51-37).**
  - `successor_runs_on(socket, probe)` now takes its probe as a parameter. `publish_record` binds it with two cfg-split `let`s: the release build passes `devflow_core::supervisor::probe`, and the test build passes `successor_probe_for(state)`.
  - `successor_probe_for` answers `Gone` and writes a marker for the named `<agent>:<stage>`. For any other launch it is the real probe.
  - The source scan `the_successor_probe_seam_is_compiled_out_of_non_test_builds` plants seven cases, including an ungated decoy call and an ungated decoy read.
  - The seam variable is allowlisted. Negative control: with the entry removed, `doc_check` fails naming `DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE`.
- **Six experiments**, listed in the coverage block. Each in-supervisor test asserts the read's own effect: the marker, or `launch_refused_over_live_pending` naming the planted launch.
- **The checker's complete mode** requires five further headings. It has new planted cases: (k) in complete mode, (l) `## Coverage check` missing, and (m) no `Cron arm: ` line. The fabricated text is now rendered with the new headings and is still rejected with exactly its 16 unresolved-citation violations.
- **The evidence file is complete.** The changes:
  - five pending rows filled
  - the `pipeline_launch.rs` locations re-located
  - six new sections

## Task Commits

1. **Task 1 RED:** `a015a39` test(51-13): add failing tests for the post-save probe answered Gone
2. **Task 1 GREEN (tracer):** `59c9280` feat(51-13): add a test-only seam that answers the post-save probe Gone
3. **Task 2:** `5559cd2` test(51-13): withhold the pending-record and resume lock probes. Tests only. `git diff 5559cd2^..5559cd2 -- crates/devflow-cli/src/pipeline_launch.rs` has one hunk, at line 18205, past `mod tests {` (line 4158).
4. **Task 3 RED:** `a433b43` test(51-13): add failing complete-mode checks for the SUPV-01 evidence
5. **Task 3 GREEN:** `5b7562c` feat(51-13): complete the SUPV-01 evidence and check it in complete mode. Test-module code and the evidence file only.

## Files Created/Modified
- `crates/devflow-cli/src/pipeline_launch.rs`:
  - outside the tests: the probe parameter, the `SuccessorProbeAnswer` alias, `publish_record`'s two bindings, and the `#[cfg(test)]` seam
  - in the test module: seven tests, the complete-mode checker change and its planted cases
- `doc-check-allowlist.toml`: the `DEVFLOW_TEST_WITHHOLD_SUCCESSOR_PROBE` exception
- `.planning/phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md`: completed

## Experiment results

**Intervention removed, post-save probe.** Both rigs were run once with the seam unset, as temporary tests deleted before the commit.
- Failover: while Codex was held, `stopped=false`, `stop_reason=None`, agent Codex and one handoff. The record named the successor, which probed Alive. No marker existed.
- Ordinary: `stopped=false` at Code while held. The record named the successor, which probed Alive.
- On release, both completed Code (`workflow_finished` `stopped_at`).
- So the outcome flips against the seam runs.

**After the release, with the seam set (observed).** Both runs ended identically:
- The successor's agent finished, and its advance emitted `advance_failed` (reason `stopped`).
- The state kept `stopped` and the record naming the successor.
- The socket was gone within the poll.
- The successor did not rewrite the state, and no supervisor outlived the drop.

**Intervention removed, pending-record lock.** The planted lock was a missing file (`Missing`), in temporary tests.
- Failover: no refusal. Codex launched and completed Code, and the planted record was replaced.
- Ordinary: no refusal. Code launched under Claude and was recorded, and the planted record was replaced.

**Resume baselines** run inside each committed test. In both, a paused fixture with the lock missing resumes and records its own supervisor. The fixtures differ from the refused ones only in the lock.

**Mutations, each restored with `cmp`.**
- `held_liveness_holder`'s `Err` arm changed to `None`: the read (6) test fails with `phase 5135: resume over an unprobeable lock launched`.
- `pending_live_socket`'s `Err` arm changed to `return None`: the read (7) test fails with `phase 5138: …`.

**Real-file negative control.** I planted `pipeline_launch::tests::no_such_test` in the read (7) row and broke the `Cron arm: ` line. `the_supv01_evidence_is_complete` then reported both. The file was restored and confirmed with `cmp`.

**Re-located `Location` cells** (in `pipeline_launch.rs`, 51-10's value → the value at `5559cd2`):
- (1): `3201` → `3241`, `2645` → `2685`
- (2): `2387` → `2391`, `2525` → `2537`, `2534` → `2546`
- (3): `2635` → `2675`, `2687` → `2727`, `2697` → `2737`
- (4): `2712` → `2752`
- `remove_abandoned_waiters`: `3562` → `3602`
- The prose in the baseline paragraph: `3187-3201` → `3227-3241`, and `3300` → `3340`

`commands.rs`, `monitor.rs`, `liveness.rs`, `gates.rs`, `preflight.rs` and `supervisor.rs` are unchanged since `21512e7`. I read every new location at `5559cd2` (`2391`, `2537`, `2546`, `2550`, `2555-2560`, `2394-2396`, `2675`, `2727`, `2737`, `2752`, and `commands.rs:1016`, `1104`, `1107`).

## TDD Gate Compliance

The RED → GREEN pairs are `a015a39` → `59c9280` and `a433b43` → `5b7562c`. Task 2 measures existing behaviour, so it has no GREEN. Its RED-equivalent is the intervention-removed runs, the in-test baselines and the two mutations. No REFACTOR commit was needed.

RED evidence came from nextest JUnit reports (a scratch `red` profile) passed through `gsd-tools check tdd-red-evidence`. All four records returned `RED_EVIDENCE_OK` (`target/red-evidence/51-13-*.json`, gitignored).

| Test | Failed on | Semantic assessment |
|------|-----------|---------------------|
| `the_successor_probe_seam_is_compiled_out_of_non_test_builds` | real file: `no literal std::env::var read of …`, `fn successor_probe_for( is not defined exactly once` | Intended: the planted cases passed first, and the seam did not exist |
| `failover_with_the_post_save_probe_answering_gone_records_the_run_stopped` | `the run was never recorded stopped` after 45 s. The state was agent Codex, not stopped, with the successor recorded | Intended: nothing answered `Gone`, so the relaunch was handed off |
| `an_ordinary_transition_with_the_post_save_probe_answering_gone_records_the_run_stopped` | the same, with Code under Claude handed off | Intended |
| `the_supv01_evidence_check_rejects_a_stub` | `(b) complete mode` found 7 violations, not 12 | Intended: the checker did not yet require the five headings |
| `the_supv01_evidence_is_complete` | 51-10's file: five `still pending` rows and both conclusion lines missing | Intended: the state before Task 3 |

## Verification

| Check | Command | Result |
|-------|---------|--------|
| Task 1 gate (re-run as the tracer gate after `59c9280`, and again at the end) | the plan's 5 chained `cargo nextest run … --no-tests=fail -E 'test(=…)'` | exit 0 |
| Task 2 gate | the plan's 4 chained nextest commands | exit 0 |
| Task 3 gate | the plan's 5 chained nextest commands, the two real-file ones with `--run-ignored only` | exit 0 |
| Plan-level | `cargo test -p devflow --bin devflow` | 683 passed, 0 failed, 4 ignored (the 4th is the new real-file test) |
| Plan-level | `cargo test -p devflow-core --lib` | 1139 passed, 0 failed, 2 ignored |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo clippy -p devflow --bin devflow -- -D warnings` (non-test build, where the seam is compiled out) | exit 0 each |
| Scope | `git diff --stat 21512e7..HEAD` | the 3 files above. `Cargo.lock` and `devflow-core` are unchanged, so no semver check applies (T-51-SC) |

**What these checks do not establish.**
- **The two evidence-file tests are `#[ignore]`d,** because `develop` and `main` carry no `.planning/`. They run only in 51-10's and this plan's gates.
- **The checker's reach.** It proves that every cited file, line and test exists. It does not prove that a cited line holds the read, or that a cited test exercises it.
- **A substituted answer, not a real end.** A `Gone` answer while the successor still runs is a substituted answer, not a successor that really ended. The pending records are planted, not written by a racing launcher.
- **Run counts.** Each e2e test passed on every run it got, which is at least 3 runs each, and 5 for the failover post-save test (the gates, the tracer and Task 3 re-runs, and the full suite). That is a weak bound and does not size flakiness. The e2e tests take about 0.2 to 1.2 s each, and the read (6) test about 4 s (two 2 s lock-release waits).

## Decisions Made
- **Gone, not an error, for the post-save probe** (R2-8). The error arm is in the evidence file as settled by reading.
- **Asserting `launch_refused_over_live_pending` in both pending-record runs.** It is how each refusal is attributed to read (3), and it is identical on both paths.
- **A `SuccessorProbeAnswer` type alias.** It keeps the probe parameter readable after `rustfmt`. It is private and adds no behaviour.

## Deviations from Plan

None. The plan was executed as written. Three notes:
- The pending-record runs took the expected path; neither changed course before the relaunch.
- R2-D1 holds: the failover run opened the gate and wrote no cron instruction.
- The resume baselines resumed with no fixture changes beyond the lock.

## Backlog finding for the orchestrator

None new. The ordinary pending-record refusal is a second reproduction of 51-06 finding 2, as extended by 51-10: an in-supervisor launch failure outside failover is left ungated. It is recorded in the evidence file's `## Backlog findings`, which also carries 51-06's R2-2 cron-arm window. File one `999.x` entry for the ungated failure, as 51-10 asked.

## Issues Encountered
- The first e2e passes took about 0.2 s, against 5 to 6 s for 51-10's publish-wait tests, so I checked before trusting them. The temporary observation runs (0.77 s including a 500 ms sleep) and the RED runs (a 45 s timeout without the seam) show the short time is real: no 5 s promotion expiry is involved here.

## Known Stubs
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Criterion 3 rests on all six experiments plus the stated gaps. 51-11 can cite `51-SUPV01-EVIDENCE.md`'s `## Conclusion` lines.
- Scratch files are removed (`gsd-orch-scratch/exec-51-13/`). The RED records stay in the gitignored `target/red-evidence/`. No test process or `/tmp/df*` dir was left behind.

## Self-Check: PASSED
- FOUND: `crates/devflow-cli/src/pipeline_launch.rs`, `doc-check-allowlist.toml`, `.planning/phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md`
- FOUND (ancestors of HEAD): `a015a39`, `59c9280`, `5559cd2`, `a433b43`, `5b7562c`
- Commits measured from the ledger: `git rev-list --count 21512e7..5b7562c` = 5
