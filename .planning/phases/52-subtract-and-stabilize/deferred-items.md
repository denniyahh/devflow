## Deferred Items

- `reset_time::parse_reset_time` accepts a day past the month's end (found in 52-01)
  status: open
  **What:** the RFC 3339 path checks `day <= 31` only, so `2026-02-31T00:00:00Z` reads as 2026-03-03 (probed, equal epoch values). The check came over unchanged from `ship.rs::parse_rfc3339ish`.
  **Impact:** the result is a later instant, never an earlier one, so a relaunch cannot fire before the named time. It is still a date the agent never meant.
  **Fix shape:** refuse a day past the month's length (leap-aware) and add a test for Feb 29 on leap and non-leap years. Fits plan 52-31 or 52-32, where the rate-limit gate consumes the parser.

- devflow-core stray-reaping primitives outlived their only destructive caller (found in 52-03)
  status: open
  **What:** 52-03 deleted `reap_stray_candidates` from devflow-cli. In `crates/devflow-core/src/agent.rs`, `process_age` and `STRAY_MIN_AGE` now have no caller outside core, and the doc comments on `process_age`, `STRAY_MIN_AGE` and `discover_stray_devflow_processes` still name `reap_stray_candidates` and `--reap-strays` as their consumer.
  **Why deferred:** both are `pub` in the published devflow-core, so removing them is a semver change, and agent.rs is not in 52-03's files.
  **Fix shape:** in a plan that touches devflow-core (covered by the 4.0.0 major bump), delete `process_age` and `STRAY_MIN_AGE` if `rg` still finds no caller, and rewrite the three doc comments to name `doctor`'s read-only report as the consumer. Run `cargo semver-checks check-release -p devflow-core` and record the expected `function_missing`/`constant_missing`.

- `gate_sweep_e2e::no_suite_on_the_common_builder_runs_a_real_reap_strays_pass` guards a removed flag (found in 52-03)
  status: open
  **What:** the source scan refuses any test that passes `--reap-strays` without `--dry-run`. Since 52-03, clap rejects `--reap-strays`, so a real pass can no longer run from any suite and the guard protects nothing. It still passes: `phase7_cli::removed_flags_are_unknown` passes the flag together with `--dry-run`.
  **Fix shape:** delete the test, `REAP_STRAYS_LITERAL`, `DRY_RUN_LITERAL`, `real_reap_strays_statements` and `code_only` if nothing else in `gate_sweep_e2e.rs` uses it. `gate_sweep_e2e.rs` was not in 52-03's files.

- Comments and test wording still describe the removed rate-limit cron resume (found in 52-10)
  status: open
  **What:** 52-10 removed the cron-instructions file and every reader. Doc comments that still describe a "resume cron" or "cron arm" as live behaviour remain in `crates/devflow-core/src/agent_result.rs` (about nine, e.g. "drives the primary rate-limit resume cron"), `monitor.rs`, `state.rs` (`fallback_agent` doc, "takes the cron-resume arm"), `reset_time.rs`, `crates/devflow-cli/src/staleness.rs` and `pipeline_outcomes.rs:94`. In `pipeline_launch.rs` tests, names and messages still say "cron arm" (`a_cron_resume_and_an_ordinary_resume_refuse_alike_*`, "a failed relaunch never takes the cron arm (R2-D1)"), and the SUPV-01 evidence check requires a `Cron arm: ` conclusion line.
  **Why deferred:** comments and test wording only; none names the `cron-instructions` token outside comments, so the 52-18 scan does not see them. The test names are cited by the 51-10 SUPV-01 evidence file, so renaming them needs that file changed in the same commit. None of these files except `pipeline_launch.rs`/`pipeline_outcomes.rs` was in 52-10's scope.
  **Fix shape:** reword the comments to say the run waits at the rate-limit gate; rename the `a_cron_resume_*` tests and their `Cron arm` evidence label together with `.planning/milestones/v3.0.0-phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md`, or leave the evidence label as history. Fits 52-18 (the CUT-04 scan plan) or 52-31/52-32.

- `gate approve` still stores a Ship answer when no waiter is registered (found in 52-11)
  status: open
  **What:** `commands.rs::refuse_a_gate_with_no_waiter` keeps its `stage == Stage::Ship ||` bypass, so `devflow gate approve|reject` at a Ship gate with no waiter writes an answer. That bypass existed so `devflow ship` could apply the answer. 52-11 removed `devflow ship` and `stop`'s matching Ship case, so nothing consumes the stored answer now. The operator is told "no registered waiter will act" and named `devflow resume` / `devflow recover --clean`, so no message is false.
  **Why deferred:** about a dozen `commands.rs` and `stop_e2e.rs` tests seed generic gate-approve behaviour at a Ship gate with no waiter (for example `gate_approve_with_no_registered_waiter_does_not_claim_advance`, `gate_respond_answers_a_registered_waiter_and_refuses_when_none_is_registered`, the `gate_sweep_*` Ship cases). Re-seeding them is the `Stage::Ship` removal's work, not 52-11's.
  **Fix shape:** in 52-15 (the `Stage::Ship` variant removal) the bypass and the Ship-seeded tests go with the variant; re-seed the kept ones at Validate with a registered or absent waiter as their subject needs.

- `GitFlow::commit_all` lost its only caller with the hooks (found in 52-12)
  status: open
  **What:** `crates/devflow-core/src/git.rs` `commit_all` was called only from the deleted `hooks.rs`. It now has no caller outside `git.rs`, beside the already-uncalled `branch_tip`, `delete_remote_branch`, `ensure_branch`, `feature_finish`, `release_start` and `release_finish` (measured with a per-name `rg -w` over `crates/` and `tools/` excluding `git.rs`).
  **Why deferred:** all are `pub` in the published devflow-core, and git.rs was in 52-12's scope only for its `hooks::` comments.
  **Fix shape:** in a devflow-core plan under the 4.0.0 bump (52-18's breaking-change check is the natural place), delete each one `rg` still shows uncalled, with its tests, and record the expected `function_missing` from `cargo semver-checks check-release -p devflow-core`.

- Docs-site pages still describe the after-ship hooks (found in 52-12)
  status: open
  **What:** `docs/index.md` ("terminal hooks"), `docs/diagrams/system.md`, `docs/diagrams/module-map.md` (a `hooks` node), `docs/architecture/state-machine.md`, `docs/architecture/overview.md` and `docs/architecture/git-ship.md` still name the hooks module and the Validate→Ship and after-ship hook batches. 52-12 changed only the scoped docs doc_check reads.
  **Fix shape:** 52-18 rewrites the docs site to the kept pipeline and deletes the whole-Ship pages; include these mentions there.

- A finished run keeps `phase_validate_failures`, so the next `start` carries it (found in 52-12)
  status: open
  **What:** `State.phase_validate_failures` is reset only when the state file is removed or a human answers the ceiling gate. Before 52-08 the finish removed the state; since 52-08 `finish_run` keeps it (`finished: true`), and `commands::fresh_state_carrying_phase_failures` copies the total from any persisted state. 52-12 corrected the two doc comments (`state.rs`, `commands.rs`) that said phase completion resets it. Not reproduced end to end: read from source only.
  **Fix shape:** decide in 52-25 (which makes `start` say what it replaces, D-35) whether a `start` over a finished state carries the total or starts at zero, and pin it with a test.

- A CLI test fixture comment still names the removed after-ship hooks (found in 52-13)
  status: open
  **What:** `crates/devflow-cli/src/test_support.rs` `init_repo`'s doc comment says the fixture exists so "the terminal-path hooks (`VersionBump`, `BranchCleanup`) exercised below" have real git plumbing. 52-12 removed those hooks and 52-13 removed `version.rs`; the fixture is still used, but not for that reason.
  **Why deferred:** comment only, and the file was not in 52-13's list; 52-13 rewrote only comments that pointed at the modules it deleted.
  **Fix shape:** reword it to say the fixture builds a main + develop repo with a committed Cargo.toml. Fits 52-18's comment-stripped scan pass.

- No test now pins that `yes_ship` cannot auto-approve a preflight gate (found in 52-13)
  status: resolved
  **Resolution (52-14):** `State.yes_ship`, `--yes-ship`, `DEVFLOW_YES_SHIP` and the toml key are removed, so nothing can pre-authorize any gate and the property has no subject.
  **What:** `preflight::tests::run_preflight_major_bump_gate_not_auto_approved_by_yes_ship` was the test that drove `run_preflight` into its gate with `yes_ship` set and proved the gate timed out instead of approving. It needed the major-bump check to produce a failing reason, so 52-13 deleted it with the check. `unattended_check_is_not_bypassed_by_yes_ship` still covers the unattended check's own predicate, not the gate.
  **Why deferred:** 52-14 deletes `State.yes_ship` and `--yes-ship`, after which the property has no subject.
  **Fix shape:** none needed if 52-14 removes `yes_ship` as planned; otherwise re-seed the test on the interactivity check (Codex + Auto + Define + no CONTEXT.md).

- `run_gate_with_timeout`'s `auto_response` parameter and safe point 5 have no production caller (found in 52-14)
  status: open
  **What:** with `yes_ship` gone, `handle_ship_outcome` passes `None`, and no production call site passes `Some`. The parameter, the safe-point-5 stop check that runs only for an auto-answered gate, and their tests in `pipeline_gate.rs` (`a_stopped_auto_answered_run_sends_no_ship_notify`, `a_pre_authorized_answer_left_unconfirmed_still_decides_its_own_gate`, helper `auto_answered_ship_gate`) are kept, so they now test a path nothing reaches.
  **Why deferred:** removing the parameter touches every call (3 in production code, 10 in tests) and the safe-point numbering, which is beyond 52-14's flag/env/key/field scope; 52-15 removes `Stage::Ship` and `handle_ship_outcome`, the last caller that ever passed one.
  **Fix shape:** in 52-15, drop the parameter and safe point 5 with `handle_ship_outcome`, and delete or re-seed those tests (they drive `Stage::Ship`).
