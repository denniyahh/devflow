# Changelog

## 2.5.0 — 2026-08-07

Phase 35 (loop termination and baseline correctness). Four defects that let an unattended
`devflow start` run **either loop without a bound or stop for the wrong reason**, plus the
release preflight that has now false-negatived on two separate release cuts.

The unifying fault is a lossy collapse: three different places treated "could not measure" as
"measured zero". A single transient `git` failure was enough to forge a fresh
`consecutive_failures` baseline (999.77), and the same forged zero made the result cascade
classify a *successful* agent as `Failed` (999.87). Both are closed by making the count's
absence representable rather than by patching each consumer.

**This release contains breaking changes to `devflow-core` and does not take a major version
bump. That is deliberate, not an oversight** — see *Public API* below for the reasoning and the
full enumeration.

### What's new

- **The Code↔Validate loop now has a bound that trivial commits cannot defeat (999.78).** A
  never-reset per-phase Validate-failure total accumulates independently of the commit count and
  gates at `MAX_PHASE_VALIDATE_FAILURES` (10). Previously the Code stage's fix command committed
  `.planning/` artifacts on cycles that changed no source, which reset the streak every cycle and
  made `MAX_CONSECUTIVE_FAILURES` unreachable in the ordinary case. The ceiling **gates, it never
  aborts**: approve, loop back, or abort are the same three choices an ordinary Validate gate
  offers, and the phase's state survives.
- **The Supervise gate message reports how long the phase has actually run (999.78).** It leads
  with the cumulative per-phase total and relegates the streak to a parenthetical, so the 2nd, 5th
  and 9th gate no longer read identically.
- **`{N}-VERIFICATION.md` goes stale (999.79).** `devflow start --phase N --force` no longer
  inherits the previous run's committed verdict. A content fingerprint of the artifact is recorded
  at run start and compared on loop-back; an artifact unchanged since then dispatches a full
  execute instead of a `--gaps-only` pass against zero matching plans.
- **`release --check`'s tag-signing preflight performs the operation instead of predicting it
  (999.86).** It now signs throwaway bytes with `ssh-keygen -Y sign` in a private per-call
  workspace and reports the exit code. The probe runs under `setsid`, so a controlling terminal's
  `/dev/tty` passphrase prompt cannot capture it, and it is bounded by a wall-clock ceiling.

### Fixed

- **A transient `git` failure granted a free `consecutive_failures` reset (999.77).**
  `phase_commit_count` collapsed an unrunnable or unparseable `git` to `0`, the baseline was
  written unconditionally, the next real count exceeded that forged zero, it read as forward
  progress, and the streak reset to 1 — one free extension of the failure ceiling per transient
  fault. The baseline write now happens only on a real measurement.
- **The same forged zero misclassified a successful agent as `Failed` (999.87).** `evaluate_layer2`
  now returns `Ok(None)` on an unmeasurable count and falls through to Layer 3, and
  `evaluate_layer3` — which carried its own independent copy of the same collapse — was re-pointed
  at the shared counter. An unmeasurable count is now `Unknown` with no commit figure. Note this
  corrects the recorded classification and the operator-facing reason string, **not** the dispatch:
  `Failed` and `Unknown` both route to `Action::GateReview`.
- **The signing predictor inferred viability from `ssh-add -l` (999.86).** Agent identity listings
  cannot see on-disk private key material, so a correct key that no agent happened to hold reported
  as not viable. It tested a condition the real signing operation does not require.
- **The worktree-mode `GateReview` checkpoint call site is now regression-tested (999.84).** The
  fix shipped in 2.4.0 was correct by construction but reverting its argument left the suite green.
  A test now drives the call site and was watched failing under the performed revert, with a
  localisation control that stayed green.

### Public API (`devflow-core`)

**This release breaks `devflow-core`'s public API under a minor version bump.** Strict semver
would say `3.0.0`. That was put to the operator and declined on the grounds that `devflow-core`
has no external consumers, so the compatibility risk is theoretical, while renaming the active
milestone would disturb a roadmap-parsing window this project has already broken twice. **The
break is documented here instead of being versioned** — this enumeration is what stands in for the
version number that was declined. A reader who finds a breaking change under a minor bump should
not have to guess whether it was an oversight.

Every row below was verified against the source tree, not transcribed from the plans.

#### Changed — breaking

- **`devflow_core::agent_result::phase_commit_count`** — return type `u32` → `Option<u32>`.
  `None` means the count could not be established (the `git` child could not be executed, or its
  stdout did not parse); `Some(0)` means git ran and the branch genuinely has no commits. Closes
  999.77 and 999.87, which are both instances of those two cases being indistinguishable.
- **`devflow_core::mode::Mode::should_gate`** — signature widened from
  `(self, stage, consecutive_failures)` to
  `(self, stage, consecutive_failures, phase_validate_failures)`. Widened rather than disjuncted at
  the call site deliberately: five tests re-derived the old two-argument expression to mirror the
  production decision, and a call-site disjunct would have left all five compiling and silently no
  longer mirroring anything. Required by 999.78.

#### Removed — breaking

- **`devflow_core::git::classify_ssh_add_status`** and **`devflow_core::git::SigningStatus`** —
  the `ssh-add -l` exit-code predictor and its status enum, removed together with the private
  `inline_key_fingerprint` orphaned alongside them. They inferred tag-signing viability from agent
  identity membership, which is not a condition signing requires; the result was a live false
  negative on two release cuts with the correct key present (999.86). Replaced by
  `check_signing_viability`'s direct `ssh-keygen -Y sign` probe. A note at their former location in
  `git.rs` records this, so a reader who reaches for them finds the reason rather than a gap.

#### Changed — behaviour only, no signature change

- **`devflow_core::agent_result::evaluate_layer3`** — signature, visibility and argument list are
  **unchanged**; a caller needs no edit. Its observable classification does change: its inline
  `rev-list --count` was deleted and re-pointed at `phase_commit_count`, so a count that cannot be
  measured now classifies as `AgentStatus::Unknown` with no commit figure, where it previously
  reported `Failed` with a forged zero. Listed separately rather than under *breaking* because
  filing a behaviour change as a compatibility break would dilute the two entries a consumer must
  actually act on — but listed at all, because someone tracing a changed verdict needs to find it.

#### Added — non-breaking

- **`devflow_core::agent_result::phase_verification_fingerprint`** — new `pub fn` returning
  `Option<u64>`, a stable FNV-1a/64 content hash of `{N}-VERIFICATION.md` (999.79). Written out
  rather than using `std`'s `DefaultHasher`, whose output is not guaranteed stable across toolchain
  versions; this value is persisted by one process and compared by another, so an unstable hash
  would read as "changed" after a Rust upgrade — the fail-open direction.
- **`devflow_core::mode::MAX_PHASE_VALIDATE_FAILURES`** — new `pub const`, value `10` (999.78),
  with a compile-time assertion that it stays strictly above `MAX_CONSECUTIVE_FAILURES`.
- **`devflow_core::mode::phase_failure_ceiling_reached`** — new `pub fn`, the single implementation
  of the ceiling comparison, shared by the gate message's ceiling clause and the reset so the two
  cannot disagree (999.78).
- **`devflow_core::state::State::phase_validate_failures`** (`u32`, 999.78) and
  **`devflow_core::state::State::last_verification_fingerprint`** (`Option<u64>`, 999.79) — two new
  `pub` fields. **Additive rather than breaking**: `State` carries `#[non_exhaustive]`, so no
  external crate can construct it with a struct literal in the first place, and both fields are
  `#[serde(default)]`, so existing `.devflow/state-NN.json` files deserialize unchanged. The
  persisted JSON shape gains two optional keys.

#### Unchanged, and stated because it was anticipated otherwise

- **`devflow_core::agent_result::phase_verification_exists`** — planning anticipated that the
  freshness work might force a third public-API break here. **It did not.** Its signature,
  visibility and behaviour are unchanged; the freshness rule was built on an additive function and
  a private path resolver that both it and the new fingerprint now share. Recorded so the absence
  reads as a checked non-event rather than an omission. It currently has no in-workspace caller.

### Known Issues

- **The verification-freshness rule infers provenance from bytes, not from run identity.** An
  artifact whose content changes for any reason other than this run's Validate agent — a worktree
  merge-back, an operator edit — reads as authored-this-run and dispatches `--gaps-only`, which is
  the failure direction 999.79 exists to prevent, reached by a different route. Tracked as 999.89.
- **The `setsid` guard on the signing probe's regression test is n=1 per arm, one host, one
  container.** `git::tests::the_signing_probe_is_not_captured_by_a_controlling_terminal` was added
  and confirmed to fail (`REGRESSION:` panic) when the `pre_exec` is removed — 999.88 is resolved,
  not open — but the test is timing-based (a pathologically loaded box could false-red it) against
  one OpenSSH build and one encrypted key.
- **`MAX_PHASE_VALIDATE_FAILURES = 10` is a judgement, not a measurement.** Nothing establishes how
  many Validate failures a genuinely-converging phase takes.
- **Two in-source comments (`idle_timeout_result` and a test-module comment) still describe a
  mechanism a previous release replaced.** Carried over from 2.4.0, explicitly out of scope here.
  Tracked as 999.85.
- **The drain gate still has not been observed to see sub-agent concurrency.** Carried over from
  2.4.0. Tracked as 999.83.

## 2.4.0 — 2026-08-06

Phase 33 (loop-back correctness for multi-wave Validate↔Code cycles) and phase 34 (stream-json
coverage, the Validate trust boundary, and Layer 0 in worktree mode). Together they close the
**structural defects blocking unattended, multi-wave `devflow start` runs** found during the phase
29 dogfood run and phase 31 planning, so unattended dogfooding can safely resume.

The headline is the Validate trust boundary (999.74): an agent that self-reports `status: failed`
alongside `verdict: pass` used to have that verdict grafted onto an otherwise-derived `Success`
result and advance to Ship unattended. `reconcile_layer0_verdict` now consults Layer 1's own status
before transplanting its verdict, and `classify_validate_outcome` was rewritten as an exhaustive
match naming all seven `AgentStatus` variants — an eighth is now a compile error, not a silent join.
The exploit was reproduced against the real cascade before the fix, with a matched positive control
proving the fix isn't indiscriminate.

### What's new

- **The Code↔Validate loop no longer false-gates on healthy work (999.66).**
  `consecutive_failures` is measured from a persisted commit-count baseline instead of an unreset
  counter, so a healthy 3+ wave phase no longer false-gates at wave 3 while a genuinely stuck loop
  still reaches `MAX_CONSECUTIVE_FAILURES`.
- **Loop-back fix selection reads the worktree (999.65).** `select_loop_back_fix` reads
  `{N}-VERIFICATION.md` from the phase's worktree instead of the main checkout, making
  `FixType::GapsOnly` reachable on the Validate path in worktree mode for the first time.
- **All five stream-json stages joined the launch path on real evidence (999.73).** Widened beyond
  `Stage::Code` against committed, PII-scrubbed production captures with per-stage drain analysis,
  not a flag flip. The capture campaign refuted its own premise — zero `background_tasks_changed`
  events across 1063 events despite 8 concurrent sub-agent dispatches — filed as a known gap rather
  than absorbed; see Known Issues.
- **Layer 0 external verification works in worktree mode (999.76).** Declaration discovery now
  reads the execution root, so a correctly-declared `external_verify` probe set no longer silently
  never executes — the plan-28-03 checkpoint auto-decide path is fixed at the same call site.

### Fixed

- Self-reported `status: failed` paired with `verdict: pass` could reach Ship unattended via the
  Layer 0 verdict graft (999.74). Closed by gating the graft on Layer 1's own status.
- The Validate classifier's status-position wildcard could discard a non-`Success` status silently;
  now every `AgentStatus` variant is named explicitly.

### Known Issues

- **The drain gate has not been observed to see sub-agent concurrency** on Claude CLI 2.1.222 — the
  safety mechanism the widened stream-json stages' unattended behavior depends on. Tracked as 999.83.
- **One call site in the worktree-mode checkpoint fix has no regression test.** The fix is correct
  by direct source read and by two root-sensitivity tests on the function it calls, but no test
  drives the call site itself: reverting its argument leaves the full test suite green. Tracked as
  999.84.
- **Two in-source comments (`idle_timeout_result` and a test-module comment) describe a mechanism
  this release's own fixes replaced.** Their conclusions are still correct; their stated reasoning
  is not. Tracked as 999.85.

## 2.3.0 — 2026-08-04

Phase 30 (the stream-json parser and the feasibility gate) and phase 31 (the
launch path itself). Together they close the **999.64 arc**: a DevFlow-driven
phase containing a multi-plan wave now completes that wave without orphaning
delegated work — the failure that had blocked every attempt at an unattended
run through this project's history. Phase 29 remains unmerged and is not in
this release.

The headline is phase 31: the Claude adapter's detached `sh` monitor is
replaced by a pipe-owning Rust monitor, and the adapter always launches with
`--input-format stream-json --output-format stream-json`, prompt delivered on
stdin rather than argv. Holding stdin open past the first turn is what lets a
background helper agent's completion actually reach the parent session — the
notification had nowhere to arrive before this. Verified with a live two-plan
acceptance run, not just integration tests: both plans produced a `SUMMARY.md`
and both merged, crossing the exact point where a prior attempt (phase 29)
orphaned both executors of a two-plan wave.

### What's new

- **Idle timeout.** A stage that goes quiet for longer than a configurable,
  floor-clamped window now fails loudly with a distinct `AgentStatus::IdleTimeout`,
  naming the agent's commits and rolling nothing back — instead of hanging
  indefinitely or having a later signal misread as an unrelated failure.
  `DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` raises the window above its floor; it can
  never be set lower.
- **Startup delivery canary.** Before a stream-json launch, DevFlow now
  confirms the background-notification path actually works with one throwaway
  task, and refuses to run rather than silently degrading if it doesn't.
- **`--legacy-claude-launch`** (and `DEVFLOW_CLAUDE_LEGACY_LAUNCH`) — an
  explicit, off-by-default escape hatch back to the pre-31 launch path, loud on
  every use, with no automatic fallback.
- Exit-code arbitration: a stream-derived `Success` can no longer stand against
  a contradicting non-zero process exit code.

### Fixed

Two defects from this release's own peer code review, neither reachable by the
acceptance run because it only exercised healthy paths: an idle timeout could
fire and overwrite a stage that had already completed successfully, and a
single non-UTF-8 byte in agent output could silently truncate the capture.
Both are covered by mutation-tested regression tests.

## 2.2.0 — 2026-07-31

Phase 27 (hermetic git invocation) and phase 28 (the checkpoint answer return
path). Phase 26 remains unmerged and is not in this release.

The headline is phase 28: a `gate="blocking-human"` checkpoint used to be a dead
end for any DevFlow-driven run. The agent stopped, asked a question, and no path
existed by which an answer could reach it — every retry spawned a fresh process
that asked the identical question again. A plan that *correctly* gated an
irreversible decision became a plan that could never finish unattended.

### ⚠ Behavior change: human-blocking checkpoints are now resolved by the agent

**This is unconditional and there is no flag or config toggle to disable it.**
When a stage halts at a task declared `gate="blocking-human"`, DevFlow now
relaunches the exact exited Claude session with an instruction to decide the
checkpoint itself, and records what it decided.

This deliberately overrides `checkpoints.md` rule 6 (`blocking-human` is never
auto-approved, in any mode). It was adopted because DevFlow has no usable
notification or response channel — the only push mechanism is an
operator-supplied `DEVFLOW_GATE_NOTIFY_CMD` that is a silent no-op when unset,
and the only pull mechanism is running `devflow status` by hand. A "wait for a
human" default would not degrade gracefully; it would hang. Given that, a flag
implying a working "off" state would have been misleading.

What bounds it: the path is reachable **only** for a checkpoint that the
operator's own approved plan declared — the authorizing check is a static scan
of the phase's `PLAN.md` files, which an agent cannot influence at runtime, and
it is evaluated strictly before any agent-controlled signal. Resumes are capped
(`MAX_CHECKPOINT_RESUMES`), and exhaustion falls through to the existing
never-silent gate. Every auto-decision writes a `checkpoint_auto_decided` event
to `.devflow/events.jsonl` naming the session, the instruction, and the policy —
with no human in the loop beforehand, that record is the only way anyone learns
after the fact what the agent decided.

### Fixed
- **A `blocking-human` checkpoint no longer strands a headless run (999.57).** DevFlow statically scans the stage's plans for a declared human-blocking task before launching, confirms from captured stdout that one actually fired, and resolves it by resuming the exact exited session rather than spawning a fresh one — no CONTEXT/RESEARCH re-read, no re-running completed tasks
- **The checkpoint confirmation reader now matches what a real run actually emits.** The reader was built against a rendering predicted by reading the *emitting* source, and shipped with that caveat recorded rather than hidden. A live end-to-end run then showed the executor renders the value as a markdown code span — ``**Gate:** `blocking-human` `` — and the matcher, which trimmed only `*` and whitespace, terminated on the backtick and produced an empty token. Genuine checkpoints fell through to the generic gate. The unit suite could not catch it: every fixture was built from the same prediction. Regression tests are now transcribed from the live capture. The reader's documented safe-direction property held throughout — a false negative degrades to the never-silent gate, so nothing was ever silently authorized
- **`devflow resume` no longer discards an unfired `--until` cap (999.60).** The stop-marker clear was unconditional, so a cap the operator set that had not yet fired was silently dropped and the pipeline ran past the stage they named — observed live during phase 27's own dogfood run, which advanced into Ship unguarded. The clear is now gated on the pipeline having actually stopped
- **The Define stage no longer invokes an interactive interview headlessly (999.59).** When `CONTEXT.md` was absent, Define issued `/gsd-discuss-phase`, which hangs on `AskUserQuestion` under `claude -p` with no operator present. The branch is deleted rather than flag-gated: whether to run an interview is decided before `devflow start` is ever invoked, so there is no runtime accommodation to make
- **All 41 production `git` invocations are hermetic (phase 27).** Every `Command::new("git")` in production code now routes through `git_command`/`hermetic_command`, including two indirect `sh → cargo → git` edges and `monitor.rs`'s spawn of the coding agent itself — the highest-consequence site, and one that was not in the original grep scope. Under a hostile `GIT_DIR` the suite went from 37 deliberately-failing tests to 0

### Added
- **`yes_ship` is settable in `devflow.toml`** (and via `DEVFLOW_YES_SHIP`), in addition to the existing `--yes-ship` flag. The flag ORs with the config value rather than replacing it, so passing it always wins. A run whose authorization came from config prints a line naming `devflow.toml` as the source — a standing default, but never a silent one
- **`checkpoint_auto_decided` events** in `.devflow/events.jsonl`, emitted before the relaunch spawns so a crash mid-relaunch still leaves the decision recorded
- **`devflow-core` public API:** `verify::phase_plan_files`, `verify::phase_has_blocking_human_checkpoint`, `agent_result::{claude_session_id, session_id_from_capture, blocking_human_checkpoint_reported, checkpoint_reported_in_capture}`, `config::yes_ship`, and `State::{session_id, checkpoint_resumes}`

### Changed
- **`State` is now `#[non_exhaustive]`.** Downstream crates must construct it through `State::new` and assign fields afterward, rather than by struct literal. Deserialization is unaffected — every `#[serde(default)]` field still loads state written by older binaries. `State` gains a field roughly every phase that introduces a run-scoped concept; without this, each addition is a semver-breaking change for any consumer using a literal. Paying that cost once here makes every future field additive
- **`--yes-ship` may now come from configuration**, deliberately reversing the phase 23 decision that made it per-run only *"so a standing unattended auto-merge can never become the silent default."* That decision's own stated cost applies: relaxing this is easy, re-tightening it after operators depend on the persisted setting is not. The never-silent notice above is the compensating control. What did **not** change: the Ship gate still fires and still records an explicit, attributed approval rather than being bypassed
- **`session_id` is read only from the result envelope's top-level key**, never from the agent-authored `DEVFLOW_RESULT` marker, and is deliberately *not* a deserialized field on `AgentResult` — otherwise an agent could nominate which session DevFlow resumes into

## 2.1.0 — 2026-07-28

Phase 25 — the blockers that stopped an unattended `devflow start` run from
reaching a completed Ship stage. Phase 23 had proven the goal unreachable: its
furthest attempt drove Define→Plan→Code unattended then halted, and two of its
three attempts needed a human to repair the base ref before `devflow start`
would launch at all.

### Fixed
- **`compute_version` no longer invents version numbers (25c).** It derived a version from three inputs — `Cargo.toml` major, raw tag *count* as minor, and commits-since as patch — which computed `~1.11.359` against a real `1.8.1`. It now derives the baseline from the highest semver tag *reachable from HEAD* and classifies the commits since that baseline with a conventional-commit parser. A baseline that exists in the repository but is not reachable from the release branch is refused outright (`UnreachableBaseline`) rather than silently producing a smaller version. `release_range_start`'s commit-range anchor now walks the full ancestry path, so a squash-sync release topology no longer drops commits from classification
- **A major version bump can no longer ship unattended (25c/D-09).** `devflow start --yes-ship` pre-authorizes the Ship gate, which meant a major bump could ship with no human ever seeing it. A major bump now opens its own preflight gate that `--yes-ship` cannot auto-approve — the auto-response is a parameter of the gate call, never derived from state. The check is evaluated against the phase's own worktree, not the main checkout, so it fires in the default execution path
- **`devflow doctor` no longer reports live, registered processes as orphans, and `gate sweep --reap-strays` no longer SIGKILLs them (25d).** The `/proc` census is structural by design — argv shape plus euid, no ownership test — and `doctor` asserted an orphan conclusion the code never established, naming a destructive repair for it. On a machine running concurrent DevFlow phases this listed every sibling run's monitor as a stray. Both surfaces now filter that census through one shared reachability set: every `monitor_pid` and lock holder across every registered root. `--root` unions into that safety set and never narrows it — narrowing would protect less while still reaping machine-wide. A process whose project root was deleted contributes nothing to the set and so remains reapable, which is the case the feature exists for
- **`devflow start` no longer launches from a stale base ref, and repairing one can no longer corrupt it (25a).** The base branch is fetched and compared before launch; when it is strictly behind it is fast-forwarded, otherwise the run refuses loudly. The fast-forward is a compare-and-swap (`git update-ref` with an expected old value), so a ref that moved between the check and the write is refused rather than silently rolled back, and its "not checked out" precondition is evaluated across every worktree in the repository rather than just the one being launched from
- **Build-staleness is adjudicated once per run (25b).** The self-dogfood staleness check ran on every stage launch, so a run that started against a current build could be blocked mid-flight by a later commit. It now runs once, at `start`, against the phase's worktree HEAD, and never again for the life of the run
- **A test-suite race that produced a ~50% CI failure rate is closed (25e/999.47).** A process's `/proc` cmdline is inherited from its parent during the window between `fork` and `exec`, so a census taken inside that window could match the wrong process. A bounded exec-visibility barrier now covers every affected spawn site, and the production reaper refuses to signal inside the window via an age floor. Closed against an 11-observation streak with the residual failure probability stated rather than claimed eliminated

### Added
- **`devflow gate sweep --reap-strays`** — opt-in reaping of orphaned monitor processes with bounded TERM→KILL escalation and verified death, plus a `--dry-run` preview. Identity is re-confirmed immediately before any signal, so a recycled PID cannot be hit
- **`devflow doctor` reports state-orphaned processes as their own finding class**, describing only what was actually checked — an argv-shape match owned by the caller — instead of asserting orphan-ness

### Changed
- **CONTRIBUTING.md's release procedure no longer drifts from what the code does (25f).** The tagging step now shows the explicit key-selection form (`git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" tag -s`) rather than a bare `git tag -s`, which signs with whatever key happens to be configured on the machine — a real hazard here, because the release key and the agent's commit key share a `user.email` and differ only by fingerprint
- **README documents where the automation stops** — the release cut itself (opening the `develop` → `main` PR, tagging, publishing) is deliberately manual; `devflow release --check` runs the preflight but does not execute the release

## 2.0.0 — 2026-07-26

The breaking change the v2.0.0 slot was held open for (milestone note,
2026-07-24: "until a genuinely breaking change earns the 2.0 slot").

### Removed (Breaking)
- **The `sequentagent` CLI verb is gone.** `devflow sequentagent --phase N --agents a,b [--force]` — two agents run sequentially on one phase, each in its own worktree, with a rebase handoff between them — no longer exists; invoking it now fails with clap's unrecognized-subcommand error (D-11, 23d). Removed with it: the CLI-side `sequentagent`/`run_agent_blocking`/`integrate_agent_branch` implementation, the two-agent Hermes cron-resume builder (`ship::build_cron_instructions`), and `status`'s two-agent slot-liveness rendering. The primary single-agent rate-limit resume path (`devflow resume --phase N`, `ship::build_single_agent_cron_instructions`) and `devflow parallel` (run N phases concurrently) are unaffected. The capability intent is preserved, not discarded — DEN-67 (999.42) tracks reimplementing agent failover on the socket-addressable supervisor (DEN-58) if and when a second agent is supported, rather than restoring this in-process handoff

### Added
- **`devflow stop --phase N` ends a running phase cleanly (23c).** If the phase has an open gate it answers that gate with a rejection, so the target unwinds through its own abort path and no signal is sent; otherwise it signals the process recorded in the per-phase lock file (`.devflow/lock-{phase:02}`) — never `state.monitor_pid`, which is the PID `devflow status` displays and the wrong one to signal. Idempotent: safe against an already-stopped, never-started, or already-dead phase. Identity is matched against the lock's recorded `(pid, start-time)` pair rather than inferred from `/proc`, so a recycled PID cannot be signalled by mistake
- **`devflow gate sweep` answers or reports aged, unattended gates across every registered root (23b).** Bounds an abandoned run's lifetime without `kill(1)` and without a supervisor. `--dry-run` reports what would be reaped without writing; `--max-age-secs` overrides the six-hour default (`DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS`); `--root` restricts the sweep to one project instead of every root the machine has registered. On-demand only — nothing schedules it for you
- **`devflow evidence --phase N` reports DevFlow's own structural record of whether a phase shipped (23-06).** A read-only oracle sourced from the append-only event log, never from an agent-authored attestation document — so "did this actually ship?" becomes a code-checked question rather than a self-report. `--require-shipped` exits non-zero unless the record shows a completed Ship, making it declarable as a Layer 0 `external_verify` probe; `--json` for machine consumption
- **`devflow start` now refuses, before creating any worktree or branch, when the target phase is not reachable from `develop`** — the branch it forks from. Reachability means both the phase's `### Phase N:` heading in `ROADMAP.md` and its `.planning/phases/NN-*/` directory are present on `develop`; either being absent (checkably) refuses the run and names the missing half. This closes the exact 2026-07-26 acceptance-run failure: a phase promoted onto a feature branch and never merged to `develop` was invisible to the run, which then spent its Define stage discovering there was nothing to define before finally aborting. The check is skipped (fails open) when `develop` carries no `.planning/ROADMAP.md` at all, so repositories that don't keep a roadmap are unaffected (23f)

### Fixed
- **`release --check` no longer reports a viable inline signing key as missing.** `check_ssh_signing_viability` treated every `user.signingkey` value as a filesystem path, so a literal key blob configured inline (`key::ssh-ed25519 AAAA…`) — which git accepts and which never corresponds to a file — was reported as "the key file does not exist", failing preflight on a correctly configured repository. The value is now parsed the way git itself parses it, honouring git's own prefix precedence, and the viable branch reports the key's `SHA256:` fingerprint. Path-valued keys are unchanged, including the genuine missing-file case; an unparseable inline key fails soft to a warning rather than hard-failing the release. No key material and no filesystem path appears in any output (Phase 24, DEN-52; found by Phase 20's code review as INF-01)
- **`devflow start`'s self-dogfood staleness guard no longer hard-blocks a build whose embedded commit has genuinely diverged from `HEAD` (neither is an ancestor of the other) but differs only in non-build files.** The divergent-lineage arm of `embedded_commit_is_stale` now runs the same content check the linear strict-ancestor arm already ran (`ancestry_range_affects_build`, reused verbatim, per 21d/999.29), instead of returning `Stale` unconditionally on any divergence. This fixes the 2026-07-26 phase-23 acceptance run, which was blocked before Define ever launched by a binary whose only divergence from the target `develop` tip was a `.planning/` doc commit. A divergent range that touches a real build-affecting file (`.rs`/`Cargo.toml`/`Cargo.lock`/`build.rs`/`rust-toolchain.toml`) still blocks, unchanged (23g)

## 1.8.1 — 2026-07-24

Test-suite containment and quality cleanup. No behavior change for `devflow`
users: every production git invocation was already correctly pinned, so the
defect fixed here could only affect people running the test suite from a
checkout — but for them it was severe.

### Fixed
- **The test suite could escape its sandbox and corrupt the developer's own repository.** Running `cargo test` from a git hook — which `scripts/hooks/pre-push` does on every push — let fixtures operate on the real checkout instead of their tempdirs. Observed damage: the main repository flipped to `core.bare=true`, its committer identity was rewritten to a fixture's, and ten fixture commits were stacked onto local `main`, the first of which deleted all 511 tracked files. Root cause: git exports `GIT_DIR` into hook environments when the gitdir is non-default — precisely the case when pushing from a linked worktree — and `GIT_DIR` outranks a process's working directory when git resolves which repository to act on. Because Rust runs a test binary's tests as threads in one process, the whole suite inherited it and every fixture retargeted the real repo despite correctly pinning `.current_dir()`. Neither `git -C` nor `GIT_CEILING_DIRECTORIES` overrides `GIT_DIR`, so pinning the working directory can never be the containment mechanism; clearing the variables is. Fixed in three layers: `scripts/hooks/pre-push` now clears `$(git rev-parse --local-env-vars)` before running anything, `devflow_core::test_support::git_command` applies the same scrub per command across 50 migrated call sites, and a new guard test fails fast and names the cause when the environment is dirty
- `gate show` and `gate respond` no longer duplicate the omitted-`--stage` resolution logic, so their behavior cannot drift apart (999.30 WR-01)
- `gate show` reads the open-gate list once instead of twice, closing a narrow time-of-check/time-of-use window (999.30 WR-03)
- `doctor`'s planning-doc reconciliation uses `config::MAIN` instead of a second hardcoded `"main"`, removing an unlinked source of truth that would emit false Problems if the base branch ever became configurable (999.30 WR-02)

### Added
- `scripts/hooks/pre-commit`, a chaining shim. `core.hooksPath` replaces the hooks directory wholesale, so the documented `git config core.hooksPath scripts/hooks` install step would otherwise silently disable a global pre-commit secret scanner. It delegates to whatever hook you already had, and is a no-op if you have none
- `devflow-core` gains an off-by-default `test-support` feature exposing `test_support::git_command` for hermetic fixture construction. Not enabled in a normal build

### Changed
- DevFlow is now described as *opinionated* rather than *agent-agnostic* across the README, ARCHITECTURE, guides, both crate descriptions and `--help`. It bakes in one developer's specific take on branching, gating and verification rather than aiming to be a universal platform. Three agents are supported today (Claude Code, Codex, OpenCode) through a shared adapter; a fully agent-neutral driver architecture is backlog work
- `status` no longer re-scans `events.jsonl` per phase for the stage-entry timestamp, folding it into the existing single-pass event summary (999.30 IN-01)

## 1.8.0 — 2026-07-23

Operator legibility and observability: make DevFlow's operator surface legible
and its self-reported state trustworthy. Every unit is single-writer,
operator-facing, and reversible or detection-only. Phase 21.

### Fixed
- DevFlow's dogfood build-staleness guard no longer hard-blocks a self-run when the only commits ahead of the running binary's embedded commit changed nothing the compiler sees. `embedded_commit_is_stale`'s strict-ancestor arm now filters `git diff --name-only <embedded> HEAD` through the same `affects_compiled_binary` predicate the dirty-tree arm already used — a docs-only (`.planning/`) range reads `Fresh`, any build-input change reads `Stale`, and a git error fails toward `Stale`. The block message no longer claims "is not an ancestor of HEAD" for the common case where the embedded commit *is* an ancestor, just behind

### Added
- `devflow gate show <phase> [--stage]` prints a gate's full context untruncated (the `gate list` view caps at 100 chars), routed through the same control-character sanitizer so it stays terminal-safe
- `devflow status` now surfaces the rate-limit reset time in its cron hints, an in-stage progress line sourced from the latest `stage_launched` event (not the phase's own `started_at`), and `resume`/`advance` recovery-verb hints when a phase is stuck
- `devflow doctor` gains a detection-only planning-doc staleness check that reconciles `ROADMAP.md`/`STATE.md` version and outcome claims against the repo's git tags and flags drift, in both human and `--json` output — it never rewrites prose
- `sequentagent`'s second agent now writes a tracked, path-free slot record so `devflow status` observes it while it runs, with RAII cleanup on every exit path and no routing through the phase state machine

## 1.7.0 — 2026-07-23

Release correctness and operator control: close the two defects that made
DevFlow's own release cut unreliable, then add the operator controls the
pipeline never had. Phase 20.

### Fixed
- `VersionBump` now rewrites every workspace-member `[workspace.dependencies]` self-pin, not just `[workspace.package] version` — previously left the self-pin on the prior release, causing `cargo publish` to reject the upload as a duplicate (shipped broken two releases running)
- `devflow cleanup --force` is now fail-closed on worktree removal: it refuses whenever the recorded agent process is alive or the monitor is active — including `Unknown` (no recorded monitor) and `Stuck` (dead monitor) liveness states — with bounded-backoff retry for genuinely-dead phases and a descriptive warning if retries exhaust. Closes a real race behind two CI flakes in `phase7_cli.rs`
- `cleanup` no longer deletes the worktree of a phase intentionally parked via `devflow start --until <stage>` — it now recognizes the stop marker the same way `doctor` does, and requires `--force` to discard a parked phase

### Added
- `devflow start --until <stage>` halts the pipeline cleanly at a named stage instead of stranding state or orphaning a worktree — `--until ship` is rejected as a semantic no-op. `doctor` and `resume` are both aware of the stop marker
- `devflow release --check`: a read-only, network-independent release-cut preflight — workspace self-pin invariant, `develop`/`main` divergence (no `git fetch`), crates.io publish order, and `gpg.format`-aware signing viability (reports only a public-key fingerprint, never key material or a filesystem path)
- `devflow ship --phase N [--force]`: drives a phase through Ship when the monitor that would have consumed its already-written gate response is dead — reuses the existing fail-closed `finish_workflow` path verbatim, guarded by a per-phase lock and ack-file check so it cannot race a live monitor or double-run the terminal hook batch

### Changed
- `find_version_in_contents`'s TOML value parser now anchors on the opening quote and scans forward for the matching close, so a trailing inline comment (e.g. `version = "1.7.0"  # pinned`) no longer corrupts the parsed value — brings the read path back in line with the comment-preserving write path
- `member_depends_on` now recognizes long-form `[dependencies.NAME]` TOML sections in addition to inline tables, so `release --check`'s publish-order topo-sort no longer silently misses that dependency edge

## 1.6.0 — 2026-07-22

Release integrity and `main.rs` decomposition: close the two defects whose blast
radius reaches outside this repository, then decompose the 8,487-line CLI entry
point as a pure-move refactor. Phase 19.

### Fixed
- DevFlow's runtime artifacts can no longer end up in **your** commits. Every `.devflow/` directory now self-protects with a `.gitignore` containing `*` at creation time, so a routine `git add . && git commit` in a project DevFlow is running against no longer sweeps agent stdout, gate context, and workflow state into that project's history. This held regardless of whether the project's own root `.gitignore` mentioned `.devflow` — it usually didn't
- The `workflow_started` event no longer records the absolute path of the DevFlow binary, which leaked the operator's home directory and OS username into `events.jsonl`
- A release tag can no longer land on an empty commit: `commit_path` no longer forces `--allow-empty`, and is now idempotent when the file it is asked to commit is unchanged

### Changed
- `crates/devflow-cli/src/main.rs` went from 8,487 lines to 478, split into nine flat sibling modules (`staleness`, `preflight`, `pipeline_launch`, `pipeline_outcomes`, `pipeline_gate`, `parallel`, `commands`, `config_parse`, `test_support`). `main.rs` now holds only the Clap types, `CliError`, dispatch, `main`, `run`, and `project_root`. This is a pure move with no behavioral change — verified by symbol reconciliation (231 functions before and after, none lost or added), a normalized body diff showing zero logic-line changes, and a test name-set identical to a committed pre-split baseline (438/438)
- The CLI's test environment lock is now a single shared mutex rather than three independent ones that were sound only by accident. Distributing `PATH`-mutating tests across five modules would otherwise have broken the serialization they depend on
- CI uses `actions/checkout@v7`, retiring the Node 20 deprecation warning

### Added
- An AI change acceptance contract (`.claude/skills/ai-change-acceptance/`, plus a `CONTRIBUTING.md` section) stating what evidence a change must carry before it is accepted, and which test shapes are rejected as false signal

## 1.5.0 — 2026-07-21

Dogfood reliability hardening: make DevFlow's own supervision layer trustworthy
and legible from a plain terminal, and close the state-machine correctness gaps
that let a broken run look healthy. Phase 18.

### Added
- `devflow doctor` is now project-aware: it reconciles the persisted state against the event log, live process IDs, open gates, and branch ancestry, and reports a repair plan — read-only by default, mutating nothing
- Monitor liveness is observable: `monitor_pid` is persisted and probed, so `status` and `doctor` render a distinct "stuck — needs `devflow resume`" state instead of a dead monitor looking identical to a healthy between-stages pause
- Worktree-aware build-staleness: a self-dogfood build behind the worktree branch it is meant to be testing is now detected and blocked

### Changed
- `devflow doctor --json` emits a single JSON document — `{ "environment": [...], "reconciliation": [...] }` — instead of two concatenated top-level arrays, so ordinary JSON parsers can read the full `--json` output
- Build-staleness for a worktree-based phase is evaluated against the worktree branch HEAD rather than the project root, and a stale self-dogfood binary is now blocked rather than warned — the false-evidence class where a two-hours-behind binary re-ran an old hook batch
- The self-dogfood staleness-block event no longer records an absolute filesystem path in `events.jsonl`; the full path stays in the terminal message only

### Fixed
- The Code↔Validate failure loop can now reach its `MAX_CONSECUTIVE_FAILURES` ceiling: the counter was being reset on every stage transition, making the bound unreachable and the loop effectively unbounded under `--mode auto`
- Validate is passable again when an external post-condition is declared: the Layer 0 affirmative-success path now consults the agent's verdict instead of discarding it, advancing automatically only when the probe and the verdict agree, and gating for a human when they disagree or no verdict arrived
- Approving a preflight gate no longer re-runs the identical deterministic check and wedges on a multi-day poll: approval is an explicit override that skips the already-adjudicated check, with a bounded retry backstop; a loop-back still re-checks
- A failed stage relaunch no longer leaves a stale `monitor_pid` that `status`/`doctor` would misreport as "stuck"
- Stabilized a flaky parallel-worktree capture test that could race the monitor's capture archival

## 1.4.0 — 2026-07-20

Pipeline reliability: a completion cascade that cannot silently advance, build
provenance the binary can prove, and pre-launch readiness gates. Phase 17.

### Added
- Typed agent outcomes `ResourceKilled` (exit 137) and `AgentUnavailable` (exit 127), classified in Layer 2 alongside a `decided_by_layer` field recording which layer reached the verdict
- `outcome_policy::decide_action` — a pure, exhaustively-matched outcome-to-action function, so the never-advance guarantee is enforced by the compiler rather than by convention
- `devflow resume --phase N` — relaunches a phase from its saved stage after a rate limit or infrastructure pause, without resetting the workflow to Define or recreating the branch
- Build provenance: the binary embeds the commit it was built from and whether that tree was dirty, degrading gracefully when git metadata is unavailable
- Self-dogfood staleness gate — refuses to drive DevFlow's own workspace from a stale build, the Phase 16 false-evidence incident class
- Preflight readiness checks before every stage launch (plan interactivity, credential validity), reported as a named gate rather than a hard exit
- Separate infrastructure-failure counter, so transient rate limits and OOM kills no longer consume the functional-failure budget that gates a genuinely broken phase

### Changed
- `advance()` dispatches on an exhaustive match over typed outcomes instead of a two-value boolean; an `Unknown` outcome can no longer advance a stage
- Layer 3 splits the former blanket `Unknown`: a vanished process with zero commits and no declaration is now a `Failed` outcome that notifies a human, while commits-exist remains `Unknown` and stays gated
- Layer 0 runs for every stage and treats an approved, all-passing external post-condition as affirmative success, without relaxing the approval-mismatch security property
- Rate-limited outcomes route to the auto-resume machinery instead of being counted as functional failures
- `ChangelogAppend` moved after `VersionBump` and now commits its own write, so a changelog heading can never outlive the tag it claims
- Stage-advance events carry a structured evidence record in place of `reason: null`

### Fixed
- `write_version` no longer drops a trailing comma when rewriting `package.json`, which produced invalid committed JSON
- The release changelog and the git tag can no longer disagree: the shipped version is threaded through the hook context rather than recomputed after tagging
- Build-staleness checks ignore files that cannot affect a compiled binary, so a modified changelog or planning document no longer reports the binary as stale
- A build made from a commit ahead of the checkout is classified as ahead rather than stale
- The concurrent-ship test can no longer wedge the suite indefinitely on an unbounded gate poll

## 1.3.69 — 2026-07-18

- Released phase via DevFlow.

## 1.3.16 — 2026-07-17

- Released phase via DevFlow.

## 1.3.0 — 2026-07-17

- Phase 15: OSS readiness — README/ARCHITECTURE/guides rewritten against v2
  reality, CONTRIBUTING + pinned devcontainer with CI parity, dual
  MIT/Apache-2.0 licensing backed by both texts, devflow-core and devflow
  1.2.0 published to crates.io, per-phase SECURITY.md threat verification.

All notable changes to DevFlow.

## [Unreleased]

### Added
- Root `ARCHITECTURE.md` documenting crates, state machine, agent model, three-layer completion evaluation, monitor daemon, worktree model, git/ship model, configuration schema, and the add-an-agent checklist
- Stale binary detection in `devflow doctor`
- Never-silent gates: every stage failure (not just Validate) now writes a gate and fires a pluggable notify hook via `DEVFLOW_GATE_NOTIFY_CMD`, so unattended runs can never halt silently (WR-11). Gate poll timeout is configurable via `DEVFLOW_GATE_TIMEOUT_SECS` (default 7 days)
- Ship stage now runs `/gsd-code-review` before `/gsd-ship` and refuses to ship on any Critical-severity finding, reporting a distinct `review:`-prefixed failure that loops back to Code instead of gating
- Native per-adapter completion parsing: Claude's `is_error`/`num_turns` envelope fields and a Codex `--json` JSONL event-stream parser (previously only a generic marker/exit-code check)
- `verdict` field (`pass`/`gaps`) on the Validate stage's `DEVFLOW_RESULT` contract — `advance()` only proceeds to Ship on `verdict: pass`, closing the gap between "the agent ran validation" and "validation passed"
- `devflow start` runs in an isolated git worktree by default; `--no-worktree` opts out (previously opt-in via `--worktree`)
- Define/Plan stage prompts are now idempotent: if the stage's deliverable (CONTEXT.md/PLAN.md) already exists, the agent reports success without re-running the GSD command or requesting input — fixes headless Codex runs hanging on GSD's "already exists" decision
- `devflow start --agent codex` pre-flights: errors immediately if the phase has no CONTEXT.md on `develop` (headless Codex cannot run an interactive discussion), with a warning if PLAN.md is also missing
- Codex sandbox (`--sandbox workspace-write`) now gets explicit writable-root grants for the linked worktree's git metadata (both the common `.git` and the worktree's admin dir under `.git/worktrees/<name>`), and commit/tag signing is disabled scoped to Codex's own process tree via `GIT_CONFIG_*` env — the sandbox has no route to the operator's ssh/gpg agent

### Changed
- Corrected docs: removed the phantom `git_flow.enabled` field from examples, fixed the completion-evaluation description (Layer 2 = exit code + commit count, Layer 3 = commit heuristic), and replaced the "3 changes" agent claim with the real checklist
- Removed local-only setup assumptions: untracked `distrobox.ini`, narrowed the `.planning/` gitignore to keep only the prompt-required convention files tracked, documented the GPG-off test setup
- Layer 2's commit-count gate is now scoped to Code-like stages only — Define and Validate legitimately produce zero commits and are no longer mis-flagged as failures
- `devflow`'s lock file (`.devflow/lock`) now reclaims itself when the recorded holder process is dead, instead of wedging every later `devflow advance` for the project

### Removed
- OMX (oh-my-codex) agent support — fully removed (adapter, enum/parser/display, module exports, Hermes skill references, and the stale `.omx/` runtime directory). It had been disabled since 1.0.0; `omx`/`oh-my-codex` are no longer accepted agent names.
- Dead v1 `ship.rs` bookkeeping: the `LastShip` record and the PR-body/goal-extraction/test-summary machinery left over from the removed `devflow confirm`/`devflow rejectpr` commands (zero non-test call sites; PR creation and merging happen entirely inside the external `/gsd-ship` slash command, not in DevFlow's Rust code)

### Fixed
- `devflow ship` now cuts the release branch from the current `HEAD` instead of `develop`, so commits unique to the branch being shipped are no longer dropped from the release
- `git tag` (DevFlow's automatic version-bump tags) no longer blocks on `$EDITOR` when the operator's global `tag.gpgsign` is set to `true` — these are internal SemVer bookkeeping tags, not signed release artifacts, so signing is scoped off per-invocation rather than depending on global config
- Codex's self-reported `DEVFLOW_RESULT` (delivered inside an `agent_message` JSONL item, not as a raw stdout line) is now read correctly — previously a self-reported failure with exit code 0 could be misclassified as success
- The rate-limit detection heuristic no longer scans JSONL event lines as plain text, which could false-match ordinary document content echoed into an agent's output and stuff a multi-KB line into a gate's notification; failure reasons surfaced in gate contexts are now capped at 300 characters

### Fixed
- `devflow ship` now cuts the release branch from the current `HEAD` instead of `develop`, so commits unique to the branch being shipped are no longer dropped from the release

## [1.0.1] — 2026-06-18

### Added
- `devflow doctor` command — environment audit with version detection, JSON output mode
- `scripts/install.sh` — single-command bootstrap for Linux/macOS
- `DEPENDENCIES.md` — full dependency matrix with install instructions
- Standard OSS files: LICENSE (MIT OR Apache-2.0), CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md

### Changed
- README completely rewritten for v1.0.0 — accurate command listing, state machine diagram, quick start
- Removed tmux references from docs
- `.github/ISSUE_TEMPLATE/` and `.github/PULL_REQUEST_TEMPLATE.md` added

## [1.0.0] — 2026-06-18

### Added
- Worktree isolation: `--worktree` flag on `start`, dedicated `parallel` and `sequentagent` commands
- Multi-agent support: `parallel` (concurrent phases) and `sequentagent` (sequential handoff)
- Reference worktrees: `reference` command for static snapshots
- PR integration: `ship` creates GitHub PR, `confirm`/`rejectpr` manage lifecycle
- Rate-limit detection: auto-detect agent 429s, write cron instructions for retry
- Monitor daemon: background agent completion detection with auto-advance
- Agent trait system: pluggable agent adapters (Claude, Codex, OpenCode)
- `cleanup` command: remove worktrees and feature branches
- Shared prompt generation: `phase_prompt()` in agent module
- `recover` command: inspect and clean stale workflow state

### Changed
- Removed tmux dependency — agents run directly via CLI
- Deprecated omx/oh-my-codex agent support (disabled; fully removed in a later release)
- CLI reorganized: new command groups for multi-agent and shipping workflows

### Fixed
- Monitor capture thread lifecycle tied to agent process
- Shell-safe quoting in state machine commands
- JSON envelope parsing for agent results

## [0.5.1] — 2026-06-17

### Added
- Ship readiness: version bump, release branch, PR creation via `gh`
- `config` command shows effective configuration
- `init` command bootstraps `.devflow.yaml`

### Changed
- State machine expanded with SHIPPING and CLEANING steps
- Config schema updated with `git_flow` section

## [0.1.0] — 2026-06-16

### Added
- Initial release
- State machine: IDLE → BRANCHING → EXECUTING → VERIFYING → DOCSING
- Git flow branch management
- Version bumper (Cargo.toml, pyproject.toml, package.json)
- `.devflow.yaml` configuration
- Basic CLI: `start`, `check`, `status`, `ship`
