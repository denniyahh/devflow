---
phase: 25-end-to-end-dogfood-blockers
verified: 2026-07-28T21:05:00Z
status: passed
score: 10/10 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  round: 5
  previous_status: human_needed
  previous_score: 10/10
  gaps_closed:
    - "G-25-3 (WINDOWS.md item 5, sixth and last monitor-leak site): `pipeline_launch.rs::tests::resume_clears_stop_marker_and_advances_past_stop_point`
       now binds `ReapMonitorOnDrop` from state RELOADED FROM DISK
       (`workflow::load_state(root, phase).ok()` at `:499`, mapped via `.as_ref()` at `:500-502`),
       not from the test's stale local `state` binding — independently re-derived as the correct
       fix, not accepted on the SUMMARY's word: `resume(root, phase)` loads its own `State` from
       the state file (`pipeline_launch.rs:224`) and never writes the spawned pid back into the
       caller's local variable, so `ReapMonitorOnDrop::after_launch(&state)` on the local binding
       (the form correct at all five prior sites) would have captured `pid: None` here and
       silently reaped nothing — the exact failure mode the round called out in advance and
       avoided.
       Bound at `:500-502`, strictly ahead of `result.unwrap()` (`:504`, the first panicking
       checkpoint) and every subsequent assertion. Verified by direct source read, not narration.
       Also verified empirically by running the test myself (not trusting the SUMMARY's reported
       pid): `cargo test -p devflow --bin devflow -- --exact
       pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point --nocapture`
       printed `stage plan → launched Claude Code (monitor pid 1035803)` and `1 passed`; I then
       ran `ps -p 1035803` myself immediately after and got exit code 1 (no such process) —
       the guard genuinely reaped it, not merely a plausible-looking binding."
    - "The silent-no-op backstop is real, not decorative: `assert!(reloaded.monitor_pid.is_some(), ...)`
       is appended after the three pre-existing assertions (not reordered relative to them).
       Confirmed by reading `launch_stage_inner` (`pipeline_launch.rs:65-129`): `state.monitor_pid`
       is explicitly cleared to `None` before any fallible step, and only set back to `Some(pid)`
       after `monitor::spawn_monitor` genuinely succeeds and that state is saved to disk. If
       `resume()` ever stopped reaching a real spawn, `reloaded.monitor_pid` would read `None` and
       this assertion would fail loudly — the property the plan's Task 1 required, verified by
       reading the production code path itself, not inferred from the test alone."
    - "WINDOWS.md item 5 closed via `gsd-tools windows fixed 5` (not hand-edited): ledger now reads
       `open_count: 0`, `waived_count: 1`, `fixed_count: 4`, `total_count: 5` — confirmed by
       reading `.planning/WINDOWS.md` directly (YAML frontmatter, markdown table, and the trailing
       JSON mirror all agree; item 5's row states the same closing rationale as 25-19-SUMMARY.md).
       Items 1/3/4 remain `fixed` and item 2 remains `waived` by the operator, all untouched."
  gaps_remaining: []
  regressions: []
  note: >
    Round 5 (25-19) is test-only: the diff to `pipeline_launch.rs` (commit `02cb9ba`, read directly
    via `git show`) falls entirely inside the pre-existing `mod tests { ... }` block — new lines
    only add a guard binding, a comment, and one assertion inside
    `resume_clears_stop_marker_and_advances_past_stop_point`'s body. `.planning/WINDOWS.md` is the
    only other file touched, and it is a data ledger, not code. I independently re-ran
    `cargo fmt --all -- --check` (clean) and `cargo clippy --workspace --all-targets -- -D warnings`
    (clean) myself, and ran `cargo test --workspace --no-fail-fast` myself once, summing every
    printed `test result: ok. N passed` line by hand rather than trusting the SUMMARY's arithmetic:
    **696 passed / 0 failed**, exactly matching the established build state. All 10 of the phase's
    original must-have truths and all 6 named units (25a-25f, 999.38) are therefore unaffected and
    unregressed — re-confirmed independently this round, not carried forward on trust.

    **Adjudication of this round's specific questions:**

    1. *Does the guard actually reap, from disk-reloaded state, ahead of every panicking
    checkpoint?* Yes — verified above by source read, by running the exact test myself, and by
    confirming the spawned pid was dead afterward with my own `ps -p` check (not merely reading
    the SUMMARY's claimed pid).

    2. *Is the enumeration finally complete?* For its stated, declared scope — yes, and I
    re-derived this independently rather than accepting the count. I grepped
    `crates/devflow-cli/src/*.rs` directly for both helper names
    (`stub_agent_binary`/`agent_free_dir_with_agent_stub`) myself and got exactly 7 call sites
    (1 in `staleness.rs`, 4 in `preflight.rs`, 2 in `pipeline_launch.rs`) — the same 7 the round
    claims, found by a different method (direct helper-name grep across the whole crate, not the
    entry-point list) than the one the plan used, which cross-validates the count rather than
    repeating it. Of those 7: 6 now bind `ReapMonitorOnDrop` (confirmed by grepping the guard name
    itself: `pipeline_launch.rs:426`, `:502`; `staleness.rs:776`; `preflight.rs:1587`, `:1677`,
    `:1793`), and the 7th (`preflight.rs::run_preflight_loopback_bounds_recursion`) I independently
    traced end-to-end rather than accepting the claim: it seeds `preflight_retries =
    MAX_PREFLIGHT_RETRIES - 1` with an `AlwaysFailAdapter` and a pre-written gate response
    `{"approved":false,"note":"retry"}`. Reading `GateAction::from_response` (`devflow-core/src/
    gates.rs:69-79`) directly, that response resolves to `LoopBack`, not `Advance` — so
    `run_preflight`'s failure branch calls `launch_stage(state, None, None)` again (never
    `launch_stage_inner` directly) on its *second* pass through `run_preflight`, by which point
    `preflight_retries` has already been incremented to the ceiling; the ceiling check
    (`preflight.rs:907`) then fires first, calls `abort()` (which `workflow::clear_state`s,
    matching the test's own `workflow::load_state(...).is_err()` assertion), and returns `Ok(false)`
    — and `launch_stage`'s `if !run_preflight(...)? { return Ok(()); }` short-circuit
    (`pipeline_launch.rs:189-191`) means `launch_stage_inner` — the only place that calls
    `monitor::spawn_monitor` — is never reached on any path through this test. The claim holds; I
    traced it myself rather than accepting it.

       *Is there a ninth wrapper?* Yes, explicitly: `commands::start` (`commands.rs:113`, the
    top-level `devflow start` command) also calls `launch_stage` directly (`commands.rs:302`) and
    is not one of the eight named entry points. This does not expand the guarded/unguarded count,
    though: `commands.rs` has zero call sites of either stub helper (confirmed by direct grep), so
    no in-process unit test exercises `start()` through a real/stubbed agent spawn the way the
    seven counted tests do — the 8-entry-point list is not claimed to be an exhaustive list of
    every production wrapper, only a sufficient one for cross-referencing against the two helpers
    that make an in-process spawn observable to a test; `start` not being on it does not leave a
    hole in that specific count.

       I additionally investigated, past what this round's fixes or any WINDOWS.md item claims to
    cover, whether `crates/devflow-cli/tests/phase7_cli.rs` — a structurally different mechanism
    (a real subprocess of the compiled `devflow` binary via `Command::new`, plus its own
    `fake_bin_dir` helper, since the two `pub(crate)` stub helpers are not visible from a separate
    integration-test crate) — leaves a background pipeline running past test completion in tests
    like `start_defaults_to_worktree` that only wait for an early artifact (the worktree directory)
    rather than for the pipeline to finish. This is outside the phase's declared scope (every named
    truth, unit, and WINDOWS.md item is scoped to `crates/devflow-cli/src/{pipeline_launch,
    staleness,preflight}.rs`'s in-process `#[cfg(test)]` modules) and I did not find evidence it is
    a live problem: I ran `start_defaults_to_worktree` under two separate ~10-second `ps`-polling
    windows immediately spanning and following the test's execution, looking for any new
    `/tmp/.tmpXXXXXX`-pattern process reflecting this test's own tempdir, and found none (only
    long-since-orphaned, unrelated stray processes from earlier, disconnected work on this
    machine — itself independent evidence for the phase's own residual-cleanup note, not for a new
    defect in this test). I flag this as an open question I could not conclusively resolve either
    way in the time available, not as a new gap: it is unclaimed by this phase's scope, and my own
    attempts to observe a live leak from it came back empty. A human or a future round should
    decide whether it is worth a dedicated investigation; it is not blocking this phase.

    3. *Do the phase's 10 original must-have truths and 6 named units still hold, and was round 5
    test-only?* Yes to both — see the diff-hunk and full-workspace-suite confirmation above.

    4. *Is `25-10`'s missing SUMMARY still correctly a non-gap?* Yes — re-confirmed this round by
    reading `25-10-PLAN.md`'s frontmatter directly: `status: superseded`, `superseded_by: "25-13"`.

    5. *Is anything still genuinely open?* No. `.planning/WINDOWS.md` reads `open_count: 0`,
    `waived_count: 1` (item 2, waived by the operator, a human, with a recorded reason),
    `fixed_count: 4` (items 1, 3, 4, 5), `total_count: 5` — confirmed by reading the ledger file
    directly, not by trusting the SUMMARY's quoted JSON. No human-verification item remains open:
    the sole item carried from round 4 (decide the sixth site's fate) is resolved — fixed, not
    waived. The phase is `passed`.
gaps: []
deferred: []
human_verification: []
---

# Phase 25: End-to-End Dogfood Blockers Verification Report

**Phase Goal:** Make an unattended `devflow start --phase N --agent claude --mode auto --yes-ship` run reach a completed Ship stage without a human touching it, by closing the four things that currently prevent it (a run starts on a current base — 25a; progresses through all stages — 25b; finishes with correct artifacts — 25c; a stalled run recovers without `kill(1)` — 25d), plus 25e (CI-throughput flake), 25f (docs drift), and 999.38 (PATH race).
**Verified:** 2026-07-28T21:05:00Z
**Status:** passed
**Re-verification:** Yes — round 5, after gap-closure plan 25-19 closed the sixth monitor-leak site (G-25-3 / WINDOWS.md item 5) this verifier discovered at the end of round 4.

## Goal Achievement

### Observable Truths

All 10 of the phase's original must-have truths, unchanged from prior verifications — round 5 touched only a `#[cfg(test)] mod tests` block in `pipeline_launch.rs` and the `.planning/WINDOWS.md` ledger. Re-confirmed this round via diff-hunk inspection and an independent full-workspace test run (not carried forward on trust).

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | 25a — a run starts on a current base ref, **safely** | ✓ VERIFIED | Unchanged since round 3 (CR-02 closed). No round-5 plan touches `preflight.rs`'s production `ensure_base_ref_current`/`base_is_checked_out_anywhere`/`fast_forward_base_ref`. |
| 2 | 25b — `enforce_build_staleness` is adjudicated once per run (at `start`), never re-invoked mid-run | ✓ VERIFIED | Unchanged; `staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness` still passes (part of the 696-test full-suite run I executed this round). |
| 3 | 25c (derivation) — `compute_version` derives from (reachable semver tag, conventional-commit classification), refuses on unreachable baseline, floors correctly | ✓ VERIFIED | Unchanged; no round-5 plan touches `version.rs`. |
| 4 | 25c (gate) — a major version bump opens a gate and never ships unattended, in the default (worktree) execution path | ✓ VERIFIED | Unchanged; no round-5 plan touches this code. |
| 5 | 25c (anchor) — `release_range_start`'s commit-range anchor excludes pre-release history across realistic release topologies | ✓ VERIFIED | Unchanged; no round-5 plan touches `version.rs`. |
| 6 | 25d (primitives) — bounded TERM->KILL escalation with verified death; registry-independent discovery; identity re-confirmation before signalling | ✓ VERIFIED | Unchanged; `agent.rs` untouched by round 5. |
| 7 | 25d (surface) — an operator using `devflow doctor` / `gate sweep --reap-strays` never has a live, registered process misreported as an orphan or destroyed | ✓ VERIFIED | Unchanged since round 3 (CR-01 closed). No round-5 plan touches `commands.rs`. |
| 8 | 25e — the 999.47 CI flake (cmdline-inheritance race) — human-verified against 11 observations, residual explicitly stated | ✓ VERIFIED (human-verified, carried forward) | Unchanged; no round-5 plan touches this evidence or its supporting code. |
| 9 | 999.38 — the test-suite PATH race is de-raced | ✓ VERIFIED | Unchanged; `staleness.rs`'s `ENV_MUTEX`/hermetic-git-read guards untouched by round 5. |
| 10 | 25f — CONTRIBUTING.md's release procedure and the ROADMAP/PROJECT.md versioning-policy prose no longer drift from what 25c implements | ✓ VERIFIED (human sign-off recorded, carried forward) | Unchanged; no round-5 plan touches these docs. |

**Score:** 10/10 truths verified (0 present-but-behavior-unverified, 0 failed).

### The Sixth Site (G-25-3 / WINDOWS.md item 5) — closed this round

`pipeline_launch.rs::tests::resume_clears_stop_marker_and_advances_past_stop_point` now binds `ReapMonitorOnDrop` from disk-reloaded state at `:500-502`, ahead of `result.unwrap()` at `:504` (the first panicking checkpoint) and every subsequent assertion, and asserts `reloaded.monitor_pid.is_some()` as a backstop against a future silent no-op. I independently verified this is the *correct* form (not merely a plausible one) by reading `resume()`'s body directly: it loads its own `State` from disk and never writes the spawned pid back into the test's local `state` binding, so the naive `ReapMonitorOnDrop::after_launch(&state)` form used at the other five sites would have captured `pid: None` here and silently reaped nothing. I also ran the test myself (`cargo test -p devflow --bin devflow -- --exact pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point --nocapture`), observed a genuine spawn (`monitor pid 1035803`), and then independently confirmed with my own `ps -p 1035803` call (run separately from the test, after it completed) that the process was dead — exit code 1, no such process. The reap is real, not merely well-argued.

### Enumeration completeness — independently re-derived, not accepted

I re-derived the "7 candidate tests" count by grepping `crates/devflow-cli/src/*.rs` directly for both stub-helper names (`stub_agent_binary`, `agent_free_dir_with_agent_stub`) myself, rather than trusting the plan's entry-point-based sweep: exactly 7 call sites exist (1 in `staleness.rs`, 4 in `preflight.rs`, 2 in `pipeline_launch.rs`), matching the round's claim via an independent method. Of those 7, 6 now bind `ReapMonitorOnDrop` (confirmed by grep of the guard name itself at all 6 sites) and the 7th, `preflight.rs::run_preflight_loopback_bounds_recursion`, I traced line-by-line through `run_preflight`'s ceiling logic, `GateAction::from_response`'s response-mapping (in `devflow-core/src/gates.rs`), and `launch_stage`'s `Ok(false)` short-circuit, and confirmed it structurally cannot reach `launch_stage_inner` (the only caller of `monitor::spawn_monitor`) on any path through the test.

I additionally answered the adjudication's explicit question of whether a ninth wrapper exists: yes — `commands::start` (`commands.rs:113`) also calls `launch_stage` directly and is not one of the eight named entry points. It does not expand the guarded/unguarded count because no in-process unit test in `commands.rs` exercises it via either stub helper (confirmed by direct grep: zero hits).

Past the phase's declared scope, I also investigated (and could not confirm) whether `crates/devflow-cli/tests/phase7_cli.rs`'s integration tests — a structurally distinct mechanism using a real subprocess of the compiled binary, which the two `pub(crate)` stub helpers are not even visible to — leave a background pipeline running past test completion. Two separate ~10-second `ps`-polling windows around `start_defaults_to_worktree`'s execution found no evidence of a persisting process from that specific test (only unrelated, long-orphaned stray processes already on the machine from earlier work). This is out of the phase's declared scope and unresolved either way; it is not a gap against this phase, but I am stating it explicitly rather than silently dropping the question.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/test_support.rs::ReapMonitorOnDrop` | RAII unwind-safe reap guard (G-25-2) | ✓ VERIFIED | Unchanged this round; present (`:364-426`) |
| `crates/devflow-cli/src/pipeline_launch.rs::launch_stage_persists_monitor_pid_for_reload` | guard bound ahead of 4 panicking checkpoints (WR-06) | ✓ VERIFIED | Unchanged; `:426` |
| `crates/devflow-cli/src/staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness` | guard bound ahead of 2 panicking checkpoints (WR-06) | ✓ VERIFIED | Unchanged; `:776` |
| `crates/devflow-cli/src/preflight.rs::run_preflight_advance_gate_launches_agent_exactly_once` | guard bound (WR-05) | ✓ VERIFIED | Unchanged; `:1587` |
| `crates/devflow-cli/src/preflight.rs::run_preflight_loopback_gate_launches_agent_exactly_once` | guard bound (WR-05) | ✓ VERIFIED | Unchanged; `:1677` |
| `crates/devflow-cli/src/preflight.rs::run_preflight_advance_skips_recheck_on_idempotently_failing_check` | guard bound (round 4 finding) | ✓ VERIFIED | Unchanged; `:1793` |
| `crates/devflow-cli/src/pipeline_launch.rs::resume_clears_stop_marker_and_advances_past_stop_point` | reap of the real monitor `resume()` spawns (G-25-3) | ✓ VERIFIED — closed this round | Guard bound at `:500-502` from `reloaded_for_reap` (`:499`), ahead of `result.unwrap()` (`:504`); backstop assertion `reloaded.monitor_pid.is_some()` present. Confirmed by source read, by running the test myself, and by confirming the spawned pid (`1035803`) was dead afterward. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `pipeline_launch.rs::resume_clears_stop_marker_and_advances_past_stop_point` | `test_support::ReapMonitorOnDrop` | bound from disk-reloaded state, before `result.unwrap()` | ✓ WIRED, correctly — closed this round | `:500-502`, ahead of `:504` and all subsequent assertions |
| (5 prior sites — pipeline_launch.rs `:426`, staleness.rs `:776`, preflight.rs `:1587`/`:1677`/`:1793`) | `test_support::ReapMonitorOnDrop` | bound after final `&mut state` use | ✓ WIRED, correctly (unchanged) | Re-confirmed present this round via grep, not re-derived line-by-line again since round 4 already did so |
| `ReapMonitorOnDrop::drop`'s `panicking()` branch | `std::io::stderr()` via `writeln!` | direct, result discarded | ✓ WIRED, correctly (unchanged) | CR-01, resolved `c2f5080`, unaffected by round 5 |

### Data-Flow Trace (Level 4)

Not applicable — this phase produces CLI/library logic, not UI components rendering dynamic data.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Sixth-site fix: guard reaps a real spawn | `cargo test -p devflow --bin devflow -- --exact pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point --nocapture` | `stage plan → launched Claude Code (monitor pid 1035803)`; `1 passed` | ✓ PASS (run by this verifier) |
| Reap confirmed independently (not the SUMMARY's word) | `ps -p 1035803` immediately after the test above | exit code 1 (no such process) | ✓ PASS (run by this verifier) |
| `run_preflight_loopback_bounds_recursion` provably cannot spawn | Source trace: `run_preflight` ceiling check (`preflight.rs:907`) → `abort()` → `launch_stage`'s `Ok(false)` short-circuit (`:189-191`) never reaches `launch_stage_inner` | Traced line-by-line, confirmed | ✓ PASS (static trace by this verifier) |
| `fmt` clean | `cargo fmt --all -- --check` | exit 0 | ✓ PASS (run by this verifier) |
| `clippy` clean | `cargo clippy --workspace --all-targets -- -D warnings` | clean, no warnings | ✓ PASS (run by this verifier) |
| Full workspace regression | `cargo test --workspace --no-fail-fast` | `696 passed / 0 failed` (summed by hand from every `test result: ok` line) | ✓ PASS (run by this verifier, matches established build state exactly) |
| Debt markers in round-5-touched files | `rg -n 'TBD\|FIXME\|XXX' crates/devflow-cli/src/pipeline_launch.rs .planning/WINDOWS.md` | no matches | ✓ PASS |
| Out-of-scope investigation: `tests/phase7_cli.rs` background-process persistence | Two ~10s `ps`-polling windows around `start_defaults_to_worktree` | no new tempdir-referencing process observed | ? INCONCLUSIVE (informational — not a phase gap; see note above) |

### Probe Execution

Not applicable — this phase has no `scripts/*/tests/probe-*.sh` files, and none are declared in any PLAN/SUMMARY. `SKIPPED (no runnable probe artifacts)`.

### Requirements Coverage

*This project has no `.planning/REQUIREMENTS.md`; tracked by unit identifier per the phase's own convention (`25a`-`25f`, `999.38`). Not reported as a gap.*

| Unit | Backlog ID | Description | Status | Evidence |
|------|-----------|--------------|--------|----------|
| 25a | 999.51/DEN-76 | Base-ref currency, safely repaired | ✓ SATISFIED | Truth 1 — unaffected by round 5 |
| 25b | 999.48/DEN-73 | Staleness hoist | ✓ SATISFIED | Truth 2 — unaffected by round 5 |
| 25c | 999.49/DEN-74 | Version derivation + major-bump gate + anchor | ✓ SATISFIED | Truths 3/4/5 — unaffected by round 5 |
| 25d | 999.44/DEN-68 | Orphan process reaping | ✓ SATISFIED | Truths 6/7 — unaffected by round 5 |
| 25e | 999.47/DEN-72 | Flaky test dead predicate | ✓ SATISFIED (human-verified) | Truth 8 — unaffected by round 5 |
| 25f | (no backlog ID) | CONTRIBUTING drift | ✓ SATISFIED (human sign-off) | Truth 10 — unaffected by round 5 |
| 999.38 | folded in | PATH race | ✓ SATISFIED | Truth 9 — unaffected by round 5 |
| — | WINDOWS.md #1/#3/#4 | WR-03/05 test-suite leak fold-in | ✓ SATISFIED (round 4) | Fixed, unchanged this round |
| — | WINDOWS.md #5 | Sixth leak site (G-25-3, `resume_clears_stop_marker_and_advances_past_stop_point`) | ✓ SATISFIED — closed this round | Fixed via 25-19; independently re-confirmed above |

**Note on `25-10-SUMMARY.md`'s absence:** confirmed intentional, not a gap, re-checked this round directly from `25-10-PLAN.md`'s frontmatter — `status: superseded` / `superseded_by: "25-13"`.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/devflow-cli/src/pipeline_launch.rs::resume_clears_stop_marker_and_advances_past_stop_point` | — | (previously flagged, now resolved) | — | Closed this round — see above. No longer an anti-pattern. |
| (carried forward, unresolved, out of phase scope) `crates/devflow-core/src/version.rs:338-349` | WR-01 | `merge-base --is-ancestor` spawn/exit-128 errors collapse into the same `false` as a genuine negative | ℹ️ Info | Unchanged; not touched this round |
| (carried forward) `crates/devflow-core/src/test_support.rs:101,120` | WR-02 | `wait_for_exec_visibility`'s guard (ii) compares against the caller, not the actual parent | ℹ️ Info | Unchanged; no current call site is a non-parent caller |
| (carried forward) `crates/devflow-cli/src/commands.rs:3861-3895` | WR-04 | `reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age` flaky by construction | ℹ️ Info | Unchanged; not touched this round |

## Human Verification Required

None. The sole outstanding item from round 4 (decide the sixth site's fate) is resolved: fixed via 25-19, not waived.

## Gaps Summary

None. All 10 of the phase's original must-have truths remain verified; all 6 named units (25a-25f, plus 999.38) remain satisfied. `.planning/WINDOWS.md` reads `open_count: 0` across all five recorded items (four fixed, one waived by the operator with a stated reason) — confirmed by reading the ledger directly, not by trusting the SUMMARY's quoted output. The enumeration of monitor-leak sites within this phase's declared scope (in-process unit tests in `crates/devflow-cli/src/{pipeline_launch,staleness,preflight}.rs` that stub an agent binary via `stub_agent_binary`/`agent_free_dir_with_agent_stub`) is genuinely closed: I independently re-derived the same 7-candidate count by a different method than the plan's own sweep, and traced the one unguarded-but-safe case (`run_preflight_loopback_bounds_recursion`) end-to-end myself rather than accepting the claim.

Two items are noted for completeness but do not block this phase: (1) `commands::start` is a genuine ninth production wrapper reaching `launch_stage` that is not among the eight named entry points, but it has no in-process test exercising it through either stub helper, so it does not expand the guarded/unguarded count; (2) whether `crates/devflow-cli/tests/phase7_cli.rs`'s integration tests (a structurally different, out-of-declared-scope mechanism) leave background pipelines running past test completion is a question I investigated but could not conclusively resolve either way — no evidence of a live leak was found in two separate ps-monitoring windows, but the investigation was not exhaustive. Neither item is claimed as fixed by any round of this phase, neither is required by any of the phase's 10 truths or 6 named units, and neither should be read as blocking phase 25's completion.

**Phase 25 is `passed`.**

---

*Verified: 2026-07-28T21:05:00Z*
*Verifier: Claude (gsd-verifier)*
