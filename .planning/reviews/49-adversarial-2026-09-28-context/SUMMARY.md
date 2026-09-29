# Adversarial review — re-scoped 49-CONTEXT.md (2026-09-28)

**Target:** `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-CONTEXT.md` at `67d6ff9`
(re-scope commit `066251f`). **Lens:** adversarial. **Snapshot:** `git archive` outside the repository,
external GSD 1.14.0 workflows copied under `_external/`.

| Lane | Model | Result |
|------|-------|--------|
| agy | gemini-3.8-flash-high, high | complete, 8 confirmed / 2 suspected |
| codex | gpt-5.6-terra, high | complete, 8 confirmed / 3 suspected |
| pi | deepseek-flash | complete, 10 confirmed / 4 suspected |

No lane dropped. The only write into the snapshot was a graphify cache stamp
(`graphify-out/cache/last_query_stamp`). Nothing was written to the repository.

Every finding below was re-checked against source by the driving session before it was recorded. The
regenerated `49-RESEARCH.md` (`7821c97`) was written in parallel and independently reached findings 1, 2, 4, 6,
7 and 9.

## Confirmed — CONTEXT corrections needed

1. **D-27's stop mechanism is not GSD behaviour** (all three lanes). A human-action checkpoint's resume
   signal is `"done"`. After the reply GSD verifies if the task specifies it; on a failure it informs and
   waits (`execute-plan.md:346-348`). Any other reply spawns a continuation agent, and the loop ends when the
   plan completes or the user stops (`execute-phase.md:1066-1073`). Resume restarts the first incomplete
   *plan* at Task 1 (`execute-phase.md:1455`). A plan that has made commits and has no SUMMARY is an illegal
   partial state (`execute-plan.md:16-20`). No `stopped:` reply exists.
2. **The Ship merge target is not in `merge_result`** (all three lanes). The event payload is
   `{"merged": …, "branch": branch}` (`crates/devflow-core/src/hooks.rs:201-202,213-214,231-232`), and the
   target appears only in a log line. D-23's "observed from the run's events (`merge_result`)" is false.
3. **`--yes-ship` is not one-shot** (pi). Non-test code only ever sets `state.yes_ship = true`; it is never
   cleared, so every routine Ship gate in the run is auto-approved. The MAJOR gate and the terminal-hook
   retry gate still cannot be auto-approved.
4. **D-28's fix vehicle collides with the phase-worktree guard** (all three lanes). While
   `feature/phase-49` has a worktree, staged `crates/*` on any other branch is refused
   (`scripts/lint-phase-worktree.sh:93-112`). The documented escape hatch is `DEVFLOW_ALLOW_OFF_WORKTREE=1`
   (`:23,40`), never `--no-verify`.
5. **D-06's "(`origin/HEAD`)" is wrong** (pi, codex). `/gsd-ship` resolves its PR base with
   `gsd_run query git.base-branch` (`ship.md:46,373`), and the scaffold's `{"workflow": {}}` config sets no
   value for it. `integration` must also be a local branch at `start` (`commands.rs:405`) and must carry the
   payload phase (`commands.rs:456`).
6. **"Exactly two gates" holds only on the happy path** (codex; pi's caveat). Preflight failures, the
   Validate failure ceiling, a terminal-hook failure and the MAJOR gate each add a gate.
7. **A gate-watcher snapshot cannot capture DECN-03's turn** (all three lanes). The auto-decide resume opens
   no gate (`pipeline_launch.rs:1886-1887`). The resumed capture has to be collected after the run from
   `.devflow/history/` and the Claude transcript.
8. **D-21's citation is off.** `preflight.rs:724-760` is a comment; `preflight_major_bump_check` is
   `:793-848`. The classifier is in `crates/devflow-core/src/version.rs`, which the CONTEXT does not cite.
9. **D-20: DevFlow runs no tests at Validate** (codex, agy). The stage prompts `/gsd-validate-phase`. The
   Validate→Ship `DocsUpdate` hook runs `cargo doc`, which fails soft (`hooks.rs:100-104,237-253`).
10. **D-24 understates outward effects** (codex). Before any deletion, GSD pushes the feature branch, opens a
    PR, pushes a ship-note commit and may push an empty CI commit (`ship.md:190-202,370-374,487-521`).

## Rejected on verification

- **agy #5, "criterion 2's `unattended_config_condition = Holds` is unsatisfiable".**
  `unattended_config_condition` is condition C1, "GSD config can hold the chain flag"
  (`preflight.rs:990-1005`). The blocking-human check is the separate condition C3,
  `unattended_planned_checkpoint_condition` (`preflight.rs:1067-1083`, listed at `:1164-1177`). C1 can hold
  while C3 does not.
- **agy S1, "`capture_retention` defaults to 5".** It is 12 (`config.rs:36`). The eviction concern stands,
  and research recommends 64.
- **pi C7's recommendation to declare `external_verify` in the payload.** External probes are evaluated for
  every stage (`agent_result.rs`, Layer 0 doc), so they would fail Define and Plan before any code exists.
  Research reached the same conclusion independently.

## Planner-level — no CONTEXT change

- D-14: assert the exact refusal bytes, backticks included (`preflight.rs:993-995`), and the absence of any
  other refusal clause. Also assert persisted `base_branch == "develop"` (codex S1, pi S3).
- D-03: the seeded tag must carry `gate="blocking-human"` literally. A bare `type="checkpoint:human-action"`
  still gates preflight, but it never arms the auto-decide resume (`verify.rs:131-134,153-155`;
  `pipeline_launch.rs:1777`) (pi C10).
- D-11: the judging plan must define "reached Ship" on a named artifact (codex S2, pi S1).
- A decimal-phase insert moves `current_phase`, and the worktree guard reads it (agy S2, unverified). Prefer
  `/gsd-quick` with the escape hatch.
