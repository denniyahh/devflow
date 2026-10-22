---
status: resolved
trigger: "Phase 48 final host verification: agent::tests::discover_stray_devflow_processes_rejects_the_999_47_false_positive_shape failed in 3 of 10 complete core-library runs at its fixed 10-second exec-visibility timeout."
created: 2026-09-19
updated: 2026-09-18
---

# Debug: Stray Exec Visibility Timeout

## Symptoms

- Expected behavior: the complete `devflow-core` library suite runs reliably; the #999.47 false-positive fixture becomes discoverable before its bounded visibility deadline.
- Actual behavior: 3 of 10 complete library runs failed only this test with `exec visibility timed out before the fixture became discoverable`; isolated drop/reacquire lock runs did not reproduce the earlier apparent lock failure.
- Error messages: the preserved suite logs name the spawned fixture PID and the 10-second visibility timeout.
- Timeline: observed during Phase 48 final review remediation on 2026-09-19.
- Reproduction: run `cargo test -p devflow-core --lib` repeatedly under the normal parallel test runner; failures occurred in runs 1, 2, and 9 of a 10-run sample.

## Current Focus

- bug_class: deterministic test-fixture semantic defect with a load-sensitive visibility symptom
- hypothesis: confirmed — a lone shell command may tail-exec `sleep`, replacing the argv shape that the test both waits for and claims to inspect.
- next_action: resolved; retain the fixture premise assertion and rerun under the pinned/container gate before relying on a reliability claim.

## Evidence

- timestamp: 2026-09-18 — the existing repository RCA in `.planning/ROADMAP.md` for backlog 999.92 records the same two-part defect: Bash tail-execs the lone `sleep 30`, so `argv[0]` changes from `sh` to `sleep` and the devflow-looking `$0` argument disappears before the census.
- timestamp: 2026-09-18 — controlled negative control: 12 direct `bash -c 'sleep 30' /tmp/devflow-scratch/looks-like-devflow` launches all exposed `/proc/<pid>/cmdline` as `sleep 30` after a 10 ms settle; none retained `sh` or the devflow-looking argument.
- timestamp: 2026-09-18 — environment contrast: this host's `sh -c 'sleep 30' ...` retained the shell shape in 20 immediate samples, showing the original test depended on shell implementation/timing rather than a guaranteed fixture contract.
- timestamp: 2026-09-18 — isolated original target passed once; the bounded helper negative-control test (`wait_for_exec_visibility_times_out_bounded_when_it_never_matches`) also passed, confirming that a non-matching argv remains a real failing direction rather than a silent skip.
- timestamp: 2026-09-18 — after the fixture repair, the target test passed exactly once with 1 passed / 819 filtered; its new premise assertion read the live proc cmdline and found `/tmp/devflow-scratch/looks-like-devflow` before the census.
- timestamp: 2026-09-18 — post-fixture-repair `cargo test -p devflow-core --lib` passed 6/6 complete runs, each 820 passed / 0 failed; a final full run after the cleanup change also passed 820/820. This seven-run sample is not a reliability bound for the prior 3/10 loaded-run symptom.
- timestamp: 2026-09-18 — direct cleanup control: the repaired script's shell and its child sleep were identified as a parent/child pair; after SIGTERM to the shell, the trap reaped the child (`kill -0` failed). The test now uses `terminate(pid)` rather than SIGKILL so that trap is reachable.
- timestamp: 2026-09-18 — `cargo fmt --check` and `cargo clippy -p devflow-core --all-targets -- -D warnings` passed after the final change; `git diff --check` was clean.

## Eliminated

- hypothesis: `discover_stray_devflow_processes` has a production visibility wait or production retry defect — eliminated by source inspection: production performs one read-only `/proc` census and never calls the test-only `wait_for_exec_visibility` helper. The failure is in the fixture that validates the census, not the runtime census implementation.
- hypothesis: changing only the expected basename from `sh` to `sleep` is a valid repair — eliminated by the Bash negative control: doing so would make the test reliable while asserting against a plain `sleep 30`, whose devflow-looking argument has already vanished.

## Resolution

root_cause: The negative fixture used a lone `sh -c 'sleep 30'` command. A POSIX shell may tail-exec that command; on Bash it does, replacing argv0 with `sleep` and removing the devflow-looking `$0` argument. The barrier then intermittently misses the brief `sh` state and times out, while a green run can assert only that a plain sleep process is not a stray. This is test-fixture behavior, not a production discovery defect.
fix: Replaced the lone command with a shell-resident, TERM-cleaning `sleep`/`wait` fixture, changed test cleanup to SIGTERM so that trap reaps its child, and added a live `/proc/<pid>/cmdline` premise assertion requiring the devflow-looking argument before the census executes. No production code changed.
verification: Direct Bash behavior supplied the negative control; the repaired target passed 1/1; six complete post-fixture-repair devflow-core library runs plus one final run after cleanup each passed 820/820; the cleanup control proved the child is reaped; fmt and all-target devflow-core clippy passed. The seven-run sample does not establish behavior under the original pinned/container load shape or quantify long-run flake probability.
files_changed:
  - crates/devflow-core/src/agent.rs
  - .planning/debug/stray-exec-visibility-timeout.md
cycles:
  investigation: 1
  fix: 1
specialist_hint: rust
prevention: why not caught: the old test verified a timing proxy rather than the fixture's live argv premise; guard: assert the devflow-looking argument from `/proc/<pid>/cmdline` immediately before the census.
