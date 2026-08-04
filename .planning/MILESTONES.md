# Milestones

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
