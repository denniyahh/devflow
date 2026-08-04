# Phase 11-Remediation: Discussion Log

**Date:** 2026-06-20
**Context:** Post-review triage of Phase 11 code review findings

---

## Decision 1: Scope — fix only criticals, defer everything else

**Question:** Should this sprint address warnings and info items alongside the criticals?

**Rationale:** The 5 criticals break observable behavior: the auto-gate threshold
is dead code (CR-02/CR-05), agent failures are opaque (CR-01), branch creation
leaves stale state on error (CR-03), and a process kill during gate ack creates a
multi-day stuck state (CR-04). All 5 must be fixed before merge.

The 11 warnings are real issues but none break core correctness. WR-07 (non-atomic
`save_state`) is the most serious warning and should move to Phase 12. The info
items are documentation gaps.

**Decision:** Fix exactly 5 criticals. No warnings, no info items, no Phase 12
features. Remediation sprint should be a tight diff that's easy to review.

---

## Decision 2: CR-02 + CR-05 — treat as one fix

**Question:** Are CR-02 (serde(skip) on consecutive_failures) and CR-05 (transition()
resets consecutive_failures unconditionally) separate bugs requiring separate fixes?

**Rationale:** CR-05 analysis in the review concludes: "The net finding is CR-02
subsumes this." Once `consecutive_failures` is persisted (CR-02 fix), the behavior
of `transition()` resetting it to 0 on a successful stage advance is correct — the
counter should reset when the pipeline moves forward, whether by gate approval or
a clean Validate pass. There is no secondary bug in `transition()` beyond what
CR-02 already covers.

The existing `consecutive_failures_is_runtime_only_not_persisted` test in
`state.rs` asserts the broken behavior and will need to be updated to assert the
correct (persisted) behavior as part of the same commit.

**Decision:** Fix CR-02 (remove `#[serde(skip)]`), update the test, and mark
CR-05 as subsumed. One commit covers both.

---

## Decision 3: CR-04 ordering fix — don't swap ack and save_state globally

**Question:** Should we fix CR-04 by reordering ack/save_state, or by making the
full gate protocol atomic (write both ack and state in one fsync'd operation)?

**Rationale:** A fully atomic multi-file write (using a staging dir + rename) would
be robust but is a larger change with more surface area. The core invariant we need
is: "if gate_pending resets to false in state.json, the pipeline can advance on
restart regardless of whether the ack file was written." The converse crash path
(ack written, state not updated) is benign: Hermes gets the ack and marks delivery
complete, DevFlow restarts with gate_pending:true, polls for a response file that
no longer exists, and blocks for GATE_TIMEOUT_SECS. This is the exact 7-day stuck
state the fix is preventing.

Swapping the order to `gate_pending=false → save_state → ack` is the minimal
correct fix: worst case after the swap is Hermes doesn't receive the ack and
retries delivery, which is idempotent.

**Decision:** Swap the two lines in `run_gate()`. Minimal, targeted, correct.

---

## Decision 4: CR-03 — move divergence check, don't add rollback

**Question:** When the divergence check fails after branch creation, should we
also roll back (delete) the stale branch before returning the error?

**Rationale:** The correct fix is to run the divergence check BEFORE branch
creation so the stale branch never exists. Adding a rollback (delete branch on
error) in addition to moving the check would be defensive programming for a
scenario that the fix eliminates. Adding rollback code introduces its own failure
modes (what if the delete fails?) and touches more code than necessary.

The "wrong branch" bug — divergence_from_develop() checking the feature branch's
divergence rather than develop's divergence — is also fixed by moving the check
before `feature_start`, since HEAD will still be on develop (or the user's current
branch) at check time.

**Decision:** Move the divergence check block (lines 295–306) to before the
worktree/branch creation block (lines 264–293). No rollback logic added.

---

## Decision 5: CR-01 — capture to file, not forward to monitor stdout

**Question:** Should agent stderr be forwarded to the monitor's own stdout (which
is currently `/dev/null`) or captured to a separate file?

**Rationale:** The monitor's stdout is discarded (`Stdio::null()` at line 123).
Forwarding agent stderr there would achieve nothing. The monitor script is a shell
one-liner that can't do conditional forwarding based on exit code. A separate file
(`.devflow/phase-NN-stderr.log`) is queryable by `devflow recover` and `devflow
advance` and persists for post-mortem debugging without complicating the hot path.

The stdout file already follows the naming convention `phase-NN-stdout.log`
(via `stdout_path()` in `agent_result.rs`). A parallel `stderr_path()` is the
natural extension.

**Decision:** Add `stderr_path()` to `agent_result.rs`, change `2>/dev/null` to
`2>{stderr_file}` in the monitor script. Four lines of change total.

---

## Execution Order Rationale

| Order | Fix | Reason |
|-------|-----|--------|
| 1st | CR-02 | State model fix — most fundamental; CR-05 is subsumed here |
| 2nd | CR-04 | Gate ordering fix — depends on correct state model being in place |
| 3rd | CR-03 | Branch ordering fix — independent of state model, but natural to do after state fixes |
| 4th | CR-01 | Stderr capture — mechanical, no dependencies, good last item |

No fix depends on another at the code level (each targets different functions),
but doing CR-02 first ensures the test update for `consecutive_failures` doesn't
conflict with any downstream state assertions touched by CR-04.
