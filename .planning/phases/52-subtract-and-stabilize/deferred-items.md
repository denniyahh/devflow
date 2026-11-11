## Deferred Items

- `reset_time::parse_reset_time` accepts a day past the month's end (found in 52-01)
  status: open
  **What:** the RFC 3339 path checks `day <= 31` only, so `2026-02-31T00:00:00Z` reads as 2026-03-03 (probed, equal epoch values). The check came over unchanged from `ship.rs::parse_rfc3339ish`.
  **Impact:** the result is a later instant, never an earlier one, so a relaunch cannot fire before the named time. It is still a date the agent never meant.
  **Fix shape:** refuse a day past the month's length (leap-aware) and add a test for Feb 29 on leap and non-leap years. Fits plan 52-31 or 52-32, where the rate-limit gate consumes the parser.
  **52-31:** not taken. 52-31 added the gate's timing functions to `reset_time.rs` and records `not_before` from the parsed reset, but did not change `parse_reset_time`. A past-the-month date still reads as a later instant, so `not_before` can only be later than meant, never earlier. Deferred to 52-32, which wires the timer.

- devflow-core stray-reaping primitives outlived their only destructive caller (found in 52-03)
  status: open
  **What:** 52-03 deleted `reap_stray_candidates` from devflow-cli. In `crates/devflow-core/src/agent.rs`, `process_age` and `STRAY_MIN_AGE` now have no caller outside core, and the doc comments on `process_age`, `STRAY_MIN_AGE` and `discover_stray_devflow_processes` still name `reap_stray_candidates` and `--reap-strays` as their consumer.
  **Why deferred:** both are `pub` in the published devflow-core, so removing them is a semver change, and agent.rs is not in 52-03's files.
  **Fix shape:** in a plan that touches devflow-core (covered by the 4.0.0 major bump), delete `process_age` and `STRAY_MIN_AGE` if `rg` still finds no caller, and rewrite the three doc comments to name `doctor`'s read-only report as the consumer. Run `cargo semver-checks check-release -p devflow-core` and record the expected `function_missing`/`constant_missing`.

- `gate_sweep_e2e::no_suite_on_the_common_builder_runs_a_real_reap_strays_pass` guards a removed flag (found in 52-03)
  status: open
  **52-18:** not deleted. The removed-surface scan read its assertion messages as leftovers, so 52-18 split the flag in those messages with `concat!` like the other controls; the test still passes and still guards nothing. Deleting it remains open (file outside 52-18's list).
  **What:** the source scan refuses any test that passes `--reap-strays` without `--dry-run`. Since 52-03, clap rejects `--reap-strays`, so a real pass can no longer run from any suite and the guard protects nothing. It still passes: `phase7_cli::removed_flags_are_unknown` passes the flag together with `--dry-run`.
  **Fix shape:** delete the test, `REAP_STRAYS_LITERAL`, `DRY_RUN_LITERAL`, `real_reap_strays_statements` and `code_only` if nothing else in `gate_sweep_e2e.rs` uses it. `gate_sweep_e2e.rs` was not in 52-03's files.

- Comments and test wording still describe the removed rate-limit cron resume (found in 52-10)
  status: open
  **52-18:** not taken. The scan strips comments by design and finds none of these; the wording is outside 52-18's files. Still open for 52-31/52-32.
  **What:** 52-10 removed the cron-instructions file and every reader. Doc comments that still describe a "resume cron" or "cron arm" as live behaviour remain in `crates/devflow-core/src/agent_result.rs` (about nine, e.g. "drives the primary rate-limit resume cron"), `monitor.rs`, `state.rs` (`fallback_agent` doc, "takes the cron-resume arm"), `reset_time.rs`, `crates/devflow-cli/src/staleness.rs` and `pipeline_outcomes.rs:94`. In `pipeline_launch.rs` tests, names and messages still say "cron arm" (`a_cron_resume_and_an_ordinary_resume_refuse_alike_*`, "a failed relaunch never takes the cron arm (R2-D1)"), and the SUPV-01 evidence check requires a `Cron arm: ` conclusion line.
  **Why deferred:** comments and test wording only; none names the `cron-instructions` token outside comments, so the 52-18 scan does not see them. The test names are cited by the 51-10 SUPV-01 evidence file, so renaming them needs that file changed in the same commit. None of these files except `pipeline_launch.rs`/`pipeline_outcomes.rs` was in 52-10's scope.
  **52-31:** not taken; this plan rewrote `handle_rate_limited_outcome`'s doc (which says no cron file is written, accurately) and touched none of the listed comments. Still open for 52-32.
  **Fix shape:** reword the comments to say the run waits at the rate-limit gate; rename the `a_cron_resume_*` tests and their `Cron arm` evidence label together with `.planning/milestones/v3.0.0-phases/51-rate-limit-agent-failover/51-SUPV01-EVIDENCE.md`, or leave the evidence label as history. Fits 52-18 (the CUT-04 scan plan) or 52-31/52-32.

- `gate approve` still stores a Ship answer when no waiter is registered (found in 52-11)
  status: resolved
  **Resolution (52-15):** the `stage == Stage::Ship ||` bypass in `refuse_a_gate_with_no_waiter` and doctor's Ship-only repair are gone with the variant. The Ship-seeded tests were re-seeded at Validate (with a registered waiter where their subject needs an answer to land) or deleted where the bypass was their subject; the 52-15 SUMMARY lists them.
  **What:** `commands.rs::refuse_a_gate_with_no_waiter` keeps its `stage == Stage::Ship ||` bypass, so `devflow gate approve|reject` at a Ship gate with no waiter writes an answer. That bypass existed so `devflow ship` could apply the answer. 52-11 removed `devflow ship` and `stop`'s matching Ship case, so nothing consumes the stored answer now. The operator is told "no registered waiter will act" and named `devflow resume` / `devflow recover --clean`, so no message is false.
  **Why deferred:** about a dozen `commands.rs` and `stop_e2e.rs` tests seed generic gate-approve behaviour at a Ship gate with no waiter (for example `gate_approve_with_no_registered_waiter_does_not_claim_advance`, `gate_respond_answers_a_registered_waiter_and_refuses_when_none_is_registered`, the `gate_sweep_*` Ship cases). Re-seeding them is the `Stage::Ship` removal's work, not 52-11's.
  **Fix shape:** in 52-15 (the `Stage::Ship` variant removal) the bypass and the Ship-seeded tests go with the variant; re-seed the kept ones at Validate with a registered or absent waiter as their subject needs.

- `GitFlow::commit_all` lost its only caller with the hooks (found in 52-12)
  status: open
  **52-18:** not taken. 52-18's semver run reports only removals already made by 52-01..52-17; deleting `commit_all` and the six other uncalled `GitFlow` functions is a new `pub` removal outside its files. Still open.
  **What:** `crates/devflow-core/src/git.rs` `commit_all` was called only from the deleted `hooks.rs`. It now has no caller outside `git.rs`, beside the already-uncalled `branch_tip`, `delete_remote_branch`, `ensure_branch`, `feature_finish`, `release_start` and `release_finish` (measured with a per-name `rg -w` over `crates/` and `tools/` excluding `git.rs`).
  **Why deferred:** all are `pub` in the published devflow-core, and git.rs was in 52-12's scope only for its `hooks::` comments.
  **Fix shape:** in a devflow-core plan under the 4.0.0 bump (52-18's breaking-change check is the natural place), delete each one `rg` still shows uncalled, with its tests, and record the expected `function_missing` from `cargo semver-checks check-release -p devflow-core`.

- Docs-site pages still describe the after-ship hooks (found in 52-12)
  status: resolved
  **Resolution (52-18):** the whole-Ship pages `docs/architecture/git-ship.md` and `docs/diagrams/ship-flow.md` are deleted; `docs/index.md`, `docs/architecture/overview.md`, `state-machine.md`, `agent-model.md`, `docs/diagrams/agent-lifecycle.md`, `module-map.md` and `system.md` describe Plan → Code → Validate with no hooks, version, lock or ship module. `doc_check::docs_site_describes_only_the_kept_pipeline` pins it.
  **What:** `docs/index.md` ("terminal hooks"), `docs/diagrams/system.md`, `docs/diagrams/module-map.md` (a `hooks` node), `docs/architecture/state-machine.md`, `docs/architecture/overview.md` and `docs/architecture/git-ship.md` still name the hooks module and the Validate→Ship and after-ship hook batches. 52-12 changed only the scoped docs doc_check reads.
  **Fix shape:** 52-18 rewrites the docs site to the kept pipeline and deletes the whole-Ship pages; include these mentions there.

- A finished run keeps `phase_validate_failures`, so the next `start` carries it (found in 52-12)
  status: resolved
  **Resolved (orchestrator, operator decision 2026-11-11):** a `start` over a state recorded `finished` now begins at zero; a stopped, unfinished run still carries its total. `carried_phase_failures` in `crates/devflow-cli/src/commands.rs`, pinned by `commands::tests::a_start_over_a_finished_run_resets_the_failure_total_and_a_stopped_one_carries_it` (RED a9ffa74, then the fix).
  **52-25:** not decided there. 52-25's plan has no truth for it, and whether a new run after a pass starts with a fresh Validate-failure budget changes a safety bound, so it is left for the operator. What 52-25 changed around it: `start` now prints `this start replaces the state of a finished run` before it carries the total, and `cleanup --force` now removes the run's state file with its worktree, which is a second way the total resets. The carry was read from source (`carried_phase_failures` returns `persisted.phase_validate_failures` for any loadable state), not reproduced end to end. Recommendation: start a run over a finished state at zero, which is what the code did before 52-08 kept the state, and pin it in `carried_phase_failures`'s tests.
  **What:** `State.phase_validate_failures` is reset only when the state file is removed or a human answers the ceiling gate. Before 52-08 the finish removed the state; since 52-08 `finish_run` keeps it (`finished: true`), and `commands::fresh_state_carrying_phase_failures` copies the total from any persisted state. 52-12 corrected the two doc comments (`state.rs`, `commands.rs`) that said phase completion resets it. Not reproduced end to end: read from source only.
  **Fix shape:** decide in 52-25 (which makes `start` say what it replaces, D-35) whether a `start` over a finished state carries the total or starts at zero, and pin it with a test.

- A CLI test fixture comment still names the removed after-ship hooks (found in 52-13)
  status: open
  **52-18:** not taken: the scan strips comments, so it does not see this one, and `test_support.rs` is outside 52-18's files. Still open.
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
  status: resolved
  **Resolution (52-15):** the parameter, the safe-point-5 stop check and the auto-answer write are removed with `handle_ship_outcome`; every call site lost its trailing `None`. The two tests and the helper that drove an auto-answered Ship gate (`a_stopped_auto_answered_run_sends_no_ship_notify`, `a_pre_authorized_answer_left_unconfirmed_still_decides_its_own_gate`, `auto_answered_ship_gate`, with `RecordedSupervisor`) are deleted. The safe points are not renumbered: 4 and 6 keep their numbers.
  **What:** with `yes_ship` gone, `handle_ship_outcome` passes `None`, and no production call site passes `Some`. The parameter, the safe-point-5 stop check that runs only for an auto-answered gate, and their tests in `pipeline_gate.rs` (`a_stopped_auto_answered_run_sends_no_ship_notify`, `a_pre_authorized_answer_left_unconfirmed_still_decides_its_own_gate`, helper `auto_answered_ship_gate`) are kept, so they now test a path nothing reaches.
  **Why deferred:** removing the parameter touches every call (3 in production code, 10 in tests) and the safe-point numbering, which is beyond 52-14's flag/env/key/field scope; 52-15 removes `Stage::Ship` and `handle_ship_outcome`, the last caller that ever passed one.
  **Fix shape:** in 52-15, drop the parameter and safe point 5 with `handle_ship_outcome`, and delete or re-seed those tests (they drive `Stage::Ship`).

- The capture-retention arithmetic still counts a five-stage run (found in 52-15)
  status: open
  **What:** the `DEFAULT_CAPTURE_RETENTION` doc in `crates/devflow-core/src/config.rs` and the `prune_history_retains_a_full_five_stage_run_with_loop_backs` doc and name in `agent_result.rs` say a clean Define→Plan→Code→Validate→Ship run makes 4 archive events. Without Ship it makes 3, so 12 now covers four loop-backs with one spare, not zero. The constant is still 12 and the test still passes; only the stated arithmetic is wrong.
  **Why deferred:** 52-17 removes Define and changes the count again; rewriting it twice is waste.
  **Fix shape:** after 52-17, restate the count for the three-stage chain in both docs and rename the test; keep the constant unless the operator wants it changed.

- `devflow doctor` still checks for the `gh` CLI, which nothing runs (found in 52-15)
  status: open
  **52-18:** not taken (outside its files). 52-18 dropped "since the Ship stage was removed" from the quickstart's gh line, which now reads "`devflow doctor` checks for it; no DevFlow command runs it". Still open.
  **What:** `commands.rs` doctor has a `cmd_check("gh CLI", "gh", ...)`. A per-name search for a `gh` invocation under `crates/` and `tools/` finds only that check (the other hits are comments about `gh pr checks`). The Ship stage's `/gsd-ship` ran inside the agent, not DevFlow, and it is gone. 52-15 reworded README.md's and the quickstart's requirement line to say so rather than drop it while doctor still checks it.
  **Fix shape:** drop the doctor check and both requirement lines together, or keep `gh` as an operator convenience and say why. Fits 52-18.

- Comments and a few test names still describe Ship as the stage after Validate (found in 52-15)
  status: open
  **52-18:** not taken. The scan strips comments and matches only the named tokens, so prose and these test names pass it; the files are outside 52-18's list. Still open.
  **What:** prose only, no `Stage::Ship` or `"ship"` literal: doc comments in `agent_result.rs` (ten lines, e.g. "should advance Validate to Ship"), `state.rs` (the module doc's stage list and the `stopped` field doc), `gsd_config.rs:352` and `:618`, `git.rs:213`, `staleness.rs:214`, `preflight.rs:1090`, `commands.rs:624`, `tests/auto_chain_leak_repair_e2e.rs:514`; the `pipeline_outcomes` tests `validate_gaps_does_not_advance_to_ship` and `grafted_failure_shape_gates_instead_of_shipping` and their messages; and `pipeline_gate::until_stop_never_emits_workflow_shipped`, which asserts the absence of an event nothing emits since 52-12. The failure-cap sentence "approve to ship" in `pipeline_outcomes.rs` is 52-33's, as 52-08 recorded.
  **Fix shape:** reword the comments to say a passing Validate finishes the run, and rename the tests. Fits 52-18's comment-stripped scan pass, which does not see prose.

- `StageIntent::for_stage_in_project` and `prompt::stage_prompt_for_project` keep a `project_root` nothing reads (found in 52-15)
  status: open
  **What:** the review angles were the only project-local input to a stage intent. 52-15 kept both `pub` functions, with the parameter now unread, because 52-26, 52-27 and `52-VALIDATION.md` name the one `for_stage_in_project(` call in `pipeline_launch::default_stage_prompt` (pinned by `every_no_override_launch_renders_through_default_stage_prompt`), and removing a `pub` fn is a semver change the plan did not ask for.
  **Fix shape:** after 52-27, either give the parameter a reader or collapse both into `for_stage`/`stage_prompt` and move the source-scan needle with them, recording the `function_missing` from `cargo semver-checks`.


- An invalid `DEVFLOW_EXTERNAL_VERIFY_ENABLED` is ignored with no line the operator can see (found in 52-23)
  status: open
  **What:** `config::external_verify_enabled` warns (tracing only) on a value that is not `true`/`false` and falls back to `devflow.toml` or the default. Its one reader, `agent_result::evaluate_layer0`, runs while the detached supervisor processes a Validate outcome, and the supervisor's stderr is `/dev/null`. So with tracing off by default nothing the operator sees says the value was ignored. 52-23 classed the site (a) (supervisor only) and left it, as its plan says. The same holds for every other supervisor-side warning (the monitor, promotion watch, idle timeout, `git_flow_for_run`'s persisted-base fallback); most of those are also recorded in an event, a verdict file or the monitor log, but this one is not.
  **Fix shape:** read and validate the variable once in the foreground at `devflow start`/`resume` (refuse or print a `warning:` line there), or record an event when the supervisor falls back. Routing the supervisor's tracing to a log file is the broader option and is outside OPS-03's text.

- `gsd_reader::tests::a_gaps_found_report_reads_as_gaps_found` failed once with `Text file busy (os error 26)` (found in 52-24)
  status: open
  **What:** the first `scripts/check.sh all` run of 52-24 (`cargo test --workspace`, devflow-core lib: 1062 passed, 1 failed) failed this test at `gsd_reader.rs:712` with `Spawn { message: "Text file busy (os error 26)" }`. Three isolated reruns and the next full `check.sh all` run passed. 52-24 did not touch `gsd_reader.rs` or `test_support.rs`. The shape is the known write-then-exec race: `test_support::plant_gsd_run_stub` writes the launcher with `fs::write` and the reader executes it at once, while other test threads fork; a child forked between the write's open and close holds the write fd until it execs, so the exec gets `ETXTBSY`. Not reproduced on demand; the cause is inferred from the error, not proved.
  **Fix shape:** write the stub to a temp name and `rename` it into place (the inode that is exec'd was never open for writing), or retry the spawn on `ETXTBSY` in the test build only. Fits whichever plan next touches `test_support::plant_gsd_run_stub` (52-27 or 52-29 reuse it).

- `agent_result::phase_verification_fingerprint` and `phase_verification_mtime_nanos` have no caller outside their own tests (found in 52-24)
  status: open
  **What:** 52-24 deleted the report-authorship heuristic and its three State fields, which were these two `pub` functions' only consumers. They stay because they are `pub` in the published devflow-core and the plan did not list them; their own unit tests still pass.
  **Fix shape:** delete both with `phase_verification_fingerprint_*` tests in a plan that already takes devflow-core semver breaks (4.0.0 covers it), and record the `function_missing` from `cargo semver-checks`; or keep them and say why.

- `pending_supervisor_e2e::stop_ends_a_launch_its_running_launcher_promotes_during_the_wait` failed once on its liveness-file check (found in 52-28)
  status: open
  **What:** in 52-28's first full `cargo nextest run --workspace` (2010 tests, 1 failed), the test failed at `crates/devflow-cli/tests/pending_supervisor_e2e.rs:309` with `its liveness file is gone`: `liveness::probe` read `Live`, not `Missing`, right after `stop` returned. Three isolated reruns passed, and the next two full runs (2012 tests, and `scripts/check.sh all`) passed. 52-28 touched no stop, supervisor or liveness code (its diff is the re-verify fix pass, the streak update and OPERATIONS.md).
  **Not established:** the cause. Suspected: the same fork-inherits-fd family as 999.221 (a child forked by another test holding a duplicate of the liveness lock's description until its `exec`), or the ended supervisor's exit racing the probe under load. Not reproduced on demand.
  **Fix shape:** have the assertion wait (bounded) for the liveness file to read `Missing`, or treat it with 999.221's fix; file a 999.x entry if it recurs.
