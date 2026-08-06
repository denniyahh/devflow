---
phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl
plan: 03
subsystem: infra
tags: [claude-code, stream-json, canary, provenance, guard, refusal, jsonl]

requires:
  - phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl
    provides: "`agent_result::token_reported_in_capture` (the provenance-checked matcher), `ClaudeAgent::exec_command`'s stream-json argv, `monitor::user_turn_line`, `monitor::CloseRule`, and `claude_stream_launch_enabled` — all landed by plan 31-01"
provides:
  - "A once-per-run startup canary that declares its own token and confirms it only inside a top-level `result` event"
  - "`CanaryOutcome` with `Absent` kept structurally distinct from `Unverified(reason)`"
  - "`State::canary`, persisting the outcome across the separate `devflow` processes one run's stage launches occupy"
  - "A D-15 refusal on the stream launch path, with two distinguishable messages"
  - "`claude_delivery_canary_{confirmed,absent,unverified}` in `.devflow/events.jsonl`"
affects: [31-04 D-11 opt-out, 31-05 acceptance run]

actuals:
  tokens: 70848
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "The trust decision lives in exactly one function; consumers delegate and hold no notion of their own about which lines are trustworthy"
    - "A guard's failure modes are separate variants when they call for different operator action — `Absent` vs `Unverified(reason)`"
    - "An injected-launcher seam so a guard's own tests never spend a real agent invocation"
    - "Every measurement carries a case that must produce the opposite result; three mutation checks stand in for Task 2's missing RED"

key-files:
  created:
    - crates/devflow-core/src/canary.rs
  modified:
    - crates/devflow-core/src/lib.rs
    - crates/devflow-core/src/state.rs
    - crates/devflow-cli/src/pipeline_launch.rs
    - crates/devflow-cli/src/staleness.rs
    - crates/devflow-cli/tests/phase7_cli.rs

key-decisions:
  - "`ClaudeCanaryLauncher` carries a `workdir` field rather than being the plan's unit struct — the trait signature has no room for a cwd and deriving one from the capture path would couple the child's working directory to where DevFlow keeps runtime files"
  - "The canary's close rule is `monitor::CloseRule`, not a locally-written one: closing stdin on the FIRST `result` would end the session before any task-notification turn could arrive, which is the very behaviour being measured"
  - "No `.process_group(0)` on the canary child — the opposite of the monitor's detached child, because this one runs in the foreground of the operator's own CLI and should die with a Ctrl-C"
  - "The canary's idle timeout is its own constant, not a reuse of the monitor's, so a future change to the stage timeout cannot silently change how patient the guard is"
  - "A refusal clears `monitor_pid` — WR-04's rationale applied to an aborted launch, so `liveness()` cannot report Stuck and send the operator to `devflow resume`, which cannot help"
  - "The `phase7_cli` fake CLI now models a CLI that DELIVERS rather than working around the guard — a fake that cannot deliver is, correctly, a CLI this pipeline refuses to run on"

patterns-established:
  - "A grep-based acceptance criterion phrased against CODE, not prose: the module explains provenance by concept so `rg 'is_top_level'` measures the implementation rather than the doc comment"

requirements-completed: [Constraint-5, D-13, D-14, D-15]

coverage:
  - id: D1
    description: "A declared token is confirmed only inside a top-level `result` event; a prompt echo and a subagent-authored result both fail to satisfy it"
    verification:
      - kind: unit
        ref: "cargo test -p devflow-core --lib canary::tests::canary_confirmed_when_token_returns_in_a_top_level_result -- --exact"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow-core --lib canary::tests::canary_absent_when_token_appears_only_as_a_prompt_echo -- --exact"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow-core --lib canary::tests::canary_absent_when_token_appears_only_in_a_non_top_level_event -- --exact"
        status: pass
    human_judgment: false
  - id: D2
    description: "`Absent` stays distinct from `Unverified`, and each token is fresh"
    verification:
      - kind: unit
        ref: "cargo test -p devflow-core --lib canary::tests::canary_unverified_when_the_launcher_fails -- --exact"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow-core --lib canary::tests::declared_tokens_differ_between_runs -- --exact"
        status: pass
    human_judgment: false
  - id: D3
    description: "The guard runs once per run, only on the stream launch path, refuses on both failure modes with distinguishable messages, and records its outcome in state and provenance"
    verification:
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::canary_runs_once_per_run -- --exact"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::canary_gate_only_applies_to_the_stream_launch_path -- --exact"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::absent_canary_refuses_to_launch -- --exact"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::unverified_canary_refuses_to_launch_with_a_distinct_message -- --exact"
        status: pass
      - kind: unit
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::canary_outcome_is_persisted_and_emitted -- --exact"
        status: pass
    human_judgment: false
  - id: D4
    description: "`launch_stage_inner` actually calls the gate, at the widened stage, with the real launcher bound"
    verification:
      - kind: integration
        ref: "cargo test -p devflow --bin devflow pipeline_launch::tests::launch_stage_inner_refuses_at_code_when_the_canary_cannot_confirm -- --exact"
        status: pass
      - kind: integration
        ref: "cargo test --test phase7_cli reference_and_cleanup_worktree_cli_flow"
        status: pass
    human_judgment: false
  - id: D5
    description: "Against the REAL Claude CLI, a token planted in a startup task comes back inside a top-level `result` event"
    verification: []
    human_judgment: true
    rationale: "Nothing in this plan has run the real `claude` binary against the canary. Two tests do execute `ClaudeCanaryLauncher::run`, but against `sh` stubs — one that exits immediately (producing an honest `Absent`) and one that echoes the token back in a canned `result` event (producing `Confirmed`). Both prove the plumbing; neither can prove the CLI still delivers task notifications, because neither stub has a notification path at all. This is the plan's `backstop` must-have and it is plan 31-05's acceptance run to settle."

duration: 40min
completed: 2026-08-03
status: complete
---

# Phase 31 Plan 03: Delivery Canary Summary

**A `devflow start` that routes the Code stage through the `stream-json` transport now plants a fresh nonce in one throwaway agent task, accepts it back only from inside a top-level `result` event, refuses to launch at all when it does not come back — distinguishing "the behaviour is gone" from "the guard could not run" — and leaves the verdict in both `state-NN.json` and `events.jsonl`.**

## Performance

- **Duration:** ~40 min
- **Tasks:** 2 of 2
- **Files:** 1 created, 5 modified

## Test numbers actually observed

Reported as printed. `cargo test --workspace` was run to completion and its **own** exit code captured, not a pipeline's (`cargo test --workspace > /dev/null 2>&1; echo $?` → `0`).

| Command | Result |
|---|---|
| `cargo test --workspace` (final) | **exit 0**; 22 `test result: ok` lines; every line `0 failed` |
| `devflow-core` lib target | `516 passed; 0 failed; 0 filtered out` (baseline before this plan: 511, so +5) |
| `devflow` bin target | `255 passed; 0 failed; 0 filtered out` (baseline 249, so +6) |
| `phase7_cli` integration target | `17 passed; 0 failed` in 1.48s |
| each of the 5 `canary::tests::*` `-- --exact` | `1 passed; 0 failed; 515 filtered out` |
| each of the 5 gate tests + the wiring test `-- --exact` | `1 passed; 0 failed; 254 filtered out` |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --check` | exit 0 |
| `scripts/check.sh all` | exit 0 |

**Negative controls for every `1 passed` above.** `cargo test -p devflow-core --lib canary::tests::this_test_does_not_exist_xyz -- --exact` prints `0 passed; 0 failed; 516 filtered out` and still reports `ok`; the same on the CLI target prints `0 passed; 0 failed; 255 filtered out`, also `ok`. That is the exact false green `CLAUDE.md` warns about, so the `1 passed` readings with a non-zero `filtered out` are real rather than vacuous.

**RED reading, recorded because it is more informative than the GREEN one.** Task 1's RED commit (`ddb8a2b`) sits at `3 passed; 2 failed; 511 filtered out` against a `run_delivery_canary` that always returns `Absent`. Only the `Confirmed` and `Unverified` cases failed — **both `Absent` tests passed vacuously against a degenerate implementation**, which is exactly what makes them worth mutation-testing rather than trusting.

**Mutation checks (four run, all reverted; none of this code is in any commit).**

| Mutation | Effect | What it establishes |
|---|---|---|
| trust decision → raw `text.contains(&token)` | both `Absent` tests redden | The two `Absent` tests DO have teeth against the realistic wrong implementation (the 30-05 substring-scan shape), despite passing vacuously in RED |
| gate ignores `stream_launch` | `canary_gate_only_applies_to_the_stream_launch_path` reddens | The stream-path condition is load-bearing |
| refusal no longer clears `monitor_pid` | `absent_canary_refuses_to_launch` reddens | The `liveness()` assertion is not decorative |
| gate never reads the recorded outcome | `canary_runs_once_per_run` reddens with `left: 2, right: 1` | The once-per-run short circuit is load-bearing |

## What these tests do NOT establish

This is the most important section here, and it is the same shape as 31-01's.

1. **Nothing has run the real `claude` binary against the canary.** `claude --version` was run once, as this plan's precondition check (`2.1.220`, exit 0) — that is all. `ClaudeCanaryLauncher::run` IS executed by two tests, but against `sh` stubs: `#!/bin/sh\nexit 0` (→ empty capture → honest `Absent`) and a stub that reads one stdin line, greps the token out of it and prints a canned top-level `result` (→ `Confirmed`). Neither stub has a task-notification path at all, so **neither can say anything about whether the CLI still delivers**. That premise is witnessed only by plan 31-05's acceptance run, and it is carried here as this plan's `backstop` must-have rather than as a passing test.
2. **The five `canary::tests` prove the MATCHER, and only the matcher.** Every one injects a launcher that writes a canned capture. They establish that a token is honoured from a top-level `result`, is not honoured from a prompt echo or a subagent-authored result, and that a launcher failure is `Unverified` rather than `Absent`. They establish nothing about the real launcher.
3. **The five gate tests prove the GATE'S WIRING, and only that.** Once per run, stream path only, refuse on both failure modes, persist, emit. They inject the outcome, so they cannot say whether the guard would reach the right verdict in production.
4. **Several real-launcher paths are executed by NO test.** Specifically: the close-rule release of stdin (neither stub keeps stdin open past a turn, so `CloseRule::should_close` never fires in any test), the 30s idle timeout, the 300s absolute deadline, and the kill-after-grace path in `reap`. They are reasoned from the production monitor's equivalents, not measured.
5. **Token distinctness is overwhelming, not absolute.** `declared_tokens_differ_between_runs` observes two calls differing; the construction (a 64-bit `DefaultHasher` over nanos + pid + counter) makes a collision ~2⁻⁶⁴, not impossible. Two observations is also a very weak reliability bound in its own right — it shows the counter feeds the hash, nothing more.
6. **`canary_outcome_is_persisted_and_emitted`'s leak assertion is partly structural.** It asserts `token_prefix` equals the constant exactly and that the prefix occurs exactly once in the line. Because the test injects an outcome, no real token exists in that run — so the test proves the payload *shape* cannot carry a token, not that a real token was withheld.

## Task Commits

1. **Task 1 RED — failing tests for the matcher** — `ddb8a2b` (test)
2. **Task 1 GREEN — matcher and real launcher** — `ae80c76` (feat)
3. **Task 2 — once-per-run gate, refusal, persistence, provenance** — `b02fe17` (feat)

No REFACTOR commit: the GREEN implementation needed none.

## Files Created/Modified

- `crates/devflow-core/src/canary.rs` (new) — `TOKEN_PREFIX`, `declare_token`, `CanaryOutcome`, `CanaryLauncher`, `ClaudeCanaryLauncher`, `canary_prompt`, `canary_capture_path`, `run_delivery_canary`, `claude_cli_version`, `reap`.
- `crates/devflow-core/src/lib.rs` — `pub mod canary;` in alphabetical position.
- `crates/devflow-core/src/state.rs` — `State::canary: Option<CanaryOutcome>` with `#[serde(default)]`, plus `canary: None` in `State::new`.
- `crates/devflow-cli/src/pipeline_launch.rs` — `canary_gate`, `refuse_launch`, `emit_canary_outcome`; `launch_stage_inner` binds `claude_stream_launch_enabled` once and passes it to both the launch-shape choice and the gate.
- `crates/devflow-cli/src/staleness.rs` — one fixture records a `Confirmed` outcome (see deviation 2).
- `crates/devflow-cli/tests/phase7_cli.rs` — the fake `claude` now models a CLI that delivers, plus an explicit assertion that the canary confirmed (see deviation 3).

## Acceptance criteria, measured

| Criterion | Reading |
|---|---|
| `rg -c 'token_reported_in_capture' canary.rs` ≥ 1 | **4** |
| `rg -c 'is_top_level\|last_top_level_result' canary.rs` == 0 | **0** (rg exit 1, no matches) |
| `rg -c 'pub mod canary;' lib.rs` == 1 | **1** |
| `rg -c 'claude_delivery_canary' pipeline_launch.rs` ≥ 3 | **5** |
| `rg -c 'canary' state.rs` ≥ 1 | **6** |
| Real launcher exists; Task 1's tests reference only the injected one | `ClaudeCanaryLauncher` at `canary.rs:252` and `:262`; the test module starts at `:443`, so **no Task 1 test references it**, and **no Task 1 test spawns a `claude` process**. Task 2's tests DO spawn a stubbed `claude` — that is what makes them wiring tests. |
| A refused launch leaves no monitor pid | Asserted inside `absent_canary_refuses_to_launch`, in memory and after reload, from a pre-set stale pid |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The plan's Task 2 verify command cannot run**

- **Found during:** Task 2 verification
- **Issue:** `<verify>` specifies `cargo test -p devflow --lib pipeline_launch::`. The `devflow` package has **no lib target** — it is a binary crate. The command prints `error: no library targets found in package 'devflow'` and exits **101**.
- **Fix:** Used `cargo test -p devflow --bin devflow pipeline_launch::…` throughout. The package name in the plan is correct (`devflow`, not `devflow-cli`); only `--lib` is wrong.
- **Worth recording separately:** piping that command through `tail` reports `EXIT=0`, because a pipeline's exit code is the last command's. That is the exact `CLAUDE.md` trap, reproduced here while measuring it. The `101` above is cargo's own exit code, captured with a redirect rather than a pipe.

**2. [Rule 3 - Blocking] Two existing fixtures launch at `Stage::Code` and now meet the gate**

- **Found during:** Task 2, first full-suite run
- **Issue:** `pipeline_launch::tests::launch_stage_inner_resets_checkpoint_resumes_counter` and `staleness::tests::mid_run_stage_transition_does_not_readjudicate_staleness` both drive a launch at the one widened stage, so they now hit the canary, get `Absent` from an `exit 0` stub, and fail on the refusal.
- **Fix:** Each records `state.canary = Some(CanaryOutcome::Confirmed)` before launching, with a comment saying why. That is what a real run looks like by its second Code launch, and it keeps each test measuring what it was written to measure.
- **Rejected alternative:** an env-var bypass. D-15 forbids a silent downgrade path, and a test-only escape hatch in production code is exactly that.
- **The blast radius was smaller than it first appeared, and worth recording.** The first full run showed **26 failures**; 25 were `PoisonError` from `ENV_MUTEX` being poisoned by the single genuine panic. A filtered run (`pipeline_launch::tests` alone) reported all-green while the staleness failure was still live — filtered runs hide cross-module effects, and only the full-suite run found it.

**3. [Rule 3 - Blocking] `phase7_cli`'s end-to-end flow stalls at the gate**

- **Found during:** Task 2, full-suite run
- **Issue:** `reference_and_cleanup_worktree_cli_flow` drives the real binary through a whole pipeline. Its fake `claude` printed a `DEVFLOW_RESULT:` line and exited, so the canary read no token, refused at Code, and the test timed out waiting for the Validate gate (`wait_for` allows 5s).
- **Fix:** The fake CLI now reads **one** line of stdin (`read -r turn` — blocking on full EOF would hang against a pipe deliberately held open past the first turn), greps the token out of it when present, and echoes it back inside a top-level `result` event. A fake CLI that cannot deliver is, correctly, a CLI this pipeline refuses to run on; modelling delivery is the faithful fix rather than a workaround.
- **Also added:** an explicit assertion that `events.jsonl` contains `claude_delivery_canary_confirmed`. Reaching the Validate gate already implied it, but only implicitly — a later change that stopped running the guard would otherwise pass silently.
- **Verification:** `17 passed; 0 failed` in 1.48s, so no idle-timeout stall was introduced.

### Deliberate departures from the plan's letter

**4. `ClaudeCanaryLauncher` is not a unit struct.** The plan specifies `pub struct ClaudeCanaryLauncher;`. It carries `workdir: PathBuf`, because `CanaryLauncher::run(&self, prompt, capture)` — a signature the plan fixes — has nowhere to put a working directory, and `hermetic_command` requires one. Deriving it from `capture.parent()` would silently couple the child's cwd to where DevFlow keeps runtime files.

**5. The canary's close rule is `monitor::CloseRule`, not "close on the first top-level `result`".** The plan says to close stdin "once the child emits a top-level `result` event". Taken literally that defeats the guard: the whole premise under test is that a session kept alive past its first turn is woken back up when a background task completes, so releasing stdin at the first `result` ends the session before any notification turn can arrive — and the guard would report `Absent` against a perfectly healthy CLI. Constraint 4's AND rule is the correct close condition, and reusing the production `CloseRule` avoids a second one. The canary prompt therefore asks for both a bare token line and a `DEVFLOW_RESULT:` marker line; they are separate so the marker's JSON body stays coupled to `AgentResult`'s schema and the token does not.

### Recorded, not auto-fixed

**6. Task 2 was not written test-first, despite `tdd="true"`.** Task 1 was (RED `ddb8a2b` → GREEN `ae80c76`, with a real behavioural RED reading). Task 2's implementation went in before its tests, so those five tests are a **pin, not a driver**. The three mutation checks in the table above are what stands in for the missing RED — each reddens exactly the intended test — but a pin and a driver are not the same thing and should not be read as such.

**Plan-level TDD gate sequence:** `test(31-03)` → `feat(31-03)` → `feat(31-03)`, so RED and GREEN gate commits both exist in order.

---
**Total deviations:** 3 auto-fixed (all Rule 3 blocking), 2 deliberate departures from the plan's letter, 1 recorded process shortfall.
**Impact on plan:** No scope creep. One test was added beyond the plan's ten named ones (`launch_stage_inner_refuses_at_code_when_the_canary_cannot_confirm`), to cover the linkage the injected-outcome tests cannot show.

## Issues Encountered

**The prompt-echo test's own negative control was wrong on its first run, and the RED phase is what caught it.** The control asserted that `"type":"user"` appeared *before* the token in the raw capture text. `serde_json` writes object keys in sorted order, so `type` lands after `message` — the check read backwards and failed against a correct fixture. Rewritten to parse each token-bearing line and assert its `type`, which is order-independent. Recording this because the failure mode is instructive: a position-based assertion over serialized JSON looks like a measurement and is really a bet on key ordering.

**The broken-windows ledger refused this plan's entries, for a pre-existing reason.** `gsd-tools windows append` failed with `Ledger counts disagree with entries: frontmatter open/waived/fixed/total=1/1/4/6 but entries yield 0/1/5/6`. `.planning/WINDOWS.md` was last modified in phase 28 (`5472719`) and this plan never touched it, so the mismatch predates this work. **Not fixed here** — it is out of this plan's scope, and hand-repairing a shared planning file from a parallel worktree while a sibling executor is running is how two agents collide on one file. Flagged for the orchestrator; the two entries this plan would have filed (deviations 1 and 6 below) are recorded in this summary instead.

## Known limitations (carried forward, not defects)

1. **A `Confirmed` outcome means the notification path works. It never means work happened.** The agent can read the token out of its own prompt and emit it without doing anything — 999.67's shape, accepted deliberately as threat T-31-11 rather than mitigated, because mitigating it needs per-child tokens and D-14 defers those. Stated in the module header, in `run_delivery_canary`'s doc, and here.
2. **`reap` signals the child only, not its process group.** A descendant the canary child itself spawned can outlive the kill. Bounded by the 300s deadline and by a prompt that dispatches a task touching nothing.
3. **The guard costs a real agent invocation at the start of every run** that reaches the Code stage on Claude. That is D-13's design, not a side effect, but it is a real per-run cost (tokens and wall-clock) that did not exist before this plan.
4. **`State::canary` is operator-writable**, so a hand-edited `state-NN.json` can pre-set `Confirmed` and skip the guard (T-31-14, accepted — the file is already operator-writable by design, and the guard defends against a silent CLI change, not against the operator). Two test fixtures now do exactly this, deliberately.
5. **No test asserts what happens on a `claude` binary that is missing entirely** at the launch site. The path is `spawn` → `Err` → `Unverified` → refusal, and `canary_unverified_when_the_launcher_fails` covers the second half with an injected error, but nothing drives the first half.

## Note on `actuals.tokens`

`70,848` = chars/4 over the full text of the six files changed (283,393 chars), the same scale 31-01 recorded as its primary reading. The diff-based alternative is `61,026 / 4 = 15,257`. Against `estimate.tokens: 62000` these read very differently (within ~14% versus a 4x overestimate). Both are recorded rather than picking the flattering one; the scale question 31-01 raised is still open and a calibration pass should settle it before either number is trusted.

## Next Phase Readiness

- **31-04 (D-11 opt-out)** — the flag selects `exec_command_single_document` + `MonitorLaunch::Legacy`, which `canary_gate` deliberately does not guard (`canary_gate_only_applies_to_the_stream_launch_path` pins that). **Whoever lands 31-04 should decide explicitly** whether the opt-out is also intended as an escape hatch from a false canary refusal; today it is one by construction, and that is not currently stated anywhere as a decision.
- **31-05 (acceptance run)** — this plan's `backstop` must-have is that plan's job. The guard is now in the path, so the acceptance run will exercise it for real: the first `devflow start` on Claude at Code will plant a token and either confirm or **refuse the run outright**. If the real CLI's behaviour has changed since 2.1.220, 31-05 will discover it as a refusal rather than as an orphaned wave — which is the entire point, but it is worth expecting rather than being surprised by.

## Self-Check: PASSED

All six claimed files exist on disk. All three claimed commits exist in `git log 51ad2c60..HEAD`: `ddb8a2b`, `ae80c76`, `b02fe17`. Working tree clean apart from this summary. `STATE.md` and `ROADMAP.md` deliberately untouched — the orchestrator owns those writes after the wave merges.

---
*Phase: 31-claude-adapter-launch-path-pipe-owning-monitor-999-64-arc-cl*
*Completed: 2026-08-03*
