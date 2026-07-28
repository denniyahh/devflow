---
phase: 25-end-to-end-dogfood-blockers
verified: 2026-07-28T20:15:00Z
status: human_needed
score: 10/10 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  round: 4
  previous_status: human_needed
  previous_score: 10/10
  gaps_closed:
    - "G-25-2 / WR-05 (WINDOWS.md item 1, `.planning/WINDOWS.md`): the two `preflight.rs` tests
       (`run_preflight_advance_gate_launches_agent_exactly_once`,
       `run_preflight_loopback_gate_launches_agent_exactly_once`) that drove a real
       `monitor::spawn_monitor` through `run_preflight`'s internal `Advance`/`LoopBack`
       recursion now bind `ReapMonitorOnDrop::after_launch(&state)` immediately after their
       final `&mut state` use, ahead of every panicking checkpoint. Independently re-confirmed
       by direct source read (`preflight.rs:1587`, `:1677`) and by running both tests by exact
       name myself (`1 passed` each)."
    - "G-25-2 / WR-06 (WINDOWS.md item 3): the two sites 25-16 originally wired
       (`pipeline_launch.rs::launch_stage_persists_monitor_pid_for_reload`,
       `staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness`) are converted
       from a plain trailing `reap_spawned_monitor(&state)` call to the same `ReapMonitorOnDrop`
       guard, bound before all 4 (resp. 2) panicking checkpoints in each test body. Independently
       re-confirmed by direct source read (`pipeline_launch.rs:426`, `staleness.rs:776`) and by
       running both tests by exact name myself (`1 passed` each)."
    - "The guard itself (`ReapMonitorOnDrop`, `test_support.rs:364-426`) is proven non-vacuous by
       a discriminating test pair, not merely asserted correct: I re-derived both tests' logic by
       reading them line-by-line (not trusting the SUMMARY's description) —
       `reap_guard_reaps_the_monitor_when_a_later_assertion_panics` binds the guard INSIDE a
       `catch_unwind` closure that then deliberately panics, and asserts the real `sleep 300`
       child is dead afterward; its control, `trailing_reap_call_is_skipped_when_a_later_assertion_panics`,
       calls the plain `reap_spawned_monitor` AFTER an identical deliberate panic (guard bound
       OUTSIDE the closure as the test's own cleanup) and asserts the child is still ALIVE. Ran
       both myself: `1 passed` each. The pair genuinely discriminates — swapping which mechanism
       runs inside vs. outside the panicking closure would flip both outcomes."
    - "The double-panic interlock (`ReapMonitorOnDrop::drop`'s `std::thread::panicking()` branch)
       is now airtight: the post-review fix (`c2f5080`) replaced `eprintln!` (which routes through
       `std::io::_eprint`, which itself panics on a failed write) with
       `let _ = writeln!(std::io::stderr(), ...)`, discarding the result. Confirmed by reading
       the current source (`test_support.rs:404-423`) and by grepping the whole file for
       `eprintln!` — the only remaining occurrence is inside a comment explaining why it is NOT
       used, not a live call. No panicking call of any kind remains reachable on that branch."
  gaps_remaining: []
  regressions: []
  new_findings:
    - "A SIXTH real leak site, NOT covered by any of this round's fixes, WINDOWS.md items 1/3/4,
       or either plan's enumeration, independently discovered by this verifier (not cited from any
       SUMMARY/REVIEW — see 'New Finding This Round' below):
       `pipeline_launch.rs::tests::resume_clears_stop_marker_and_advances_past_stop_point`
       (`:457-505`) calls `resume(root, phase)` with a real `stub_agent_binary(\"claude\")` on
       PATH; `resume` internally calls `launch_stage`, which spawns a real detached monitor
       wrapper — empirically confirmed by running the test myself with `--nocapture`
       (`stage plan -> launched Claude Code (monitor pid 852403)` printed) — and the test never
       reaps it. 25-18-SUMMARY.md's explicit closing claim ('no path to `monitor::spawn_monitor`
       exists in this codebase's test suite beyond the three functions named in the grep') is
       false as stated: `resume()` is a fourth wrapping entry point neither plan's enumeration
       searched for, because both enumerations grepped for literal
       `launch_stage`/`launch_stage_inner` call TEXT (or `run_preflight`'s specific internal
       recursion), and `resume(root, phase)` at `pipeline_launch.rs:480` matches neither pattern
       even though `resume`'s own body (`:230`) calls `launch_stage` directly — the identical
       'reachability, not call-site' blind spot that let WR-05 survive round 3, recurring in
       round 4 despite an explicit claim of exhaustiveness."
  note: >
    Round 4 (25-17, 25-18) is test-only: every changed hunk in both plans' commits falls strictly
    inside a `mod tests { ... }` block (confirmed by reading each `git diff` hunk header — every
    `@@` in both plans' diffs to pipeline_launch.rs/staleness.rs/preflight.rs starts at or after
    each file's `mod tests` line), and the new `ReapMonitorOnDrop`/`reap_monitor_pid` items in
    `test_support.rs` live in a file whose own module doc states it is declared
    `#[cfg(test)] mod test_support;` — confirmed no production caller exists anywhere in
    `crates/devflow-cli/src/*.rs` outside the five known test-mod binding sites. All 10 of the
    phase's original must-have truths and all 6 named units (25a-25f, 999.38) are therefore
    unaffected and unregressed by this round — re-confirmed by re-running the full workspace
    suite myself (`696 passed / 0 failed`, matching the established build state exactly), plus
    `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`,
    both clean, run by me. WR-05 and WR-06 are genuinely closed at the 5 sites this round
    targeted — but this verifier's own independent re-derivation of "is the enumeration complete"
    (the adjudication task explicitly asked for this) found it is NOT: a sixth site, unrelated to
    either plan's declared scope, still leaks. This is why overall status remains `human_needed`
    rather than advancing to `passed` — the same category of ancillary, Warning-severity
    test-hygiene defect as WR-05/WR-06 were before this round, not a phase must-have failure.
gaps: []
human_verification:
  - test: >
      NEW (this round, not previously tracked in WINDOWS.md): decide whether to fix
      `pipeline_launch.rs::tests::resume_clears_stop_marker_and_advances_past_stop_point`
      (bind `ReapMonitorOnDrop::after_launch(&state_after_reload)` after loading the
      post-`resume()` state — note the test's own `state` variable at `:465` is a stale copy
      that never sees the pid `resume`'s internal reload assigns; the guard would need to bind
      on the `reloaded` value read at `:491`, or `resume` would need to hand the pid back some
      other way) in a follow-up plan, or record and waive a new `WINDOWS.md` item (suggested id
      5) with a stated reason, consistent with how items 1-4 were tracked this phase.
    expected: >
      Either a follow-up plan wires a reap into this test (after re-deriving how to obtain the
      pid, since `resume`'s signature does not hand the caller a `&mut State` the way
      `launch_stage`/`run_preflight` do), or the residual is explicitly logged and waived with a
      stated reason, per the project's own "`/gsd-ship` blocks while `open_count > 0`" policy —
      this finding is not yet in `.planning/WINDOWS.md` at all, so it is not currently blocking
      anything; a human needs to decide whether it should be added.
    why_human: >
      Same category as WR-05/WR-06 before them — a scope/priority call (spend a fifth
      gap-closure round on test-hygiene work outside the phase's 6 named units, log it as debt
      and move on, or decide it's low-enough-priority to leave undocumented), not a fact this
      verifier can resolve differently by looking harder. The underlying fact (a real,
      unreaped `monitor::spawn_monitor` spawn) is already independently confirmed by live
      execution with `--nocapture` (pid printed: 852403), not merely inferred from source.
deferred: []
---

# Phase 25: End-to-End Dogfood Blockers Verification Report

**Phase Goal:** Make an unattended `devflow start --phase N --agent claude --mode auto --yes-ship` run reach a completed Ship stage without a human touching it, by closing the four things that currently prevent it (a run starts on a current base — 25a; progresses through all stages — 25b; finishes with correct artifacts — 25c; a stalled run recovers without `kill(1)` — 25d), plus 25e (CI-throughput flake), 25f (docs drift), and 999.38 (PATH race).
**Verified:** 2026-07-28T20:15:00Z
**Status:** human_needed
**Re-verification:** Yes — after gap-closure round 4 (25-17/25-18), against round 3's two remaining human-verification items (WR-05, WR-06).

## Goal Achievement

### Observable Truths

All 10 of the phase's original must-have truths (unchanged from prior verifications — round 4 touched only test-mod code in `pipeline_launch.rs`, `staleness.rs`, `preflight.rs`, and `test_support.rs`, confirmed by reading every diff hunk header). Truths 1-10 received a regression check this round (confirmed no production code outside `mod tests` was touched by 25-17/25-18); none needed full re-derivation since none of this round's changes touch production logic.

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | 25a — a run starts on a current base ref, **safely** | ✓ VERIFIED | Unchanged since round 3 (CR-02 closed, re-confirmed then). No round-4 plan touches `preflight.rs`'s production `ensure_base_ref_current`/`base_is_checked_out_anywhere`/`fast_forward_base_ref` — confirmed round-4 diffs to `preflight.rs` are confined to `mod tests`, lines 1568-1827. |
| 2 | 25b — `enforce_build_staleness` is adjudicated once per run (at `start`), never re-invoked mid-run | ✓ VERIFIED | Unchanged; no round-4 plan touches `commands.rs`'s staleness call site or `staleness.rs`'s production `enforce_build_staleness`/`combined_staleness`. `staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness` still passes (re-run by me: `1 passed`). |
| 3 | 25c (derivation) — `compute_version` derives from (reachable semver tag, conventional-commit classification), refuses on unreachable baseline, floors correctly | ✓ VERIFIED | Unchanged; no round-4 plan touches `version.rs`. |
| 4 | 25c (gate) — a major version bump opens a gate and never ships unattended, in the default (worktree) execution path | ✓ VERIFIED | Unchanged; no round-4 plan touches this code. |
| 5 | 25c (anchor) — `release_range_start`'s commit-range anchor excludes pre-release history across realistic release topologies | ✓ VERIFIED | Unchanged; no round-4 plan touches `version.rs`. |
| 6 | 25d (primitives) — bounded TERM->KILL escalation with verified death; registry-independent discovery; identity re-confirmation before signalling | ✓ VERIFIED | Unchanged; `agent.rs`'s `discover_stray_devflow_processes`, `STRAY_MIN_AGE`, `process_age`, `terminate_and_verify`, `is_same_process` are untouched by round 4 (confirmed: neither 25-17 nor 25-18 modifies `agent.rs` — `terminate_and_verify` is only CALLED, by `test_support.rs::reap_monitor_pid`, not redefined). |
| 7 | 25d (surface) — an operator using `devflow doctor` / `gate sweep --reap-strays` never has a live, registered process misreported as an orphan or destroyed | ✓ VERIFIED | Unchanged since round 3 (CR-01 closed, re-confirmed then). No round-4 plan touches `commands.rs`. |
| 8 | 25e — the 999.47 CI flake (cmdline-inheritance race) — human-verified against 11 observations, residual explicitly stated | ✓ VERIFIED (human-verified, carried forward) | Unchanged; no round-4 plan touches this evidence or its supporting code. |
| 9 | 999.38 — the test-suite PATH race is de-raced | ✓ VERIFIED | Unchanged; `staleness.rs`'s `ENV_MUTEX`/hermetic-git-read guards are untouched — round-4's only edit to that test function is the guard-binding line and its surrounding comment, confirmed by reading the diff (the `PATH` mutate/restore block is byte-identical). |
| 10 | 25f — CONTRIBUTING.md's release procedure and the ROADMAP/PROJECT.md versioning-policy prose no longer drift from what 25c implements | ✓ VERIFIED (human sign-off recorded, carried forward) | Unchanged; no round-4 plan touches these docs. |

**Score:** 10/10 truths verified (0 present-but-behavior-unverified, 0 failed).

### New Finding This Round (not one of the 10 truths above): a sixth, previously-unnamed leak site

Both round-4 plans closed their declared scope correctly (see "Adjudication of round-4 fix quality" below) — but the adjudication task for this verification explicitly asked whether the fold-in's enumeration is now COMPLETE, not just whether the declared fixes work. I re-derived this independently rather than trusting either plan's "no fourth path exists" claim, by re-running the exact enumeration methodology (grep for every place a real/stub agent binary is placed on `PATH` via `stub_agent_binary`/`agent_free_dir_with_agent_stub`, then classifying every caller) instead of grepping for `launch_stage`/`launch_stage_inner` text directly.

**Result: the enumeration is NOT complete.** `pipeline_launch.rs::tests::resume_clears_stop_marker_and_advances_past_stop_point` (`:457-505`) stubs a real `claude` binary on `PATH` (`stub_agent_binary("claude")`, `:472`) and calls `resume(root, phase)` (`:480`), which internally calls `launch_stage` (`pipeline_launch.rs:230`) — a real spawn. I confirmed this empirically, not just by reading source: running the test with `--nocapture` printed `stage plan → launched Claude Code (monitor pid 852403)`, proving a real detached monitor wrapper was spawned. The test never reaps it — no `reap_spawned_monitor`/`ReapMonitorOnDrop` call appears anywhere in its body. By the time I checked `ps -p 852403` afterward the process was already gone — the same self-exiting-in-under-a-millisecond timing accident `.planning/WINDOWS.md` item 2 already documents for the stub-agent shape, which is why this is not observable by a before/after process-count delta (matching this project's own prior finding: delta-0 is not evidence of no leak).

**Why this was missed by both 25-16 and 25-18's enumerations:** both searched for literal `launch_stage`/`launch_stage_inner` call text (25-16), later extended to `run_preflight`'s own internal recursion arms (25-18) — but `resume(root, phase)` at `pipeline_launch.rs:480` matches neither search pattern, even though `resume`'s own body (line 230) calls `launch_stage` directly. This is the identical "reachability, not call-site" blind spot that let WR-05 survive round 3, recurring in round 4 despite 25-18-SUMMARY.md's explicit closing statement that no fourth path exists ("no path to `monitor::spawn_monitor` exists in this codebase's test suite beyond the three functions named in the grep") — that statement is false as written. I confirmed no other `stub_agent_binary`/`agent_free_dir_with_agent_stub` call site was missed: grepping both helpers across `crates/devflow-cli/src/*.rs` returns exactly 7 hits (2 function definitions + 5 fixed-this-round call sites + this 1 unfixed call site) — a small, closed, fully-enumerable set, so I am confident this is the only remaining site, not merely the next one I happened to find.

This is Warning-severity, same bucket as WR-05/WR-06 before this round's fix — test-suite hygiene, not production behavior; `resume` itself, `launch_stage`, and `monitor::spawn_monitor` are all unchanged by this finding. Not yet recorded in `.planning/WINDOWS.md`. See Anti-Patterns and Human Verification below.

### Adjudication of round-4 fix quality (the six numbered questions this re-verification was asked to answer independently)

1. **Are WR-05 and WR-06 genuinely closed, or relocated?** Genuinely closed at the 5 sites targeted. I read all five `ReapMonitorOnDrop::after_launch(&state)` binding sites directly (`pipeline_launch.rs:426`, `staleness.rs:776`, `preflight.rs:1587`, `:1677`, `:1793`) and confirmed each binds strictly AFTER the launch call that populates `state.monitor_pid` (not before, which would capture `None`) and strictly BEFORE every subsequent panicking statement in its test body (not after, which would reproduce the original bug). Neither failure mode is present at any of the 5 sites.
2. **Is the double-panic interlock now airtight?** Yes. Read `test_support.rs:389-426` in full: the `panicking()` branch's only side effect is `let _ = writeln!(std::io::stderr(), ...)`, whose `Result` is explicitly discarded via `let _ =`, and `writeln!`/`std::io::stderr().write_fmt` do not panic on a failed write (unlike the `eprintln!` macro, which does, via `std::io::_eprint`). Grepped the whole file for `eprintln!`: the only occurrence left is inside the comment explaining why it was removed, confirmed by the `git show c2f5080` diff.
3. **Is the discriminating test pair genuinely non-vacuous?** Yes, confirmed by reading both tests line-by-line (not the SUMMARY's description of them): the "proves the fix" test binds the guard INSIDE the panicking closure (so it drops during the real unwind, reaping the subject); the control binds the guard OUTSIDE as its own cleanup and calls the OLD plain-call form INSIDE the panicking closure, AFTER the panic point (so the plain call never executes, and the subject survives to the post-`catch_unwind` assertion). Swapping which mechanism sits inside vs. outside the closure would flip both tests' outcomes — this is the actual criterion for non-vacuousness, and it holds.
4. **Was the third site (found by 25-18) a real leak, and is there a fourth?** The third site (`run_preflight_advance_skips_recheck_on_idempotently_failing_check`) is a real leak — 25-18-SUMMARY.md documents empirically observing pid `745043` spawned and reaped during its own verification probe, and I independently re-ran the now-fixed test by exact name (`1 passed`) with its new `state.monitor_pid.is_some()` assertion in place, confirming the premise holds. **The enumeration is NOT complete**, however: see "New Finding This Round" above — a sixth site (`resume_clears_stop_marker_and_advances_past_stop_point`) leaks and neither plan's search methodology could have found it, because both searched for spawn-function call text rather than every place a real/stub agent binary is placed on `PATH`.
5. **Do the phase's 10 original must-have truths and 6 named units still hold? Was round 4 test-only?** Yes to both. Every hunk in both plans' commits falls inside a `mod tests { ... }` block (confirmed via hunk-header inspection of `git diff` output for all touched files), `test_support.rs`'s new items live in a file that is itself declared `#[cfg(test)]` at its `mod` site, and I re-ran the full workspace suite myself (`696 passed / 0 failed`, exactly matching the established build state) plus `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings`, both clean.
6. **Is `25-10`'s missing SUMMARY still correctly a non-gap?** Yes — re-confirmed by reading `25-10-PLAN.md`'s frontmatter directly this round: `status: superseded`, `superseded_by: "25-13"`, and no `25-10-SUMMARY.md` exists, matching the plan's own design (not an omission).

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/devflow-cli/src/test_support.rs::ReapMonitorOnDrop` | RAII unwind-safe reap guard (G-25-2) | ✓ VERIFIED | Present (`:364-426`), `Drop` impl confirmed to reap unconditionally with a non-panicking fallback branch; captures pid by value per its own doc rationale |
| `crates/devflow-cli/src/test_support.rs::reap_monitor_pid` | shared non-asserting reap primitive | ✓ VERIFIED | Present (`:300-307`), delegates to `terminate_and_verify` then returns verified liveness; both the guard and the plain `reap_spawned_monitor` helper delegate to it — confirmed by reading both call sites |
| `crates/devflow-cli/src/pipeline_launch.rs::launch_stage_persists_monitor_pid_for_reload` | guard bound ahead of 4 panicking checkpoints (WR-06) | ✓ VERIFIED | Confirmed at `:426`, ahead of `result.unwrap()` (`:435`) and all 3 subsequent assertions |
| `crates/devflow-cli/src/staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness` | guard bound ahead of 2 panicking checkpoints (WR-06) | ✓ VERIFIED | Confirmed at `:776`, ahead of `result.expect(...)` (`:786`) and the `assert_eq!` on `blocked_count` |
| `crates/devflow-cli/src/preflight.rs::run_preflight_advance_gate_launches_agent_exactly_once` | guard bound, plus a `state.monitor_pid.is_some()` runtime-verified premise (WR-05) | ✓ VERIFIED | Confirmed at `:1587`; premise assertion at `:1615-1620` |
| `crates/devflow-cli/src/preflight.rs::run_preflight_loopback_gate_launches_agent_exactly_once` | same, LoopBack arm (WR-05) | ✓ VERIFIED | Confirmed at `:1677`; premise assertion at `:1705-1710` |
| `crates/devflow-cli/src/preflight.rs::run_preflight_advance_skips_recheck_on_idempotently_failing_check` | guard bound at the third site 25-18's re-derivation found | ✓ VERIFIED | Confirmed at `:1793`, ahead of the `matches!(result, Ok(false))` assertion at `:1807` |
| `crates/devflow-cli/src/pipeline_launch.rs::resume_clears_stop_marker_and_advances_past_stop_point` | reap of the real monitor `resume()` spawns | ✗ MISSING — new finding this round, see above | No `reap_spawned_monitor`/`ReapMonitorOnDrop` call anywhere in this test body; empirically confirmed to spawn a real monitor (`monitor pid 852403` printed with `--nocapture`) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `pipeline_launch.rs::launch_stage_persists_monitor_pid_for_reload` | `test_support::ReapMonitorOnDrop` | bound after final `&mut state` use | ✓ WIRED, correctly | `:426`, ahead of all panicking checkpoints |
| `staleness.rs::mid_run_stage_transition_does_not_readjudicate_staleness` | `test_support::ReapMonitorOnDrop` | bound after final `&mut state` use | ✓ WIRED, correctly | `:776`, ahead of both panicking checkpoints |
| `preflight.rs::run_preflight_advance_gate_launches_agent_exactly_once` | `test_support::ReapMonitorOnDrop` | bound after final `&mut state` use | ✓ WIRED, correctly | `:1587` |
| `preflight.rs::run_preflight_loopback_gate_launches_agent_exactly_once` | `test_support::ReapMonitorOnDrop` | bound after final `&mut state` use | ✓ WIRED, correctly | `:1677` |
| `preflight.rs::run_preflight_advance_skips_recheck_on_idempotently_failing_check` | `test_support::ReapMonitorOnDrop` | bound after final `&mut state` use | ✓ WIRED, correctly | `:1793` |
| `pipeline_launch.rs::resume_clears_stop_marker_and_advances_past_stop_point` | `test_support::ReapMonitorOnDrop` / `reap_spawned_monitor` | — | ✗ NOT WIRED | Confirmed absent — new finding this round |
| `ReapMonitorOnDrop::drop`'s `panicking()` branch | `std::io::stderr()` via `writeln!` | direct, result discarded | ✓ WIRED, correctly | No panicking call reachable on this branch (CR-01 from round-4 review, resolved in `c2f5080`) |

### Data-Flow Trace (Level 4)

Not applicable — this phase produces CLI/library logic, not UI components rendering dynamic data.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Guard reaps during a real unwind | `cargo test --package devflow --bin devflow test_support::tests::reap_guard_reaps_the_monitor_when_a_later_assertion_panics -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| Control: trailing-call form does NOT reap during an unwind | `cargo test --package devflow --bin devflow test_support::tests::trailing_reap_call_is_skipped_when_a_later_assertion_panics -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| WR-06 fix: pipeline_launch.rs site | `cargo test --package devflow --bin devflow pipeline_launch::tests::launch_stage_persists_monitor_pid_for_reload -- --exact` | `1 passed` | ✓ PASS (run by this verifier, via full-file run) |
| WR-06 fix: staleness.rs site | `cargo test --package devflow --bin devflow staleness::tests::mid_run_stage_transition_does_not_readjudicate_staleness -- --exact` | `1 passed` | ✓ PASS (run by this verifier, via full-file run) |
| WR-05 fix: Advance arm | `cargo test --package devflow --bin devflow preflight::tests::run_preflight_advance_gate_launches_agent_exactly_once -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| WR-05 fix: LoopBack arm | `cargo test --package devflow --bin devflow preflight::tests::run_preflight_loopback_gate_launches_agent_exactly_once -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| Third site fix (25-18) | `cargo test --package devflow --bin devflow preflight::tests::run_preflight_advance_skips_recheck_on_idempotently_failing_check -- --exact` | `1 passed` | ✓ PASS (run by this verifier) |
| **New finding: sixth site spawns a real, unreaped monitor** | `cargo test --package devflow --bin devflow pipeline_launch::tests::resume_clears_stop_marker_and_advances_past_stop_point -- --exact --nocapture` | `stage plan → launched Claude Code (monitor pid 852403)`; `1 passed` (test itself is correct about what it asserts — the leak is a separate, silent omission) | ⚠️ CONFIRMS NEW FINDING (run by this verifier) |
| `eprintln!` removed from the panicking-unwind branch | `rg -c 'eprintln!' crates/devflow-cli/src/test_support.rs` | 1 match, inside a comment explaining its removal, not a live call | ✓ PASS (run by this verifier) |
| Full workspace regression | `cargo test --workspace --no-fail-fast` | `696 passed / 0 failed` | ✓ PASS (run by this verifier, matches established build state exactly) |
| `cargo fmt --all -- --check` | — | exit 0 | ✓ PASS (run by this verifier) |
| `cargo clippy --workspace --all-targets -- -D warnings` | — | clean, no warnings | ✓ PASS (run by this verifier) |
| Debt markers in round-4-touched files | `rg -n 'TBD\|FIXME\|XXX'` across `test_support.rs`, `pipeline_launch.rs`, `staleness.rs`, `preflight.rs` | no matches | ✓ PASS |

### Probe Execution

Not applicable — this phase has no `scripts/*/tests/probe-*.sh` files, and none are declared in any PLAN/SUMMARY. `SKIPPED (no runnable probe artifacts)`.

### Requirements Coverage

*This project has no `.planning/REQUIREMENTS.md`; tracked by unit identifier per the phase's own convention (`25a`-`25f`, `999.38`). Not reported as a gap.*

| Unit | Backlog ID | Description | Status | Evidence |
|------|-----------|--------------|--------|----------|
| 25a | 999.51/DEN-76 | Base-ref currency, safely repaired | ✓ SATISFIED | Truth 1 — unaffected by round 4 |
| 25b | 999.48/DEN-73 | Staleness hoist | ✓ SATISFIED | Truth 2 — unaffected by round 4 |
| 25c | 999.49/DEN-74 | Version derivation + major-bump gate + anchor | ✓ SATISFIED | Truths 3/4/5 — unaffected by round 4 |
| 25d | 999.44/DEN-68 | Orphan process reaping | ✓ SATISFIED | Truths 6/7 — unaffected by round 4 |
| 25e | 999.47/DEN-72 | Flaky test dead predicate | ✓ SATISFIED (human-verified) | Truth 8 — unaffected by round 4 |
| 25f | (no backlog ID) | CONTRIBUTING drift | ✓ SATISFIED (human sign-off) | Truth 10 — unaffected by round 4 |
| 999.38 | folded in | PATH race | ✓ SATISFIED | Truth 9 — unaffected by round 4 |
| — | WINDOWS.md #1/#3/#4 | WR-03/05 test-suite leak fold-in | ✓ SATISFIED (this round) | Fixed and independently re-confirmed above |
| — | (new, not yet in WINDOWS.md) | Sixth leak site (`resume_clears_stop_marker_and_advances_past_stop_point`) | ⚠️ NOT SATISFIED | See New Finding above and Human Verification below |

**Note on `25-10-SUMMARY.md`'s absence:** confirmed intentional, not a gap, re-checked this round directly from `25-10-PLAN.md`'s frontmatter — `status: superseded` / `superseded_by: "25-13"`, matching `ROADMAP.md`'s own record.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/devflow-cli/src/pipeline_launch.rs` | `:457-505` (`resume_clears_stop_marker_and_advances_past_stop_point`) | **NEW (independently discovered by this verifier, not cited from any SUMMARY/REVIEW):** stubs a real `claude` binary on `PATH` and calls `resume(root, phase)`, which internally spawns a real detached monitor wrapper via `launch_stage` — confirmed empirically (`monitor pid 852403` printed with `--nocapture`) — and never reaps it. Not recorded in `.planning/WINDOWS.md`. | ⚠️ Warning | Test-suite process leak, same load-sensitive/timing-accident category as WR-05/WR-06 before their fix — masked on this fast, unloaded machine by the stub agent's near-instant exit, not by any structural guarantee. Does not affect `devflow start`'s own shipped behavior. |
| (carried forward, unresolved, out of round-4 scope) `crates/devflow-core/src/version.rs:338-349` | WR-01 | `merge-base --is-ancestor` spawn/exit-128 errors collapse into the same `false` as a genuine negative | ℹ️ Info | Unchanged; not touched this round |
| (carried forward) `crates/devflow-core/src/test_support.rs:101,120` | WR-02 | `wait_for_exec_visibility`'s guard (ii) compares against the caller, not the actual parent | ℹ️ Info | Unchanged; no current call site is a non-parent caller |
| (carried forward) `crates/devflow-cli/src/commands.rs:3861-3895` | WR-04 | `reap_stray_candidates_refuses_a_candidate_younger_than_the_minimum_age` flaky by construction | ℹ️ Info | Unchanged; not touched this round |

## Human Verification Required

One new item this round, plus a status note on the ledger:

### 1. NEW — a sixth test spawns a real monitor and never reaps it (`resume_clears_stop_marker_and_advances_past_stop_point`)

**Test:** Decide whether to fix `pipeline_launch.rs::tests::resume_clears_stop_marker_and_advances_past_stop_point` in a follow-up plan (this is not a trivial guard-bind like the other five: `resume(root, phase)` does not hand the caller a `&mut State`, so the fix needs to either bind the guard against the `reloaded` state already read at `:491`, or `resume` needs to expose the spawned pid some other way), or record and waive a new `.planning/WINDOWS.md` item with a stated reason.
**Expected:** Either the test reaps what it spawns, or the residual is explicitly logged and accepted.
**Why human:** Priority/scope call, same category as WR-05/WR-06 before this round — the underlying fact (a real, unreaped spawn) is already independently confirmed by live execution with `--nocapture`, not in dispute.

**Note on `.planning/WINDOWS.md`'s current state:** the ledger shows `open_count: 0` (items 1/3/4 fixed, item 2 waived by the operator), which is accurate for the four items it tracks — but this verification's independent re-derivation of enumeration-completeness (explicitly requested by this round's adjudication task) found a fifth real, unaddressed defect the ledger does not yet know about. `open_count: 0` should not be read as "no monitor-reap defects remain in the test suite" until a human either adds and resolves a new item for this finding or explicitly declines to.

## Gaps Summary

None against the phase's 10 original must-have truths — round 4 is entirely test-only, confirmed by hunk-level inspection of every diff, and the full workspace suite (696/696), `fmt`, and `clippy` all re-ran clean under my own execution, matching the established build state exactly. All 10/10 truths verified; all 6 named units (25a-25f, plus 999.38) remain satisfied on their own unit-level merits.

WR-05 and WR-06 (WINDOWS.md items 1 and 3) are genuinely closed at the 5 sites targeted this round — independently re-derived by direct source reading and by re-running every named test myself, not accepted on either executor's or the reviewer's word. The discriminating test pair proving the guard's correctness is genuinely non-vacuous, and the double-panic interlock is now airtight (both independently re-confirmed).

However, this round's own explicit closing claim of exhaustiveness ("no fourth path exists") is not accurate: an independent re-derivation of the enumeration — which this round's adjudication task specifically asked for — surfaced a sixth site (`resume_clears_stop_marker_and_advances_past_stop_point`) that neither plan's search methodology could have found, for the same structural reason (call-site text search vs. actual reachability) that let WR-05 itself survive round 3. This is Warning-severity, ancillary test-hygiene work in the same category as WR-05/WR-06 were before this round — it does not attach to any of the phase's 10 original truths or 6 named units — but it is a real, empirically-confirmed defect, not recorded anywhere in `.planning/WINDOWS.md` yet, and a human should decide whether to fix it or explicitly waive it before treating this round of gap-closure work as fully exhausted.

---

*Verified: 2026-07-28T20:15:00Z*
*Verifier: Claude (gsd-verifier)*
