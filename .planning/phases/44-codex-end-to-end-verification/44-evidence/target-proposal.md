# Phase 44 Plan 04 — Task 1: Dogfood Target Proposal

**Written:** 2026-08-26, from real captured evidence in this directory (`codex-version.txt`,
`git-baseline.txt`, `cargo-test-workspace.txt`, `cargo-test-workspace-nofailfast.txt`,
`cargo-test-codex-driver.txt`, `codex-diff-vs-merge-base.txt`). No content below is
hand-authored fabrication of command output — every command cited was actually run in this
worktree during this task, and its output is captured verbatim in the sibling files.

## 1. Baseline summary

- `codex --version` → `codex-cli 0.150.0`; `command -v codex` →
  `/home/linuxbrew/.linuxbrew/bin/codex`. Binary present and resolvable.
- `git rev-parse HEAD` → `8074d1d12b65d2dc52e429c6a009dac97bdf95ae` (branch
  `worktree-agent-a1a98e3fc918c2fc4`, forked from `feature/phase-44`). `git status --short` →
  clean at capture time.
- `cargo test --workspace` (plain, default fail-fast): devflow-cli's `main.rs` unittests all
  pass (347/347), three integration suites pass (`auto_chain_flag_e2e`,
  `auto_chain_leak_repair_e2e`, plus the earlier `commands`/`pipeline_*` groups embedded in
  `main.rs`), then the run **stops** at `tests/build_provenance.rs`:
  `build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild` panics with
  `copy .planning/UPSTREAM-GSD-ISSUES.md: No such file or directory`. This is the
  project's own documented, worktree-isolation-specific issue (CLAUDE.md § "Where the upstream
  GSD issue ledger lives") — the symlink target is a sibling `gsd-core-personal-workspace`
  checkout that does not exist inside this isolated agent worktree. **Not a Codex-driver
  regression.** Because `cargo test --workspace` fail-fasts on the first failing test binary,
  devflow-core's lib tests (which hold the 5 Codex driver tests) never got a chance to run under
  the plain invocation — see next bullet.
- `cargo test -p devflow-core --lib agents::tests::codex` (the plan's required assertion) →
  **5 passed, 0 failed**: `codex_define_and_plan_require_an_existing_artifact`,
  `codex_disables_signing_via_env_others_do_not`, `codex_and_pi_drivers_reproduce_legacy_behavior`,
  `codex_grants_writable_roots_for_worktree_git_metadata`, `codex_wraps_prompt_in_exec_and_json`.
- `cargo test --workspace --no-fail-fast` (run additionally, to get a complete baseline the
  fail-fast run couldn't provide): devflow-core lib **735 passed, 0 failed** (confirms the 5
  Codex tests plus the full `resume --agent` test set from 44-01: `resume_with_agent_hands_off_
  and_relaunches_under_the_new_driver`, `resume_with_agent_refuses_before_touching_state_when_
  target_cannot_run_the_stage`, `resume_with_agent_preserves_every_state_field_except_agent_and_
  monitor_pid`, `resume_with_same_agent_is_an_ordinary_idempotent_resume`,
  `resume_with_agent_allows_plan_stage`, `resume_with_agent_from_a_rate_limited_state_relaunches`,
  `resume_without_agent_leaves_the_saved_agent_untouched` — all green). devflow-cli's `main.rs`
  **347 passed, 0 failed** again. Several small integration suites all green
  (`agent_kind_antigravity`, `decimal_phase_paths`, `devflow_dir_gitignore`, `monitor_e2e`, etc.).
  **Two failing targets:**
  1. `build_provenance.rs` — the same dangling-symlink issue described above (environmental,
     not a regression).
  2. **`phase7_cli.rs::status_prints_cron_hint_when_cron_instructions_exist` — FAILED.** This one
     is real and directly in scope. It asserts the OLD, now-intentionally-removed string:
     `stdout.contains(&format!("Cron instruction pending (phase 7): hermes cron create
     --from-devflow {}", root.display()))`. D-10 (44-03, commit `d4067db`) deliberately removed
     the unsupported `--from-devflow` flag from `cron_hint_line`'s output — confirmed correct by
     the passing unit test `commands::tests::cron_hint_line_never_emits_the_unsupported_devflow_
     intake_flag` in the very same run. **`phase7_cli.rs` was never updated to match** — it is a
     stale integration-level assertion left behind by 44-03, now failing for exactly the reason
     44-03 intended (the string it's looking for no longer exists). This is a genuine,
     concrete, closeable gap surfaced by this baseline capture, squarely in #148/CODE-01 scope.
     **Flagging for the operator's Task 2 decision rather than fixing it here** — Task 1's file
     scope is `44-evidence/` only, and fixing `phase7_cli.rs` is source-code work.
- `git diff <merge-base-with-origin/develop> -- crates/devflow-core/src/agents/codex.rs` → merge
  base `0e1a94dc3d7e07de108e077a96909abaaef6fa3b`, **zero-line diff**. `codex.rs` is untouched by
  the 44-00/44-01/44-02/44-03 work — D-04 (driver parity guard) holds trivially here, since
  there is nothing to diverge.

## 2. Structural finding: `.planning/` is never on `develop` in this repository — D-02's literal
   "confirmed present on develop" check cannot pass for ANY phase

Checked, not assumed, per the plan's own instruction:

- `.gitignore:46` → `.planning/` is fully gitignored in this repository.
- `git ls-tree -r --name-only origin/develop -- .planning/phases/` → **empty output**. `develop`
  carries zero `.planning/phases/*` paths, for any phase, ever — by this project's own git-flow
  design (CLAUDE.md: planning artifacts live only on the operator's personal
  `workspace/denniyahh` branch, never on `develop` or `feature/*`).
- The gate this feeds, `commands::phase_artifact_on_develop` (`crates/devflow-cli/src/
  commands.rs:90`), runs `git ls-tree -r --name-only develop -- .planning/phases/` and checks
  whether any listed path matches `.planning/phases/{NN}-*/…{suffix}`. Since that tree listing
  is unconditionally empty in this repository, **this predicate returns `false` for every phase,
  unconditionally** — not because any given phase's `-CONTEXT.md` is actually missing, but
  because `.planning/` is never tracked on `develop` here at all.
- Consequence, read directly from `preflight_interactivity_check`
  (`crates/devflow-cli/src/preflight.rs:607`): the refusal only fires when
  `state.mode == Mode::Auto && state.stage == Stage::Define`. So **`devflow start --agent codex
  --phase N` in `--mode auto` at `Stage::Define` will refuse on this repository for every phase,
  regardless of real artifact state.** `Stage::Plan` is NOT gated by this check at all — the
  function's own doc comment says so explicitly ("Plan is deliberately un-gated because PLAN.md
  is an output the phase itself produces") — even though `CodexDriver::interactivity_mode`
  declares `Stage::Plan` as `RequiresExistingArtifact` too. That declaration is currently inert
  for Plan; only Define is actually enforced.
- This is a property of *this repository's* git-flow convention, not a defect in the Codex
  driver or the interactivity gate — but it does mean the plan's literal instruction ("confirm
  artifacts are present on develop") cannot be satisfied for any phase here, and D-02's intended
  protection (never let Codex touch an interactive Define/Plan turn) has to be enforced by
  **operator discipline** (never launch Codex at Define/Plan on this repo) rather than by the
  auto-mode gate, since Plan is unguarded and Define's guard is unconditionally true regardless
  of real state.

## 3. Candidate phase check: Phase 45 (DECN-01) is NOT actually ready

ROADMAP.md names Phase 45 "the obvious candidate," but checked directly against disk (the only
place `.planning/` content genuinely lives, since it's untracked): `ls .planning/phases/` in the
main checkout shows **no `45-*` directory exists at all** — no CONTEXT.md, no PLAN.md, nothing.
Phase 45's ROADMAP entry itself says `**Plans**: TBD`. Picking Phase 45 today would require a
full `/gsd-discuss-phase 45` + `/gsd-plan-phase 45` pass under Claude first (consuming real
backlog-item planning capacity) before any Codex hand-off could even be attempted at Code — this
is not a same-session dogfood target.

## 4. Recommended dogfood shape

Given §2 and §3, the safe, evidence-grounded plan is:

1. **Do not attempt `--agent codex` at Define or Plan**, ever, on this repository — matches D-02
   and avoids the dead-end in §2.
2. **Use `devflow resume --phase N --agent codex`** (#147, already implemented and passing 7/7
   dedicated tests in this baseline — see §1) against a phase that already has a real,
   Claude-authored `PLAN.md` on disk and a saved state at `Stage::Code`. This is exactly D-09's
   motivating case and is not blocked by the Define-only gate in §2.
3. **Target a disposable scratch phase**, not Phase 45 — Phase 45 has zero artifacts today (§3)
   and promoting it now would spend real backlog-item planning capacity just to build a Codex
   test fixture, and any Codex-authored churn on it would need cleanup before Phase 45's real
   work could start. A throwaway phase (e.g. a trivial single-file, single-plan Code-only unit)
   costs nothing to discard and isolates Codex's real output from any phase whose artifacts
   matter.
4. **Exact launch command**, once a target phase has a saved state at `Stage::Code`:
   ```
   devflow resume --phase <N> --agent codex
   ```
   (`crates/devflow-cli/src/main.rs` `Command::Resume { phase, agent, .. }` →
   `pipeline_launch::resume`, confirmed at `main.rs:592-597`.) This mutates only `state.agent`
   and monitor-launch fields per `resume_with_agent_preserves_every_state_field_except_agent_
   and_monitor_pid` (passing, §1) and relaunches under `CodexDriver` — argv
   `codex -a never exec --sandbox workspace-write --json <prompt>` (unchanged per §1's
   zero-diff finding).
5. **Host**: given `.planning/` is untracked and per-worktree here, and per this repo's own
   CLAUDE.md convention ("Create a git worktree for every new phase before doing its GSD work"),
   run against a **dedicated fresh worktree off `develop`**, not the main checkout — the main
   checkout (`/var/home/denniyahh/Github/devflow`) is currently on `workspace/denniyahh` with
   real, valuable local-only planning content that must not be disturbed by a throwaway Codex
   dogfood run.

## 5. Cost if it goes wrong

- **Scoped to a throwaway phase + dedicated worktree**: bounded blast radius. A failed/garbage
  Codex run only pollutes a disposable branch and worktree; `git worktree remove` + branch
  delete cleans it up completely. No real backlog phase is touched.
- **If Phase 45 were used instead** (not recommended, §3): a bad Codex run would leave
  half-authored PLAN.md/CONTEXT.md content mixed with genuine future planning work for a real
  roadmap item, needing careful manual disentanglement before Phase 45 could actually be
  planned for real.
- **Either way**: Codex runs with signing disabled (`GIT_CONFIG_KEY_0/1` overrides, confirmed
  unchanged in §1) and `workspace-write` sandbox scoped to the worktree plus the linked-worktree
  git metadata roots — it cannot push, cannot touch `origin`, and cannot sign commits/tags. Any
  commits it makes stay local until a human explicitly ships them.

## 6. Open item carried to Task 2 (not fixed here, per Task 1's file scope)

`crates/devflow-cli/tests/phase7_cli.rs::status_prints_cron_hint_when_cron_instructions_exist`
(line 787) asserts a removed string and fails on current HEAD (§1). This should be fixed
(update the assertion to match `cron_hint_line`'s current, correct output) as part of closing
out the #148 hardening surface this phase already claims to have addressed — flagging for the
operator's disposition at the Task 2 checkpoint.
