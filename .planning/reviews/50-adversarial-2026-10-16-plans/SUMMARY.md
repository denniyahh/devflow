# Phase 50 plan review, round 6 (2026-10-16)

Target: the 25-plan set at `b6fe77b` (gates rewritten to nextest and Rust scans; 50-25 added).
Lanes: codex `gpt-6.1-sol` effort high (8 findings), pi `deepseek-flash` (6 confirmed, 6 suspected).
agy `gemini-3.8-flash-high`: DROPPED (print timeout at 30m, 0 bytes). Not a clean result.

Every finding below was checked against the files before being fixed or dismissed.

## Fixed (confirmed)
- codex 1: `--require-all` lost the round-5 exactly-once count per case. Restored, with selftest case `fault require-all fails a case recorded twice` appended to the 50-18 and 50-25 name loops (32 and 55 cases).
- codex 2: 50-02's empty-directory R was never seen by a child, so it could not fail. Replaced with a `get_envs()` check that the builder sets HOME and each cache variable under H; the R2 negative control stays.
- codex 3: V-13 needed N's socket to survive until the first 5 ms poll that sees N+1. The overlap is now checked at 50-13's `after_publish` hold; the poller's no-Gone check is supplementary.
- codex 4: `pipe2` does not exist for Apple targets in libc 0.2.186 (checked in the registry source). 50-07 now uses `pipe2` on Linux and `pipe` + `fcntl` elsewhere.
- codex 5: 50-05 mapped a connect NotFound (unlink race) to Gone, against locked R-6. It now follows R-6 (a probe error). RESEARCH's sketch contradicts R-6 and is not changed.
- codex 7: 50-22 renamed `launch_stage_persists_monitor_pid_for_reload` without its `const NAME` child-entry string; now renames both.
- codex 8: 50-09's `ProcStat` fields were private, but `backstop.rs` reads them; now `pub(crate)`.
- pi 1: VALIDATION V-52/V-56 still showed wave 16 for 50-20; now 17.
- pi 2: 50-19 said 50-25 runs in its wave; it runs in wave 16, after it.
- pi 3: crash-table B9 cited V-39 (the `--launch` variant); now cites 50-14's `stop_ends_a_live_run_whose_state_will_not_load`.
- pi 6: 50-01 said to keep `**Plans**: TBD`; the line is no longer TBD. Now "the `**Plans**:` line".
- pi S4: 50-17's docs walk panicked on any non-UTF-8 file under `docs/`; it now skips such files.

## Dismissed
- pi S1 (nextest list | wc -l off by one): `cargo nextest list -p devflow --test help_snapshot` printed 1 line, phase7_cli 28, matching the source `#[test]` counts; a missing target printed 0.
- pi 5 (C15 evidence unplanned): 50-15 Task 2 replaces the planted monitor pid with a real launched supervisor and gates the whole `start_lock_e2e` suite, which holds that test. C15 stays STATED for its contended-lock half.
- pi 4 (A8 COVERED with no end-to-end test): the row already says the evidence is unit-level. Left as is.
- codex 6 (reap-strays scan fooled by `"--dry-run"` as an env value): real but needs a deliberately misleading statement; the scan guards against accidental real passes. Accepted.
- pi S2, S3, S5, S6: low-probability or already handled by the plan text.
