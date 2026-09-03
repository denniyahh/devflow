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

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| gsd-hygiene | 1 | 1 | First milestone closed by an interactive session end-to-end same-day; first use of a backfilled PLAN/SUMMARY pair to satisfy a completion-projection tooling gap; first unversioned/plain-label milestone archive |
| v2.7.0 | 1 (multi-session) | 3 | First milestone driven end-to-end by pi — the agent DevFlow itself now supports; first live subagent-dispatch transcript as e2e evidence |
| v2.8.0 | multi-session | 6 | All 6 harnesses on the `AgentDriver` contract; first driver verified by a real dogfood run rather than unit tests alone; released incrementally (v2.9.0–v2.12.0) instead of one milestone drop; Nyquist coverage reconstructed for half the phases at close |

### Cumulative Quality

| Milestone | Tests | Coverage | Zero-Dep Additions |
|-----------|-------|----------|-------------------|
| gsd-hygiene | N/A — no crates code | N/A | 0 |
| v2.7.0 | 13 `agents::pi` tests + full workspace + `clippy -D warnings` | N/A | 0 |
| v2.8.0 | 1235 workspace tests, 0 failed; 6-driver conformance suite + `BrokenDriver` control; per-driver marker-less regressions (pi/antigravity/hermes at CLI level) | N/A | 0 (serde/clap/thiserror/tracing only) |

### Top Lessons (Verified Across Milestones)

1. Live re-verification of a claimed-fixed condition, rather than trusting the claim, is what
   this project's own history keeps rewarding — repeated across the v2.3.0 close (`milestone.complete`
   bypassed after catching a live over-sweep) and this milestone's close (issue 18 found the same
   way).
