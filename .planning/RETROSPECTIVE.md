# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: gsd-hygiene — GSD Workflow Hygiene

**Shipped:** 2026-08-04 (unversioned — pure `.planning/` documentation, no crates code, nothing
published to crates.io)
**Phases:** 1 | **Plans:** 1 (backfilled) | **Sessions:** 1

### What Was Built
- Confirmed `.planning/ROADMAP.md`'s layout — the active milestone's `### Phase N:` heading and
  `## Progress` table living inside its own heading-to-next-heading window — was already correct,
  closing backlog 999.72/999.72a (`roadmap.analyze` phase_count:0 misfire; `milestone.complete
  --dry-run`'s pass-all degrade).
- Closed the loop: flipped 999.72/999.72a's backlog status labels, added a CLAUDE.md convention
  documenting the two root causes for future milestone boundaries, and independently re-verified
  the phase goal via a standalone `gsd-verifier` run rather than trusting the prior claim.
- Resolved an unrelated open item found during close (`stale-blockers-gate-gsd-next` debug
  session) — its STATE.md fix was already live from an earlier session, and its coupled
  "isComplete two-scale bug" concern is now genuinely resolved as a side effect of this
  milestone's own fix.

### What Worked
- **Live re-verification instead of trusting a prior session's claim.** STATE.md asserted the
  ROADMAP fix was "verified live" before this session started; re-running the same three checks
  independently (not just reading the claim) is what surfaced that the milestone-close workflow's
  own readiness gate would still misfire for a different reason (issue 18 below) — trusting the
  first claim would have missed it.
- **`--dry-run` before every write operation that archives or restructures.** `milestone.complete
  --dry-run` correctly previewed the exact scope (1 phase) before the real run touched anything;
  used the same discipline for `phase.complete` and `roadmap.analyze` throughout.

### What Was Inefficient
- **Two `git mv`/rename operations left their "delete" half uncommitted.** The `commit` query
  wrapper, when passed only the new path, staged the add but left the pre-staged deletion of the
  old path sitting uncommitted in the working tree — twice (the debug-session file move, and the
  phase-directory archival). Caught both times by checking `git status` after, not by the wrapper
  itself. Worth remembering for any future `git mv` in this project: verify `git status` is clean
  after, don't assume the wrapper captured the whole rename.
- **A tooling gap (gsd-core issue 18) required a workaround mid-close.** Discovering that
  `init.manager`'s `phase_complete` predicate hardcodes `false` for any zero-plan phase — and that
  `/gsd-complete-milestone`'s readiness gate depends on exactly that field — cost real
  investigation time mid-session rather than being known going in. The workaround (an
  explicitly-labeled backfilled PLAN/SUMMARY pair) was operator-approved rather than silently
  applied, which was the right call, but a milestone with more zero-plan phases would hit this
  every time.

### Patterns Established
- **Backfilled artifacts must be labeled as backfilled, in their own body, not just in a commit
  message.** Both `32-01-PLAN.md` and `32-01-SUMMARY.md` open with an explicit "this is backfilled,
  not prospective" statement — matches the precedent Phase 22's backfilled SUMMARY/VERIFICATION
  set, and keeps provenance legible to any future reader who doesn't have this session's context.
- **Unversioned milestones use a plain label, not a fabricated version.** `gsd-hygiene` archived to
  `.planning/milestones/gsd-hygiene-ROADMAP.md` (not `v0.0.0` or similar), and skipped the
  `git tag` step entirely — an operator decision, made explicit rather than assumed, when
  `/gsd-complete-milestone`'s own contract expected a real `v[X.Y]`.

### Key Lessons
1. A tool's own success on a `--dry-run` or a downstream command (here, `phase.complete`
   succeeding) does not guarantee every OTHER code path that reads the same state agrees — verify
   the specific field the next workflow step actually gates on (`init.manager`'s
   `phase_complete`), not just a nearby command's exit code.
2. When a milestone-close workflow assumes shipped-and-versioned by default, an intentionally
   unversioned/docs-only milestone needs its own explicit handling for every version-shaped field
   (archive filename, git tag) rather than silently substituting a placeholder.
3. `audit-open`'s "1 open item" can be stale bookkeeping, not a live blocker — but the fix is to
   verify that directly (re-run the underlying check, read the actual file) and update the
   session's own status frontmatter, not to just re-acknowledge-and-defer indefinitely.

### Cost Observations
- Model mix: not tracked this session
- Sessions: 1 (interactive, `/gsd-discuss-phase 32` through `/gsd-complete-milestone`)
- Notable: this milestone's entire scope was closed within a single session because the
  underlying fix had already landed — the actual work was verification, bookkeeping, and closing
  two independently-discovered gaps (the debug session and the new gsd-core issue), not
  implementation.

---

## Milestone: v2.4.0 — Resume Unattended Dogfooding

**Shipped:** 2026-08-06 (planning close; not released)
**Phases:** 2 (33, 34) | **Plans:** 12 | **Tasks:** 25

### What Was Built

The structural defects blocking unattended multi-wave `devflow start` runs. Phase 33 fixed the
Code↔Validate loop's two failures: `consecutive_failures` never resetting on real progress
(999.66), and loop-back fix selection reading the main checkout instead of the worktree (999.65).
Phase 34 closed the Validate trust boundary — the status-gated verdict graft plus an exhaustive
classifier naming all seven `AgentStatus` variants (999.74) — widened all five stages onto the
stream-json launch path on real production captures (999.73), and made Layer 0's declaration
discovery worktree-aware (999.76).

### What Worked

**Negative controls, applied as a standing habit rather than on request.** Nearly every claim in
this milestone shipped with a case that had to produce the opposite result: the graft fix's
positive half (`layer0_verdict_graft_still_transplants_a_passing_layer1_verdict`) proving the
filter is not indiscriminate; the PII scan's `linuxbrew` control proving the scanner functions
before its zero was believed; the root-sensitivity test pair each asserting the opposite root. The
milestone's most valuable finding came from exactly this discipline.

**Self-disclosed gaps beat discovered ones.** Plan 34-04 reverted its own fix and re-ran the full
suite to prove its coverage gap was real, then said so in its SUMMARY. The verifier reproduced it
independently rather than accepting the claim. Nothing about criterion 6b had to be caught later.

**The capture campaign was allowed to refute its own premise.** Zero `background_tasks_changed`
events across 1063 events despite 8 sub-agent dispatches — the opposite of what the widening
decision assumed. It was filed as 999.83 and reported near the top of the verification, not buried.

### What Was Inefficient

**A prohibition protected text while the claim underneath it rotted.** Criterion 5 forbade editing
`idle_timeout_result`'s doc comment. The phase honoured that exactly — and its own 34-01 and 34-03
fixes invalidated both mechanisms the comment cites. Nobody noticed until the security audit at
close. A "do not edit" constraint on a comment needs a paired check that the comment is still true
after the phase's other changes land.

**"Correct by construction" accumulated without a guard.** 999.76's second call site is right, and
nothing would catch it going wrong. The phase knew this, recorded it honestly, and shipped anyway —
which was the correct call — but the item then rode all the way to milestone close as the sole
blocker on a requirement's traceability row.

### Patterns Established

- **Mitigated-by-construction vs. mitigated-by-demonstration** is now an explicit distinction in
  threat registers (`34-SECURITY.md`, T-34-04-04), not an implicit one.
- **Audit Limitations sections.** `34-SECURITY.md` states what its ASVS L1 depth does *not*
  establish, including a measurement the auditor got wrong and corrected mid-run. A security
  document that hides its own corrected measurement is worth less than one that shows it.
- **Per-stage evidence needs a discriminating observation, not a completion.** Each capture's
  `run.log` records what distinguishes it from a legacy single-document run, and explicitly states
  that "the stage completed" is not that observation.

### Key Lessons

1. **A passing test suite is not coverage of the thing you changed.** Reverting the criterion-6b
   argument left 279 tests green. The suite's size was never evidence about that line.
2. **A live run only counts if its configuration can discriminate.** The Phase 34 capture campaign
   ran `--no-worktree`, which collapses `execution_root` to `project_root` — structurally unable to
   test the worktree fix, no matter how real the run was.
3. **Fixing a defect leaves stale descriptions of it behind.** 999.85 exists because two comments
   still explain a mechanism this milestone deleted. Worth a sweep at the end of any phase that
   changes a documented invariant's route.

### Cost Observations

- Sessions: milestone spanned 2026-08-04 → 2026-08-06.
- Notable: Phase 34's live capture campaign declared ~8.2 USD across five stage captures (the CLI's
  own `total_cost_usd`, recorded as reported), dominated by Code at 6.10 USD / 49 turns / 695s.

## Milestone: v2.6.0 — Multi-Agent Adapter Migration

**Shipped:** 2026-08-16
**Phases:** 2 | **Plans:** 6 | **Tasks:** 8

### What Was Built
- Pi as a fourth, selectable agent (`AgentKind::Pi` + `pi auth check` health, `--no-approve`).
- Deterministic release signing (999.104) + `release --check` version-bump row (999.96).
- The modular `AgentDriver` contract (9-method) replacing `AgentAdapter`, with `StageIntent`
  de-Claude-ification that fixed Codex's `/gsd-*` defect; Pi as the second native driver; a
  `test_contract()` conformance suite.

### What Worked
- The adversarial-review gate (plans + code) caught four renderer regressions and a hardcoded path
  the automated suite missed — remediating before ship was cheaper than a post-release fix.
- The DriverShim kept Claude/OpenCode byte-identical through a full contract swap — the
  zero-regression bar held.
- Spawn-verifying CLI flags against the installed binaries (`-a never` for codex,
  `--no-approve`/`--no-refresh` for pi) caught a flag form the 999.31 audit had assumed.

### What Was Inefficient
- The first `render_workflow_style` draft was too generic — it dropped the per-stage contracts
  (Validate verdict, Ship gate, Define no-op, Plan idempotency). The code review caught it; a
  negative-control conformance test up front would have caught it at write time.
- The worktree-vs-container gitdir limitation forced `DEVFLOW_SKIP_CONTAINER_CHECK=1` on every
  push — CI re-verified, but local fast-feedback stayed host-only.

### Patterns Established
- Driver-owned prompt rendering: `StageIntent` (data) + per-driver `render_prompt` (syntax); no
  shared prompt.
- Conformance suite (`test_contract()`) with a deliberately-broken negative control.
- Per-driver `workflow_root` (Codex vs Pi install dirs).

### Key Lessons
- A "generic" renderer that drops stage contracts is worse than a per-stage renderer that shares
  boilerplate — preserve the contracts, parameterize the rest.
- Enumerate call sites before deciding a trait removal is "conditional" — the deferral (999.106)
  was honest but the blast radius was only visible after the code review.

### Cost Observations
- Adversarial reviews: 2 (plan + code), each claude(opus) + codex + antigravity.
- Notable: antigravity's timeout root cause was MCP servers hanging, not prompt size — recorded in
  the review skill.

## Milestone: v2.7.0 — Pi End-to-End + Driver Contract Completion

**Shipped:** 2026-08-18 (milestone closed; NOT released — `Cargo.toml` still `2.6.0`, no tag yet)
**Phases:** 3 | **Plans:** 2 | **Tasks:** 15

### What Was Built
- 37.1 (research spike): a **VIABLE** verdict for `@bacnh85/pi-subagent` — primary-source research overturned the original NOT VIABLE verdict (which read zero lines of source).
- 38: `AgentAdapter`/`DriverShim` deleted, all call sites migrated to `AgentDriver`, `InteractivityMode` wired (Define/Plan), 999.107 fixed (`turn.failed` precedence + non-UTF-8 writable-root refusal).
- 39: Pi end-to-end — provider-aware health, `Legacy` launch, vetted capability detection, and a live subagent-dispatch run captured as a transcript.

### What Worked
- The adversarial code review (FIX-FIRST) caught the provider-fix BLOCKER — `models.json`-based probing false-rejects standard installs and false-greens any-ready providers — before ship.
- Re-running the e2e with the session transcript captured (the `toolCall: subagent` → nested `bash` → `DEVFLOW_RESULT` chain) replaced a proxy-only first smoke that recorded a bash side-effect file.
- Backfilling formal SUMMARY/VERIFICATION artifacts for already-complete phases (37.1, 38) let the milestone close against real artifacts rather than an override.

### What Was Inefficient
- The first e2e smoke's "dispatch proof" was a bash side-effect file the parent's own `bash` tool could produce, and it ran on the wrong provider (the throwaway profile lacked `settings.json`). Needed a full re-run.
- The first provider fix read `models.json` instead of `settings.json` — caught by review post-hoc rather than at planning time.
- `phase.complete` / `roadmap update-plan-progress` re-injected stray blank lines into ROADMAP prose (hand-fixed twice), and `phase.complete` misplaced a STATE.md frontmatter field.
- A zero-plan research phase (37.1) can't project `phase_complete: true` in `init.manager`, so a genuinely-complete phase still needed a backfill to satisfy the close.

### Patterns Established
- Provider-aware health: probe what `build_command` actually uses (`settings.json` `defaultProvider`), not a catalog of "anything that could work".
- Live e2e evidence = the captured session transcript (tool-call nesting), never a side-effect file.
- Research-phase summaries point at the decision gate rather than restating it.

### Key Lessons
- "Any ready provider" is a false-green and "no models.json" is a false-reject — probe the active provider, fall back to the built-in default, never refuse on a missing catalog.
- A file written by bash does not prove a tool was dispatched; capture the transcript.

### Cost Observations
- Adversarial reviews: 1 code review (claude/codex/antigravity), plus the earlier planning review.
- The live dispatch run: parent on `litellm` (deepseek-v4-pro), subagent on `openrouter` (free nemotron) — the subagent's model is the extension's own chain, not the parent's provider.

## Milestone: v2.8.0 — Remaining Harness Support + Pi Dogfood

**Shipped:** 2026-09-03 (closed `override_closeout`) — released incrementally v2.9.0 … v2.12.0
**Phases:** 6 (40-45) | **Plans:** 16 | **Tasks:** 26

### What Was Built
- 40: Pi (shipped v2.7.0) dogfooded through a real supervised Define→Validate run with a witnessed reviewer-subagent dispatch and a live gate; 999.85 stale comments rewritten (MAINT-01); 3 Pi-transport regression tests.
- 41: Antigravity CLI driver — `agy` headless (stream-json, `--print-timeout 60m`, prompt on stdin), agent-aware `CloseRule`/idle-timeout, conformance-enrolled. Plus HYG-01 (phase-7 tests reap their own monitors: 0 leaked, was 43) and HYG-02 (`check-in-container.sh` under uid 0 from worktree + main).
- 42: Hermes driver — `hermes -z … --yolo --accept-hooks` on a Legacy monitor, process-exit + `DEVFLOW_RESULT` contract. The Antigravity supervised dogfood in this phase unlocked `--mode auto` for Antigravity at the C2 gate (ANTG-04); idle floor raised to 300s after a ~163s quiet gap killed a healthy run.
- 43: OpenCode stub completed (28 → 569 lines): real launch argv, `parse_opencode_event_result` (torn-tail → error-anywhere → last-marker) against 3 real vendored captures, fail-closed `health` (exit-success AND positive credential count), header-anchored `capabilities` probe. Code review found 4 fail-closed gaps — all fixed same-phase.
- 44: Codex verified end-to-end — a real `devflow resume --phase 900 --agent codex` drove Code→Validate to a clean finish; the run surfaced stale `phase7_cli.rs` / `pre_push_signing_policy.rs` assertions (closed); driver contract byte-unchanged (D-04); 7 post-hoc review findings all fixed. Shipped v2.11.0.
- 45: Unattended `--mode auto` hardening — `config::base_branch` resolver (env > file > `develop`) resolved once and fed to both the worktree fork point and the git-flow merge target (AUTO-01); `affects_compiled_binary` scoped to `crates/*` + root build files (AUTO-02); shared `CODE_STAGE_POLICY` decision-checkpoint constant (DECN-01, partial). Shipped v2.12.0.

### What Worked
- The modular `AgentDriver` contract held: two brand-new drivers (Antigravity, Hermes) onboarded with no agent-specific logic leaking into core; the 6-driver conformance suite plus a `BrokenDriver` negative control kept it honest.
- Completion parsers regression-tested against real captured streams vendored byte-identical as fixtures, never an assumed schema (OpenCode's 3 captures; Codex's dogfood evidence).
- Verifying Codex with a real dogfood run — not only unit tests — surfaced stale test assertions a green `cargo test --workspace` was hiding.
- Phase 45's one-value base resolution caught a dangerous regression it introduced itself (`cleanup_merged` would delete a protected trunk) before merge, via a negative-controlled test.

### What Was Inefficient
- `workspace/denniyahh` drifted 21 commits behind `develop` — the entire v2.12.0 release was absent from the branch `/gsd-audit-milestone` was invoked on. The integration checker had to verify Phase 45 against a throwaway `develop` worktree. "Sync the personal branch before phase work" is a documented rule that was skipped.
- The REQUIREMENTS.md register lagged reality: CODE-01 + HRMS-01/02/03 stayed `[ ]` / "Pending" long after they were verified and shipped; a Phase-44 VERIFICATION.md even asserted the CODE-01 checkbox was `[x]` when the file read `[ ]`.
- Nyquist coverage was never reconciled during execution: Phase 42's VALIDATION.md stayed `status: pending`, Phase 44's was a `44-XX-XX TBD` skeleton, Phase 45 had none. All three had to be audited/reconstructed at milestone close.
- DECN-01 shipped partial. Both external review lanes REJECTed the Phase 45 plans across two rounds and flagged the prompt-coverage shape; the policy still went out wired to only 3 of 4 Code-prompt paths (missing the primary agent's post-Validate-failure loop-back) and was deferred to 999.115/999.116.

### Patterns Established
- New driver checklist: a `driver_for` arm in a *total* match (no `_ =>`), FromStr/Display/serde round-trip tests, conformance enrollment, a `phase7_cli.rs` marker-less-never-advances regression, and agent-aware `CloseRule`/idle-timeout **only** if the driver runs `PipeOwning`.
- One config value feeds both the worktree fork point and the git-flow merge target, resolved once and persisted, so they cannot drift.
- Dogfood a driver end-to-end before calling it verified — the run finds what the suite doesn't.
- Nyquist VALIDATION.md is reconciled by `/gsd-validate-phase` at execution time, not left as a plan-phase skeleton for the milestone close to rebuild.

### Key Lessons
- A green `cargo test --workspace` over a driver that was never dogfooded proves the parser, not the pipeline.
- "Deferred by operator decision" changes who owns the remaining work, not whether the criterion is true — DECN-01's roadmap criterion is still false on the primary agent's path.
- Sync `workspace/denniyahh` before any phase or audit work: a stale personal branch runs the tooling against a tree missing the milestone's own shipped code.
- Constant-pass checks keep recurring (`--exact` matches nothing and exits 0; `rg -c` prints nothing and exits 1 on zero matches). Assert on a real `N passed` with non-zero `filtered out`, and print the command's own exit code.

### Cost Observations
- 6 phases across ~2 weeks; released incrementally v2.9.0 … v2.12.0 rather than one milestone drop.
- Adversarial review used on Phase 45 plans (2 rounds, both external lanes REJECT) and Phase 44 post-execution (3 external reviewers + canonical `/gsd-code-review`).
- Milestone close did real remediation work: branch sync, register reconciliation, 3 Nyquist reconstructions, 2 new backlog filings (999.120/999.121).

## Milestone: v3.0.0 — Unattended Run Survivability

**Shipped:** 2026-11-08 (closed `override_closeout`) — Phases 46-50.1 released as 3.0.0 on 2026-11-03/04; Phase 51 merged to `develop`, 3.1.0 skipped
**Phases:** 7 (46, 47, 48, 49, 50, 50.1, 51) | **Plans:** 120 | **Tasks:** 246 | **Timeline:** 2026-09-05 → 2026-11-07 (1,555 scoped commits)

### What Was Built
- 46: the sequential 2-CPU CI job, `base_branch` validated as a ref not a revision, empty-`PATH` test windows moved into child processes, the plan-bashism scanner over staged bytes.
- 47: one `CODE_STAGE_POLICY` on every Code prompt including the Validate loop-back; a resumed agent's reasoning no longer reads as a new blocking-human checkpoint; `insta` baselines for all six adapters.
- 48: collision-retrying state and gate writes, first-answer-wins gate responses, `start`/`resume` refuse beside a live run under the phase lock, `recover --clean` serialized, honest no-waiter gate recovery.
- 49: the live `--mode auto` run on a throwaway project (attempt 3 shipped), the evidence collector, sanitizer and checker, the gate-notify snapshot hook.
- 50: the socket-addressed supervisor (ALIVE / STALE / GONE, no pid read), one supervisor body for both launch shapes, the pidfd backstop, `stop` through the socket, `monitor_pid` deleted from the state file.
- 50.1: one short OS lock per state write with a re-read, gate ids with quarantined wrong answers, pending-supervisor liveness lock, superseded supervisors end their agent, 30 plans of review residuals and e2e hygiene.
- 51: a Code-stage quota denial hands the run to a configured fallback agent in the same worktree; a torn final capture line outranks a parsed denial; idle-timeout failures carry their cause; handoff events.

### What Worked
- Live runs as the instrument: Phase 49's throwaway-project rig was reused for Phase 50's fault arm, Phase 50.1's attempt and the 2026-11-08 failover probe, so every survivability claim has at least one real record.
- Dogfooding Phase 51 through DevFlow itself produced 35 backlog entries (999.183-217) in four days — more defects than any review round, and each with a reproduction.
- The adversarial-review loop (GSD review → fix → external lanes → fix) kept finding real defects in the failover path: the torn-line ordering (X-01), the worktree-arm test gap (X-03), the stale evidence citations (X-04).
- Negative controls settled arguments that reading code could not: the escaped-stdout-holder case (999.209) and the Phase 46 2-CPU race were each confirmed by a paired run that had to produce the opposite result.

### What Was Inefficient
- Seven phases, 120 plans and 1,555 commits were spent making the tool survive itself; no phase in this milestone shipped software for anyone else, and Phase 51 by DevFlow needed four by-hand exceptions that a plain GSD session would not have.
- Five of seven verifications were `stale` at close because later phases edited covered files; the digest mechanism (gsd-core#4623) turns every follow-on phase into an override at milestone close.
- GSD's milestone-close audit read a resolved entry's evidence tables as ten open deferred items and could not acknowledge them; the close needed a hand edit and an incident entry.
- The supervise-mode gate loop cannot carry an operator's note to the agent, re-runs the whole phase on a reject, and cannot close a `gaps_found` verification (999.189, 999.192, 999.205, 999.208) — the reason every fix round ran outside DevFlow.
- Define launches a paid agent session that does nothing (999.214); Ship pushes before its gate fires, so a by-hand close-out can only end a passed run with "abort" (999.210).

### Patterns Established
- A throwaway GSD project with a one-task phase and a wrapper agent is the cheapest live test of a supervisor seam; the synthetic denial copied a live event's shape with one field flipped.
- Every dogfood defect is filed as a `999.x` entry in the same session with evidence, what is not established and a fix shape; hand-run stages are recorded as numbered exceptions in the phase's run notes.
- Reports to the operator sort into decision / awareness / handled, lead with impact, and name what a check does not establish.

### Key Lessons
- The product is the repository as the session: codex finished Claude's interrupted Code stage because GSD had written the state to disk, not because DevFlow carried context. Build on that; do not duplicate what GSD and Claude Code already orchestrate.
- "Passed" on stub agents is not "works": the headline feature had never run against a real agent until the probe, and the only real denials on record (Phase 43, Phase 51) were classified live but never archived.
- A monitoring filter that buffers (`cut` after `grep` without line buffering) delivers nothing; a gate fired 23 minutes before anyone saw it.
- The ratio of backlog entries about the tool to work the tool shipped for others is the number to watch; for this milestone it was effectively infinite.

### Cost Observations
- Model mix: Opus for orchestration, planning and verification; Sonnet executors; codex, DeepSeek and Gemini as external review lanes; codex as the live fallback agent.
- Sessions: multi-session across ~9 weeks; the Phase 51 close-out, probe, inventory and milestone close ran in one session on 2026-11-08.
- Notable: two full `scripts/check.sh all` runs per fix round (~2,125 tests) plus three CI rounds per PR; the external review's agy lane timed out at 30 minutes and returned partial findings.

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| gsd-hygiene | 1 | 1 | First milestone closed by an interactive session end-to-end same-day; first use of a backfilled PLAN/SUMMARY pair to satisfy a completion-projection tooling gap; first unversioned/plain-label milestone archive |
| v2.7.0 | 1 (multi-session) | 3 | First milestone driven end-to-end by pi — the agent DevFlow itself now supports; first live subagent-dispatch transcript as e2e evidence |
| v2.8.0 | multi-session | 6 | All 6 harnesses on the `AgentDriver` contract; first driver verified by a real dogfood run rather than unit tests alone; released incrementally (v2.9.0–v2.12.0) instead of one milestone drop; Nyquist coverage reconstructed for half the phases at close |
| v3.0.0 | multi-session (~9 weeks) | 7 | First milestone whose phases DevFlow drove itself (Phase 51 dogfood); live runs as the measuring instrument; the first milestone to close with a strategic re-direction (v4.0 pare-down) rather than a next feature |

### Cumulative Quality

| Milestone | Tests | Coverage | Zero-Dep Additions |
|-----------|-------|----------|-------------------|
| gsd-hygiene | N/A — no crates code | N/A | 0 |
| v2.7.0 | 13 `agents::pi` tests + full workspace + `clippy -D warnings` | N/A | 0 |
| v2.8.0 | 1235 workspace tests, 0 failed; 6-driver conformance suite + `BrokenDriver` control; per-driver marker-less regressions (pi/antigravity/hermes at CLI level) | N/A | 0 (serde/clap/thiserror/tracing only) |
| v3.0.0 | ~2,125 tests in `scripts/check.sh all` (0 failed after round 3); 13 CI checks green on PR #234; two e2e tests flaky under full-suite load (999.188, 999.191) | N/A | 0 (pidfd via libc only) |

### Top Lessons (Verified Across Milestones)

1. Live re-verification of a claimed-fixed condition, rather than trusting the claim, is what
   this project's own history keeps rewarding — repeated across the v2.3.0 close (`milestone.complete`
   bypassed after catching a live over-sweep) and this milestone's close (issue 18 found the same
   way).
