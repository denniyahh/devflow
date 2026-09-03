# Milestones

## v2.8.0 Remaining Harness Support + Pi Dogfood (Shipped: 2026-09-03)

**Phases completed:** 6 phases, 16 plans, 26 tasks

**Key accomplishments:**

- Backfilled 2026-08-23
- Backfilled 2026-08-23
- OpenCode driver now launches `opencode run "<prompt>" --auto --format json` and its JSONL output resolves through a new `parse_opencode_event_result` (marker, error, torn-tail) regression-tested against three real live captures.
- `OpenCodeDriver::health` now refuses an OpenCode launch on a machine with zero configured provider credentials (parsed from `opencode providers list`, never from exit code), and `capabilities` reports `subagent_dispatch` from a real, fail-closed `opencode agent list` probe.
- `devflow resume --phase 900 --agent codex` drove a throwaway phase's Code and Validate stages to a clean finish (2 real commits, 0-finding review, passing validation), and the phase7_cli.rs and pre_push_signing_policy.rs stale test assertions this run's own regression checks surfaced are both closed with cited commits — CODE-01 is now complete.
- DevFlow now ignores non-workspace spikes for its self-dogfood stale-build block while retaining true-positive member-source and downstream-project detection.
- Full-execute Code prompts now share a merit-based unattended-decision policy across all four supported agent families, with explicit human-only carve-outs and auditable-reasoning instructions.

---

## v2.7.0 Pi End-to-End + Driver Contract Completion (Shipped: 2026-08-18)

**Phases completed:** 3 phases, 2 plans, 15 tasks

**Key accomplishments:**

- Pi subagent dispatch is VIABLE: `@bacnh85/pi-subagent` works as-shipped (synchronous, in-process, fails closed headless), so the full-dispatch arm is re-scoped to `MonitorLaunch::Legacy` with no `CloseRule`/drain gate.
- Deleted the legacy `AgentAdapter` surface, migrated all call sites to `AgentDriver`, wired the driver-driven `InteractivityMode` gate, and fixed both 999.107 Codex-parser defects — Claude launch argv byte-identical, 633 + 322 tests green.
- Pi driver health reads the active provider from settings.json (google fallback), Pi pinned to Legacy launch with a discriminating regression test, and capability detection matching only the vetted @bacnh85/pi-subagent — with a FIX-FIRST adversarial review closed.

---

## v2.6.0 Multi-Agent Adapter Migration (Shipped: 2026-08-16)

**Phases completed:** 2 phases, 6 plans, 0 tasks

**Key accomplishments:**

- 36-01 (tracer, wave 1) — `AgentKind::Pi` + `PiAgent` + preflight + doctor integration.
- 36-02 (wave 2) — 999.104 (deterministic release signing + probe removal) + 999.96
- 37-01 (tracer, wave 1) — `StageIntent` enum + per-driver `render_prompt` for all five stages + `fix_prompt`.
- 37-02 (wave 2) — `AgentDriver` contract + `ClaudeDriver`/`OpenCodeDriver` with zero regression.
- 37-03 (wave 3) — migrate Codex and Pi onto `AgentDriver`; all four agents now resolve through it.
- 37-04 (wave 4) — `test_contract()`, `DriverHealth`/`InteractivityMode`, docs, and the

---

## v2.5.0 Loop-Termination and Release Hardening (Shipped: 2026-08-15)

**Phases completed:** 4 phases, 15 plans, 22 tasks
**Closeout type:** override_closeout — work shipped and released, but Phases 35.1 and 35.2 were
marked complete without formal `-VERIFICATION.md` artifacts (their verification lives in
`-DRILL.md` / `-VALIDATION.md`). Recorded here rather than silently treated as `verified_closeout`.

**Key accomplishments:**

- 999.77 (criterion 1).
- `advance_with_worktree_declared_checkpoint_reads_the_execution_root`
- `release --check`'s tag-signing preflight now signs throwaway bytes with `ssh-keygen -Y sign` and reports the exit code, replacing the `ssh-add -l` predictor that false-negatived live on two release cuts with the correct key present.
- 999.78/WR-01 — the bound.
- 999.79 / criterion 3 — the rule.
- The deliverable here is an enumeration, and the failure mode is enumerating from memory.
- A real `devflow __monitor` process now holds GSD's `workflow._auto_chain_active` true for exactly the lifetime of a `Mode::Auto` Code-stage child, proven by the child's own reading of the config file while it ran, with a supervise-mode arm that reports the opposite.
- A real `SIGKILL` is shown to leave GSD's `workflow._auto_chain_active` set — and the next real `devflow resume` is shown to clear it in the working tree, clear it at the branch tip with a scoped commit, refuse that commit when the file carries an operator's edit, and announce every repair on stdout and in `events.jsonl`.
- An unattended launch is now refused, before any agent is spawned, when the GSD config cannot hold the chain flag, the Code stage would not launch on the pipe-owning arm, or a plan declares a checkpoint no mode auto-approves — with each of the three watched refusing a fixture built to make it refuse, and each paired with a passing counterpart one change away.
- The chain-flag mechanism works. The drill was run end-to-end against a real Claude agent. Auto-approval is blocked by a GSD orchestrator behavior (upstream #3370) that conflates `gate="blocking"` with `gate="blocking-human"` — a narrow edge case in practice because GSD's default `end-of-phase` mode suppresses the checkpoint tasks that trigger it.
- `verification_authored_this_run` now refuses to read provenance from a content fingerprint alone: a `verification_run_nonce` DevFlow stamps per Validate dispatch gates the decision, and its absence forces a full execute — closing 999.89's residual without changing the artifact format or asking GSD to write anything.
- The cross-repo branch-naming coincidence that keeps `{N}-VERIFICATION.md` from being routinely replaced is now pinned by two tests in `phase_id.rs` — and the pin revealed the assumption behind it was already safe: production code was using `padded()` all along.
- complete — 2/2 tasks, 4 captures committed.
- complete — 1/1 task.
- complete — 2/2 tasks.

---

## v2.4.0 Resume Unattended Dogfooding (Shipped: 2026-08-06)

**Phases completed:** 2 phases (33, 34), 12 plans, 25 tasks
**Closeout type:** override_closeout — see Known Gaps below. All phases verified
(`phase_complete: true`, `verification_status: passed`) and the pre-close artifact audit was clear,
but DOGFOOD-04's traceability row remains `Pending`.

**Delivered:** the structural defects blocking unattended, multi-wave `devflow start` runs are
closed — the loop can now survive a healthy 3+ wave phase, the Validate stage's reported outcome
reflects derived status rather than the agent's self-report, and Layer 0 verification is no longer
inert in worktree mode.

**Key accomplishments:**

- **999.66 / DOGFOOD-02 — the loop no longer false-gates on healthy work.**
  `handle_validate_outcome`'s consecutive-failures counter resets on a real new commit and only
  accumulates when a Validate→Code loop produces nothing, so a healthy 3+ wave phase reaches the
  end while a genuinely stuck loop still hits `MAX_CONSECUTIVE_FAILURES`.

- **999.65 / DOGFOOD-01 — loop-back fix selection reads the right root.** `select_loop_back_fix`
  reads `{N}-VERIFICATION.md` from the phase's worktree rather than the main checkout, making
  `FixType::GapsOnly` reachable on the Validate path in worktree mode for the first time. Proven by
  a test that failed with the inverted value against unchanged code.

- **999.74 / DOGFOOD-04 — the Validate trust boundary.** `reconcile_layer0_verdict` now consults
  Layer 1's own status before transplanting its verdict, and `classify_validate_outcome` was
  rewritten as an exhaustive match naming all seven `AgentStatus` variants with no wildcard in the
  status position. An agent-written `verdict: pass` attached to its own `status: failed` no longer
  advances to Ship unattended — the exploit was reproduced against the real cascade before the fix,
  and pinned by a regression test with a matched positive control.

- **999.73 / DOGFOOD-03 — every stream-json stage joined on real evidence.** All five `Stage`
  variants widened onto the stream-json launch path against committed, PII-scrubbed production
  captures with per-stage drain analysis, rather than on assumption. **The campaign refuted its own
  premise** — zero `background_tasks_changed` events across 1063 events despite 8 concurrent
  sub-agent dispatches — and that was filed as backlog 999.83 rather than quietly absorbed.

- **999.76 — Layer 0 works in DevFlow's default operating shape.** External-verification discovery
  reads the execution root, so a correctly-declared probe set no longer silently never executes in
  worktree mode.

### Known Gaps

Recorded rather than waved through — this is an `override_closeout`.

- **DOGFOOD-04 traceability row is `Pending`, not `Complete`.** Criteria 3 and 4 close the core
  self-report-vs-derived-status guarantee directly, with live tests and negative controls. What
  remains is 999.76's second call site — `phase_has_blocking_human_checkpoint` under
  `Action::GateReview` — which is correct by construction but has **no regression guard**:
  reverting its root argument leaves the full 279-test binary suite green. Phase 34's UAT closed
  this by operator attestation rather than demonstration. Tracked as **999.84 / DEN-106**.

- **999.85 / DEN-107** — two in-source comments (`idle_timeout_result`'s doc comment and a residual
  instance inside `stream_success_cannot_stand_against_nonzero_exit_code`) still justify themselves
  by mechanisms this milestone's own fixes deleted. Conclusions correct, reasons stale. Surfaced by
  the Phase 34 security audit.

- **999.83** — the drain gate, the safety mechanism the widened stages' unattended behaviour
  depends on, is currently proven *not* to see sub-agent concurrency on Claude CLI 2.1.222.

- **No `/gsd-audit-milestone` was run** before this close. Requirements coverage was checked against
  REQUIREMENTS.md's traceability table and all phases were independently verified, but the
  cross-phase integration and E2E-flow audit that `/gsd-audit-milestone` performs did not happen.

### Release status

**This milestone close is a planning-state operation only — v2.4.0 is not released.** At close:
the workspace version is still `2.3.0`, `CHANGELOG.md` has no 2.4.0 section, the work sits on
`feature/phase-34` (66 commits ahead of `develop`, unmerged), and no `v2.4.0` tag exists. Cutting
the release requires the project's real release path — PRs through both protected-branch hops, the
version bump, the changelog entry, a signed tag, and publishing core before cli.

---

## gsd-hygiene GSD Workflow Hygiene (Shipped: 2026-08-04)

**Phases completed:** 1 phases, 1 plans, 3 tasks

**Key accomplishments:**

- Confirmed the ROADMAP.md layout fix (HYGIENE-01/02/03) had already landed as a side effect of creating this milestone, then closed the loop: flipped the 999.72/999.72a backlog entries, documented a durability convention, and independently re-verified the phase goal from scratch.

---

## v2.3.0 the unattended run (Shipped: 2026-08-04)

**Phases completed:** 2 phases (30-31), 10 plans

**Key accomplishments:**

- A Claude `--output-format stream-json` JSONL capture now produces a real Layer-1 verdict through `evaluate_layer1` — last-result-wins across turns, with `decided_by_layer` derived rather than trusted — while both capture shapes that ship today are proven untouched by runtime assertion.
- `task-notification` delivery survives DevFlow's production launch environment — 8 trials, 3 environments, 0 refutations — so Phase 31 could be planned; but 8 successes bound reliability only above ~69%, so it was planned with a retry/timeout path, not a happy path.
- Rate-limit classification, `is_error` failure attribution and `session_id` now survive a Claude `stream-json` capture with the single-document path's precedence and read discipline intact.
- The 0.38s exit figure was refuted by direct measurement (169.5–279.7 ms, median 242.0 ms), closing stdin with pending tasks turned out to be benign rather than undefined, and the ~12s idle-timeout floor was raised to ≥30s after 2 of 7 trials showed a quiet gap above it.
- A Claude stage now launches with the bidirectional `stream-json` argv and is supervised by a std-only Rust monitor that owns both pipes, releasing stdin only when a `DEVFLOW_RESULT` marker has landed in a top-level `result` AND the background-task list has drained — making Phase 30's stream parser reachable from a production launch path for the first time.
- A nonce-canary guard now distinguishes "the behaviour is gone" from "the guard could not run," and a stream-vs-exit-code arbitration layer resolves disagreements between the two signals, with `--legacy-claude-launch` as a documented, off-by-default escape hatch.
- Live acceptance run **passed on attempt 1** (D-18: both plans produced a SUMMARY.md and both merged), independently verified from git — two merge commits with independent-forking parents — closing the 999.64 arc that has blocked every prior unattended-run attempt in this project's history.

**Review:** one adversarial plan review + one peer code review across the milestone; 3 CRITICAL/HIGH findings fixed with mutation-proven tests (`522e905`), 1 filed to backlog (999.75, itself resolved 2026-08-04 in `2c20ab4`/PR #82).

**Known verification overrides:** 2 (see `STATE.md` § Deferred Items — an unrelated gsd-core tooling debug session, and a Phase 24 UAT audit false-positive, both acknowledged at close).

Full detail: `.planning/milestones/v2.3.0-ROADMAP.md`

## v2.0.0 (open-ended label, spanned releases 1.2.0-2.2.0) (Closed: 2026-08-02, archived retroactively 2026-08-04)

**Phases completed:** 16 phases (12-25, 27, 28), 125 plans. Phase 26 (closed partial, not shipped)
and Phase 29 (aborted) are explicitly excluded — see `.planning/superseded/26-release-cut-automation/`
and the Phase 29 prose still in `ROADMAP.md`.

**Key accomplishments (representative — see `.planning/milestones/v2.0.0-ROADMAP.md` for full detail):**

- The GSD-native architecture replaced the original tmux-based agent launcher with direct process
  spawning and a monitor daemon, and the CLI surface was rebuilt and then substantially hardened
  across a dozen phases of reliability work (outcome typing, build provenance, preflight gates,
  hermetic git invocation, `main.rs` decomposed from 8,487 to 478 lines with zero behavioral change).

- The `sequentagent` CLI verb was removed as the breaking change the v2.0.0 slot was held open for,
  replaced by `devflow parallel` and a single-agent rate-limit resume path.

- `devflow start --phase N --agent claude --mode auto --yes-ship` reached a completed Ship stage
  unattended for the first time in this project's history (Phase 25), after Phase 23 proved the
  goal unreachable and closed four specific, individually-evidenced blockers.

- A `blocking-human` checkpoint stopped being a dead end for unattended runs — DevFlow can now
  relaunch the exact exited session and resolve the checkpoint itself, with a recorded audit trail
  (Phase 28).

- All 41 production `git` invocations made hermetic against a hostile `GIT_DIR` (Phase 27),
  unblocking both the release executor goal and `devflow sync`.

**Milestone-level decision:** this label was deliberately left open-ended (2026-07-23) rather than
bounded to a phase count, which let it drift across three releases (2.0.0, 2.1.0, 2.2.0) and 16
phases before being closed by operator decision on 2026-08-02. That experience is why the
successor milestone (v2.3.0) was declared bounded from the start.

**Not archived here:** Phase 26 (CLOSED PARTIAL — two review rounds found Critical defects no test
caught; re-opened as backlog 999.25) and Phase 29 (ABORTED after independent cross-AI review found
5 Criticals + 1 High). Neither shipped, so neither belongs in a "shipped" milestone archive.

Full detail: `.planning/milestones/v2.0.0-ROADMAP.md`

## v1.0 Core Workflow, Versioning, and the GSD-Native Rewrite (retroactive label; declared and archived 2026-08-04)

**Phases completed:** 11 phases (1-11), 12 plans (single-plan phases — this project hadn't yet
adopted multi-plan waves).

**No milestone was ever declared for this era at the time** — DevFlow didn't use the "milestone"
concept until v2.0.0 (2026-07-23). This label is a retroactive convenience grouping, applied by
operator decision on 2026-08-04 so this history follows the same archival convention as v2.0.0
and v2.3.0 rather than sitting permanently un-grouped. Not to be confused with the pre-existing
`v1.0-ASSESSMENT.md` (2026-06-17), a one-time feature-comparison snapshot unrelated to this
archive.

**Key accomplishments:**

- Core workflow, versioning, and state machine established (phases 1-5, versions 0.1.0-0.6.0).
- Agent completion protocol, worktree + PR integration, docs/onboarding, and open-source polish
  shipped incrementally (phases 6-9, versions 1.0.0-1.2.0).

- **The GSD-native rewrite (Phase 11)** replaced the original `tmux`-based agent launcher entirely
  with direct process spawning and a monitor daemon — `tmux` stopped being a runtime dependency
  from this point on, and the CLI surface was substantially rebuilt, setting the architecture every
  later phase (12 onward) built on.

Full detail: `.planning/milestones/v1.0-ROADMAP.md`
