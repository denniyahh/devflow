---
phase: 35
reviewers: [opencode, hermes]
reviewers_attempted: [codex, cursor, opencode, antigravity, hermes, agycli, qwen]
reviewed_at: 2026-08-07T00:30:22Z
plans_reviewed:
  - 35-01-PLAN.md
  - 35-02-PLAN.md
  - 35-03-PLAN.md
  - 35-04-PLAN.md
  - 35-05-PLAN.md
  - 35-06-PLAN.md
pass: 2
note: "Second adversarial pass, plans. The first pass ran on CONTEXT.md (opencode + hermes) and produced A-01..A-17."
---

# Cross-AI Plan Review — Phase 35

> **This pass is effectively single-lane. Read the roster before weighting anything below.**
> Five of seven attempted lanes produced no usable review, and the one that did agree with the
> plans (hermes) is a demonstrated false negative. Treat this as **one** substantive external
> opinion — corroborated by the orchestrator's own source verification, not by a second reviewer.

## Lane roster — what actually ran

| Lane | Result | Detail |
|---|---|---|
| **opencode** | ✅ **substantive** | ~7 min, 14.8 KB, 21 `file:line` citations. 6 concerns (1 HIGH, 3 MEDIUM, 2 LOW). |
| **hermes** | ⚠️ **null result, low value** | ~16 min, 5.4 KB, **2** `file:line` citations. Reported "no new problems". Missed all four findings that were independently verified as real. **Not corroboration.** |
| codex | ❌ dropped | `You've hit your usage limit … try again at Aug 11th, 2026 8:01 AM` — unavailable for ~5 days. |
| cursor | ❌ dropped | `ActionRequiredError: You've hit your usage limit`. |
| antigravity (`agy`) | ❌ dropped | `agy` is a shim doing `exec antigravity "$@"`; `antigravity` is not on PATH. The working binary is **`agycli`** (v1.1.10), which gsd-review does not invoke. |
| antigravity (`agycli`) | ❌ dropped | Retried manually: `no output produced — a tool required the "command" permission that headless mode cannot prompt for, so it was auto-denied`. Needs `--dangerously-skip-permissions` or a `permissions.allow` rule. **Not retried — that is an operator decision.** |
| qwen | ❌ unusable | `No auth type is selected`. Note it **exited 0** while failing — a false-green worth knowing about. |

**Why hermes does not count as a second opinion.** Its summary asserts "each proposed test would
fail if the corresponding fix were reverted" and "all assertions in the plans are verified against
actual source code". It cites two line numbers in the whole document, and it did not surface the
`#[non_exhaustive]` misclassification, the ceiling-detection gap, the borrow conflict, or the
`save_state` fixture dependency — all four of which were confirmed against source. Agreement from a
reviewer that did not look is not evidence.

---

## OpenCode Review

**Overall risk: MEDIUM.** Verdict: no concern blocks implementation; C1 should be corrected before
the CHANGELOG entry is written.

### Confirmed findings

Each of the four below was **independently verified against current source by the orchestrator**
after the review returned. These are not taken on the reviewer's word.

#### C1 — HIGH — `35-06` misclassifies the new `State` fields as a breaking change

`crates/devflow-core/src/state.rs:33` carries `#[non_exhaustive]` on `pub struct State`. External
crates therefore cannot build it with a struct literal at all, so adding `pub` fields with
`#[serde(default)]` is additive and backward-compatible.

`35-06-PLAN.md:78-84` files `phase_validate_failures` and `last_verification_fingerprint` under
**"Changed, breaking"**, with the rationale *"breaking for anyone constructing the struct by
literal."* That rationale is false — `#[non_exhaustive]` already forecloses it.

The source says so itself, two lines above the attribute (`state.rs:30-31`):
*"Paying that cost once here makes every future field additive."*

**Impact:** a CHANGELOG asserting a break that does not exist dilutes the signal for the two real
breaks in this release (`phase_commit_count`'s return type, and the two removed `pub` signing
items). Reclassify as **Added, non-breaking**.

#### C2 — MEDIUM — `35-04` Task 3 does not say how a ceiling gate is distinguished from an ordinary Supervise gate

`crates/devflow-core/src/mode.rs:170-179`:

```
Stage::Validate => match self {
    Mode::Supervise => true,                                    // :174
    Mode::Auto => consecutive_failures >= MAX_CONSECUTIVE_FAILURES,
}
```

In Supervise mode every Validate gates, unconditionally — the ceiling condition and the
ordinary-gate condition overlap completely. The plan says to reset `phase_validate_failures` when
"the gate fired because the phase ceiling was reached", but specifies no check that separates the
two cases.

**Impact:** either the reset fires on every Supervise gate (the cumulative total is reset at every
failure, so it never accumulates — defeating criterion 2 in exactly the mode where the operator is
watching), or it never fires (the total grows unbounded past operator acknowledgement). The
saturating add prevents overflow but not either wrong semantic.

#### C3 — MEDIUM — adding `&mut State` to `select_loop_back_fix` collides with an outstanding mutable borrow

`select_loop_back_fix` is `pipeline_outcomes.rs:315`, called at `:389`, `:441`, `:454`. Two of those
call sites sit inside `match run_gate(project_root, state, …)?` arms opened at `:385` and `:437`,
where `state` is already mutably borrowed for the duration of the match.

**Impact:** routine Rust ownership friction, not a design flaw — but the plan tells the executor to
"let the compiler surface all three call sites" without warning that two will surface as borrow
errors requiring control-flow restructuring. Worth naming so it is not mis-diagnosed as a mistake.

#### C4 — MEDIUM — the two-cycle test drives `handle_validate_outcome`, which calls `save_state`

`pipeline_outcomes.rs:423` calls `workflow::save_state(state)?`, which writes
`.devflow/state-{NN}.json`. `init_repo` builds a git repo but no `.devflow/` directory.

**Impact:** the criterion-1 test may fail on a missing parent directory — a confusing failure in the
single most important test of the phase. One `create_dir_all` in the fixture avoids it.

### Reported but not independently verified

- **C5 — LOW.** `NoGitPath` is a PATH-based guard, so a future refactor to an absolute `git` path
  would silently disarm it while every dependent test kept passing. The reviewer notes all current
  git construction is PATH-resolved (`git.rs` `git_command` / `hermetic_command`), so this is a
  documented structural limitation rather than a live defect.
- **C6 — LOW.** The signing probe workspace name is specified as "unique to this process"; if that
  means PID alone, PID reuse could collide and fall through to `Unknown`. Fail-soft is correct;
  adding a sub-millisecond component removes even the theoretical case.

### Strengths the reviewer confirmed against source

- The `Option<u32>` change genuinely forces both consumers open at compile time (`agent_result.rs:1905`,
  `pipeline_outcomes.rs:400-401`), and widening `should_gate` forces all 10 call sites.
- The absent-`git` guard is the correct shape, and the per-crate placement structurally prevents the
  two-mutex `PATH` race (`lib.rs:79`).
- The two-cycle 999.77 test discriminates where a single-cycle test would not.
- `35-02` extends the right base test (`pipeline_launch.rs:2302`) — it drives `advance()`
  synchronously and already exercises all five checkpoint preconditions.
- `35-03` found three tests coupled to the deleted predictor through **reason strings** rather than
  symbols, which a symbol search misses and which fail at runtime, not compile time.

---

## Hermes Review

Recorded in full for the record. **Do not weight its verdict.** See the roster note above.

**Verdict given:** overall risk LOW, no concerns at any severity.

**Suggestions offered (the only actionable content):**
1. Record the specific changed lines of `handle_validate_outcome`'s Some/None arms in `35-01`'s SUMMARY.
2. Record in `35-03`'s SUMMARY the exact command used to verify the SSH signature namespace against a real signature.
3. Add a `35-04` test for saturating behaviour of `phase_validate_failures` at `u32::MAX`.

Suggestion 3 is worth keeping — it is a genuine boundary the edge-probe row `HARDEN-02 precision`
already flagged, and an explicit test is cheap.

---

## Consensus Summary

**There is no consensus to report.** One substantive lane, one non-substantive lane, five dropped.
The findings below rest on the OpenCode review plus the orchestrator's own verification against
source — stated plainly so nobody reads a second signature into this document.

### Agreed Strengths

Not assessable — a strength named by one reviewer and echoed by a reviewer that did not read
closely is not agreement. OpenCode's strengths list is recorded above on its own merit; each item it
cites was checked and matched source.

### Agreed Concerns

Not assessable at the two-reviewer bar. The four confirmed concerns (C1–C4) are carried on
verification, not on reviewer agreement.

### Divergent Views

**OpenCode found six concerns; hermes found none.** This divergence resolves in OpenCode's favour on
evidence, not on preference: C1–C4 were each reproduced against current source. The divergence is
recorded because it is itself the most useful signal in this pass — a reviewer returning "no
concerns" on this plan set is measuring its own depth, not the plans.

### What this pass does NOT establish

- **It is not a broad external cross-check.** Five lanes dropped; two of them (codex, cursor) for
  billing reasons that will recur, and one (antigravity) for a fixable PATH/shim defect.
- **Nothing here is evidence about code behaviour.** No test in this phase exists yet. Every finding
  is about plan text checked against *current* source.
- **A clean result from any single lane does not mean the plans are clean.** hermes demonstrates
  exactly that failure mode within this very document.
- **C5 and C6 were not independently verified** — they are recorded at the reviewer's confidence,
  not the orchestrator's.
