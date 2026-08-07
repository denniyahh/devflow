---
status: complete
phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-
source: [35-01-SUMMARY.md, 35-02-SUMMARY.md, 35-03-SUMMARY.md, 35-04-SUMMARY.md, 35-05-SUMMARY.md, 35-06-SUMMARY.md]
started: 2026-08-07T19:30:31Z
updated: 2026-08-07T20:40:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Signing viability is probed, not predicted
expected: `devflow release --check` reports tag-signing Viable for an on-disk private key with no agent holding it, from a real `ssh-keygen -Y sign` probe; no key value, path, or ssh-keygen stderr fragment leaks into the output. (999.86 false-negatived two release cuts.)
result: pass
note: "Operator ran the premise check (key on disk, fingerprint absent from the agent), the command (signing viable, <1s), an independent sign control (SSH_AUTH_SOCK= ssh-keygen -Y sign, exit 0) and the leak grep (0 hits, with the grep's own control firing at 1). The printed public-key fingerprint was judged in-spec: D5 forbids the key value, a path, and ssh-keygen stderr, none of which a fingerprint is."

### 2. The signing probe cannot be captured by a controlling terminal
expected: Run from an interactive terminal, `devflow release --check` returns in well under a second rather than parking for ~10s on a `/dev/tty` passphrase prompt. (D8 — the coverage block still classifies this as human-judgment because it shipped untested; a committed test now exists.)
result: pass
note: "Passed on the committed test, NOT on the live command. `git::tests::the_signing_probe_is_not_captured_by_a_controlling_terminal` (git.rs:3028) was run during this UAT: `1 passed; 575 filtered out`. Its arm 1 is a premise check that fails loudly if the pty never took effect, and arms 0/1 must disagree. LIMIT: on this host the configured key is unencrypted, so `devflow release --check` returns fast with or without the setsid — the live command cannot discriminate here and was not treated as evidence. This also supersedes the 35-03 coverage block's `human_judgment: true` rationale for D8, which still reads 'no committed test covers this' and is now stale."

### 3. The dry-run preview names the new per-phase failure ceiling
expected: `devflow start --phase N --mode auto --dry-run` prints a validate line carrying BOTH `[GATE after 3 consecutive failures]` and `[GATE at 10 validate failures for this phase]`. In `--mode supervise` the validate line stays a bare `[GATE]`, because the ceiling changes nothing about where a supervised run stops.
result: pass
note: "Verified with a real before/after: the Homebrew-installed pre-phase-35 binary prints `[GATE after 3 failures]` (one clause, older wording) while target/debug/devflow prints both clauses. Second control: `--mode supervise` keeps a bare `[GATE]`, so the clause is keyed on the ceiling predicate rather than unconditionally appended — the exact direction 35-04's F-7 deviation records as having been unreachable under the plan's specified ordering. Read-only confirmed: no .devflow/state-07.json created, git status unchanged. NOTE: both binaries self-report `devflow 2.4.0`, so the version string cannot distinguish them; the path is the discriminator. LIMIT: this is the preview string only — it says nothing about the ceiling actually firing at 10 (test 5)."

### 4. The Validate gate message leads with the per-phase total
expected: A Validate gate's context now reads `Validation has failed N time(s) for this phase (M in the current consecutive streak) — human review needed.` At the ceiling it gains a clause saying the run is paused for a human, not aborted — approve to ship, reject to loop back, or abort.
result: pass
note: "Operator passed without requesting steps; no live gate was fired during this UAT. Evidence re-run here rather than transcribed: `validate_gate_message_leads_with_the_per_phase_total` and `ceiling_clause_appears_only_at_the_ceiling_even_in_supervise_mode` each `1 passed; 302 filtered out`. 35-04 records NC-6 as the discriminating control (reverting the format string to interpolate only consecutive_failures makes the 1st and 5th gate read identically, which is WR-04's complaint verbatim) and F-6's control (an unconditional append makes a below-ceiling Supervise gate carry the ceiling clause). LIMIT: those two mutations were performed once during 35-04 and are NOT re-run by cargo test; this UAT observed only the green direction."

### 5. A Code↔Validate loop that commits trivial artifacts now terminates
expected: A phase whose Code stage commits `.planning/` artifacts every cycle no longer loops unbounded. The per-phase Validate-failure total accumulates independently of the commit count, and at 10 it gates for a human. Nothing resets it on a rising commit count; it survives `devflow start --force` and clears only on phase completion or operator approval at the ceiling gate. (999.78)
result: pass
note: "Five tests re-run during this UAT, each `1 passed; 302 filtered out`: phase_validate_failure_ceiling_gates_despite_trivial_commit_progress, phase_validate_failures_survive_a_forced_restart, phase_validate_failures_reset_when_the_phase_completes, phase_validate_failures_reset_on_operator_approval_at_the_ceiling_gate, phase_validate_failures_increment_saturates. 35-04's NC-5 is the discriminating control (removing the ceiling disjunct reproduces the unbounded loop: ten cycles, streak pinned at 1, no gate). LIMITS carried forward, none closed by this UAT: (a) NO end-to-end run exists anywhere in phase 35 — every test drives handle_validate_outcome directly with a pre-seeded gate response; no real agent ran and no monitor spawned; (b) the bound is on RECORDED failures only — a hung agent or dead monitor is not bounded by it (999.85, out of scope); (c) the carry-forward is tested at fresh_state_carrying_phase_failures, not through a real start(), so an edit moving it out of start()'s path would keep the test green; (d) MAX_PHASE_VALIDATE_FAILURES = 10 is a judgement, not a measurement — see open item."

### 6. A transient git fault no longer buys a free extension of the failure ceiling
expected: A cycle whose commit count could not be measured leaves `last_validate_failure_commit_count` byte-identical instead of writing zero. Previously one transient git fault made the next real count read as forward progress, resetting the consecutive-failure streak to 1 — one free extension of MAX_CONSECUTIVE_FAILURES per fault. (999.77)
result: pass
note: "Re-run during this UAT: validate_failure_with_unmeasurable_count_accumulates_the_streak `1 passed; 302 filtered out`. NC-4's discriminating pair passes in the SAME run and must disagree on cause, not on value — phase_commit_count_reports_zero_without_a_branch (git ran, branch genuinely absent, Some(0)) alongside phase_commit_count_reports_none_when_git_cannot_run (None); the split is on 'did the command run', not on 'was the answer zero'. 35-01's NC-2 is the mutation control (restoring the unconditional .unwrap_or(0) write makes the two-cycle test fail with left Some(0) / right Some(1)), and NC-3 shows a single-cycle variant would NOT discriminate — it is identical under buggy and fixed code, which is precisely why the sequence has two cycles. LIMIT: NC-2 and NC-3 were one-time mutations during 35-01 and are not re-run by cargo test."

### 7. An unmeasurable commit count reports Unknown, not Failed
expected: With exit code 0 on a commit-gated stage and an unrunnable git, classification reports Unknown with `commits: None` — at Layer 3 as well as Layer 2. Previously both layers collapsed "could not count" into "counted zero" and reported Failed with a reason naming the branch. (999.87)
result: pass
note: "Seven tests re-run during this UAT. Layer 3 (3 passed; 573 filtered out) includes a discriminating pair that RE-RUNS on every cargo test: evaluate_layer3_unmeasurable_count_is_unknown_not_failed alongside evaluate_layer3_zero_commits_is_failed_and_flags_human_review — an unmeasurable count is Unknown while a genuine zero is still Failed, so the fix did not simply stop reporting Failed. Cascade + Layer 2 (4 passed; 299 filtered out): evaluate_agent_result_with_unrunnable_git_does_not_report_failed, evaluate_layer2_unrunnable_git_falls_through_to_layer3, plus the two tests added by the CR-01/IN-03 correction that pair an unmeasurable count with a NON-zero exit (exit 137 stays ResourceKilled; a non-commit-gated stage stays Success). 35-01's NC-12 is the key control: under a Layer-3-only revert the cascade test FAILS while the layer-level test PASSES — that asymmetry is the direct demonstration that a Layer-2-only unit test was the proxy which hid the Layer 3 defect through planning. LIMIT: NC-12 was a one-time mutation and is not re-run by cargo test. LIMIT: AgentStatus::Failed and ::Unknown both map to Action::GateReview, so this changes the recorded classification and the operator-facing reason, NOT what the run does next."

### 8. Worktree-mode checkpoints are read from the execution root
expected: When `state.worktree_path` is set, the GateReview checkpoint auto-decide arm reads the blocking-human PLAN from the worktree, not from the project root. Reverting the argument makes the new regression test fail with zero `checkpoint_auto_decided` events while its no-worktree sibling still passes. (999.84)
result: pass
note: "Both re-run during this UAT, each `1 passed; 302 filtered out`: advance_with_worktree_declared_checkpoint_reads_the_execution_root and its byte-unchanged no-worktree sibling advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records. 35-02 performed the revert and recorded verbatim output for both directions — reverted: `0 passed; 1 failed` with zero checkpoint_auto_decided events and a generic gate written instead; sibling under the SAME revert: still ok. That asymmetry is what localises the defect to root selection. The test also carries a mechanical opposite-result assertion that DOES re-run every cargo test (the two roots must disagree for the fixture). LIMITS: (a) the fixture is a plain create_dir_all directory standing in for a worktree — nothing here establishes real git-worktree semantics (shared refs, linked .git files, index separation); 999.76's open question about a real linked-worktree harness stays open. (b) The arm's other four preconditions (agent kind, capture confirmation, session id, resume ceiling) are pinned to satisfying values and unexercised — this discriminates on the root-selection axis only. (c) The revert was performed once, in one form, on one host."

### 9. A stale VERIFICATION.md is no longer inherited across a forced re-run
expected: `devflow start --phase N --force` no longer inherits the previous run's committed verdict. The loop-back selector compares the artifact's content fingerprint against a baseline captured at run start; an artifact unchanged since then dispatches a full execute instead of a `--gaps-only` pass against zero matching plans. (999.79)
result: pass
note: "Eight tests re-run during this UAT. Both directions green in the SAME run (4 passed; 299 filtered out): stale_verification_artifact_dispatches_full_execute AND verification_written_this_run_dispatches_gaps_only, plus verification_freshness_truth_table_is_exhaustive and ship_loop_back_still_issues_gaps_only_when_verification_absent. Core: the two fingerprint tests and the two serde tests (2 passed; 574 filtered out each). 35-05's controls are unusually strong and worth naming: TWO stubs (always-stale and always-fresh) each fail the truth table on DIFFERENT rows — that is exhaustiveness evidence, not one thing tested twice — and the direction pair inverts its asymmetry under each stub. The always-stale stub additionally broke Phase 33's two pre-existing gaps-only tests, which is the over-correction cost measured rather than argued. LIMITS: (a) HARDEN-03 remains UNCLASSIFIED — the rule keys on content change alone and cannot establish provenance; a worktree merge-back or an operator edit changing the artifact's bytes mid-run reads as authored-this-run and dispatches --gaps-only, which is the failure HARDEN-03 exists to prevent, reached by another route. See open item. (b) No test drives capture-then-compare inside a single real `devflow start --force`; the capture site is pinned ONLY by a source-position assertion (line 344 > line 287, with the wrong site at 167 as its negative control), so an edit moving the capture out of start()'s path would keep the grep satisfied. (c) F-11's kill window is ACCEPTED, not closed, and its direction is fail-OPEN: a process killed between the selector's baseline update and prepare_loop_back_to_code's save_state leaves a later same-run loop-back comparing against an older baseline, reading fresh and dispatching --gaps-only where a full execute was correct."

### 10. CHANGELOG 2.5.0 enumerates the public-API break
expected: `CHANGELOG.md` carries a `## 2.5.0 — 2026-08-07` entry with a `### Public API (devflow-core)` section naming 2 breaking signature changes, 2 breaking removals, 1 behaviour-only change, 5 additions, and 1 explicit non-event — and states plainly that breaking changes ship under a minor bump, and why. (D-08)
result: pass
note: "Read and verified against CHANGELOG.md during this UAT. `## 2.5.0 — 2026-08-07` sits at line 3, above `## 2.4.0` at line 150. `### Public API (devflow-core)` at line 61 carries all five buckets as claimed: Changed-breaking (phase_commit_count u32 -> Option<u32>; Mode::should_gate widened) = 2; Removed-breaking (classify_ssh_add_status, SigningStatus) = 2; Changed-behaviour-only (evaluate_layer3, signature explicitly unchanged) = 1; Added-non-breaking (phase_verification_fingerprint, MAX_PHASE_VALIDATE_FAILURES, phase_failure_ceiling_reached, State::phase_validate_failures, State::last_verification_fingerprint) = 5; Unchanged-despite-anticipation (phase_verification_exists) = 1. The minor-bump statement is present and explicit. LIMIT: 35-06's derivation covers DECLARATION SITES, not rustdoc's resolved public graph — a cargo public-api-style check against real rustdoc JSON would be strictly stronger and was not run, in that plan or in this UAT."

### 11. The 2.5.0 Known Issues section is current
expected: Every entry under `### Known Issues` in the `## 2.5.0` changelog entry is still true as of the release date, and each backlog id it cites is still open. A release-facing artifact that reports a resolved gap as open misinforms exactly the reader it exists to serve.
result: issue
reported: "Found while verifying test 10. CHANGELOG.md:139-141 states 'The setsid guard on the signing probe has no regression test... removing the pre_exec would not fail the suite. Tracked as 999.88.' That is false as of 2026-08-07. ROADMAP.md:539 marks 999.88 RESOLVED the same day, delivered as git::tests::the_signing_probe_is_not_captured_by_a_controlling_terminal (commit 8917dcd, arm-0 correction d33a837), and its resolution note records the performed mutation — commenting out the pre_exec yields `test result: FAILED. 0 passed; 1 failed` with a REGRESSION: panic. I re-ran the test during this UAT: `1 passed; 575 filtered out`. Commit fb46f9d updated ROADMAP.md and closed DEN-109 but did not update the changelog entry written by 35-06."
severity: minor

### 11. D1 — release --check verdict comes from a real sign probe
expected: release --check reports Viable/NotViable from a real ssh-keygen -Y sign probe over a throwaway payload, never from an ssh-add -l fingerprint comparison
result: pass
source: automated
coverage_id: D1

### 12. D2 — on-disk key with no agent reports Viable
expected: A configured key whose private sibling is on disk and which no agent holds reports Viable — the live false negative 999.86 was filed for twice
result: pass
source: automated
coverage_id: D2

### 13. D3 — the probe cannot hang an unattended preflight
expected: SSH_ASKPASS_REQUIRE=never plus a wall-clock ceiling that kills and reaps
result: pass
source: automated
coverage_id: D3

### 14. D4 — inline keys return Unknown without probing
expected: Inline key:: / raw ssh- values return Unknown with a fixed reason and are never probed
result: pass
source: automated
coverage_id: D4

### 15. D5 — no reason string leaks key material or paths
expected: No reason string carries the configured signingkey, a filesystem path, or any fragment of ssh-keygen's stderr
result: pass
source: automated
coverage_id: D5

### 16. D6 — the predictor and its types are removed
expected: classify_ssh_add_status, SigningStatus and inline_key_fingerprint removed with their tests; workspace builds clean
result: pass
source: automated
coverage_id: D6

### 17. D7 — probe workspace name is unique per call
expected: The per-call probe workspace name is unique across concurrent threads (F-8)
result: pass
source: automated
coverage_id: D7

## Summary

total: 18
passed: 17
issues: 1
pending: 0
skipped: 0
blocked: 0

## Gaps

- gap_id: G-35-11
  truth: "Every entry under `### Known Issues` in the 2.5.0 changelog entry is still true as of the release date"
  status: failed
  reason: "User reported: CHANGELOG.md:139-141 reports 999.88 (the setsid guard has no regression test) as an open known issue. ROADMAP.md:539 marks 999.88 RESOLVED 2026-08-07, delivered as git::tests::the_signing_probe_is_not_captured_by_a_controlling_terminal — re-run during this UAT at `1 passed; 575 filtered out`, and its resolution note records the performed mutation producing a REGRESSION: panic. Commit fb46f9d updated ROADMAP.md and closed DEN-109 without updating the 35-06 changelog entry."
  severity: minor
  test: 11
  artifacts:
    - path: "CHANGELOG.md"
      issue: "Known Issues bullet at :139-141 reports a resolved gap (999.88) as open"
    - path: ".planning/phases/35-loop-termination-and-baseline-correctness-999-77-999-78-999-/35-03-SUMMARY.md"
      issue: "coverage block D8 still carries human_judgment: true with the rationale 'No committed test covers this — removing the pre_exec would not fail the suite', which is the same stale claim. This is what caused uat.classify-coverage to present D8 as a human checkpoint during this UAT."
  missing:
    - "Correct or remove the 999.88 bullet in CHANGELOG.md's 2.5.0 Known Issues; if kept, restate it as the residual limit the resolution note actually records (n=1 per arm, one host, one container, timing-based) rather than 'has no regression test'."
    - "Update 35-03-SUMMARY.md's D8 coverage entry to human_judgment: false with a unit verification ref, so the classifier stops presenting a covered deliverable as needing human judgment."
  root_cause: "Not diagnosed by subagent — per standing operator instruction this session did not spawn diagnosis or gap-closure planner agents. Cause is evident from the git history and recorded above: a resolution landed in ROADMAP.md without a companion edit to the changelog entry a different plan had already written."
