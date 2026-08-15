# Phase 35: Loop-Termination and Baseline Correctness - Research

**Researched:** 2026-08-06
**Domain:** Rust workflow-orchestration internals (DevFlow's own `devflow-core`/`devflow-cli` crates) — no external library research; this is a source-grounded defect-repair phase.
**Confidence:** HIGH — every claim below with a `file:line` citation was opened and read this session (not grepped-and-inferred). Two citations in the ROADMAP/CONTEXT source material were found stale and are corrected inline (noted where they occur).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**D-01 (999.86):** The probe runs non-interactively via `SSH_ASKPASS_REQUIRE=never` AND a wall-clock
timeout — both, not either. Reversibility: reversible.

**D-02 (999.86):** The probe's exit code is the sole verdict, and `NotViable` reasons are a fixed
set keyed by failure class — no second spawn, and `ssh-keygen`'s stderr is never re-emitted.
Three classes distinguishable from the probe alone: probe timed out; probe exited non-zero;
`ssh-keygen` absent (→ `Unknown`, fail-soft). Accepted cost: strictly less actionable than today on
the failure path (the operator loses the "agent reachable but this key not loaded" vs "no agent at
all" distinction). D-08's redaction contract still binds — the configured `user.signingkey` value
must never appear in any reason string, in any form.

**D-03 (999.86):** An inline `key::` (or deprecated raw `ssh-`) signing key returns
`SigningViability::Unknown` and is not probed. Fail-soft per phase 20d's D-06. What this does not
cover: `key::`/raw `ssh-` users get no verdict at all. `inline_signing_key_blob` is still required
(classification only); only the *probing* of inline values is dropped.

**D-04 (999.86):** `classify_ssh_add_status` and `SigningStatus` are deleted, and the release treats
it as the public-API break it is. Orphan created by this phase's own change: `inline_key_fingerprint`
(private) loses its only production caller under D-03 — remove it and its tests with the same
change. `Viable { fingerprint }` keeps reporting the fingerprint (orchestrator's call, not open) —
sourced from `public_key_fingerprint`, which is already written, tested, D-08-compliant.
Reversibility: one-way — removes two `pub` items from a crate published to crates.io.

**D-05 (999.84):** The worktree fixture is a plain `create_dir_all` directory, and `project_root`
gets a decoy PLAN — same phase number, no `blocking-human` gate — so the revert at `:1070` fails
because the wrong root was read, not because the main checkout happened to be empty. Rejected: the
bare version with `project_root` left empty (same cost, weaker control); a real linked `git
worktree` fixture (disproportionate — the argument under test resolves a path, and a linked
worktree's files are ordinary files). Correction established during discussion: real
`git worktree add` fixtures already exist at `staleness.rs`, `preflight.rs:1198`, `worktree.rs` —
999.76's open question about needing the workspace's first is false.

**D-06 (999.84):** A mechanical opposite-result assertion ships inside the same test, alongside the
performed revert. The performed revert (actually reverting `:1070` to `project_root`, watching the
new test fail, restoring) is binding and must happen — it is a one-time act nothing re-runs. The
test ALSO asserts directly that `phase_has_blocking_human_checkpoint(project_root, phase)` is
`false` (the re-running control). Neither replaces the other: the mechanical half proves the two
roots disagree; only the performed revert proves `:1070` passes `execution_root`. Rejected:
prose-only recording in SUMMARY.md and a doc comment (silently stops discriminating under a future
refactor); a committed `35-evidence/` capture (heavier than warranted).

**D-07 (999.78, operator carve-out):** Exhausting the never-reset per-phase Validate-failure total
fires a human gate; the run stays alive — same shape as `MAX_CONSECUTIVE_FAILURES` today. Rejected:
aborting the phase outright (destructive/irreversible); gate in Supervise, abort in Auto
(contradicts Auto's existing ceiling). Accepted cost: an unattended overnight run now parks on a
gate instead of looping to completion.

**D-08 (999.77, operator carve-out — breaking change to `devflow-core`):** `phase_commit_count`'s
return type changes to `Option<u32>`; Phase 35 ships a breaking `devflow-core` change. Rejected: the
backlog's sibling function (leaves the lossy call site compiling unchanged — and there is now a
NAMED instance of that harm, the `evaluate_layer2` finding, D-09 below); a `#[deprecated]`
delegating wrapper (declined because the break is already bought by D-04). **Version: the release
stays `v2.5.0`.** Strict semver would say `3.0.0`; declined because `devflow-core` has no external
consumers. Do NOT rename the milestone. The break is documented in `CHANGELOG.md` (every
changed/removed `pub` item) and crate docs (a deprecation note) instead of versioned — this is a
phase deliverable, not release-time paperwork. Reversibility: one-way, pooled with D-04 in a single
cut.

**D-09 (999.87, folded in, operator carve-out):** 999.87 is folded into this phase, and
`evaluate_layer2` returns `Ok(None)` — falling through to Layer 3 — when the commit count cannot be
measured. This is NOT "a defect a fix reveals, filed not fixed" (34/D-04) — D-08 makes
`let commits: u32 = phase_commit_count(..)` a type error, so the phase must edit that exact line
regardless. Matches the idiom already three lines up in the same function (`Err(_) => return
Ok(None), // fall to Layer 3`). Rejected: returning `Unknown` to gate (introduces a new stall mode
on a git blip, contradicting the milestone's point); classifying on exit code alone (fail-open,
contradicts `31/D-18` — "pass is a landed artifact, never a reported status"). Accepted cost: on a
`git` blip the decision moves to Layer 3, which may itself be degraded, and the run continues rather
than gating. Reversibility: reversible. **Scope consequence: ROADMAP Phase 35 gains criterion 6;
REQUIREMENTS.md gains HARDEN-07; the phase is six items, not five.**

### Claude's Discretion

- **999.77 — change `phase_commit_count`'s return type; do not add a sibling.** Continues 34/D-06's
  structural-over-hand-audited line. `evaluate_layer2` maps `None` to its existing zero-treatment
  explicitly at the call site, with a comment.
- **999.77 — the two doc comments are part of the deliverable, not cleanup.**
  `phase_commit_count`'s "Every consumer treats all three the same way" line and
  `pipeline_outcomes.rs`'s over-promising guarantee comment must both be corrected.
- **999.77 — the regression test is the two-cycle sequence, and nothing less.** A single-cycle test
  passes against both the buggy and the fixed code.
- **999.78 — the new counter's shape.** A new `State` field with `#[serde(default)]`, following
  `last_validate_failure_commit_count`'s backward-compat pattern, NOT touched by `transition()`. The
  ceiling is a named constant meaningfully above `MAX_CONSECUTIVE_FAILURES = 3`; ~10 is the
  orchestrator's suggestion — the planner may argue the number, not the shape.
- **999.78 — the gate message leads with the cumulative total,** named as a per-phase total; the
  streak may appear as a secondary clause only if it cannot be mistaken for the headline.
- **999.78 — IN-02.** A distinct `loop_back` reason string for the absent-baseline case.
- **999.79 — the freshness signal. DEPARTS FROM THE BACKLOG ENTRY; overrule freely.** Preferred
  shape: record a content fingerprint of `{N}-VERIFICATION.md` in `State`, treat the artifact as
  fresh only once it has changed within this run (not the backlog's plan-count comparison). Both
  directions must be tested. Prohibition, carried forward unrelaxed: do NOT revert the probe to
  `project_root` — that reintroduces CR-01.
- **999.86 — probe mechanics left to the planner.** The `-n` namespace must be verified against a
  real git-produced signature (done this session — Section F); the timeout duration constant; where
  the throwaway payload lives. The GPG/openpgp branch is untouched and not the planner's to widen.
- **Plan decomposition and sequencing.** The five (now six) items have no structural dependency on
  each other except where noted in Section G. `workflow.granularity` is `medium`.

### Deferred Ideas (OUT OF SCOPE)

- **999.76's open question** — whether the workspace needs a real linked `git worktree` integration
  harness. D-05 declines it for this phase's purposes; stays open, but the "workspace has none"
  framing is false (real fixtures exist in three places).
- **Richer `NotViable` diagnostics for `release --check`** — D-02 accepts a real loss of
  actionability; the recorded way back (if it bites in practice) is retaining `ssh-add -l` for prose
  only, never for a verdict.
- **Probing inline `key::` signing keys** — D-03 declines it; measured working first, so reopening
  needs only a temp file and a cleanup path.
- **999.85** — two comments justifying themselves by a mechanism Phase 34 deleted. Out of scope in
  REQUIREMENTS.md; editing `idle_timeout_result`'s doc comment (same file this phase edits for
  999.77) is still out of scope — leave it.
- **DEN-50 — `devflow release`'s real signing executor.** Unaffected by 999.86, still separate; must
  still run the real signed `git tag`, not call this probe as a substitute.
- **Any defect these fixes reveal** — filed as a numbered `999.x` entry plus a Linear issue, not
  fixed in-phase (34/D-04) — EXCEPT 999.87, which was explicitly promoted into scope by D-09.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| HARDEN-01 | Operator can trust `consecutive_failures` reflects real repeated failure, not a single transient `git` hiccup (999.77) | Section A (harness), Section B (`phase_commit_count`/`handle_validate_outcome` exact sites, A-04/A-06 split), Validation Architecture row 1 |
| HARDEN-02 | Operator can trust an unattended Code↔Validate loop has a bound independent of trivial per-cycle commits, and the gate message reports a real cumulative total (999.78) | Section C (`State` fields, `transition()`, `State::new`, the `--force` open question), Validation Architecture row 2 |
| HARDEN-03 | Operator can `--force` re-run a phase without inheriting a stale `VERIFICATION.md` (999.79) | Section D (`phase_verification_exists`, `select_loop_back_fix`, the pub-API-or-not fork), Validation Architecture row 3 |
| HARDEN-04 | Operator can trust the worktree-mode `GateReview` checkpoint auto-decide path is regression-tested (999.84) | Section E (exact call site, exact base test, exact delta), Validation Architecture row 4 |
| HARDEN-05 | Operator can trust `release --check`'s signing preflight reflects a real probe (999.86) | Section F (every symbol, every consumer, the verified `-n git` namespace, the timeout template), Validation Architecture row 5 |
| HARDEN-07 | Operator can trust a transient `git` failure does not make a successful agent read as failed, at both consumers (999.87) | Section B (`evaluate_layer2` exact site), Section A (shared harness), Validation Architecture row 6 |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

Extracted from `./CLAUDE.md` (project root), binding on this phase's plan and execution:

- **Never run git operations while an executor holds the working tree.** If any plan in this phase
  runs its executor on the main checkout rather than a worktree, the orchestrator must not touch git
  (`add`/`commit`/`push`/branch/tag) until the executor reports.
- **`cargo test --exact <name>` exits 0 when the name matches nothing.** Every test command this
  phase's plans or verification steps run must assert on a real `N passed` line with a non-zero
  `filtered out` count where relevant — never trust exit code alone. The package under test is
  `devflow` (confirmed `crates/devflow-cli/Cargo.toml:2`), not `devflow-cli`.
- **A pipeline's exit code is the last command's.** Any verification step piping test output through
  another command (`| tail`, `| grep`, etc.) must capture the exit code of the command that matters,
  not the pipeline's.
- **`git commit` runs against whatever branch is currently checked out.** Before any commit not
  immediately preceded by a `git checkout` in the same tool call, run `git rev-parse --abbrev-ref
  HEAD` and confirm it matches the intended target.
- **Keep `DEV-SETUP-CHECKLIST.md` in sync** if this phase's commits touch git policy, hooks, CI,
  devcontainer/toolchain pins, or GSD config (unlikely for this phase's scope, but noted since the
  Cargo.toml version bump and CHANGELOG discipline border that territory).
- **Prefer GSD commands over doing it by hand** for the phase lifecycle — this research and its
  consuming plan should be produced via `/gsd-plan-phase`, not hand-authored `.planning/` edits.

## Summary

This phase has no open design space — CONTEXT.md's D-01 through D-09 are locked, and the six
success criteria in ROADMAP.md are the acceptance contract. This research does not re-derive those
decisions; it re-verifies every `file:line` citation CONTEXT.md relies on against current source,
and fills in the one genuine implementation gap CONTEXT.md left explicitly open: **the mechanics of
the forced-`git`-failure test harness**, which every one of criteria 1 and 6's tests depends on and
which no test in the workspace currently builds.

**The single most important finding of this research, refining CONTEXT.md's A-13:** the harness
must make `Command::new("git").output()` return `Err` (a spawn failure), not merely a non-zero exit
status. CONTEXT.md calls this "a failing-`git` shim placed first on PATH." A shim that *runs and
exits non-zero* is the WRONG shape — under A-06's own decision (`.output()` returning `Ok` with a
non-zero status is a *real observation*, classified `Some(0)`, not `None`), a shim of that shape
would test the already-correct "branch absent" path, not the "git could not be run" path criterion
1 and criterion 6 actually need. The correct, portable mechanism is a `PATH` that has **no `git`
binary resolvable on it at all** — an empty stand-in directory, not a broken script — which reliably
makes `execvp("git", …)` fail with `ENOENT` and Rust's `Command::output()` surface that as
`Err(io::Error)`. Section A below gives the exact construction, mirroring `NeutralPath`'s existing
RAII shape.

**Second finding, independently verified rather than assumed (D-08's finding 4):** the `ssh-keygen
-Y sign -n <namespace>` namespace git itself uses is **`git`** (3 ASCII bytes) — extracted directly
from this repository's own real SSH-signed tag `v2.4.0` and decoded per the SSHSIG wire format. Not
inferred from documentation; read straight out of the signature blob this session (Section F).

**Third finding, a genuine open design question CONTEXT.md flags but does not resolve (A-11):**
`State::new` (`commands.rs:124`) unconditionally zeroes every counter on **every** `devflow start`
invocation, `--force` included. The 999.78 "never-reset per-phase" counter, if stored as an ordinary
`State` field, resets on a `--force` restart — surviving only the ordinary "loop back through the
same run's persisted state" path, not a full re-`start`. CONTEXT.md says this must be "stated
explicitly," not decided by this researcher; Section C lays out the exact mechanism and the two
live options without picking one.

**Primary recommendation:** build the forced-`git`-failure harness first (it gates criteria 1 and
6, and per D-09 the marginal cost of criterion 6 is small once it exists) as its own reusable
`test_support` primitive; the other four criteria's fixes (999.78's counter, 999.79's freshness
signal, 999.84's regression test, 999.86's probe) have no cross-dependency on it or on each other and
can be planned as independent waves, consistent with CONTEXT.md's own sequencing note.

## Architectural Responsibility Map

This is a single-binary Rust CLI tool with no client/server split; the conventional browser/SSR/API
tiers do not apply. The relevant "tiers" here are the crate boundary and the pipeline layer:

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Commit-count measurement (`phase_commit_count`) | `devflow-core` (pure git-shell-out) | — | Published library crate; the primitive both consumers below call |
| Forward-progress baseline / `consecutive_failures` bookkeeping | `devflow-cli` (`pipeline_outcomes.rs`) | `devflow-core` (`mode.rs` predicate) | State mutation lives in the CLI's stage-transition logic; the pure decision function lives in core |
| Layer-2 result classification (`evaluate_layer2`) | `devflow-core` (`agent_result.rs`) | — | Pure classifier, no I/O beyond reading the exit file and calling `phase_commit_count` |
| Never-reset failure ceiling + gate message | `devflow-cli` (`pipeline_outcomes.rs`, `state.rs`) | `devflow-core` (`mode.rs` for the ceiling constant/predicate) | State field lives in core's `State` struct; the gate-firing decision and message text live in the CLI |
| `{N}-VERIFICATION.md` staleness signal | `devflow-cli` (`select_loop_back_fix`, private) | `devflow-core` (`phase_verification_exists`, pub) | The existence probe is a core primitive; the freshness comparison is CLI-side state bookkeeping — whether it needs to touch core's pub surface is an open question, see Section D |
| Worktree-mode checkpoint call site | `devflow-cli` (`pipeline_launch.rs`) | `devflow-core` (`verify.rs`) | The root-selection bug lives in the CLI's dispatch arm; the function being called is already correct in core |
| Signing preflight probe | `devflow-core` (`git.rs`) | `devflow-cli` (`commands.rs`, sole consumer) | Probe logic and process-spawn discipline live in core; the CLI only maps the enum to `release --check` output |

## Standard Stack

No new dependencies. This phase edits `devflow-core` and `devflow-cli` (workspace crates already in
`Cargo.toml`) and adds tests using only what is already a dev-dependency (`tempfile`, `serde_json`)
and the standard library (`std::process::Command`, `std::env`).

**Version verification (this session):**
```
$ cargo --version   → cargo 1.97.1
$ rustc --version    → rustc 1.97.1
$ git --version      → git version 2.55.0
$ ssh -V             → OpenSSH_10.4p1, OpenSSL 3.6.3
```
`REPO_LOCAL_GIT_VARS`'s doc comment (`git.rs:21`) already states "15 entries on git 2.55" — this
host's installed git matches the version the constant was audited against.

### Core
No table needed — no new library dependency is introduced by this phase.

### Package Legitimacy Audit
**Not applicable.** This phase installs no external packages. Skip the gate.

## Architecture Patterns

### System Architecture Diagram

```
devflow start --phase N
        │
        ▼
  commands::start()                     [commands.rs:112]
   ├─ State::new(...)  ──────────────── ALWAYS zeroes every counter,
   │    (commands.rs:124)               --force included (A-11 finding)
   ├─ ensure_phase_worktree()  ──────── sets state.worktree_path
   │    (commands.rs:239→244)           (worktree mode only)
   └─ launch_stage() ─────────────────► spawns agent, detached monitor
                                              │
                    ┌─────────────────────────┘
                    ▼
         [monitor watches agent exit, then runs `devflow advance`]
                    │
                    ▼
         pipeline_launch::advance()             [pipeline_launch.rs:936, pub(crate)]
           ├─ evaluate_agent_result → Layer 0/1/2/3 cascade
           │      Layer 2: evaluate_layer2()     [agent_result.rs:1892]
           │        reads exit file, calls
           │        phase_commit_count()         [agent_result.rs:1841]
           │        (git rev-parse + rev-list, shells to `git` via PATH)
           │        ── currently returns u32, collapsing 3 causes into one 0
           │
           ├─ Action::Evaluated (Stage::Validate)
           │      → handle_validate_outcome()    [pipeline_outcomes.rs:353]
           │         ├─ reads evidence_root = worktree_path.unwrap_or(project_root)
           │         ├─ select_loop_back_fix(evidence_root, phase)
           │         │    → phase_verification_exists()  [agent_result.rs:2654, pub]
           │         │    → FixType::GapsOnly | FullExecute
           │         ├─ on Failed: phase_commit_count(project_root, ...) [again,
           │         │    DIFFERENT root than evidence_root — CR-01 note]
           │         │    → mode::consecutive_failures_made_progress()  [mode.rs:149]
           │         │    → state.consecutive_failures = 1 | +1 (saturating)
           │         │    → state.last_validate_failure_commit_count = Some(current)
           │         │        (UNCONDITIONAL write — the 999.77 defect,
           │         │         pipeline_outcomes.rs:419-422)
           │         ├─ state.mode.should_gate(Validate, consecutive_failures)
           │         │    [mode.rs:170] — Auto: >= MAX_CONSECUTIVE_FAILURES (3);
           │         │                    Supervise: always
           │         └─ GateAction::LoopBack → loop_back_to_code()
           │              [pipeline_gate.rs:115] → emits "loop_back" event
           │              BEFORE re-spawning the agent via launch_stage
           │
           └─ Action::GateReview (any stage that failed AND a checkpoint may apply)
                  [pipeline_launch.rs:1042]
                  execution_root = worktree_path.unwrap_or(project_root)
                  checkpoint_confirmed =
                     agent==Claude
                     && verify::phase_has_blocking_human_checkpoint(execution_root, phase)
                          [pipeline_launch.rs:1070 ← THE 999.84 CALL SITE]
                     && agent_result::checkpoint_reported_in_capture(project_root, phase)
                  → true: relaunch_checkpoint_session() (resume, no new gate)
                  → false: fall through to per-stage never-silent gate
```

### Recommended Project Structure
No new files. Test additions land inside the existing `#[cfg(test)] mod tests` blocks of the files
they exercise (`agent_result.rs`, `pipeline_outcomes.rs`, `pipeline_launch.rs`, `git.rs`), plus one
new shared harness primitive in `crates/devflow-cli/src/test_support.rs`. `advance()` is
`pub(crate)` (verified: `pipeline_launch.rs:936`), so the 999.84 test cannot live under
`crates/devflow-cli/tests/`.

### Pattern 1: PATH-replacement RAII guards under `ENV_MUTEX` (the harness family)

**What:** every mutation of process-global `PATH` in this crate's tests is wrapped in an RAII guard
that restores the previous value in `Drop` (so a mid-test panic cannot leave `PATH` corrupted for
parallel sibling tests), and every guard's construction/use is serialized under
`crate::test_support::ENV_MUTEX` via `env_lock()`.

**When to use:** any test that needs `Command::new("git")` (or an agent binary) to resolve to
something other than the host's real installation.

**Existing family, read this session (`crates/devflow-cli/src/test_support.rs`):**

```rust
// test_support.rs:50 — the one mutex every PATH mutation in this crate shares.
pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::new(());

// test_support.rs:94-98 — the only sanctioned way to acquire it (poison-tolerant,
// because every mutation under it is restored by an RAII guard's Drop before the
// next test observes the poisoned lock).
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// test_support.rs:286-298 — builds a PATH containing ONLY a real `git` symlink,
// no agent CLIs at all (used by NeutralPath below).
pub(crate) fn agent_free_git_only_path_dir() -> tempfile::TempDir { /* ... */ }

// test_support.rs:327-359 — RAII guard: REPLACES PATH (not prepends) for its scope,
// restores the captured original PATH (or removes it, if unset) in Drop.
pub(crate) struct NeutralPath { _dir: tempfile::TempDir, original: Option<std::ffi::OsString> }
impl NeutralPath {
    pub(crate) fn install() -> Self { /* set_var("PATH", dir) */ }
}
impl Drop for NeutralPath {
    fn drop(&mut self) { /* restores captured original PATH */ }
}
```

`NeutralPath` is used at exactly three sites, all under `env_lock()`, all inside a scoped block so
`Drop` fires before the test's own assertions run (`pipeline_outcomes.rs:1999`, `:2055`, `:2119`).

**The new primitive this phase needs (does not exist yet — build it, following this exact shape):**
a `PATH` with **no `git` binary at all**, so `Command::new("git")` fails to spawn. This is
*structurally simpler* than `NeutralPath` (no symlink needed — an empty `tempdir()` alone suffices),
and it is the correct mechanism per the Summary's finding above:

```rust
/// RAII guard that REPLACES `PATH` with an empty directory — no `git`, no
/// anything — for the scope it is bound in, restoring the previous `PATH`
/// on Drop. Unlike `NeutralPath` (real git, no agents), this makes EVERY
/// `Command::new("git")` spawn fail with `io::ErrorKind::NotFound`, which is
/// what `.output()` surfaces as `Err` — the "git could not be run" case
/// `phase_commit_count`/`evaluate_layer2` must map to `None`, distinct from
/// a real non-zero exit status (which is a successful spawn and therefore
/// NOT this case — see A-06).
///
/// **The caller must already hold `ENV_MUTEX`** (same precondition as
/// `NeutralPath`).
pub(crate) struct NoGitPath {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

impl NoGitPath {
    pub(crate) fn install() -> Self {
        let dir = tempfile::tempdir().unwrap(); // deliberately empty — no `git`
        let original = std::env::var_os("PATH");
        unsafe { std::env::set_var("PATH", dir.path()) };
        Self { _dir: dir, original }
    }
}

impl Drop for NoGitPath {
    fn drop(&mut self) {
        unsafe {
            match &self.original {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
    }
}
```

Place this beside `NeutralPath` in `crates/devflow-cli/src/test_support.rs` (same file, same
`ENV_MUTEX` precondition, same Drop-restores-unconditionally shape). It needs no `agent_free_*`
helper since it deliberately resolves nothing.

**Negative control for the harness itself (mandatory — a green two-cycle test could otherwise mean
"the guard was a no-op"):** before writing the real two/three-cycle sequence test, write (or run as
a throwaway probe, then discard) a one-line assertion that `devflow_core::test_support::git_command
(tmp).arg("--version").output()` returns `Err(_)` while a `NoGitPath` guard is installed, and `Ok(_)`
immediately after it drops. If that probe does not discriminate, nothing built on top of it does
either.

**Contention surface (999.19's finding, reconfirmed this session):** `ENV_MUTEX`'s own doc comment
(`test_support.rs:35-42`) states `PATH` is mutated **36 times across 12 lock regions** in this
crate's test suite already. A `NoGitPath`-guarded region adds to that count but does not change the
serialization discipline — every existing region already assumes exclusive `PATH` ownership for its
scope.

### Pattern 2: bounded spawn + deadline + `try_wait`/kill (the D-01 timeout template)

**What:** the codebase's existing pattern for giving a blocking child process a wall-clock ceiling
without a timeout crate (`devflow-core` has none, and `Command::output()` cannot time out on its
own).

**When to use:** D-01's `ssh-keygen -Y sign` probe, which the operator's own measurement showed can
block indefinitely on an encrypted key with a working askpass (Section F).

**Existing precedent, read this session (`crates/devflow-core/src/canary.rs:415-433`):**
```rust
/// Wait a bounded time for the canary child to exit, then kill it.
fn reap(child: &mut std::process::Child) {
    let deadline = Instant::now() + Duration::from_secs(CANARY_REAP_GRACE_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(err) => { warn!("could not poll the canary child: {err}"); return; }
        }
        if Instant::now() >= deadline { break; }
        std::thread::sleep(REAP_POLL);
    }
    let _ = child.kill();
    let _ = child.wait();
}
```
A related deadline-poll idiom (no spawn, just a liveness poll against an existing pid) is
`agent::terminate_and_verify` (`agent.rs:118-159`, deadline loop starting `agent.rs:135`).

**What this template does NOT give you for free:** `canary.rs`'s pattern spawns the child on one
thread and reads its stdout on separate reader/writer threads feeding an `mpsc` channel (the
`Duration`-bounded `recv_timeout` loop at `canary.rs:366-391`) — that's more machinery than a
throwaway signing probe needs, since the probe does not need to stream output, only to know
"did it finish, and with what exit code." The `reap()` function's `spawn → loop{try_wait, sleep} →
kill → wait` shape is the part to copy; the channel/thread plumbing around it is not required.

### Anti-Patterns to Avoid
- **A shim that "fails" `git` by exiting non-zero.** Per A-06's decision, this is classified
  `Some(0)` (a real observation — branch genuinely absent), not `None` (could not measure). This
  does not exercise the code path the two-cycle test needs to discriminate.
- **Re-deriving the `-n` namespace from documentation or memory.** D-08's finding 4 explicitly
  requires verification against a real git-produced signature — see Section F, already done this
  session.
- **Retargeting `phase_commit_count`'s root to the worktree.** `pipeline_outcomes.rs:342-352`'s
  CR-01 comment explains this is deliberate: git refs/objects are shared across worktrees, so the
  main-checkout root already sees worktree-made commits; retargeting "would fix nothing and would
  break the 999.66 wiring."

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Making `git` unavailable to a test | A `git` shell script that `exit 1`s | `NoGitPath` (Section A/Pattern 1) — empty PATH dir | Only an empty-PATH spawn failure produces `Err`, not `Ok(nonzero)` |
| Bounding a blocking child process | A timeout crate / new dependency | `canary.rs:415-433`'s `spawn → try_wait loop → kill` shape | Already audited, already in-crate, A-07 confirmed no new dependency is needed |
| Detecting whether a worktree fixture "worked" | A real `git worktree add` | `create_dir_all` + a decoy PLAN under `project_root` (D-05) | The argument under test resolves a path string, not git worktree semantics; D-05 already rejected the heavier fixture explicitly |

**Key insight:** every "don't hand-roll" item in this phase is really "don't reinvent a pattern this
same codebase already has one line up" — there is no external-library gap here.

## Common Pitfalls

### Pitfall 1: A shim that exits non-zero looks like it tests "git could not run" but doesn't
**What goes wrong:** the two/three-cycle regression test passes, but it was actually exercising the
already-correct "branch does not exist" (`Some(0)`) path, not the "git could not be run" (`None`)
path the defect is about.
**Why it happens:** `phase_commit_count`'s doc comment (`agent_result.rs:1838-1840`, quoted in full
in Section B) collapses all three causes into one `0` today; it's easy to reach for "a git that
fails" without checking which of `Err`/`Ok(nonzero)` that produces.
**How to avoid:** use `NoGitPath` (empty PATH dir), never a script. See the harness negative control
above.
**Warning signs:** if the shim script has a `#!/bin/sh` line and an `exit N`, it's the wrong shape.

### Pitfall 2: Extending the wrong pre-existing checkpoint test
**What goes wrong:** building 999.84's test on `code_unknown_does_not_transition_to_validate`
(`pipeline_launch.rs:1453`) or `relaunch_checkpoint_session_emits_exactly_one_audit_event`
(`pipeline_launch.rs:1626`) instead of `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records`
(`pipeline_launch.rs:2302`) — CONTEXT.md's own A-02 amendment already corrected this from the
original document, and it is worth restating because the ROADMAP entry (999.84) still names the
first two as "harness pieces" in a way that could be misread as "the base."
**Why it happens:** all three tests are checkpoint/relaunch-adjacent; only one of them (`:2302`)
actually drives a real synchronous `advance()` through `Action::GateReview` with all five
preconditions satisfied.
**How to avoid:** extend `:2302` directly (set `state.worktree_path = Some(worktree)`, move the
`blocking-human` PLAN to exist only under the worktree, add D-05's decoy PLAN under `project_root`).
**Warning signs:** if the new test spawns a scoped thread and polls a gate file, it copied the wrong
base (`:1453`'s shape) — `:2302`'s base is a plain synchronous call.

### Pitfall 3: Treating a non-zero `ssh-keygen -Y sign` exit as agent-membership evidence
**What goes wrong:** a test (or a future diagnostic message) assumes `Viable` correlates with
"the agent holds this key" — which is exactly the false premise that produced the two live false
negatives 999.86 exists to fix.
**Why it happens:** intuitive but wrong; `ssh-keygen -Y sign -f <pub>` resolves the private key by
stripping `.pub` from *that path* OR via the agent — an on-disk private-key sibling with no agent
involvement at all still signs successfully (measured live, Section F table row 1).
**How to avoid:** any fixture/test asserting `Viable` must arrange the on-disk-private-key-sibling
case explicitly and must NOT gate the assertion on agent state.
**Warning signs:** a test that calls `ssh-add -l` (or asserts on agent contents) anywhere near a
`Viable` assertion.

### Pitfall 4: Assuming `State` survives a `--force` restart
**What goes wrong:** implementing 999.78's ceiling as an ordinary `State` field and believing it
bounds "the phase," when it actually only bounds "this `devflow start` invocation" — a `--force`
re-run resets it to 0 (A-11's finding, reconfirmed this session at `commands.rs:124`).
**Why it happens:** every other counter in `State` (`preflight_retries`, `checkpoint_resumes`,
`last_validate_failure_commit_count`) already has this exact property and nobody has needed it to
survive `--force` before — 999.78 is the first counter whose whole point is bounding something a
restart could otherwise dodge.
**How to avoid:** see Section C — this is presented as an open question for the planner to resolve
explicitly, not silently inherited.
**Warning signs:** a plan that describes the counter as "per-phase" without saying what happens to
it across `--force`.

## Code Examples

### The exact PATH-replacement pattern to copy for `NoGitPath`
```rust
// Source: crates/devflow-cli/src/test_support.rs:327-359 (NeutralPath, read this
// session) — NoGitPath (Pattern 1 above) is a structural sibling with an empty
// directory instead of a git-only symlinked one.
pub(crate) struct NeutralPath {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}
impl NeutralPath {
    pub(crate) fn install() -> Self {
        let dir = agent_free_git_only_path_dir();
        let original = std::env::var_os("PATH");
        unsafe { std::env::set_var("PATH", dir.path()) };
        Self { _dir: dir, original }
    }
}
impl Drop for NeutralPath {
    fn drop(&mut self) {
        unsafe {
            match &self.original {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
    }
}
```

### The exact base test to extend for criterion 4 (999.84)
```rust
// Source: crates/devflow-cli/src/pipeline_launch.rs:2302-2357, read this session.
// The positive case: declared + reported + Claude + session id + under the
// ceiling -> resumes and records exactly one audit event, with no
// `gate_fired` for this stage.
#[test]
fn advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let phase = 88;
    write_declared_checkpoint_plan(root, phase);       // .planning/phases/{phase:02}-checkpoint-fixture/{phase:02}-01-PLAN.md
    write_confirmed_checkpoint_capture(root, phase);
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    state.session_id = Some("sess-checkpoint-1".to_string());
    workflow::save_state(&state).unwrap();
    let stub_dir = stub_agent_binary("claude");
    let original_path = std::env::var_os("PATH");
    let stubbed_path = prepend_path(&stub_dir, &original_path);
    unsafe { std::env::set_var("PATH", &stubbed_path); }
    let result = advance(root, Some(phase));
    // ... restore PATH, reap monitor, then assert exactly one checkpoint_auto_decided event
}
```
**999.84's delta on this base:** add `state.worktree_path = Some(worktree)`; call
`write_declared_checkpoint_plan` against the **worktree** path instead of `root`; write D-05's decoy
PLAN (same phase number, no `gate="blocking-human"` attribute) under `root` itself, mirroring
`write_declared_checkpoint_plan`'s exact shape (`pipeline_launch.rs:2247-2256`, quoted below) but
with the checkpoint attribute stripped.

```rust
// Source: crates/devflow-cli/src/pipeline_launch.rs:2247-2256, read this session —
// the exact fixture shape the D-05 decoy PLAN must mirror, minus the checkpoint attribute.
fn write_declared_checkpoint_plan(root: &Path, phase: u32) {
    let dir = root
        .join(".planning/phases")
        .join(format!("{phase:02}-checkpoint-fixture"));
    std::fs::create_dir_all(&dir).unwrap();
    let body = format!(
        "---\nphase: {phase}\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE_FOR_TEST}\">\n</task>\n"
    );
    std::fs::write(dir.join(format!("{phase:02}-01-PLAN.md")), body).unwrap();
}
```

### The exact 999.84 call site under test
```rust
// Source: crates/devflow-cli/src/pipeline_launch.rs:1067-1071, read this session.
let mut reason = result.reason.clone();
let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
let checkpoint_confirmed = state.agent == AgentKind::Claude
    && verify::phase_has_blocking_human_checkpoint(execution_root, phase)   // <-- THE argument under test
    && agent_result::checkpoint_reported_in_capture(project_root, phase);
```
Reverting `execution_root` to `project_root` on this single line, running the new/extended test, and
watching it fail is criterion 4's mandatory demonstration (D-06).

### The real SSHSIG namespace, extracted this session from this repo's own signed tag
```
$ git cat-file tag v2.4.0 | sed -n '/BEGIN SSH SIGNATURE/,/END SSH SIGNATURE/p' \
    | sed '2,$!d;$d' | base64 -d | xxd | head -5
00000000: 5353 4853 4947 0000 0001 0000 0033 0000  SSHSIG.......3..
00000010: 000b 7373 682d 6564 3235 3531 3900 0000  ..ssh-ed25519...
00000020: 2057 400b 51e0 5ee2 a2b4 6a4f 07f0 3984   W@.Q.^...jO..9.
00000030: de4d 416c 2247 8d71 e733 187d 38dd e2a2  .MAl"G.q.3.}8...
00000040: 3400 0000 0367 6974 0000 0000 0000 0006  4....git........
```
The bytes at offset `0x40`: `00 00 00 03` (length-prefix 3) then `67 69 74` = `"git"`. This is the
`namespace` field of the SSHSIG blob (per the format's own field order: magic, version, pubkey,
**namespace**, reserved, hash-algorithm, signature). **Verified directly against a real
`git`-produced signature — `-n git` is the correct namespace, confirmed this session, not assumed.**

## State of the Art
Not applicable in the conventional sense (no external ecosystem shift). The relevant "state of the
art" is entirely this codebase's own prior phases:

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `ssh-add -l` fingerprint comparison for signing viability | Direct `ssh-keygen -Y sign` probe | This phase (999.86) | Removes the class of false negative that hit two live release cuts |
| `phase_commit_count() -> u32` (lossy) | `phase_commit_count() -> Option<u32>` | This phase (D-08) | Forces both consumers (`handle_validate_outcome`, `evaluate_layer2`) to distinguish "could not count" from "counted zero" |
| `consecutive_failures` as the only loop-termination signal | `consecutive_failures` (streak) + a new never-reset per-phase total (999.78) | This phase (D-07) | Adds a backstop bound independent of trivial-commit resets |

**Deprecated/removed by this phase:** `classify_ssh_add_status`, `SigningStatus`, and (as an orphan
of D-03) `inline_key_fingerprint` — all confirmed this session to have exactly one production caller
each, all inside `git.rs`, all removed together (Section F).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|----------------|
| A1 | The ~10x-`MAX_CONSECUTIVE_FAILURES` ceiling suggested for the 999.78 counter is a reasonable backstop value | Section C, CONTEXT.md's own discretion item | Low — CONTEXT.md explicitly says "the planner may argue the number, not the shape"; this is not this researcher's assumption, it is a stated-open numeric knob |
| A2 | `--force` restart resetting the 999.78 counter is an *accepted* behavior rather than a defect, absent an explicit planner decision otherwise | Section C | Medium — if the planner assumes survival-across-`--force` without building it, criterion 2 could be satisfied in testing but not in the exact unattended-overnight scenario D-07 was written for |
| A3 | No existing test in the workspace already exercises a failing-`git` spawn anywhere (i.e., the harness genuinely does not exist yet) | Section A | Low — confirmed by `rg -n "NeutralPath"` returning only the `NeutralPath`/`Drop`/comment hits already covered; no failing-git construction found anywhere in either crate |

**All three of the above are LOW-to-MEDIUM risk framing notes, not unverified factual claims** —
every `file:line` citation elsewhere in this document was read this session. A2 is flagged because
CONTEXT.md itself (A-11) declines to resolve it and instructs the planner to state it explicitly.

## A. The Forced-`git`-Failure Harness (criteria 1 and 6)

**`NeutralPath` and `ENV_MUTEX`, exact names/signatures/lock order** (all read this session,
`crates/devflow-cli/src/test_support.rs`):

- `pub(crate) static ENV_MUTEX: Mutex<()> = Mutex::new(())` — `:50`.
- `pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()>` — `:94-98`, poison-tolerant,
  because every PATH mutation under it is restored by an RAII `Drop` before the guard is next
  acquired (see `env_lock`'s own doc comment, `:52-93`, which documents this precisely and measured
  the poison cascade: "a single induced `assert!(false)` under the lock reported 25 failures (24 of
  them `PoisonError`) through `.lock().unwrap()`, and exactly 1 through this accessor").
- `NeutralPath::install()` / `impl Drop for NeutralPath` — `:327-359`. **Replaces** `PATH` (does not
  prepend); restores the captured original on every exit path via `Drop`, including a panic mid-scope.
- `stub_agent_binary(name: &str) -> tempfile::TempDir` — `:393-402`, an unrelated primitive (writes a
  no-op executable) used together with `prepend_path` (`:407-416`) at the 999.84 test's own PATH
  setup — this is a *prepend*, not a replace, since the surrounding real PATH (containing `sh`,
  `git`) is still needed there.

**Lock acquisition order:** every site that uses `NeutralPath` or `stub_agent_binary`+`prepend_path`
first calls `env_lock()` (binding `let _guard = env_lock();`), THEN constructs the PATH-mutating
guard, all within the guard's scope. The three `NeutralPath` call sites
(`pipeline_outcomes.rs:1999`, `:2055`, `:2119`) all follow: acquire `env_lock()` at the *enclosing
test's* top (visible earlier in each test function, not shown in the inline snippet), then a nested
`{ let _path_guard = NeutralPath::install(); ...call under test... }` block so `Drop` restores PATH
before the test's own event-log assertions run.

**Concretely, how to place a failing-`git` shim on `PATH` for criteria 1/6:** do NOT use a shim
script. Build `NoGitPath` (Section "Pattern 1" above, Code Examples section) as a new primitive
beside `NeutralPath` in the same file, same preconditions, same Drop-restore shape. It replaces
`PATH` with a freshly created **empty** `tempfile::tempdir()` — no `git`, no `sh`, nothing. Any
`Command::new("git")` — including every call inside `phase_commit_count`, `evaluate_layer2`, and any
`git_command()`/`hermetic_command()` call reachable from the code under test during that scope —
fails to spawn: `.output()` returns `Err(io::Error)` with `kind() == io::ErrorKind::NotFound`.

**It must fail for every subcommand, not selectively `rev-list`/`log`:** because it works by making
the *binary itself* unresolvable, there is no subcommand-selective failure to configure — this is a
strength, not a limitation: it exactly matches the real-world failure mode CONTEXT.md's `999.77`
entry describes ("git could not be run"), which is a whole-invocation failure, not a
per-subcommand one.

**How the guard is released:** `Drop` restores the captured pre-guard `PATH` value (or removes the
var if it was previously unset) unconditionally — same mechanism as `NeutralPath`, so a panic inside
the guarded scope still restores `PATH` for the next parallel test.

**Negative control for the harness itself** (mandatory, per the objective's `<required_sections>`
instruction and this document's own philosophy): before relying on `NoGitPath` in the real
regression test, confirm — as a throwaway probe or as the harness's own accompanying unit test —
that with the guard installed, `devflow_core::test_support::git_command(tmp_dir).arg("--version")
.output()` is `Err(_)`, and immediately after the guard drops (leaving the scope), the identical call
is `Ok(_)`. **A green two/three-cycle test without this control could mean "`NoGitPath` never
actually blocked anything"** — e.g. if a future refactor changes `phase_commit_count` to use an
absolute `/usr/bin/git` path instead of PATH-resolved `Command::new("git")`, the guard would
silently stop working and every test built on it would keep passing for the wrong reason.

**Contention surface — every existing test that mutates `PATH`:** `ENV_MUTEX`'s own doc comment
(`test_support.rs:35-42`, quoted verbatim) states: *"This mutex currently guards five variables:
`PATH`, `DEVFLOW_GATE_TIMEOUT_SECS`, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`,
`DEVFLOW_GATE_NOTIFY_CMD`, `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`... `PATH` alone is mutated 36
times across 12 lock regions spanning at least three future target clusters."* This count was NOT
re-derived this session (the comment is the source of truth CONTEXT.md's A-13 also cites) — treat it
as the contention surface the planner should expect: a new `NoGitPath`-guarded region is the 37th
mutation / 13th lock region, following the exact same discipline as the other 36.

## B. `phase_commit_count` and Its Two Consumers (criteria 1 and 6)

**Definition, verified this session (`crates/devflow-core/src/agent_result.rs:1836-1861`):**
```rust
/// checkout, which is the property every caller already relies on.
///
/// A `0` return is deliberately indistinguishable across three causes:
/// genuinely no commits, the branch does not exist, or `git` could not be
/// run. Every consumer treats all three the same way.
pub fn phase_commit_count(project_root: &Path, git_flow: &GitFlowConfig, phase: u32) -> u32 {
    let branch = format!("{}phase-{:02}", git_flow.feature_prefix, phase);
    let branch_exists = git_command(project_root)
        .args(["rev-parse", "--verify", &branch])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !branch_exists {
        return 0;
    }
    let range = format!("{}..{branch}", git_flow.develop);
    git_command(project_root)
        .args(["rev-list", "--count", &range])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0)
}
```
**The exact doc-comment text that overpromises (criterion 1 names this):** the final sentence,
"Every consumer treats all three the same way," at line **1840**. `pipeline_outcomes.rs:283-286`'s
own guarantee-claiming comment, ALSO to be corrected (999.77's proposed fix explicitly says so):
> *"The failure direction is toward gating: an unrunnable `git` or a missing branch counts zero
> every cycle, so once a baseline is recorded the counter accumulates and the gate stays reachable."*
(quoted verbatim from `pipeline_outcomes.rs:337-340`, part of the doc comment attached to
`handle_validate_outcome`, not `select_loop_back_fix` as a naive line-count might suggest).

**Both call sites, exact lines (verified this session, both correct as CONTEXT.md states):**
1. `agent_result.rs:1905` inside `evaluate_layer2`: `let commits: u32 = phase_commit_count(project_root, git_flow, phase);` — the `999.87` (D-09) site.
2. `pipeline_outcomes.rs:400-401` inside `handle_validate_outcome`:
   `let current = agent_result::phase_commit_count(project_root, &GitFlowConfig::default(), state.phase);`
   — the `999.77` (D-08) baseline-write site.

**The unconditional baseline write — A-14's correction confirmed at the CORRECTED line, not the
ROADMAP's original stale citation:**
```rust
// Source: crates/devflow-cli/src/pipeline_outcomes.rs:399-423, read this session.
if result == ValidateResult::Failed {
    let current =
        agent_result::phase_commit_count(project_root, &GitFlowConfig::default(), state.phase);
    if mode::consecutive_failures_made_progress(
        state.last_validate_failure_commit_count,
        current,
    ) {
        state.consecutive_failures = 1;
    } else {
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
    }
    // The baseline advances on every recorded failure regardless of
    // which branch ran above — updating it only on the progress branch
    // would let a stale low baseline report progress forever.
    state.last_validate_failure_commit_count = Some(current);          // <-- line 422
    workflow::save_state(state)?;
}
```
**The comment at lines 419-421 is the unconditional-write justification A-04 identifies as the
actual defect site** — CONTEXT.md's A-14 correction (ROADMAP cited `:357`, which is the tail of the
function *signature*, not this write) is reconfirmed: the write is at **line 422**, not 357.

**The `Option<u32>` plumbing change forces open (compile errors), enumerated this session:**
1. `phase_commit_count`'s own signature (`agent_result.rs:1841`): `-> u32` becomes `-> Option<u32>`.
   The two `.unwrap_or(0)`/`return 0` sites inside it (`:1851`, `:1861`) must become
   `return None` (spawn failed to run — see A-06's split below) vs. the branch-exists-but-empty
   real-`Ok` case, which stays `Some(0)`/a real number.
2. `evaluate_layer2` (`agent_result.rs:1905`): `let commits: u32 = phase_commit_count(...)` is a
   type error the moment the signature changes — D-09 requires mapping `None` to `Ok(None)`
   (fall to Layer 3), matching the idiom already three lines up in the SAME function (`agent_result.rs:1901`:
   `Err(_) => return Ok(None), // fall to Layer 3`, quoted verbatim below).
3. `handle_validate_outcome` (`pipeline_outcomes.rs:400-401`): `let current = phase_commit_count(...)`
   becomes a `match`/`if let` on `Option<u32>`; A-04's decision — "when the measurement returns
   `None`, treat it as not-progress AND skip the baseline write entirely" — means the `Some`/`None`
   arms must diverge: the `Some(current)` arm keeps today's logic verbatim (progress check, then the
   unconditional-within-that-arm baseline write); the `None` arm must accumulate
   `consecutive_failures` (not-progress, i.e. behave like the `else` branch) **but must NOT touch
   `state.last_validate_failure_commit_count` at all** — the whole point of A-04's fix.
4. `mode::consecutive_failures_made_progress(previous: Option<u32>, current: u32) -> bool`
   (`mode.rs:149`) — its SECOND parameter is `u32`, not `Option<u32>`. Under A-04's shape, this
   function is only called from the `Some(current)` arm above, so its signature does **not** need to
   change — the `None` arm never calls it at all, resolving the not-progress decision inline instead.
   (This is a design option this research surfaces, not a lock: the alternative — widening
   `consecutive_failures_made_progress`'s second parameter to `Option<u32>` too, and defining
   `None`-for-current as "not progress" inside the pure predicate — is equally valid and arguably
   more testable in isolation. CONTEXT.md does not decide between these two shapes; either satisfies
   D-08/A-04.)

**The `Err`/`Ok(nonzero)` split — quoted verbatim from CONTEXT.md's A-06, reconfirmed against source
this session (the split is not yet implemented; both `.output()` calls inside `phase_commit_count`
currently collapse to `u32` `0`/`.unwrap_or(0)` regardless of which case occurred):**
> *"Decision: split on whether the command RAN. `.output()` returning `Err` (could not execute git)
> → `None`. `.output()` returning `Ok` with a non-zero status (branch genuinely absent) → `Some(0)`,
> because that is a real observation, not a measurement failure."*

**Existing tests calling these functions directly (verified this session, all pass a `u32`/exact
literal today and will need type updates under D-08, not behavior rewrites):**
- `phase_commit_count_reports_zero_without_a_branch` (`agent_result.rs:6631`) — `assert_eq!(count,
  0, ...)` becomes `assert_eq!(count, Some(0), ...)`.
- `evaluate_layer2_falls_back_to_exit_code_and_commit_count` (`:6650`),
  `evaluate_layer2_exit_zero_no_commits_is_failed` (`:6668`),
  `evaluate_layer2_nonzero_exit_is_failed` (`:6688`) — all assert `result.commits ==
  Some(N)` already (the `AgentResult.commits` field is already `Option<u32>` — only
  `phase_commit_count`'s own return type and the internal `let commits: u32 = ...` binding change).

## C. The 999.78 Never-Reset Counter (criterion 2)

**`State` struct, all fields relevant to the reset-behavior question, read in full this session
(`crates/devflow-core/src/state.rs:32-211`):** `consecutive_failures` (`#[serde(default)]`, reset
conditionally by `transition()` — see below), `infra_failures` (`#[serde(default)]`, reset
UNCONDITIONALLY by `transition()`), `preflight_retries` (`#[serde(default)]`, explicitly documented
"NOT touched by `transition()`"), `last_validate_failure_commit_count: Option<u32>`
(`#[serde(default)]`, also explicitly "NOT touched by `transition()`"), `checkpoint_resumes`
(`#[serde(default)]`, reset by "every ordinary fresh stage launch," NOT by `transition()`).

**The `#[serde(default)]` backward-compat pattern, verbatim from `state.rs:99` (the field
`last_validate_failure_commit_count`'s doc comment) — this is the pattern to follow for the new
counter:**
> *"A serde-absent value (state written by a binary predating this field) deserializes to `None`,
> which is exactly the 'no prior record' meaning above — the same backward-compat pattern as every
> other `#[serde(default)]` field added since 17-01."*

**`transition()`, read in full this session (`crates/devflow-cli/src/pipeline_gate.rs:51-111`) —
confirms CONTEXT.md's A-01 correction independently:**
```rust
state.stage = to;
if mode::transition_resets_consecutive_failures(from, to) {   // conditional
    state.consecutive_failures = 0;
}
state.infra_failures = 0;                                      // UNCONDITIONAL
state.gate_pending = false;
```
`transition()` does **not** touch `preflight_retries`, `checkpoint_resumes`, or
`last_validate_failure_commit_count` at all — confirming the "third group" CONTEXT.md's corrected
`code_context` bullet describes. The 999.78 counter belongs in this third group per that same
bullet.

**`State::new`, read in full this session (`state.rs:256-280`) — confirms A-11's finding
independently:**
```rust
pub fn new(phase: u32, agent: AgentKind, mode: Mode, project_root: PathBuf) -> Self {
    State {
        stage: Stage::Define,
        phase, agent, mode,
        gate_pending: false,
        consecutive_failures: 0,
        infra_failures: 0,
        preflight_retries: 0,
        last_validate_failure_commit_count: None,
        // ... every other field similarly zeroed/None/false ...
    }
}
```
**`commands::start()`, read in full this session (`crates/devflow-cli/src/commands.rs:112-124`) —
confirms `State::new` is called unconditionally, `--force` included:**
```rust
pub(crate) fn start(
    project_root: &Path, phase: u32, agent: AgentKind, mode: Mode,
    force: bool, worktree: bool, dry_run: bool, until: Option<Stage>,
    yes_ship: bool, legacy_claude_launch: bool,
) -> Result<(), CliError> {
    let mut state = State::new(phase, agent, mode, project_root.to_path_buf());   // line 124
    state.stop_until = until;
    // ... `force` is only consulted much later, for the worktree/branch fork itself
    //     (`ensure_phase_worktree(project_root, phase, force)` at line 239,
    //     `git.feature_start_force(phase)` at line 248) — never to skip State::new.
```

**What this means concretely for the 999.78 counter, presented as an open implementation question
(not a locked decision — CONTEXT.md's A-11 explicitly declines to resolve it, instructing "the
planner must state the counter's persistence explicitly"):**

1. **Option (a) — accept the reset.** Store the counter as an ordinary `#[serde(default)] u32`
   field on `State`, matching `preflight_retries`'s shape exactly. It bounds "this `devflow start`
   invocation" (which is what actually matters for the *unattended* overnight-run scenario D-07 is
   about — an unattended run never calls `start()` a second time on its own; only a human running
   `--force` does). Document explicitly that a human-initiated `--force` restart is a deliberate
   reset of the budget, same as it already is for `consecutive_failures` and every other counter.
   This is the cheapest option and is consistent with every existing counter's behavior.
2. **Option (b) — survive `--force`.** Before `State::new` is called in `start()`, read any
   pre-existing `state.json` for this phase (if present — i.e. a resume/`--force` case, not a
   phase's true first start) and transplant the counter's prior value into the freshly constructed
   `State`. This requires a new read site in `commands::start()` immediately before line 124, and a
   decision about what "phase completion" (the other reset event A-11 names as legitimate) means
   operationally — most naturally, `finish_workflow`/`clear_state` clearing `.devflow/state.json`
   already deletes the whole file, so a `--force` restart of a *newly re-triggered* phase (one that
   previously completed) would see no prior state to transplant, which is exactly the "phase
   completion is a real reset event" semantics A-11 calls for.

This research does not pick between (a) and (b) — CONTEXT.md's own text ("If it cannot outlive
`State`, that is a finding to escalate, not to paper over") makes clear this needs an explicit
planner decision recorded in the plan, not a silent default. **Recommendation, stated as a
recommendation and not a decision:** option (a) is far cheaper and is defensible — D-07's own
wording is "Exhausting the never-reset per-phase Validate-failure total fires a human gate," and the
adjective "never-reset" most naturally reads as "never reset by ordinary loop-back," which option
(a) already satisfies; a `--force` restart is itself a human intervention, which is arguably the
correct point to grant a fresh budget rather than a gap to close.

**`MAX_CONSECUTIVE_FAILURES` and `Mode::should_gate`, read in full this session
(`crates/devflow-core/src/mode.rs`):**
```rust
pub const MAX_CONSECUTIVE_FAILURES: u32 = 3;                    // line 18

pub fn should_gate(self, stage: Stage, consecutive_failures: u32) -> bool {   // lines 170-179
    match stage {
        Stage::Ship => true,
        Stage::Validate => match self {
            Mode::Supervise => true,
            Mode::Auto => consecutive_failures >= MAX_CONSECUTIVE_FAILURES,
        },
        _ => false,
    }
}
```
The 999.78 ceiling gate must be a **separate** check alongside this one (D-07's ceiling is a
backstop above `MAX_CONSECUTIVE_FAILURES`, not a replacement for it) — most naturally added as an
additional `||` condition in the `should_gate` call site (`pipeline_outcomes.rs:426-429`,
`if state.mode.should_gate(Stage::Validate, state.consecutive_failures) { ... }`), or as a second
explicit check before/after it. Either shape keeps `should_gate`'s existing signature and callers
untouched if the new check is added at the call site rather than inside `should_gate` itself
(`should_gate` currently takes no argument for the new counter at all — widening its signature is
one option, adding a sibling check at the one call site is another; both are viable, CONTEXT.md
does not choose).

**Gate message construction site, read this session (`pipeline_outcomes.rs:430-436`):**
```rust
let context = match result {
    ValidateResult::Passed => "Validation passed — approve to ship?".to_string(),
    ValidateResult::Failed => format!(
        "Validation failed {} time(s) — human review needed.",
        state.consecutive_failures
    ),
};
```
This is the exact string WR-04 (999.78) complains about — it interpolates the **streak**
(`consecutive_failures`), not a cumulative total, and per the ROADMAP entry reads identically ("1
time(s)") at the 2nd, 5th, and 9th Supervise-mode gate whenever a trivial commit lands each cycle.
The new counter must become the headline number here; the streak may still appear as a secondary
clause.

**Serde round-trip tests that will need a sibling** (pattern to copy, `state.rs:415-447` for
`last_validate_failure_commit_count`'s pair — a present-value round-trip test and an
absent-from-JSON-defaults-correctly test): the new field needs the identical pair.

## D. Stale VERIFICATION.md Detection Under `--force` (criterion 3)

**`phase_verification_exists`, read in full this session (`agent_result.rs:2654-2672`):**
```rust
pub fn phase_verification_exists(evidence_root: &Path, phase: u32) -> bool {
    let Ok(phases) = std::fs::read_dir(evidence_root.join(".planning/phases")) else {
        return false;
    };
    let prefix = format!("{phase:02}-");
    for entry in phases.flatten() {
        if entry.file_name().to_str().is_some_and(|name| name.starts_with(&prefix)) {
            let verification = entry.path().join(format!("{phase:02}-VERIFICATION.md"));
            if verification.exists() { return true; }
        }
    }
    false
}
```
Pure existence check, `pub`, exactly as CONTEXT.md describes. **`--force` dispatch decision is made
one layer up**, in `select_loop_back_fix` (`pipeline_outcomes.rs:315-321`, `fn(evidence_root: &Path,
phase: u32) -> FixType`, private to the crate) — this is the only caller of
`phase_verification_exists` in the whole workspace (confirmed: `rg -n
"phase_verification_exists"` returns only its own definition, its own test, and this one call site).

**Where `--force` re-checkout happens, confirming A-05/A-12's "first-encounter fork":**
`commands.rs:239`, `let wt = ensure_phase_worktree(project_root, phase, force)?;` — this happens
AFTER `State::new` at `:124` (confirming A-05's correction that the fresh-`State` fact alone does
not establish where the fingerprint should be captured — the evidence root the artifact would live
under does not exist yet at `:124`).

**The recommended shape (999.79's discretion item, D-09-adjacent, a departure from the backlog entry
CONTEXT.md flags as overrulable):** record a content fingerprint of `{N}-VERIFICATION.md` in `State`
at the moment it is first observed each run, and treat the artifact as "fresh" (eligible for
`GapsOnly`) only once its content has changed from that run-start baseline. **The `Option<String>`
(or hash) field this needs is a NEW `State` field**, same `#[serde(default)]` pattern as Section C.

**Whether this requires a `phase_verification_exists` signature change (CONTEXT.md hedges "likely
needs," not locked) — this research's finding, presented as an option, not a decision:** the
fingerprint comparison can be implemented ENTIRELY inside `select_loop_back_fix`
(`pipeline_outcomes.rs:315`, private) without touching `agent_result.rs`'s public surface at all:

```rust
// Illustrative shape only — NOT a locked design, presented to show the
// "no third pub-API break" path is viable and should be weighed against
// CONTEXT.md's "likely needs a signature change" hedge.
fn select_loop_back_fix(evidence_root: &Path, phase: u32, state: &mut State) -> FixType {
    if !agent_result::phase_verification_exists(evidence_root, phase) {
        return FixType::FullExecute;
    }
    let path = /* the discovered {N}-VERIFICATION.md path — phase_verification_exists
                  does not currently return it, only a bool; this shape would need
                  either a small pub helper that returns the path, or re-deriving
                  the same prefix-scan locally */;
    let current_fingerprint = std::fs::read(&path).ok().map(|bytes| /* hash */);
    if state.last_verification_fingerprint == current_fingerprint {
        FixType::FullExecute   // unchanged since run start -> stale, mid-arc
    } else {
        state.last_verification_fingerprint = current_fingerprint.clone();
        FixType::GapsOnly      // changed this run -> genuine gaps loop
    }
}
```
This shape needs `select_loop_back_fix` to take `&mut State` (currently takes neither `state` nor a
mutable reference — a private-fn signature change, non-breaking) AND either a new small helper
returning the discovered path (adds one more small `pub` surface to `agent_result.rs`) or a
re-derivation of the prefix-scan logic locally. **Whether that constitutes "the third pub-API
change" CONTEXT.md's D-08 section flags depends entirely on which of these shapes the planner picks
— this is exactly the kind of enumeration D-08 asks the planner to do ("the planner should enumerate
every `pub` item the phase touches so that list is complete").**

**The both-directions test this needs (mandatory, A-12):**
1. A stale artifact from a **prior** run (fingerprint recorded before this run started, unchanged
   since) → `FullExecute`.
2. An artifact the Validate agent authored **this run** (fingerprint changes from what was recorded
   at run start, or was absent at run start and now present) → `GapsOnly`.
A test covering only (1) passes against a rule that marks everything stale forever — this is
explicitly named in CONTEXT.md as the risk a too-strict rule creates.

**Prohibition, reconfirmed by re-reading the CR-01 comment this session
(`pipeline_outcomes.rs:298-314`, quoted in Section B's diagram commentary):** do not revert the
evidence-root probe to `project_root` — that is CR-01, already fixed, and 33-05/two external
reviews already closed it.

## E. The Worktree-Mode Checkpoint Regression Test (criterion 4)

**Call site, confirmed at the exact line CONTEXT.md cites (`pipeline_launch.rs:1068-1071`):**
```rust
let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
let checkpoint_confirmed = state.agent == AgentKind::Claude
    && verify::phase_has_blocking_human_checkpoint(execution_root, phase)   // line 1070
    && agent_result::checkpoint_reported_in_capture(project_root, phase);
```

**`phase_has_blocking_human_checkpoint`, read in full this session (`verify.rs:130-136`):**
```rust
pub fn phase_has_blocking_human_checkpoint(project_root: &Path, phase: u32) -> bool {
    const HUMAN_BLOCKING_GATE: &str = r#"gate="blocking-human""#;
    phase_plan_files(project_root, phase)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|contents| contents.contains(HUMAN_BLOCKING_GATE))
}
```

**Its two root-sensitivity tests, exact lines (small correction to CONTEXT.md's citation — it says
`:340-400` and `:351`/`:376`; the actual lines, read this session, are `:351` and `:377`, one line
off from CONTEXT's second citation, immaterial to the shape):**
- `phase_has_blocking_human_checkpoint_reads_the_execution_root_in_worktree_mode` — `verify.rs:351`.
  Writes a `blocking-human` PLAN only under a `worktree` subdir, asserts `true` for `worktree` and
  **`false` for the bare tempdir root** (the opposite-result assertion in the SAME test).
- `phase_has_blocking_human_checkpoint_still_reads_the_project_root_without_a_worktree` —
  `verify.rs:377`. The main-checkout mirror: without a worktree, the two roots coincide.

D-06's mechanical control ("`phase_has_blocking_human_checkpoint(project_root, phase)` is `false`")
follows this exact shape.

**The base test to extend (A-02's correction, reconfirmed this session, exact line):**
`advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records` —
`pipeline_launch.rs:2302-2357`. Full body read; see Code Examples section above for the relevant
excerpt. It:
- Calls `env_lock()` first, then `init_repo(root)` (real git repo, `main`+`develop`).
- Writes a declared-checkpoint PLAN via `write_declared_checkpoint_plan(root, phase)`
  (`pipeline_launch.rs:2247-2256`) — writes to
  `root/.planning/phases/{phase:02}-checkpoint-fixture/{phase:02}-01-PLAN.md`.
- Writes a confirmed capture via `write_confirmed_checkpoint_capture(root, phase)`
  (`:2263` onward) — a `.devflow/`-resident stdout capture containing both the checkpoint-reached
  marker and a `DEVFLOW_RESULT` failed marker, so Layer 1 deterministically classifies `Failed` →
  `Action::GateReview` with no background thread or exit-file polling needed.
- Sets `state.session_id = Some(...)`, saves state, then stubs `claude` on `PATH` via
  `stub_agent_binary` + `prepend_path` (a **prepend**, keeping real `git`/`sh` reachable — NOT the
  `NeutralPath`-class full replace, because this test needs real `git` for `init_repo` and a real
  `sh` for the monitor's backgrounding script, only `claude` itself is stubbed).
- Calls `advance(root, Some(phase))` synchronously (no scoped thread, no polling — `advance` is
  `pub(crate)`, confirmed at `pipeline_launch.rs:936`).
- Asserts exactly one `checkpoint_auto_decided` event and that no `gate_fired` event exists for
  `stage == "code"`.

**Its negative sibling (small correction to CONTEXT.md's `:2361` citation — the doc comment for this
test starts at `:2359`; the `#[test] fn` itself is at `:2364`):**
`advance_without_declared_checkpoint_falls_through_to_generic_gate` — `pipeline_launch.rs:2364-2390`.
Same phase-88-style fixture shape but **deliberately no** `write_declared_checkpoint_plan` call, plus
a pre-written rejected gate response (`write_abort_gate_response`, `:2288-2296`) so the fall-through
path's own gate resolves immediately instead of blocking. Asserts zero `checkpoint_auto_decided`
events and at least one `gate_fired`.

**999.84's delta, concretely, on the `:2302` base:**
1. Add `state.worktree_path = Some(worktree_path)` before `save_state`.
2. Change `write_declared_checkpoint_plan(root, phase)` to write against `worktree_path` instead of
   `root` (either by parameterizing the helper to take a root argument, or by writing the fixture
   inline for this one test — the helper currently hardcodes `root` as its first parameter name but
   is generic over whatever `Path` is passed, so passing `&worktree_path` instead of `root` already
   works with zero changes to the helper itself).
3. Add D-05's decoy PLAN under `root` (project_root) for the same phase number, using the identical
   fixture shape MINUS the `gate="blocking-human"` attribute (see Code Examples section — this is a
   `<task type="checkpoint:human-verify">` with a non-blocking gate value, or simply a PLAN with no
   checkpoint task at all, as long as the string `gate="blocking-human"` does not appear anywhere in
   its body).
4. Keep the rest of the `:2302` test's shape (env_lock, real `init_repo`, `stub_agent_binary`,
   `advance()`, the `checkpoint_auto_decided`/`gate_fired` assertions) unchanged.
5. Add D-06's own mechanical opposite-result assertion INSIDE the same test:
   `assert!(!verify::phase_has_blocking_human_checkpoint(root, phase))`.
6. **Perform the revert** (`execution_root` → `project_root` at `pipeline_launch.rs:1070`), run
   `cargo test -p devflow --lib pipeline_launch::tests::<new_test_name> --exact` (asserting a real
   `1 passed`/`1 failed` per CLAUDE.md's own warning about `--exact` matching nothing), confirm it
   FAILS, then revert the revert and confirm it passes again. Record both outcomes as evidence (the
   phase's own SUMMARY, not a committed capture directory — D-06 explicitly rejects a heavier
   `35-evidence/` capture as disproportionate).

## F. The `ssh-keygen -Y sign` Probe (criterion 5)

**Every named symbol, exact lines, read in full this session (`crates/devflow-core/src/git.rs`):**
- `SigningStatus` enum — `:728-739` (`NoAgent`, `AgentEmpty`, `KeysListed`, `Unknown(i32)`).
- `classify_ssh_add_status(exit_code: i32) -> SigningStatus` — `:743-750`. Public.
- `SigningViability` enum — `:757-767` (`Viable { fingerprint: Option<String> }`, `NotViable {
  reason: String }`, `Unknown { reason: String }`). Public, unchanged by this phase.
- `public_key_fingerprint(pub_key_path: &Path) -> Option<String>` — `:786-800`. Private,
  `Command::new("ssh-keygen").args(["-lf", path_str])`. **Kept under D-04** — the only helper still
  needed once D-03 routes only the path form to `Viable`.
- `inline_signing_key_blob(signingkey: &str) -> Option<&str>` — `:814-823`. Private, pure (no I/O).
  **Still required under D-03** — classifies inline vs. path form so the probe can skip inline
  values.
- `inline_key_fingerprint(key_blob: &str) -> Option<String>` — `:841-866`. Private, spawns
  `ssh-keygen -lf -` over stdin. **Orphaned by D-03/D-04, delete with its test.**
- `check_ssh_signing_viability(project_root: &Path) -> SigningViability` — `:874-945`. The function
  D-02's probe replaces the body of.
- `check_gpg_signing_viability` — `:949-975`. **Untouched** (D-02 scopes this fix to SSH only).
- `check_signing_viability(project_root: &Path) -> SigningViability` — `:983-988`. The dispatcher,
  unchanged.

**Every consumer of `SigningStatus`/`classify_ssh_add_status`, enumerated by a workspace-wide search
this session (`rg -n "classify_ssh_add_status|SigningStatus" crates/`) — exactly two hits outside
the definitions themselves:**
1. `git.rs:910`, `match classify_ssh_add_status(exit_code) { ... }` — inside
   `check_ssh_signing_viability`, the production caller D-04 removes.
2. `git.rs:1829-1834`, `classify_ssh_add_status_maps_all_three_documented_exit_codes` — the test A-15
   flags. **Confirmed: this is the ONLY test referencing either symbol, and no file in
   `devflow-cli` references either at all.** Delete this test with the same change (A-15's
   instruction, now fully verified — nothing else was missed).

**`SigningViability`'s only consumer, confirmed by the same search discipline
(`rg -n "SigningViability" crates/` outside `git.rs`):** exactly one hit —
`crates/devflow-cli/src/commands.rs:2383-2400`, inside `fn check_signing(project_root: &Path) ->
Check` (`:2379`):
```rust
fn check_signing(project_root: &Path) -> Check {
    const NAME: &str = "tag-signing viability";
    match devflow_core::git::check_signing_viability(project_root) {
        devflow_core::git::SigningViability::Viable { fingerprint } => Check { /* status: "ok" */ },
        devflow_core::git::SigningViability::NotViable { reason } => Check { /* status: "fail" */ },
        devflow_core::git::SigningViability::Unknown { reason } => Check { /* status: "warn" */ },
    }
}
```

**The exact `ssh-keygen -Y sign` invocation — namespace independently VERIFIED this session (not
assumed), see Code Examples for the extraction:**
```
ssh-keygen -Y sign -n git -f <identity-file-or---for-stdin> <throwaway-payload-path>
```
- **`-n git`** — confirmed against a real signature this repository itself produced
  (`v2.4.0`'s tag signature, decoded byte-for-byte, Code Examples section). Do not use any other
  namespace string.
- **The throwaway payload** — must be a real file on disk (`ssh-keygen -Y sign` signs a **file**, not
  stdin, writing `<file>.sig` beside it — this is a divergence from `ssh-keygen -lf -`'s stdin-piping
  shape used by `inline_key_fingerprint` above, which is a *different* subcommand, `-lf`, not `-Y
  sign`). A `tempfile::NamedTempFile` with arbitrary throwaway bytes (e.g. a UUID or the current
  timestamp) is sufficient — content is irrelevant, only the sign/verify roundtrip matters.
  **Cleanup required:** `ssh-keygen -Y sign` writes `<payload>.sig` alongside the payload; both must
  be removed after the probe (a `tempfile::TempDir` scope handles this automatically if the payload
  lives inside one — the `.sig` sibling is written into the same directory).
- **`-f`** — takes the identity: for the path form, the configured `user.signingkey` value itself
  (git resolves the *private* key by stripping `.pub` from that same path, or via the agent — see
  Pitfall 3). D-03 means the inline (`key::`/raw `ssh-`) form never reaches this probe at all — no
  `-f -`/stdin identity form is needed.

**How `SSH_ASKPASS_REQUIRE=never` and a wall-clock timeout apply to a `std::process::Command` in
this codebase:** no existing site sets `SSH_ASKPASS_REQUIRE` yet (a new addition, one
`.env("SSH_ASKPASS_REQUIRE", "never")` call on the `Command` builder before `.spawn()`). The timeout
must use the `canary.rs:415-433` `spawn → try_wait loop → kill → wait` shape (Pattern 2 above) —
`ssh-keygen -Y sign` produces no meaningful streaming output to read incrementally, so the simpler
half of `canary.rs`'s pattern (no reader thread, no mpsc channel) suffices; only the
deadline-bounded `try_wait` loop and the terminal `kill`+`wait` on timeout are needed.

**The measured 8-row behavior table (operator's host, 2026-08-06, n=1 — quoted from CONTEXT.md
verbatim, NOT independently re-run this session; presented with its own stated limitation intact):**

| condition | result |
|---|---|
| configured pub key, private sibling on disk, key **not** in agent | signs, exit 0 |
| fresh key, private key on disk, not in agent | signs, exit 0 — the case `ssh-add -l` false-negatives |
| pub key with no private key anywhere | exit 255, `No private key found for public key` |
| bogus path | exit 255, `Couldn't load public key <path>` |
| encrypted key, no agent, working askpass | **blocks** — timed out at 6s against a 30s askpass |
| same, `SSH_ASKPASS_REQUIRE=never` | exit 255 in 0s |
| real key + agent + `SSH_ASKPASS_REQUIRE=never` | exit 0 — positive control |
| inline blob copied to temp file, agent holds key | exit 0 |
| same, key removed from agent | exit 255 — negative control |

**`release_check.rs`'s existing surface tests, enumerated this session (`crates/devflow-cli/tests/release_check.rs`, 562 lines):**
`release_check_passes_when_pins_match`, `release_check_flags_self_pin_drift`,
`release_without_check_is_rejected`, `release_check_reports_divergence_when_main_not_ancestor`,
`release_check_divergence_degrades_when_origin_main_absent`, `release_check_states_publish_order`,
`release_check_signing_output_leaks_no_key_material_or_path`,
`release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material`,
`release_check_signing_degrades_when_ssh_add_absent`,
`release_check_inline_signingkey_degrades_to_warn_when_ssh_tooling_absent`. The last two (`ssh_add
_absent` framing) will need renaming/rewriting once `ssh-add` is no longer part of the probe at all
— they currently exercise the `ssh-add`-absent degrade path, which no longer exists after D-04;
their replacement should exercise "`ssh-keygen` absent" instead (D-02's third distinguishable
class), matching `check_ssh_signing_viability`'s new fail-soft branch.

## G. Cross-Cutting

**Version locations, confirmed this session (project-devflow-release-mechanics: "the version is set
in two places"):**
1. `Cargo.toml:9` — `[workspace.package]` block, `version = "2.4.0"`.
2. `CHANGELOG.md:3` — `## 2.4.0 — 2026-08-06` heading (new-version-heading pattern to follow for
   `## 2.5.0`).
Both `crates/devflow-core/Cargo.toml:3` and `crates/devflow-cli/Cargo.toml:3` declare
`version.workspace = true` — confirmed, no separate per-crate version to bump.
`crates/devflow-cli/Cargo.toml:2` is `name = "devflow"` (not `devflow-cli"`) —
`crates/devflow-core/Cargo.toml:2` is `name = "devflow-core"`. Matches CLAUDE.md's standing warning.

**Every `pub` item this phase changes or removes, enumerated per D-08's own instruction ("the
planner should enumerate every `pub` item the phase touches so that list is complete"):**
1. `phase_commit_count(...) -> u32` → `-> Option<u32>` (D-08). **Signature change, not a removal.**
2. `classify_ssh_add_status(exit_code: i32) -> SigningStatus` — **removed** (D-04).
3. `SigningStatus` enum — **removed** (D-04).
4. `phase_verification_exists(...) -> bool` — **possibly unchanged**, depending on which 999.79
   implementation shape the planner picks (Section D). If the fingerprint comparison is built
   entirely inside the private `select_loop_back_fix`, this stays untouched and D-08's "third
   public-API change" does not materialize. If a new public helper is added to expose the discovered
   path, that IS a new addition (not a signature change to an existing item) and should be listed
   here explicitly once chosen.
5. `inline_key_fingerprint` — **private**, not a public-API change, but an orphan removal (D-04) that
   still needs its own test (`git.rs`, not yet located by name this session — search
   `inline_key_fingerprint` for its test siblings before deleting) removed alongside it.

**`CHANGELOG.md` entry contents (D-08's binding instruction):** must name every item in the list
above under items 1-3 (and 4, if it changes), plus a crate-doc deprecation note in `git.rs`'s module
doc comment or the removed items' former location saying the old forms are gone and why.

**Which of the six work items are genuinely independent (confirms CONTEXT.md's own sequencing
note, re-verified against source dependencies this session):**
- **999.86 (criterion 5)** — fully self-contained. Touches only `git.rs` (production) and
  `release_check.rs`/`git.rs`'s own test module. Zero overlap with any other criterion's source
  files.
- **999.84 (criterion 4)** — one test, touches only `pipeline_launch.rs`'s test module (plus the
  one-line revert-and-restore of `:1070` itself, which is production code but a single already-
  correct line, not shared with any other criterion's fix).
- **999.77 + 999.87 (criteria 1 and 6)** — share the `Option<u32>` plumbing (D-08) AND the
  forced-`git`-failure harness (Section A). These two are NOT independent of each other — 999.87's
  fix is a direct, compiler-forced consequence of 999.77's signature change (D-09's own reasoning:
  "D-08 makes `let commits: u32 = phase_commit_count(..)` a type error, so the phase must edit that
  exact line regardless"). Plan these as one wave, or as two plans in strict sequence within the
  same wave, never in parallel with each other.
- **999.78 (criterion 2)** — shares `State`/`consecutive_failures`/`last_validate_failure_commit_count`
  neighborhood with 999.77's fix (both touch `handle_validate_outcome`), but is not compiler-forced
  by it — 999.78 adds a NEW field and a NEW gate check; it does not require the `Option<u32>` change
  to compile. Sequencing them in the same wave is efficient (same file, same function) but not
  load-bearing; they could be split across waves if needed.
- **999.79 (criterion 3)** — touches `select_loop_back_fix`/`State` (a new field, same pattern as
  999.78's) but a different code path (`handle_validate_outcome`'s evidence-root/artifact
  probe, not its commit-count/baseline logic). Independent of 999.77/999.87/999.78 at the
  compiler level; shares only the general `State` struct and the `#[serde(default)]` convention.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (standard Rust test harness), workspace crates `devflow-core` and `devflow` (devflow-cli's package name — confirmed `Cargo.toml:2`) |
| Config file | none dedicated — `scripts/check.sh`/`scripts/check-in-container.sh` define "green": fmt, clippy `-D warnings`, test |
| Quick run command | `cargo test -p devflow-core --lib <module>::` or `cargo test -p devflow --lib <module>::` for a targeted module |
| Full suite command | `scripts/check.sh all` (host) / `scripts/check-in-container.sh all` (pinned CI image) |

**CLAUDE.md's own warning, re-stated because this phase's tests are exactly the shape it warns
about:** `cargo test --exact <name>` exits 0 when the name matches nothing. Every test command in
this document's Phase Requirements table must be run and its output checked for a literal `N
passed` line with the expected `N`, not merely a zero exit code.

### Phase Requirements → Test Map

For each success criterion: the test, the **negative control** (the mutation that must make it
fail), and what a green result does **not** establish.

| Req ID | Criterion | Test | Negative Control | What Green Does NOT Establish |
|--------|-----------|------|-------------------|-------------------------------|
| HARDEN-01 | 1 (999.77) | New multi-cycle sequence test in `pipeline_outcomes.rs`'s test module: seed `state.last_validate_failure_commit_count = Some(N)` (or run one real preceding failure to establish it), then drive `handle_validate_outcome` with `NoGitPath` installed (git unresolvable → measurement `None`), then again with real git and an UNCHANGED count (still `N`). Assert `consecutive_failures` accumulated across both post-seed cycles (e.g. 2 then 3), never reset to 1, and `last_validate_failure_commit_count` stayed `Some(N)` throughout the `NoGitPath` cycle. | **Revert A-04's fix** — restore the unconditional `state.last_validate_failure_commit_count = Some(current)` write with `current` computed via the old lossy `u32` (`None`→`0`) mapping. The same test must then show `consecutive_failures` RESET to 1 on the post-`NoGitPath` cycle (`N > 0` reads as progress). A single-cycle test (one failure, one measurement) passes against BOTH the buggy and fixed code — explicitly named a proxy in the ROADMAP entry and CONTEXT.md; do not accept a plan that proposes only a single-cycle version. | Real-world frequency of transient `git` failures in production; behavior for a git that runs and exits non-zero for a genuinely broken repo (that's the `Some(0)` case, a DIFFERENT, already-correct path — not exercised by this test). |
| HARDEN-02 | 2 (999.78) | New test asserting: (a) a loop that commits something trivial every cycle (so `consecutive_failures` keeps resetting to 1 via the progress check) still fires a human gate once the NEW counter reaches its ceiling; (b) the Supervise-mode gate message's leading text names the cumulative total, not the streak, across at least two gates with different streak/total values (proving the two numbers are visibly different in the string, not merely both present). | Remove the new ceiling check (or the new counter increment) and re-run (a) — the loop must no longer gate, looping indefinitely instead (bounded by the test's own iteration cap, not by the code). For (b), revert the gate-message format string to interpolate only `consecutive_failures` — the assertion that the total and the streak read as DIFFERENT numbers at the 2nd vs. 5th gate must fail. | Whether the counter survives a `--force` restart (Section C's open question — this must be its own separate test, explicitly covering whichever option (a)/(b) the plan picks, or explicitly documented as accepted-not-tested if option (a) is chosen). |
| HARDEN-03 | 3 (999.79) | Two-direction test in `pipeline_outcomes.rs`'s (or `agent_result.rs`'s) test module: (a) a `{N}-VERIFICATION.md` present and UNCHANGED from a run-start baseline → `handle_validate_outcome`'s `Failed` path dispatches `FullExecute`; (b) a `{N}-VERIFICATION.md` that changed (or newly appeared) within the current run's `State` lifetime → dispatches `GapsOnly`. | A rule that marks everything stale forever (always `FullExecute`) passes (a) but MUST fail (b) — this is the exact regression A-12 names ("branch (b) is the silent permanent regression this document warns about"). A test covering only (a) cannot catch it. | Behavior across a genuine multi-process resume (state.json read by a SEPARATE `devflow advance` invocation later in the same run) unless the test explicitly persists and reloads `State` through `workflow::save_state`/`load_state` rather than mutating an in-memory `State` only. |
| HARDEN-04 | 4 (999.84) | Extended `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records` (`pipeline_launch.rs:2302`) with `state.worktree_path = Some(worktree)`, the checkpoint PLAN moved to exist only under the worktree, and D-05's decoy PLAN under `project_root`. Plus D-06's mechanical `assert!(!phase_has_blocking_human_checkpoint(project_root, phase))` inside the same test. | **The performed revert is mandatory, not optional:** change `pipeline_launch.rs:1070`'s `execution_root` argument to `project_root`, run the test, confirm it FAILS (checkpoint no longer auto-resumes because the wrong root is read), then revert the revert. The mechanical D-06 assertion alone is a re-running control but does NOT by itself prove `:1070` passes `execution_root` — only the performed revert does (CONTEXT.md states this distinction explicitly; neither substitutes for the other). | Anything about a REAL `git worktree` fixture (D-05 deliberately uses a plain directory) — this test proves root-selection correctness for a directory standing in for a worktree, not git-worktree-specific semantics. |
| HARDEN-05 | 5 (999.86) | New test(s) in `git.rs`'s or `release_check.rs`'s test module driving the real `ssh-keygen -Y sign` probe against a fixture key: (a) positive — private key on disk, agent EMPTY or absent → `Viable`; (b) negative — no private key anywhere → `NotViable`; (c) block-then-recover — encrypted key + `SSH_ASKPASS_REQUIRE=never` → fast `NotViable`/`Unknown`, not a hang. | For (a): remove the private key file (leave only the pub key) — must flip to `NotViable`, proving the assertion isn't vacuously true regardless of key state. For (c): omit `SSH_ASKPASS_REQUIRE=never` against the SAME encrypted-key fixture — the operator's own measurement shows this blocks for the full duration of a working askpass (6s against a 30s askpass in the live measurement); the test's timeout budget must be short enough that this negative-control run visibly exceeds it, proving the env var (not the timeout alone, and not the fixture) is what prevents the hang. | Behavior across OpenSSH versions/builds or key types other than ed25519 — the operator's own measurements are explicitly n=1, one host, one build, one key type, and CONTEXT.md states this outright: "they fix the shape of the design; they are not a claim about behaviour across OpenSSH versions or key types, and the phase's own tests should not cite them as coverage." |
| HARDEN-07 | 6 (999.87) | New test in `agent_result.rs`'s test module: `evaluate_layer2` driven with `exit_code = 0`, `stage = Stage::Code`, and `NoGitPath` installed (git unresolvable during the `phase_commit_count` call inside `evaluate_layer2`). Assert the result is `Ok(None)` (falls to Layer 3), never `AgentStatus::Failed`. | The ROADMAP entry's own stated proxy: a test of the ORDINARY `commits == 0` case (real git, genuinely empty branch) passes against BOTH the buggy and fixed code — `evaluate_layer2_exit_zero_no_commits_is_failed` (`agent_result.rs:6668`, already exists) already covers that case and correctly asserts `Failed`. The NEW test's discriminating case is specifically `exit_code = 0` + `Stage::Code` + unrunnable git — do not accept a plan that reuses or extends the existing zero-commits test as if it covered this criterion. | How often Layer 2 is actually the deciding layer in production (explicitly "Not established" in the `999.87`/`999.77` backlog entries — the code path was read and verified, frequency was not measured). |

### Sampling Rate
- **Per task/plan commit:** targeted `cargo test -p devflow-core --lib <module>::` or `cargo test -p
  devflow --lib <module>::` for the module touched, asserting a real `N passed` count.
- **Per wave merge:** `scripts/check.sh all` (fmt + clippy `-D warnings` + full test suite).
- **Phase gate:** full suite green before `/gsd-verify-work`; additionally, criterion 4's performed
  revert-and-restore is a one-time manual demonstration that must be recorded in the phase's own
  SUMMARY (not re-run by `cargo test` — the mechanical D-06 assertion is what re-runs).

### Wave 0 Gaps
- [ ] `crates/devflow-cli/src/test_support.rs` — new `NoGitPath` RAII guard (Section A), the
  prerequisite for both criterion 1's and criterion 6's tests.
- [ ] A throwaway/probe-level negative control confirming `NoGitPath` actually blocks
  `Command::new("git")` (Section A) — write this first, before the real regression tests, and keep
  it if cheap enough to retain as a permanent harness-sanity test.
- [ ] `crates/devflow-core/src/agent_result.rs` — the multi-cycle 999.77 test and the 999.87
  `evaluate_layer2`-unrunnable-git test (both need `NoGitPath`; 999.77's test may need to live in
  `pipeline_outcomes.rs` instead, since `handle_validate_outcome` is the actual function under test
  — the planner should decide which module owns it based on whether the test drives
  `handle_validate_outcome` directly or the lower-level `phase_commit_count`/
  `consecutive_failures_made_progress` pair).
- [ ] `crates/devflow-core/src/state.rs` — new field(s) for 999.78's counter and 999.79's
  fingerprint, each with a serde round-trip pair (present + absent-defaults) mirroring
  `last_validate_failure_commit_count`'s existing pair (`state.rs:415-447`).
- [ ] `crates/devflow-cli/src/pipeline_launch.rs` — the extended 999.84 test on the `:2302` base.
- [ ] `crates/devflow-core/src/git.rs` — the rewritten `check_ssh_signing_viability` and its new
  probe-based test fixtures (positive/negative/block-then-recover).
- [ ] `crates/devflow-cli/tests/release_check.rs` — rewrite the two `ssh_add_absent`-named tests to
  exercise "`ssh-keygen` absent" instead, since `ssh-add` leaves the probe entirely under D-04.
- Framework install: none — `cargo test` is already fully configured in this workspace.

## Security Domain

`security_enforcement` was not checked against `.planning/config.json` this session — treated as
enabled per protocol (absent-or-true both mean enabled).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V2 Authentication | No | No auth surface touched. |
| V3 Session Management | No | `session_id`/checkpoint-resume machinery is read, not modified, by this phase (999.84 only adds a test around an existing, correct call site). |
| V4 Access Control | No | No access-control surface changes. |
| V5 Input Validation | No, narrowly | The commit-count/measurement changes are internal state-machine hardening, not user/agent input validation in the ASVS sense. |
| V6 Cryptography | **Yes** | The signing-probe rewrite (999.86) is exactly a cryptographic-operation-invocation change. **Never hand-roll signature verification or key handling** — this phase does not: it shells out to `ssh-keygen -Y sign`, the same tool `git tag -s` itself uses, rather than reimplementing any cryptographic logic. D-08's redaction contract (the configured `user.signingkey` value must never appear in any reason string) is a V6-adjacent control this phase must preserve — `git.rs`'s own doc comments already document this discipline (`:752-756`) and D-02 names a concrete leak vector this phase must avoid (`ssh-keygen`'s own stderr embeds the path, e.g. "Couldn't load public key ./does-not-exist.pub" — never pass that stderr through). |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|-----------------------|
| A predictor (`ssh-add -l` fingerprint match) diverges from the real operation it predicts | Tampering (of the trust signal) / — closer to a reliability defect than a classic STRIDE threat, but the shape matches "spoofed status" | Replace prediction with the real operation on disposable input (999.86's whole premise) — do not reintroduce a second predictor anywhere in this phase. |
| A reason string leaking the configured signing-key path or private key material | Information Disclosure | D-08's redaction contract: never pass `ssh-keygen`'s raw stderr through to any `NotViable`/`Unknown` reason string (D-02's explicit reasoning). |
| A transient infrastructure fault (`git` unavailable) silently degrading a safety gate's guarantee | Denial of Service (of the safety mechanism itself, not of the system) | Distinguish "could not measure" from "measured zero" at the type level (`Option<u32>`, D-08) so the compiler — not a hand audit — enumerates every place that distinction must be honored (34/D-06's structural-over-hand-audited precedent, continued here). |

## Sources

### Primary (HIGH confidence) — all read in full this session, all with exact `file:line`
- `crates/devflow-cli/src/test_support.rs` — `ENV_MUTEX` (50), `env_lock` (94-98), `init_repo`
  (103-124), `commit_on_feature_branch` (160-185), `agent_free_git_only_path_dir` (286-298),
  `NeutralPath` (327-359), `stub_agent_binary` (393-402), `prepend_path` (407-416).
- `crates/devflow-core/src/test_support.rs` — `wait_for_exec_visibility` (92-129), re-exports of
  `git_command`/`hermetic_command` from `crate::git` (138-140).
- `crates/devflow-core/src/agent_result.rs` — `phase_commit_count` (1836-1861, doc comment
  1836-1840), `evaluate_layer2` (1892-1958, `Err(_) => return Ok(None)` at 1901, `commits` binding at
  1905), `evaluate_layer3` (1971+), `phase_verification_exists` (2654-2672),
  `phase_commit_count_reports_zero_without_a_branch` (6631-6647), `evaluate_layer2_*` tests
  (6650-6699+).
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `select_loop_back_fix` (315-321, its CR-01 doc
  comment 286-314), `handle_validate_outcome` (323-458, its progress-tracking doc comment 323-352,
  baseline write 399-423, gate-message construction 426-436), `NeutralPath` call sites (1999, 2055,
  2119), worktree-mode mid-arc/discriminator tests (2033-2129).
- `crates/devflow-core/src/mode.rs` — full file read (1-329): `MAX_CONSECUTIVE_FAILURES` (18),
  `transition_resets_consecutive_failures` (111-113), `consecutive_failures_made_progress`
  (149-151), `Mode::should_gate`/`should_auto_loop` (170-185), all tests (215-329).
- `crates/devflow-core/src/state.rs` — full file read (1-666): `State` struct (32-211),
  `State::new` (256-280), all serde round-trip tests (290-666).
- `crates/devflow-cli/src/pipeline_gate.rs` — `transition` (51-111), `loop_back_to_code` (115-123),
  `prepare_loop_back_to_code` (130-159).
- `crates/devflow-cli/src/commands.rs` — `start()` (112-340), `check_signing` (2379-2400).
- `crates/devflow-cli/src/pipeline_launch.rs` — `advance` signature (936), the `Action::GateReview`
  arm and the 999.84 call site (1042-1113, `execution_root`/call at 1068-1070),
  `code_unknown_does_not_transition_to_validate` (1453), `relaunch_checkpoint_session_emits_
  exactly_one_audit_event` (1626), `write_declared_checkpoint_plan`/`write_confirmed_checkpoint_
  capture` (2247-2270+), `write_abort_gate_response` (2288-2296),
  `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records` (2302-2357),
  `advance_without_declared_checkpoint_falls_through_to_generic_gate` (2359-2390),
  `advance_with_declared_checkpoint_but_unreported_gate_falls_through` (2395-2414).
- `crates/devflow-core/src/verify.rs` — `phase_has_blocking_human_checkpoint` (130-136), its two
  root-sensitivity tests (351, 377).
- `crates/devflow-core/src/git.rs` — full signing section read (1-1018): `REPO_LOCAL_GIT_VARS`
  (27-43), `git_command`/`hermetic_command` (72-94), `SigningStatus` (728-750),
  `SigningViability` (757-767), `public_key_fingerprint` (786-800), `inline_signing_key_blob`
  (814-823), `inline_key_fingerprint` (825-866), `check_ssh_signing_viability` (874-945),
  `check_gpg_signing_viability` (949-975), `check_signing_viability` (983-988), test module opening
  (999-1018), `classify_ssh_add_status_maps_all_three_documented_exit_codes` (1829-1834).
- `crates/devflow-core/src/canary.rs` — the bounded reap pattern (355-433, `fn reap` at 415-433).
- `crates/devflow-core/src/agent.rs` — `terminate_and_verify` (118-159, deadline loop at 135-141).
- `crates/devflow-cli/tests/release_check.rs` — full test-name enumeration (10-562).
- `Cargo.toml:8-9` (`[workspace.package]`, `version = "2.4.0"`), `CHANGELOG.md:1-3` (`## 2.4.0`
  heading), `crates/devflow-core/Cargo.toml:2-3`, `crates/devflow-cli/Cargo.toml:2-3` (package
  names + `version.workspace = true`).
- `.planning/phases/35-.../35-CONTEXT.md` — read in full (777 lines): all decisions D-01…D-09, all
  amendments A-01…A-17, all discretion items, all deferred items.
- `.planning/ROADMAP.md` — Phase 35's goal/criteria (lines 33-84) and all six 999.x backlog entries
  (999.87: 328-394; 999.86: 395-490; 999.84: 544-643; 999.79: 708-752; 999.78: 754-803; 999.77:
  805-877+), read in full.
- `.planning/REQUIREMENTS.md` — read in full.
- `.planning/STATE.md` — head (through the active-phase/current-position sections) read; sufficient
  for phase context, milestone `v2.5.0`/Phase 35 confirmed current.
- Live tool verification this session: `cargo --version`, `rustc --version`, `git --version`, `ssh
  -V`; direct extraction and byte-level decode of `v2.4.0`'s real SSHSIG tag signature via `git
  cat-file tag v2.4.0 | ... | base64 -d | xxd`.

### Secondary (MEDIUM confidence)
- The operator's 2026-08-06 live-measurement table (Section F's 8-row table) — quoted verbatim from
  CONTEXT.md, not independently re-run this session. CONTEXT.md itself already caveats it as n=1.
- `ENV_MUTEX`'s "36 mutations across 12 lock regions" count — quoted from the doc comment
  (`test_support.rs:35-42`), not independently re-counted via a fresh `rg` tally this session.

### Tertiary (LOW confidence)
None — every claim in this document is either read from source this session (Primary) or an
explicit, labeled quotation from CONTEXT.md/ROADMAP.md with its own stated limitations preserved
(Secondary). No claim rests on training-data recall alone.

## Metadata

**Confidence breakdown:**
- Forced-`git`-failure harness design: HIGH — the `Err`-vs-`Ok(nonzero)` distinction was verified
  directly against `phase_commit_count`'s actual code (A-06's split), not inferred from CONTEXT.md's
  looser "failing shim" phrasing alone.
- `Option<u32>` plumbing and all four forced compile sites: HIGH — every call site read and quoted.
- 999.78 counter's `--force`-survival question: MEDIUM — the underlying fact (`State::new` always
  zeroes) is HIGH-confidence (read directly), but the *resolution* is deliberately left open per
  CONTEXT.md's own instruction, so no confidence claim is made about which option the planner should
  pick.
- 999.79's "does `phase_verification_exists` need a signature change" question: MEDIUM — presented
  as two viable shapes, not resolved, per CONTEXT.md's own hedge ("likely needs").
- 999.84 test extension: HIGH — base test read in full, delta enumerated against verified fixture
  helper code.
- 999.86 probe mechanics: HIGH for the namespace (independently verified via direct signature
  extraction) and the consumer enumeration (workspace-wide search performed); MEDIUM for the exact
  timeout duration constant (left to the planner per CONTEXT.md, no value recommended here beyond
  "use the canary.rs shape").

**Research date:** 2026-08-06
**Valid until:** This is a source-grounded, single-repository research document, not a
library/ecosystem survey — it is valid until the cited source lines are next edited. Any plan
consuming this document should re-grep the cited `file:line`s if more than a few days elapse before
planning begins, since this is an actively developed repository (STATE.md shows continuous same-day
edits to the exact files this research cites).
