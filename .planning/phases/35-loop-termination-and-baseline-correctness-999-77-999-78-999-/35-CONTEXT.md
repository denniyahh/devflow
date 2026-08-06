# Phase 35: Loop-Termination and Baseline Correctness (999.77 + 999.78 + 999.79 + 999.84 + 999.86) - Context

**Gathered:** 2026-08-06
**Status:** Ready for planning

> **Two of five items were discussed with the operator; three were explicitly delegated.** The
> operator selected the 999.86 signing probe and the 999.84 test harness for discussion, and later
> took one carve-out decision from the 999.78 area. Everything else in 999.77 / 999.78 / 999.79 is
> recorded under **Claude's Discretion** with the reasoning visible — those are resolutions, not
> open questions, and the planner should act on them rather than re-surface them. They are flagged
> so the operator can overrule any of them on sight.
>
> **One discretion item departs from its backlog entry's stated fix direction** (999.79's freshness
> signal). It is marked as such rather than presented as settled.
>
> **This phase's decisions are `D-01`…`D-08`.** D-08 was added 2026-08-06 after adversarial review,
> when an item wrongly filed under discretion was escalated and the operator decided it.

---

## AMENDMENT NOTICE — adversarial review, 2026-08-06

Four adversarial lanes were run against this document the day it was written (two internal, two
external). **Every finding below was re-verified against source by the orchestrator before being
accepted** — one was found overstated and is recorded in its corrected form (A-07). Corrections are
marked `[CORRECTED]` inline where the original text would otherwise be reconstructed by a reader.

**A-08 was escalated and is now ANSWERED (operator, 2026-08-06) — see D-08.** Everything is settled.

- **A-01 — the `transition()` reset claim in `<code_context>` was FALSE.** The original said
  `consecutive_failures` and `infra_failures` are both reset by `transition()`. Only `infra_failures`
  is unconditional. `consecutive_failures` is gated on
  `mode::transition_resets_consecutive_failures` (`mode.rs:111-113`), which is
  `!matches!((from, to), (Stage::Code, Stage::Validate))` — it deliberately does **not** reset on
  the Code→Validate hop, i.e. the only transition the loop under repair makes. This mattered because
  a planner would have used the false version to justify the new 999.78 field ("the existing counter
  resets, so we need a new one") and the justification would not survive review. The correct reason
  is the *progress-based* reset at `pipeline_outcomes.rs:405-412`. Corrected inline.

- **A-02 — the 999.84 base fixture named was the wrong one.** The document sent the planner to
  `code_unknown_does_not_transition_to_validate` (a scoped-thread, gate-polling test for a different
  path) and to `relaunch_checkpoint_session_emits_exactly_one_audit_event` (a helper-level test that
  never calls `advance()`). **`advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records`
  (`pipeline_launch.rs:2302`) already drives `advance()` through `Action::GateReview` with all five
  preconditions**, synchronously, with a negative sibling at `:2361`. 999.84's test is a small delta
  on it. This violated the document's own "extend, do not rebuild" instruction on the phase's only
  test deliverable. Corrected inline.

- **A-03 — D-02's "no second spawn" contradicted D-04's retained fingerprint.**
  `public_key_fingerprint` (`git.rs:786`) is a `Command::new("ssh-keygen")` spawn, so D-04 mandated
  exactly what D-02's wording forbade. **Resolution: D-02's "no second spawn" is scoped to the
  verdict-and-reason path** — nothing may spawn a second process to *decide* viability or to author
  a `NotViable` reason. The fingerprint is a cosmetic field on the already-decided success path.
  D-02's rejection rationale should be read as "doubles the spawn surface *on the failure path*".
  If literal minimality is preferred, dropping it is a one-line change to `Viable { fingerprint:
  None }`, which the type already permits.

- **A-04 — criterion 1's actual fix site had no decision.** The discretion item specified the
  `Option<u32>` return type and `evaluate_layer2`'s mapping, but said nothing about
  `handle_validate_outcome`, which is where the defect lives: the **unconditional baseline write at
  `pipeline_outcomes.rs:419`** ("The baseline advances on every recorded failure regardless of which
  branch ran above"). **Decision: when the measurement returns `None`, treat it as not-progress AND
  skip the baseline write entirely**, so the next successful measurement compares against the last
  *real* observation. That is the half of the backlog's fix the document had dropped, and it is what
  criterion 1 actually names.

- **A-05 — the 999.79 `State::new` evidence was a proxy measurement.** "`start()` calls `State::new`
  unconditionally at `commands.rs:124`" is true, but it does not establish what it was used to
  establish. The artifact to be fingerprinted lives at the **evidence root**, which in worktree mode
  is created by `ensure_phase_worktree` at `commands.rs:239` — *after* `:124`. At `:124` the file
  does not yet exist on any reachable path. The fresh-`State` fact supports "a run-scoped baseline
  slot exists"; it does **not** support "the stale artifact's fingerprint can be captured at start".
  The planner must site the capture after the evidence root is resolved, and the both-directions test
  requirement is unchanged and now load-bearing.

- **A-06 — `Option<u32>` is two-valued; the doc comment names three causes.** `phase_commit_count`
  returns `0` early when `rev-parse --verify` fails, covering both "branch does not exist yet"
  (normal on a phase's first Validate) and "git is broken". **Decision: split on whether the command
  RAN.** `.output()` returning `Err` (could not execute git) → `None`. `.output()` returning `Ok`
  with a non-zero status (branch genuinely absent) → `Some(0)`, because that is a real observation,
  not a measurement failure. Collapsing the branch-absent case to `None` would change progress
  semantics on the first failure of every phase.

- **A-07 — D-01's reversibility rating measured the constant, not the mechanism [agent claim
  corrected].** A review lane asserted the timeout needs a new dependency. **That is overstated.**
  `devflow-core` has no timeout crate and `Command::output()` cannot time out — but `canary.rs:364-426`
  already carries a spawn + deadline + `try_wait` + kill + reap pattern in this same crate, and
  `agent.rs:135` a related one. So it is a copy of an in-crate precedent, not a new dependency.
  D-01's "one env assignment and one duration constant" still understates it: the mechanism is the
  bulk of D-01's work. Rating stands as `reversible`; the effort estimate does not.

- **A-09 — citations were stale and one pointed at out-of-scope material.** Every `ROADMAP.md` line
  range was shifted ~110-135 lines by the roadmap edits committed after this document was written,
  and the 999.84 range had come to cover the **999.85** entry this phase declares out of scope.
  Source citations for `git.rs`, `mode.rs`, `state.rs`, `verify.rs` and `pipeline_outcomes.rs` were
  also off. All corrected; ROADMAP refs now locate by heading, which does not drift.

- **A-10 — criterion 5 is satisfied for the path form only.** Under D-03, inline `key::` keys return
  `Unknown`, so they get neither `Viable` nor `NotViable`. This is a deliberate, recorded gap, not
  an oversight — but it should be stated at verification time rather than discovered there.

### Added by the external lane (opencode), verified independently

- **A-11 — the new 999.78 counter is defeated by restarting the phase, and nothing said so.** The
  counter is specified as a "never-reset per-phase total" living in `State`. But `State::new`
  (`state.rs:263-272`) zeroes **every** counter — `consecutive_failures: 0`, `preflight_retries: 0`,
  `checkpoint_resumes: 0`, `last_validate_failure_commit_count: None` — and `start()` calls it
  unconditionally on every run, `--force` included. So a phase that hits the ceiling and is
  restarted gets a fresh budget: **`State` is per-RUN, while the bound is specified as per-PHASE.**
  Those are not the same lifetime, and the whole point of the counter is to bound a phase that keeps
  failing.
  **Decision: the planner must state the counter's persistence explicitly**, and the reset event must
  be a real event (phase completion / operator approval at the ceiling gate), not "whenever a new
  process starts". If it cannot outlive `State`, that is a finding to escalate, not to paper over —
  a bound that resets on restart does not bound the unattended case D-07 exists for.

- **A-12 — the 999.79 fingerprint has a first-encounter fork the document never resolved, and one
  branch permanently regresses `--gaps-only`.** Sharpens A-05. On a `--force` re-run the stale
  artifact is already on disk when the run starts, so the implementer must choose:
  **(a)** record the stale artifact's hash *before* the Validate agent runs, so the agent's rewrite
  registers as a change; or **(b)** record only when Validate finishes, in which case the first check
  sees `None` → `FullExecute`, and every later check compares the unchanged stale hash → `FullExecute`
  **forever**. Branch (b) is the silent permanent regression this document warns about but does not
  structurally prevent. **Take (a).** The both-directions test in the 999.79 discretion item is what
  discriminates them, which is why it is mandatory rather than nice-to-have.

- **A-13 — the 999.77 two-cycle test needs a way to force a `git` failure, and none was specified
  [agent claim refined].** `phase_commit_count` shells out to real `git`, so "force a measurement
  failure" needs a mechanism. The lane reported this as an unsolved gap. **It is a gap in this
  document, but the tooling already exists:** `crates/devflow-cli/src/test_support.rs` carries
  `NeutralPath` plus the `PATH`-guarding mutex (`PATH` is mutated 36 times across 12 lock regions),
  which is exactly the shape `stub_agent_binary` uses. A failing-`git` shim placed first on `PATH`
  under that guard is the intended route. The planner must still specify it — without a mechanism
  the two-cycle sequence cannot be written at all, and the fix becomes unverifiable.

- **A-14 — the ROADMAP's own 999.77 entry cited the baseline write at `pipeline_outcomes.rs:357`;
  it is at `:422`.** Line 357 is the tail of `handle_validate_outcome`'s signature. Corrected in
  `ROADMAP.md`. This is the second stale citation in that entry (see A-09 for the doc-comment one) —
  treat every `file:line` in the 999.x entries as needing re-checking before use, not as canonical.

### A-08 — RESOLVED: escalated to the operator, who chose the breaking change (now D-08)

`pub fn phase_commit_count(...) -> u32` (`agent_result.rs:1841`) is public API of the published
`devflow-core` crate. Changing its return type is the **same class of irreversible act as D-04** —
same crate, same publish, undo-by-republication — but D-04 was put to the operator and this was not;
it sits under "Claude's Discretion" with an instruction to act on it.

The document's own precedent (D-04 rated `one-way`, D-07 escalated as behavioural) says anything
one-way goes to the operator. It was escalated, and **the operator chose the breaking change** —
recorded as **D-08** in `<decisions>`.

Related and also uncounted: the 999.79 freshness check likely needs a signature change to
`pub fn phase_verification_exists` (`agent_result.rs:2654`), which would be a **third** public-API
change in the same cut. Under D-08 that is no longer a reason to hesitate — the release stays
`v2.5.0` and the breaks are documented rather than versioned — but the planner must still enumerate
every `pub` item it changes, because that list *is* the deliverable now.

---

<domain>
## Phase Boundary

Five confirmed, already-diagnosed defects in the machinery that decides **when an unattended run
stops**, plus one release preflight. No new capability; every item has a fix direction already
sketched in its `999.x` backlog entry.

- **999.77** — a transient `git` failure overwrites the `consecutive_failures` forward-progress
  baseline with a false zero, buying one free reset of the `MAX_CONSECUTIVE_FAILURES` ceiling. The
  source doc comment currently promises the opposite guarantee.
- **999.78** — the Code↔Validate loop has no bound independent of trivial per-cycle commits, and
  the Supervise gate message interpolates a *streak length* where a human reads it as a total.
- **999.79** — `{N}-VERIFICATION.md` never goes stale, so a `--force` re-run inherits the previous
  run's verdict, dispatches `--gaps-only` against zero matching plans, and gates unresolvably.
- **999.84** — the worktree-mode `GateReview` checkpoint call site (`pipeline_launch.rs:1070`) is
  correct by construction with no test driving it.
- **999.86** — `release --check`'s tag-signing preflight infers viability from `ssh-add -l` and has
  false-negatived live on two separate release cuts with the correct key present.

**Not in this phase:** 999.83 (drain-gate concurrency — Phase 36); 999.85 (two comments justifying
themselves by a mechanism Phase 34 deleted — explicitly Out of Scope in `REQUIREMENTS.md`);
999.67; 999.61; DEN-50 (`devflow release`'s real signing executor); 999.76's still-open
linked-worktree-harness question.

</domain>

<decisions>
## Implementation Decisions

> **Label scope.** This phase's decisions are `D-01`…`D-08`. Phase 34's decisions are cited with a
> `34/` prefix and Phase 31's with `31/`. They are different decisions with overlapping numbers.

### Signing probe surface (999.86)

- **D-01:** **The probe runs non-interactively via `SSH_ASKPASS_REQUIRE=never` AND a wall-clock
  timeout.** Both, not either. Established by measurement during discussion, with controls: an
  encrypted key with no agent and a *working* askpass made `ssh-keygen -Y sign` **block** (timed
  out at 6s against a 30s askpass); `SSH_ASKPASS_REQUIRE=never` turned that into exit 255 in 0s;
  and the positive control confirmed the working signing path still exits 0 under the same env var.
  The timeout covers what the env var does not — a wedged `ssh-agent`, a stalled PKCS11 provider.
  Rejected: env var alone (leaves non-askpass blocking routes open, and a hung preflight is the
  DOGFOOD-01 class); timeout alone (every `release --check` on a host with a working graphical
  askpass pops a passphrase dialog and then eats the timeout, so the resulting `NotViable` would be
  an artifact of the timeout rather than a real signing failure).
  — **Reversibility:** reversible — one env assignment and one duration constant inside a single
  function, no persisted state, no published contract.

- **D-02:** **The probe's exit code is the sole verdict, and `NotViable` reasons are a fixed set
  keyed by failure class — no second spawn, and `ssh-keygen`'s stderr is never re-emitted.**
  Rejected: retaining `ssh-add -l` purely to author prose (keeps today's richer diagnostics but
  doubles the spawn surface and leaves the deleted predictor's logic alive); classifying
  `ssh-keygen`'s own stderr strings (couples DevFlow to OpenSSH message text, which is not a stable
  interface across versions).

  **Three classes are distinguishable from the probe alone** — probe timed out; probe exited
  non-zero; `ssh-keygen` absent (→ `Unknown`, fail-soft, matching D-06 of phase 20d). Two existing
  pre-probe early returns are unchanged: `user.signingkey` unset, and the path form whose file does
  not exist.

  **Accepted cost, stated explicitly:** this is strictly *less* actionable than today on the
  failure path. The operator loses the "agent reachable but this key not loaded" vs "no agent at
  all" distinction. That is the deliberate trade for removing the mechanism that produced two live
  false negatives.

  **D-08's redaction contract still binds** — the configured `user.signingkey` value must never
  appear in any reason string, in any form. Note concretely why this matters here: `ssh-keygen`'s
  own stderr embeds the path (`Couldn't load public key ./does-not-exist.pub`), so passing it
  through would violate D-08. That is a second, independent reason for this decision.

- **D-03:** **An inline `key::` (or deprecated raw `ssh-`) signing key returns
  `SigningViability::Unknown` and is not probed.** Fail-soft per phase 20d's D-06. Rejected:
  materializing the blob to a `0600` temp file and probing it — *this was measured to work* during
  discussion (exit 0 when the agent holds the key, 255 when it does not, negative control run), so
  it was rejected on surface cost rather than on feasibility. The operator chose the smaller
  surface: no temp file, no cleanup path, no new failure mode inside a preflight check.

  **What this does not cover, stated so nobody reads more into it:** `key::` and raw `ssh-` users
  get *no verdict at all*, where today they get a fingerprint comparison. This repository's own
  config uses the path form, so the corner is rare but not empty.

  **`inline_signing_key_blob` is still required** — it classifies the value as inline so this arm
  can be taken. Only the *probing* of inline values is dropped. git's own prefix precedence
  (20d D-01/D-02/D-03) remains authoritative for that classification and must not be re-derived.

- **D-04:** **`classify_ssh_add_status` and `SigningStatus` are deleted, and the release treats it
  as the public-API break it is.** Both are `pub` in `devflow_core::git`; `devflow-core` is
  published to crates.io; and their only production caller is the `ssh-add -l` branch this phase
  removes. `devflow-cli` never references either (it consumes only `SigningViability`). Rejected:
  retaining them unused with an explanatory doc comment — dead public API that still reads like the
  sanctioned way to judge signing viability is precisely how this predictor survived review twice.
  Rejected: keeping the enum while deleting the fn — a public enum no public function produces is
  worse than either whole option.
  — **Reversibility:** one-way — removes two `pub` items from a crate published to crates.io. Undo
  is not a code revert but a re-publication. **Version handling settled by D-08: the release stays
  `v2.5.0`** (no external consumers; strict semver's `3.0.0` was explicitly declined), and the
  removal is recorded in `CHANGELOG.md` and the crate docs instead. Per
  `project-devflow-release-mechanics` the version is set in two places and `devflow-core` publishes
  before `devflow-cli`.

  **Orphan created by this phase's own change:** `inline_key_fingerprint` (private,
  `git.rs:841`) loses its only production caller under D-03, since inline values no longer reach
  the `Viable` arm. Remove it and its tests with the same change — it is an orphan *this* work
  created, which the standing rule says to clean up (as distinct from pre-existing dead code, which
  is to be reported, not deleted).

  **`Viable { fingerprint }` keeps reporting the fingerprint** (orchestrator's call, recorded so
  the planner does not treat it as open): sourced from the existing `public_key_fingerprint`
  helper, which is already written, tested and D-08-compliant. The enum is public and dropping the
  value would change `release --check` output for no gain. Under D-03 only the path form reaches
  `Viable`, so `public_key_fingerprint` is the only helper still needed.

### Test harness depth (999.84)

- **D-05:** **The worktree fixture is a plain `create_dir_all` directory, and `project_root` gets a
  decoy PLAN.** The worktree holds a `{N}-PLAN.md` declaring `gate="blocking-human"`; the project
  root holds a PLAN *for the same phase* declaring no such gate. Reverting `pipeline_launch.rs:1070`
  to `project_root` then fails because the **wrong root was read**, not because the main checkout
  happened to be empty.

  Rejected: the bare version with `project_root` left empty — this is what the backlog entry
  proposes and what `verify.rs:351` does, and it does discriminate, but partly by a condition
  production never satisfies (the main checkout always carries `.planning/phases/`, often including
  a previous run's copy of this phase). Same cost, weaker control. Rejected: a real linked
  `git worktree` fixture — disproportionate here, because the argument under test resolves a path
  and a linked worktree's files are ordinary files.

  **Correction to the 999.84 / 999.76 entries' shared premise, established during discussion.**
  Both describe a real linked-`git worktree` test as something the workspace does not have and this
  would be the first of. That is true of the `verify.rs` tests specifically and **false at
  workspace scope** — real `git worktree add` fixtures already exist at
  `crates/devflow-cli/src/staleness.rs` (`worktree_staleness_fixture`),
  `crates/devflow-cli/src/preflight.rs:1198` (CR-02), and `crates/devflow-core/src/worktree.rs`.
  This does not change D-05, but it means 999.76's open question is cheaper than its entry implies,
  and a planner should not cite "the workspace has no such harness" as a reason for anything.

- **D-06:** **A mechanical opposite-result assertion ships inside the same test, alongside the
  performed revert.** Criterion 4's demonstration — actually reverting `:1070` to `project_root`,
  watching the new test fail, restoring — is binding and must happen. But it is a one-time act that
  nothing re-runs. So the test *also* asserts directly that
  `phase_has_blocking_human_checkpoint(project_root, phase)` is `false`, the same
  "opposite-result case" shape `verify.rs:351` and `:376` already carry, which re-runs on every
  `cargo test`.

  **What the mechanical half does and does not establish**, stated because the distinction is the
  whole point: it proves the two roots *disagree* for this fixture — which is what makes the revert
  meaningful — and it does **not** by itself prove `:1070` passes `execution_root`. Only the
  performed revert establishes that. Neither replaces the other.

  Rejected: prose-only recording in SUMMARY.md and a doc comment — if a later refactor makes the
  two roots coincide in the fixture, the test silently stops discriminating while the prose still
  claims it does. Rejected: a committed `35-evidence/` capture of the revert run — heavier than a
  one-line revert warrants, and it still does not re-run.

- **Settled by fact, not preference — the planner should not treat these as open:**
  - **The test lives in `pipeline_launch.rs`'s own `#[cfg(test)]` mod**, not
    `crates/devflow-cli/tests/`. `advance()` is `pub(crate)` (`pipeline_launch.rs:936`), so an
    integration test in `tests/` cannot call it. The backlog entry's phrase "one integration test"
    is loose on this point.
  - **[CORRECTED — see Amendment A-02. The base fixture named here was the wrong one.]**
    **The test to extend is `advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records`
    (`pipeline_launch.rs:2302`).** It already drives a real `advance()` **through the
    `Action::GateReview` arm** with all five preconditions satisfied — `env_lock()`,
    `stub_agent_binary("claude")`, `init_repo`, a declared-checkpoint PLAN, a confirmed capture, a
    session id — and asserts exactly one `checkpoint_auto_decided` event. It is a synchronous
    `advance()` call: no scoped thread, no gate-file polling. Its negative sibling is at `:2361`.
    **999.84's test is a small delta on it** — set `state.worktree_path = Some(worktree)`, move the
    `blocking-human` PLAN so it exists only under the worktree, and add the decoy PLAN under
    `project_root` per D-05. (`code_unknown_does_not_transition_to_validate` at `:~1452`, named in
    the original bullet, drives a *different* code path and is not the right base.)
  - **Nothing real is launched.** Satisfying all five preconditions makes the code call
    `relaunch_checkpoint_session`, which spawns an agent —
    `relaunch_checkpoint_session_emits_exactly_one_audit_event` (`pipeline_launch.rs:1626`) already
    solves this with a `stub_agent_binary("claude")` helper and an `env_lock()` guard. The
    observable is the `checkpoint_auto_decided` event, emitted *before* the spawn by design (D-07 of
    plan 28-03). Assert that event fires rather than the per-stage never-silent gate.

### Loop bound (999.78) — operator carve-out

- **D-07:** **Exhausting the never-reset per-phase Validate-failure total fires a human gate; the
  run stays alive.** Operator decision, 2026-08-06, taken as a carve-out after the operator had
  declined the surrounding area — the ceiling's consequence changes *when unattended runs stop*,
  which is not the orchestrator's to choose. Same shape as `MAX_CONSECUTIVE_FAILURES` today: the
  run stops auto-looping and waits, and the human may approve, loop back, or abort.

  Rejected: aborting the phase outright — destructive and irreversible relative to gating; a phase
  one cycle from converging gets killed. Rejected: gate in Supervise, abort in Auto — two
  behaviours to reason about, and it contradicts Auto's existing ceiling, which gates.

  **Accepted cost, stated explicitly:** an unattended overnight run now parks on a gate instead of
  looping to completion. That is the intent, and it is still a behaviour change from today's
  "looped forever unnoticed."

### Breaking change to `devflow-core` (999.77) — operator carve-out

- **D-08:** **`phase_commit_count`'s return type changes to `Option<u32>`; Phase 35 ships a breaking
  `devflow-core` change.** Operator decision, 2026-08-06, escalated from Claude's Discretion after
  adversarial review flagged that a one-way public-API break had been resolved without asking (A-08).

  Rejected: the backlog's **sibling function** — it leaves the lossy call site compiling unchanged,
  and there is now a *named instance* of that harm (see the `evaluate_layer2` finding below).
  Rejected: a **`#[deprecated]` delegating wrapper** (`phase_commit_count` = `…_checked(..).unwrap_or(0)`),
  which would have been non-breaking while keeping one implementation — a genuine option, declined
  because the break is already bought.

  **Why the marginal cost is zero.** D-04 already removes two `pub` items, which is itself breaking.
  A major bump is a **fixed cost, not per-item**, so adding this change costs nothing further in
  version terms. `phase_commit_count` is also public by accident of module layout rather than design
  — it takes a `GitFlowConfig` and a phase number, and no external consumer would plausibly call it.

  **Version: the release stays `v2.5.0`. Operator decision, 2026-08-06.** Strict semver would say
  `3.0.0`, and that was put to the operator explicitly. It was declined on the grounds that
  **`devflow-core` has no external consumers**, so the compatibility risk is theoretical, while
  renaming the active milestone would mean editing `STATE.md`, `ROADMAP.md`, `REQUIREMENTS.md` and
  `PROJECT.md` and disturbing the milestone-heading window `CLAUDE.md` flags as parser-fragile
  (999.72/999.72a broke exactly that way). **Do not rename the milestone.** Both crates still share
  `version.workspace = true`, and per `project-devflow-release-mechanics` the version is set in two
  places with `devflow-core` publishing before `devflow-cli`.

  **The break is documented instead — this is a phase deliverable, not release-time paperwork.**
  A `CHANGELOG.md` entry under the new version must name every changed or removed `pub` item
  (`phase_commit_count`'s signature, plus D-04's `classify_ssh_add_status` and `SigningStatus`), and
  the crate docs must carry a deprecation note saying the old forms are gone and why. The planner
  should enumerate every `pub` item the phase touches so that list is complete — 999.79 may add a
  third via `phase_verification_exists`.
  — **Reversibility:** one-way — same class as D-04, and now pooled with it in a single 3.0.0 cut.

  **Defect surfaced while reasoning about this decision, NOT fixed here (34/D-04).**
  `evaluate_layer2` (`agent_result.rs:1905`) computes `no_work_done = commit_gated && commits == 0`
  and routes it to `AgentStatus::Failed`. A transient `git` failure returns `0`, so an agent that
  **exited 0 and committed real work** is classified `Failed — no work done` and fed back into the
  Code↔Validate loop. That is the same root cause as 999.77 but a worse consequence — a
  misclassification rather than a weakened bound. Under D-08 the compiler will force this call site
  to be confronted; **the phase still maps it to today's zero-treatment explicitly and files the
  defect**, rather than silently widening scope. Filing is pending the operator's go-ahead.
  *Not established:* how often Layer 2 is the deciding layer in production — the code path was read,
  the frequency was not measured.

### Claude's Discretion

The operator declined to discuss the 999.77 / 999.78 / 999.79 area (except D-07 above). These are
**resolutions with reasoning, not open questions** — act on them.

- **999.77 — change `phase_commit_count`'s return type; do not add a sibling.** The backlog entry
  proposes "add a sibling returning `Option<u32>`". That reintroduces exactly the hazard
  `phase_commit_count`'s own doc comment says the extraction removed ("what made the two counts able
  to silently diverge before this extraction"), and nothing stops a future caller reaching for the
  lossy one. Changing the single implementation to return `Option<u32>` keeps one implementation
  *and* makes "could not count" representable at the type level, so the compiler enumerates every
  consumer once — continuing 34/D-06's structural-over-hand-audited line. `evaluate_layer2` maps
  `None` to its existing zero-treatment explicitly at the call site, with a comment, so the
  behaviour it keeps is a visible choice rather than an inherited accident.
  **This stacks a second public-API change on D-04** — same crate, same cut.

- **999.77 — the two doc comments are part of the deliverable, not cleanup.**
  `phase_commit_count`'s "Every consumer treats all three the same way" line
  (`agent_result.rs:~1838-1840`) is deliberately falsified by the fix. `pipeline_outcomes.rs:337-340`
  promises a guarantee the code does not have; the backlog says correct it *even if the code fix is
  deferred*. ROADMAP criterion 1 names the doc comment explicitly.

- **999.77 — the regression test is the two-cycle sequence, and nothing less.** Force a measurement
  failure, then a success with an unchanged *real* count, and assert the streak **accumulated**
  rather than reset. The backlog states plainly that a single-cycle test passes against both the
  buggy and the fixed code — so a single-cycle test is a proxy measurement, and the sequence is the
  negative control. No current test exercises a failing `git` at all.
  **Do not treat Gemini 3.1 Pro's AGREE on this logic as clearing it** — that review analysed
  failure-then-failure and never examined failure-then-success, which is the actual defect. Recorded
  in the 999.77 entry.

- **999.78 — the new counter's shape.** A new `State` field with `#[serde(default)]`, following
  `last_validate_failure_commit_count`'s established backward-compat pattern, and **not** touched by
  `transition()` — matching how `preflight_retries` and `checkpoint_resumes` are handled, since it
  is a per-phase total rather than a per-streak counter. The ceiling is a named constant
  meaningfully above `MAX_CONSECUTIVE_FAILURES = 3` so it functions as a backstop rather than a
  competing primary bound; ~10 is the orchestrator's suggestion, and the planner may argue the
  number, not the shape.

- **999.78 — the gate message leads with the cumulative total.** WR-04's complaint is that
  `"Validation failed 1 time(s)"` reads identically at the 2nd, 5th and 9th gate. The total must be
  the headline number and must be named as a per-phase total; the streak may appear as a secondary
  clause, but only if it cannot be mistaken for the headline.

- **999.78 — IN-02.** A distinct `loop_back` reason string for the absent-baseline case, so an
  operator who upgraded a binary mid-phase gets a signal that the failure budget widened.

- **999.79 — the freshness signal. THIS DEPARTS FROM THE BACKLOG ENTRY; overrule freely.** The
  entry proposes comparing a recorded plan count against the phase's current plan set. That works
  only indirectly — it detects "the plan set changed", and false-negatives when a replan happens to
  produce the same count. Established during discussion: `start()` calls `State::new(...)`
  unconditionally at `commands.rs:124`, before any `--force` handling, so **every run begins with
  fresh `State`** — which makes a run-scoped freshness signal available without git archaeology.
  Preferred shape: record a content fingerprint of `{N}-VERIFICATION.md` in `State` and treat the
  artifact as fresh only once it has changed within this run. The entry's rejection of mtime is
  about mtime as an *age* signal (it survives `git checkout`); as change-detection against a
  run-start baseline the objection does not apply, and a content hash removes it entirely. ROADMAP
  criterion 3 permits this explicitly — "via a recorded plan-count comparison **or equivalent**".

  **Both directions must be tested, and this is the risk in the decision:** a freshness rule that is
  too strict never lets `--gaps-only` fire again, silently regressing what Phase 33 built. The test
  must cover (a) a stale artifact from a prior run → `FullExecute`, and (b) an artifact the Validate
  agent authored *this run* → `GapsOnly`.

  **Prohibition, carried forward unrelaxed:** do **not** "fix" this by reverting the probe to
  `project_root`. That reintroduces the CR-01 defect 33-05 closed, confirmed independently by two
  external peer reviews.

- **999.86 — probe mechanics left to the planner.** The `-n` namespace **must be verified against a
  real git-produced signature rather than assumed** (the probe's whole value is being the operation
  rather than an approximation of it); the timeout duration constant; where the throwaway payload
  lives. The GPG/openpgp branch (`check_gpg_signing_viability`) is untouched — the backlog scopes
  this to `check_ssh_signing_viability` only, and that scope is not the planner's to widen.

- **Plan decomposition and sequencing.** The five items have no structural dependency on each other.
  999.77 and 999.78 share `consecutive_failures` and its `State` neighbourhood; 999.86 is fully
  self-contained; 999.84 is one test. `workflow.granularity` is `medium`.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Binding scope and requirements
- `.planning/ROADMAP.md` § "Phase 35: Loop-Termination and Baseline Correctness" — the
  authoritative goal and its five success criteria. Binding over this document's framing.
- `.planning/REQUIREMENTS.md` — HARDEN-01…HARDEN-05, and the Out of Scope table.
- `.planning/ROADMAP.md` § "Phase 999.77" (locate by heading — line numbers drift) — the failure sequence, the false doc
  comment, the required two-cycle test, and the Gemini-AGREE caveat.
- `.planning/ROADMAP.md` § "Phase 999.78" (locate by heading) — WR-01, WR-04, IN-02.
- `.planning/ROADMAP.md` § "Phase 999.79" (locate by heading) — including the `project_root`
  prohibition.
- `.planning/ROADMAP.md` § "Phase 999.84" (locate by heading) — the five preconditions, why the Phase
  34 capture campaign does not cover it, and the negative-control requirement.
- `.planning/ROADMAP.md` § "Phase 999.86" (locate by heading) — why D-10's premise stopped holding,
  and the explicit scope discipline.

### Prior decisions this phase inherits
- `.planning/milestones/v2.4.0-phases/34-stream-json-coverage-and-the-validate-trust-boundary-999-73-/34-CONTEXT.md`
  — 34/D-06 (structural guards over hand-audited equality tests), 34/D-08 (named positive controls
  and layer-paired mirrors), 34/D-04 (a revealed defect is filed, not fixed in-phase), 34/D-15 (why
  checking a function's inputs does not establish a whole-system property).
- `.planning/superseded/26-release-cut-automation/26-CONTEXT.md` § D-10 — the original
  "don't predict signing viability" decision. 999.86 is a *new* decision made after D-10's premise
  stopped holding, not a rewrite of it; D-10 stays as the historical record, unedited.

### Source under change
- `crates/devflow-core/src/git.rs:728-1010` — `SigningViability`, `SigningStatus`,
  `classify_ssh_add_status`, `inline_signing_key_blob`, `inline_key_fingerprint`,
  `public_key_fingerprint`, `check_ssh_signing_viability`, `check_gpg_signing_viability`. Read the
  doc comments in full — they record 20d's D-01/D-02/D-03/D-06/D-08/D-12 and why each branch spawns
  what it spawns, in what order.
- `crates/devflow-core/src/agent_result.rs:1821-1861` — `phase_commit_count`, including the
  "Every consumer treats all three the same way" line the 999.77 fix falsifies.
- `crates/devflow-core/src/agent_result.rs:2654-2672` — `phase_verification_exists`, the pure
  existence check 999.79 makes staleness-aware.
- `crates/devflow-core/src/mode.rs:120-151` — `consecutive_failures_made_progress` and the doc
  comment deferring the remedy to "a follow-up if the assumption proves wrong". 999.78 is that
  follow-up; cite it from the doc comment.
- `crates/devflow-core/src/mode.rs:163-186` — `Mode::should_gate` / `should_auto_loop`.
  **`MAX_CONSECUTIVE_FAILURES` is at `mode.rs:18`**, not in that window, and
  `transition_resets_consecutive_failures` is at `mode.rs:111`.
- `crates/devflow-core/src/state.rs:45-110` — `consecutive_failures`, `infra_failures`,
  `preflight_retries`, `last_validate_failure_commit_count`, `worktree_path`. The `#[serde(default)]`
  backward-compat pattern and the "NOT touched by `transition()`" convention both live here.
- `crates/devflow-cli/src/pipeline_outcomes.rs:283-470` — `select_loop_back_fix`,
  `handle_validate_outcome`, the unconditional baseline write, and the gate-message interpolation.
  The CR-01 note explaining why the two root-consuming reads are deliberately on *different* roots
  is load-bearing and must survive.
- `crates/devflow-cli/src/pipeline_launch.rs:1042-1090` — the `Action::GateReview` arm and the
  `execution_root` argument under test.
- `crates/devflow-cli/src/pipeline_launch.rs:743-782` — `relaunch_checkpoint_session` and the
  `checkpoint_auto_decided` event emitted before the spawn.
- `crates/devflow-cli/src/commands.rs:111-124` — `start()`'s unconditional `State::new`, which is
  what makes a run-scoped freshness signal available for 999.79.
- `crates/devflow-cli/src/commands.rs:2380-2400` — `SigningViability`'s only consumer, the
  `release --check` output mapping.

### Existing tests to extend or mirror
- `crates/devflow-core/src/verify.rs:340-400` — the two root-sensitivity tests and their
  opposite-result assertions. D-06's mechanical control follows this shape.
- `crates/devflow-cli/src/pipeline_launch.rs:~1452` —
  `code_unknown_does_not_transition_to_validate`, the `init_repo` + scoped-thread + gate-polling
  `advance()` harness.
- `crates/devflow-cli/src/pipeline_launch.rs:1626` —
  `relaunch_checkpoint_session_emits_exactly_one_audit_event`, the `stub_agent_binary` +
  `env_lock` pattern.
- `crates/devflow-cli/src/staleness.rs:~556-600` — `worktree_staleness_fixture`, a real
  `git worktree add` fixture, should a linked-worktree harness ever be wanted.
- `crates/devflow-cli/tests/release_check.rs` — `release --check`'s existing surface tests.

### Repository rules that bind this phase
- `CLAUDE.md` § "Verification habits this repo has already paid for" — `cargo test --exact` exits 0
  on a name matching nothing; assert on a real `1 passed` with a non-zero `filtered out`; the
  package is `devflow`, not `devflow-cli`.
- `CLAUDE.md` § "Never run git operations while an executor holds the working tree".
- `CLAUDE.md` § "Keep DEV-SETUP-CHECKLIST.md in sync".

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- **`stub_agent_binary("claude")` + `env_lock()`** (`pipeline_launch.rs`, used at `:1638`) — the
  existing way to drive a code path that spawns an agent without launching one. 999.84's test needs
  exactly this.
- **`init_repo` + scoped-thread `advance()` + gate-file polling** (`pipeline_launch.rs:~1452`) — a
  working harness for driving a real `advance()` over a real git repo. Extend it; do not rebuild.
- **`devflow_core::test_support::git_command`** — the scrubbed git invoker every real-git fixture
  in the workspace already routes through.
- **`public_key_fingerprint`** (`git.rs:786`) — already written, tested, and D-08-compliant; keeps
  `Viable { fingerprint }` populated under D-04 with no new code.
- **Real `git worktree add` fixtures** — `worktree_staleness_fixture` (`staleness.rs:~578`),
  `preflight.rs:1198`, `worktree.rs:~380`. Not needed under D-05, recorded because both the 999.84
  and 999.76 entries assert these do not exist.

### Established Patterns

- **Opposite-result assertions in the same test** (`verify.rs:351`/`:376`, and 34/D-08) — a pair
  that returns the same answer for both inputs is measuring the wrong thing. D-06 continues it.
- **`#[serde(default)]` for every numeric/optional `State` field added since 17-01**, with the
  absent case given an explicit documented meaning (`last_validate_failure_commit_count`'s
  `None`-vs-`Some(0)` note is the model to follow for 999.78's and 999.79's new fields).
- **[CORRECTED — the original form of this bullet was false; see Amendment A-01.]** The reset rule
  is *conditional*, not a clean two-group split. `infra_failures` is reset unconditionally
  (`pipeline_gate.rs:97`), but `consecutive_failures` is reset only when
  `mode::transition_resets_consecutive_failures(from, to)` returns true — and that function
  (`mode.rs:111-113`) is `!matches!((from, to), (Stage::Code, Stage::Validate))`, i.e. it
  **deliberately does NOT reset on the Code→Validate hop**, the only transition the loop under
  repair executes. `preflight_retries`/`checkpoint_resumes`/`last_validate_failure_commit_count` are
  untouched by `transition()`, as stated. 999.78's new total belongs with that third group.
  **Do not justify the new field with "the existing counter is reset by `transition()`"** — inside
  the loop it is not. The real reason `consecutive_failures` cannot serve as the bound is the
  progress-based reset at `pipeline_outcomes.rs:405-412`.
- **Structural guards over hand audits** (34/D-06) — an equality test compiles untouched against a
  new variant; a type change does not. This is the basis for the 999.77 `Option<u32>` resolution.
- **Fail-soft preflight** (20d D-06) — an absent tool yields `Unknown`, never a hard-fail
  `NotViable`, and never a crash. D-02 and D-03 both stay inside this.

### Integration Points

- `SigningViability` is consumed in exactly one place, `commands.rs:2380-2400`, which maps its three
  variants onto `release --check`'s `Check` output. The enum's shape is a de-facto output contract.
- `phase_commit_count` has two production consumers — `evaluate_layer2` and
  `handle_validate_outcome`'s forward-progress check. A return-type change touches both, by design.
- `select_loop_back_fix` is called from three arms of `handle_validate_outcome`, all reading the
  single `evidence_root` binding established at the top (WR-08). 999.79's staleness check belongs
  inside `phase_verification_exists` or `select_loop_back_fix`, not duplicated per arm.

### Traps

- **`ssh-add -l` exiting 0 does not mean the agent holds *your* key.** Verified live on this host
  during discussion: `ssh-add -l` exits 0, the configured signing key is **not** among the listed
  fingerprints, and `ssh-keygen -Y sign` with that key still exits 0 — because the unencrypted
  private-key sibling is on disk. That is the live false negative, reproduced. Any test asserting
  `Viable` must not depend on agent membership.
- **`ssh-keygen -Y sign -f <pub>` resolves the private key by stripping `.pub` from *that path*, or
  via the agent.** A public blob copied to a new location has neither unless the agent holds it.
  This is why D-03's rejected alternative needed the agent, and it is a trap for any test fixture
  that copies key files around.
- **A single-cycle 999.77 test passes against both the buggy and the fixed code.** Stated in the
  backlog entry. The two-cycle sequence is the only discriminating test.
- **A too-strict 999.79 freshness rule silently regresses Phase 33's `--gaps-only` path.** Test both
  directions.
- **`advance()` is `pub(crate)`** — no test in `crates/devflow-cli/tests/` can call it.

</code_context>

<specifics>
## Specific Ideas

### Measurements taken during discussion (reproduce rather than trust)

All run on the operator's host, 2026-08-06, against the real configured signing key. Recorded
because they establish the 999.86 design and one of them refutes a claim made earlier in the same
discussion.

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

**Self-correction recorded because the error is repeatable.** An earlier reading of the first row
attributed the success to the ssh-agent. It was the on-disk private-key sibling; the agent does not
hold the configured key at all. The negative control (garbage blob agreeing with the positive case)
is what exposed it. A probe design that assumed agent membership would have been wrong in exactly
the direction that produced the original defect.

**What these do not establish:** n=1, one host, one OpenSSH build, one key type (ed25519). They fix
the *shape* of the design; they are not a claim about behaviour across OpenSSH versions or key
types, and the phase's own tests should not cite them as coverage.

</specifics>

<deferred>
## Deferred Ideas

- **999.76's open question — whether the workspace needs a real linked `git worktree` integration
  harness**, motivated by `phase_commit_count`'s shared-refs property. D-05 declines it for this
  phase's purposes. Stays open. Note that its framing needs correcting first: real `git worktree
  add` fixtures already exist in three places (see D-05), so the question is "should the
  999.76-touched tests use one", not "should the workspace build its first".

- **Richer `NotViable` diagnostics for `release --check`** — D-02 accepts a real loss of
  actionability on the failure path. If that bites in practice, the rejected option (retain
  `ssh-add -l` for prose only, never for a verdict) is the recorded way back, and it does not
  reintroduce the defect because it cannot produce a verdict.

- **Probing inline `key::` signing keys** — D-03 declines it, and it was *measured working* first,
  so this is a surface-cost deferral rather than an open feasibility question. Reopening it needs
  only a temp file and a cleanup path.

- **999.85** — two comments justifying themselves by a mechanism Phase 34 deleted. Explicitly Out of
  Scope in `REQUIREMENTS.md`; low severity, no functional risk. Note the overlap: 999.85's F-34-01
  concerns `idle_timeout_result`'s doc comment in `agent_result.rs`, a file this phase edits for
  999.77. Editing it is still out of scope — leave it.

- **DEN-50 — `devflow release`'s real signing executor.** Unaffected by 999.86 and still separate.
  The executor must still run the real signed `git tag`, not call this probe as a substitute.

- **Any defect these fixes reveal** — filed as a numbered `999.x` entry plus a Linear issue, not
  fixed in-phase (34/D-04).

</deferred>

---

*Phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-79-999-84-999-86*
*Context gathered: 2026-08-06*
