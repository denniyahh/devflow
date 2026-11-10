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
