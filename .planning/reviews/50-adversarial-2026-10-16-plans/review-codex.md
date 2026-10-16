## CONFIRMED

All `50-*` references below are under `.planning/phases/50-addressable-monitor-liveness/`. These are findings against the written contracts; no compilation or planned gate execution was performed.

1. **[P2 — Dropped property] Fault evidence no longer has to contain each case exactly once.**  
   `REVIEW-ROUND6-DIFF.patch:2018` removes the assertion `[ "${n:-0}" -eq 1 ]` for each live case, plus the equivalent pointer count. Its replacement, `50-18-PLAN.md:202–204`, requires only that each case “printed `ok live`” and the window “printed `ok pointer`.” Neither the new acceptance criteria nor the evidence-mode specification requires uniqueness. The repeated-name rejection at `50-18-PLAN.md:139–140` concerns **selftest cases**.

   Reproducer: copy a successful case JSON into another `case-*.json` file while retaining its `case` field. The old exact-once gate rejects the duplicate output; the new specified membership rule accepts it. I checked both predicates in memory. Restore evidence-case uniqueness and add a duplicate-record selftest.

2. **[P1 — Passes broken work] The HOME-isolation negative control checks an unrelated directory.**  
   `50-02-PLAN.md:125–129` says to create R, launch through `hermetic_devflow(&H)`, then “assert `entries_under(R)` is empty.” Nothing connects R to that child’s environment. Only the second launch receives `.env("HOME", R2)` at line 130.

   Delete just the builder’s HOME override while retaining its XDG/cache overrides: registry entries still appear under H, unused R remains empty, and the R2 control still succeeds. The named test can therefore pass while the child inherits the operator’s HOME. Run the positive fixture beneath a parent process whose HOME is R, and test an actual HOME-relative write.

3. **[P1 — Fails correct work] The handoff observer requires an overlap that polling cannot guarantee.**  
   `50-13-PLAN.md:126` specifies an observer “polling every 5 ms” and requires: “at the first observation naming N+1, N’s socket file still exists.”

   Correct execution can save N+1’s record and unlink N’s socket entirely between two polls. The first observation naming N+1 then finds N gone and fails. Conversely, the observer can miss a short erroneous pre-clear gap. The simulated-gap control at line 127 does not establish sensitivity to the production window. Synchronize publication and observation with a held window instead of requiring scheduler luck.

4. **[P1 — Execution blocker] The signal-pipe recipe introduces another macOS compile failure.**  
   `50-07-PLAN.md:193–195` directs an unconditional `pipe2(O_CLOEXEC | O_NONBLOCK)` implementation. `50-VALIDATION.md:47` requires the macOS compile gate after every wave.

   I checked the cached libc 0.2.186 source: Linux exports `pipe2`; Apple does not. The common Unix API exports `pipe`. Following this recipe with `libc::pipe2` breaks the compile requirement despite the explicit portability fixes elsewhere. Specify a platform-specific `pipe`/`fcntl` fallback.

5. **[P2 — Locked-decision contradiction] The probe silently reclassifies an unlink race as GONE.**  
   `50-CONTEXT.md:215–219` requires every other connect error, explicitly including “an unlink race,” to be “a reported probe error.” But `50-05-PLAN.md:173` instructs: “NotFound (unlink race) → Gone.”

   These are different observable outcomes. An implementation following the plan violates the locked decision, and the listed fixtures do not require an unlink-race error case. Reconcile the instruction and add that discriminating fixture.

6. **[P1 — Passes broken work] The reap-strays scan mistakes any `--dry-run` string for a command argument.**  
   `50-03-PLAN.md:135–139` defines the guard as semicolon-separated text containing `"--reap-strays"` but not `"--dry-run"`.

   This destructive command passes that predicate:

   ```rust
   common::hermetic_devflow(&home)
       .args(["gate", "sweep", "--reap-strays"])
       .env("LABEL", "--dry-run")
       .status();
   ```

   The environment value never enables dry-run. I reproduced the specified matcher accepting this statement while rejecting the same command without the unrelated environment value. The planted controls exercise token presence, not argument attribution. Require dry-run through a controlled command builder or inspect the actual argument construction.

## SUSPECTED

7. **[P2 — Rename trap] The persistence test has another child-entry name that needs updating.**  
   `50-22-PLAN.md:99–101` says to rename `launch_stage_persists_monitor_pid_for_reload`, but does not mention its internal entry string. Current source, `crates/devflow-cli/src/pipeline_launch.rs:2135`, contains:

   > `const NAME: &str = "pipeline_launch::tests::launch_stage_persists_monitor_pid_for_reload";`

   Lines 2138–2139 launch and verify the child using that string. Renaming only the function makes the child select zero tests. The plan explicitly handles this problem for the *other* renamed test at `50-22-PLAN.md:110–113`. An executor may repair both automatically, so this is an omission rather than a demonstrated future failure.

8. **[P2 — Visibility trap] The specified process-stat fields are inaccessible to the backstop.**  
   `50-09-PLAN.md:134–135` specifies:

   > `pub(crate) struct ProcStat { state: char, pgrp: i32, session: i32, start_time: u64 }`

   Lines 138–142 then place `Option<ProcStat>` in the sibling backstop module’s facts and require classification using those values. Making the struct crate-visible does not expose its fields. Literal implementation requires crate-visible fields or accessors. No generated implementation exists to inspect, so this remains conditional.

## COVERAGE

Read the verification rewrites and supporting instructions in **all 25 plans, `50-01-PLAN.md` through `50-25-PLAN.md`**, plus `50-VALIDATION.md`, `50-CRASH-TABLE.md`, the locked decisions in `50-CONTEXT.md`, ROADMAP Phase 50/50.1, and the relevant source/dependency definitions.

Coverage was focused on changed gates and their supporting contracts, not every unchanged paragraph. No files were modified, no compilation commands were run, and no review-created temporary files or background processes remain.
