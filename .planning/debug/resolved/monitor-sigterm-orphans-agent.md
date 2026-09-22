---
status: resolved
trigger: "monitor::tests::sigterm_to_monitor_also_kills_the_agent fails intermittently in the pinned CI container: the agent survives the monitor's SIGTERM, orphaned to PID 1."
created: 2026-09-22
updated: 2026-09-22 (session 4, closed)
---

# Debug: Monitor SIGTERM Orphans the Agent

## Symptoms

- Expected behavior: when the monitor shell receives SIGTERM, its `trap cleanup TERM INT` kills the backgrounded agent (`kill "$apid"`) and the agent is gone within the test's 5-second poll.
- Actual behavior: intermittently the agent survives: `agent (pid N) was orphaned — still running after monitor SIGTERM`. The monitor has exited (zombie); the agent is alive with PPid=1.
- Error messages (both failures identical in shape):
  - `monitor after:  ALIVE Name=sh State=Z (zombie) PPid=38 cmdline=[<empty>]`
  - `agent before:   ALIVE Name=sh State=R (running) PPid=<monitor> cmdline=[sh -c apid=''; cleanup() { ... }; trap cleanup TERM INT; cd ... "$@" > ... & apid=$!; echo $apid > ...-agent-pid; wait $apid; ...]` — i.e. at the moment the test snapshotted it, the agent pid was still the monitor's forked subshell and had NOT yet exec'd `"$@"`.
  - `agent after:    ALIVE Name=sh State=S (sleeping) PPid=1 cmdline=[sh -c sleep 30]` — the exec happened after the TERM, and the exec'd agent survived.
  - Logs: `/home/denniyahh/.claude/jobs/28cb39f7/tmp/flake/run-7.log`, `/home/denniyahh/.claude/jobs/28cb39f7/tmp/flake-head2/run-15.log` (job-local, not durable).
- Timeline: the test's own comments record containerised-CI failures since 2026-07-26 and forbid widening its window ("Leave this red until that is fixed. Do NOT widen it again."). Measured 2026-09-22 in the pinned container (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`, `taskset -c 0,1`, `/bin/sh` = dash): 2 of 30 `cargo test -p devflow-core --lib` runs at `1d93895` failed this test; 0 of 20 at phase-48 base `fbf6e98` (not a significant difference; Phase 48 only appended `--stage` to the post-`wait` advance tail, outside the fork→exec window, but grew the suite from 774 to 825 tests with many more child-process spawns).
- Reproduction: `cargo test -p devflow-core --lib` repeated inside the pinned container (script: `/home/denniyahh/.claude/jobs/28cb39f7/tmp/flake-loop2.sh N <repo_root> <out_dir>` — same image, mounts, per-checkout cache volumes and cpu pin as `scripts/check-in-container.sh`). Warm host runs have historically hidden this class (see the 999.47 notes and the pre-push container gate memory).

## Current Focus

- status_note: CLOSED (session 4). Operator confirmed the fix ("Confirmed fixed") on 2026-09-22 and chose backlog (option A) for the PipeOwning finding. Commits on feature/phase-48: 4fbe23c (RED tests), 745ad9b (harness correction, still RED), 3ca90b6 (fix), c830edb (backlog entry 999.134). See Resolution.
- tdd_checkpoint:
    test_file: "crates/devflow-core/src/monitor.rs (tests module, after sigterm_to_monitor_also_kills_the_agent)"
    test_names:
      - "monitor::tests::sigterm_before_the_agent_child_resets_its_traps_still_kills_the_agent  (window A; RED under dash only)"
      - "monitor::tests::sigterm_before_the_monitor_records_the_agent_pid_still_kills_the_agent  (window F; RED under dash and bash)"
      - "monitor::tests::sigterm_after_a_held_agent_child_execs_kills_the_agent  (opposite-result control; green before and after)"
    status: "green (3ca90b6); was red at 4fbe23c and 745ad9b"
    failure_output: "container (dash), 3/3 runs: A and F FAILED with `agent (pid N) survived a TERM that reached it before its trap reset, orphaned: ALIVE Name=sh State=S (sleeping) PPid=1 cmdline=[sh -c sleep 30]` (F: `...reached the monitor before it recorded the agent pid, orphaned: ...PPid=1...`); control + existing test ok. Host (bash): A ok (bash keeps the TERM), F FAILED `...orphaned: ALIVE Name=sleep ... PPid=2090 cmdline=[sleep 30]`, control ok."
- green_plan (APPLIED in 3ca90b6, session 3; originally validated in the scratch harness as `fix3`, evidence entry 9):
    1: "agent_result.rs — add `stop_marker_path(project_root, phase)` (e.g. `.devflow/phase-NN-monitor-stop`), next to agent_pid_path."
    2: "monitor.rs Legacy arm — before spawning `sh`, remove a stale marker in Rust (NotFound ignored, any other error returned) so no extra fork precedes the agent fork and a stale marker cannot kill the next agent."
    3: "monitor.rs Legacy script — cleanup becomes `echo > STOP; [ -n \"${apid:-$!}\" ] && kill \"${apid:-$!}\" 2>/dev/null; exit 0` (marker BEFORE kill; `echo`, not `:`, because a failed redirect on the special builtin `:` would exit the shell before the kill); agent launch becomes `{ [ -e STOP ] && exit 143; exec \"$@\"; } > OUT 2>ERR &`. Keep the `trap cleanup TERM INT` substring (agent.rs MONITOR_WRAPPER_MARKER depends on it)."
    4: "Update the WR-08 comment block above the script and the `Leave this red` comment in sigterm_to_monitor_also_kills_the_agent to cite this root cause. Do NOT widen any poll window."
    5: "Add a stale-marker regression test (pre-create the marker, launch normally, agent must run and its exit code be captured) — the fix introduces that failure mode, so it needs its own guard."
    6: "Check gitignore/doc parity for the new .devflow file (crates/devflow-core/tests/devflow_dir_gitignore.rs, crates/devflow-cli/tests/gitignore_coverage.rs, doc_check path-shape parity)."
    7: "Verify: the 4 sigterm tests x10 in the container and on the host, each module-qualified with `1 passed`; mutation: revert the marker -> A red, revert `${apid:-$!}` -> F red, drop the Rust-side remove -> stale-marker test red; full `cargo test -p devflow-core --lib` in the container; `scripts/check.sh all` via scripts/check-in-container.sh."
- reasoning_checkpoint:
    hypothesis: "Under dash, a TERM that the monitor's cleanup sends while the forked agent child is still before dash's forkreset() is caught by the inherited onsig handler and then discarded by dotrap (trap[TERM] is NULL after the reset); the child execs the agent, which never receives a TERM and is orphaned (PPid=1) when the monitor exits. Separately (both shells), a TERM that reaches the monitor between fork and `apid=$!` runs cleanup with apid='' and kills nothing."
    confirming_evidence:
      - "Case A: pre-reset hold + TERM → 5/5 survive under dash, SigCgt bit 15 set at TERM time, after-state PPid=1 `sh -c sleep 30` — identical to the CI signature."
      - "Case B: post-reset pre-exec hold + TERM → 5/5 die (falsifies the broader window claim; localises it to pre-reset)."
      - "dash 0.5.12 source: onsig records gotsig; FORKRESET nulls trap[] and resets to SIG_DFL; dotrap `if (!p) continue;`."
      - "bash differential: case A 3/3 die under bash — explains host-never-reproduces."
      - "Case F: parent hold before apid=$! → survive under dash 5/5 AND bash 3/3."
    falsification_test: "If a TERM delivered while the child is held pre-reset still killed the agent under dash (case A died), or if a TERM delivered post-reset (case B) also orphaned it, the hypothesis would be wrong. Neither happened."
    fix_rationale: "The agent child re-checks a stop marker that cleanup creates BEFORE it sends the kill; the check runs only after the child's trap reset, so a swallowed TERM is always followed by a check that sees the marker (no timing assumption). `${apid:-$!}` in cleanup closes window F. The stale marker is removed in Rust before spawn so no extra fork precedes the agent fork."
    blind_spots: "Not re-measured: whether newer dash releases fixed the swallow (not needed — DevFlow must work on the dash CI ships). The PipeOwning (Rust __monitor) arm was not examined for an analogous window. The in-suite RED for window A is only RED where /bin/sh is dash (container/CI); on the bash host it passes pre-fix because bash does not lose the signal."
    candidate_causes:
      - "code: monitor script relies on a single TERM reaching the agent (window A) and on apid being set before any trap can run (window F)"
      - "environment: /bin/sh = dash 0.5.12 discards a pre-reset caught signal in a forked child (bash does not)"
      - "environment (load): 2-CPU pinned container + 825-test suite delays the child's first scheduling after fork, widening window A from µs to ms so the test's immediate TERM lands in it"
    and_gate: "yes — the observed flake needs the code assumption (single TERM) AND dash's discard behaviour AND scheduling delay widening the window. Removing the code assumption breaks the chain regardless of the other two, which are environmental and not ours to change."
- bug_class: Heisenbug by manifestation (timing-dependent), but hypothesised Bohrbug mechanism (deterministic once the TERM lands in a specific window) — route: deterministic fault injection, not SBFL/stress.
- hypothesis (refined 2026-09-22, session 2; CONFIRMED): the lost-TERM window is NOT the whole fork→exec window. In dash 0.5.12 the forked child inherits the monitor's *caught* TERM handler (`onsig`) until its fork-reset code clears traps and resets TERM to SIG_DFL. A TERM landing BEFORE that reset is caught by `onsig` (sets gotsig) and then silently discarded by `dotrap` because `trap[TERM]` is now NULL; the child continues to exec the agent, which never sees a TERM. A TERM landing AFTER the reset but before exec (e.g. during redirection opening) hits SIG_DFL and kills the child — so a blocking-redirection hold should NOT reproduce the orphan (falsification test for the refined claim).
- test: in the pinned image, (A) LD_PRELOAD fork() shim holds the first forked child in the pre-reset window, TERM the monitor during the hold → predict agent survives with PPid=1; (B) blocking FIFO stdout redirect holds the child post-reset → predict agent dies; (C) TERM after exec → predict agent dies; (D) shim present but TERM sent after the hold+exec → predict agent dies (shim itself is not the cause). Record child's /proc SigCgt bit 15 in each case to show which disposition was live when TERM landed.
- expecting: A survives, B/C/D die. If B also survives, the refined window is wrong (whole pre-exec window matters). If A dies, the hypothesis is falsified.
- green_blocker (2026-09-22 session 3): container x10 post-fix: 8/10 all 5 pass; runs 8 and 9 FAILED window-A test on its PRECONDITION (`the held agent child never received the monitor's TERM`, monitor.rs:3106), not on the orphan assertion. The monitor did exit 0 through its trap (the preceding assertion passed), so cleanup's kill was sent.
  hypothesis_H1: "observation race in the harness: under dash the held child's signals are unblocked, so a TERM is only recorded if it interrupts the shim's nanosleep (EINTR). A TERM that arrives while the child is outside nanosleep, or runnable-but-descheduled after a tick expired (loaded 2-CPU container), runs dash's handler with nanosleep returning 0 — nothing recorded, and SigPnd/ShdPnd are clear because it was handled."
  hypothesis_H2: "the kill is sometimes not delivered to the held child (would be a real defect)."
  test: "shim blocks all signals for the hold (sigprocmask), so a TERM generated in the hold stays kernel-visible in ShdPnd until release and is delivered to the inherited handler at the end of the shim's fork(), still before the shell's post-fork code. Under H1 the precondition then passes every run; under H2 it still fails. Re-check RED semantics with mutation M1 (A must fail on the orphan assertion)."
- green_blocker_resolution: H1 supported, H2 not observed: with the blocking shim the precondition held 30/30 in the container (evidence, session 3). The harness correction was committed separately (745ad9b) and re-verified RED in the container (A and F red 2/2 on their orphan assertions).
- next_action: none — session closed and archived after operator verification (2026-09-22). PipeOwning follow-up lives in ROADMAP 999.134, not here.
- constraints: prefer deterministic fault injection over widening timeouts; module-qualified exact test names must show `1 passed`; any fix needs an opposite-result control; do not widen the existing test's poll window.

## Evidence

- timestamp: 2026-09-22 (session 2)
  checked: Phase 0 knowledge base.
  found: `.planning/debug/knowledge-base.md` does not exist; MemPalace not reachable from this agent (no MCP tool surfaced). No prior-pattern candidate.
  implication: proceed without a known-pattern hypothesis.

- timestamp: 2026-09-22
  checked: fault-injection harness in the pinned image (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`, dash 0.5.12-2, `taskset -c 0,1`). Harness: `/home/denniyahh/.claude/jobs/28cb39f7/tmp/sigterm-exp/{holdfork.c,holdparent.c,run.sh}` (job-local). Monitor = a byte-for-byte copy of the production Legacy script shape minus the advance tail; agent = `sh -c 'sleep 30'`. Liveness = /proc State != Z; 5 s poll, same as the test. Log: `.../sigterm-exp/orig.log`.
  found: |
    Case A (LD_PRELOAD fork() shim holds the agent child 1.5 s immediately after fork, BEFORE dash's forkchild runs; TERM the monitor during the hold): 5/5 SURVIVED. At TERM time the child's SigCgt had bit 15 set (TERM caught — inherited trap handler) and cmdline was still the monitor script; afterwards PPid=1, cmdline `sh -c sleep 30`. Identical signature to the CI failure.
    Case B (child held AFTER fork-reset but before exec: stdout redirect target is a FIFO with no reader): 5/5 DIED, SigCgt bit 15 clear.
    Case C (TERM after the agent exec'd): 5/5 DIED.
    Case D (shim loaded, but TERM only after hold + exec): 5/5 DIED — the shim itself does not cause survival.
    Case F (PARENT held just after fork(), before `apid=$!`; TERM the monitor during the hold): 5/5 SURVIVED — cleanup runs with apid='' and kills nothing.
    Case N (no signal): 5/5 exit=7, stdout captured — harness baseline.
  implication: the orphan is deterministic once TERM lands in the child's pre-reset window (A); the post-reset pre-exec window is safe (B), so the refined hypothesis holds and the "whole fork→exec window" version is falsified. F is a second, parent-side window of the same class that the test cannot hit (it only signals after the pidfile exists) but an operator/`stop` can in principle.

- timestamp: 2026-09-22
  checked: dash 0.5.12 source (git.kernel.org tag v0.5.12, src/trap.c + src/jobs.c, fetched to `.../sigterm-exp/dash-0.5.12-*.c`).
  found: `onsig()` sets `gotsig[signo-1]=1; pending_sig=signo`. `forkchild()` calls `forkreset()` whose trap.c FORKRESET block frees every non-empty trap, sets `trap[sig]=NULL` and `setsignal(sig)` (→ SIG_DFL). `dotrap()` clears `gotsig[i]` and does `p = trap[i+1]; if (!p) continue;` — a TERM caught before the reset is recorded, then discarded without any action.
  implication: code-level mechanism confirmed; this is dash behaviour (arguably a dash bug), not DevFlow timing.

- timestamp: 2026-09-22
  checked: same cases A and C with `bash` 5.2.15 as the monitor shell (in the pinned image). Log `.../sigterm-exp/bash.log`.
  found: A with the original script under bash: 3/3 DIED (SigCgt bit 15 was set at TERM time, so the hold did land pre-reset). C: 3/3 DIED.
  implication: bash does not lose the pre-reset TERM; dash does. The Fedora host's /bin/sh is bash, which is why host runs never reproduce this. Production on Debian/Ubuntu hosts (dash as /bin/sh) and CI are affected; the operator's Fedora host is not affected by window A.

- timestamp: 2026-09-22
  checked: prototype fixes (scratch only, NOT in the repo) under the same harness, dash. `fix` = stop marker: script starts with `stop=<file>; rm -f "$stop"`; cleanup does `echo > "$stop"` BEFORE `kill "$apid"`; the agent is launched as `{ [ -e "$stop" ] && exit 143; exec "$@"; } > out 2>err &`. `fix2` = `fix` + cleanup kills `"${apid:-$!}"`. Log `.../sigterm-exp/fix.log`.
  found: fix: A 0/5 survived, B 0/5, C 0/5, D 0/5, F 5/5 survived; N and S (stale marker pre-created) 5/5 exit=7 with stdout captured; R (marker path unwritable, TERM after exec) 0/5 survived — the failed `echo >` redirect does not abort cleanup (echo is a regular builtin). fix2: A/B/C/D/F all 0/5 survived; N/S 5/5 normal.
  implication: the marker closes window A without timing assumptions (the marker is created before the kill, and the child checks it only after its trap reset, so a swallowed TERM is always followed by a check that sees the marker). `${apid:-$!}` additionally closes window F.

- timestamp: 2026-09-22
  checked: case F (parent held after fork, before `apid=$!`) under bash 5.2.15, orig/fix/fix2; fix2 A and N under bash. Log `.../sigterm-exp/bash-F.log`.
  found: orig F under bash: 3/3 SURVIVED — window F is NOT dash-specific; it affects the Fedora host too. fix/fix2 F under bash printed "WARN agent never exec'd" 3/3: bash forks the prototype's startup `rm -f "$stop"` with fork() (dash used vfork, which the shim does not intercept), so the parent hold landed on `rm`, not on the agent fork — those two F results are INVALID, not passes. fix2 A under bash 3/3 DIED; fix2 N 2/2 normal.
  implication: (1) the stale-marker reset must not be an external command in the script — do it in Rust (`remove_file`, NotFound ignored) before spawning, so the agent `&` stays the monitor's first fork. (2) Any fault-injection test that targets "the monitor's first fork" is only valid while that holds; the tests must assert the hold landed (interrupt marker + pre-exec cmdline) rather than assume it.

- timestamp: 2026-09-22
  checked: scratch variant `fix3` = marker + `${apid:-$!}` with the stale marker removed by the launcher instead of `rm -f` in the script (emulates the Rust-side remove), cases A B C D F N R, under dash (4 reps each) and bash (4 reps each). Log `.../sigterm-exp/fix3.log`.
  found: dash and bash alike: A, B, C, D, F, R 0/4 survived, no "never exec'd" warnings (every hold landed on the agent fork); N 4/4 exit=7 with stdout captured.
  implication: the green design closes both windows on both shells in the harness. Not yet shown: the same through the real `spawn_monitor` (that is the green phase's job).

- timestamp: 2026-09-22
  checked: in-suite RED tests (uncommitted) in crates/devflow-core/src/monitor.rs, run as `cargo test -p devflow-core --lib -- monitor::tests::sigterm_`. Container via `/home/denniyahh/.claude/jobs/28cb39f7/tmp/sigterm-exp/ctest.sh 3 <repo> <out> monitor::tests::sigterm_` (same image, mounts, volumes, cpu pin as check-in-container.sh); tally `.../sigterm-exp/red-container/tally.txt`. Host: same filter, once.
  found: |
    Container (dash), 3/3 runs identical: 2 passed (control, existing sigterm test), 2 failed (A, F), each failing on its final orphan assertion with PPid=1, cmdline `sh -c sleep 30` — i.e. past every precondition (hold landed on the agent fork, child pre-exec, TERM caught, TERM received during the hold, monitor exited 0 through its trap).
    Host (bash /bin/sh): A ok, control ok, existing ok, F FAILED on the orphan assertion (PPid=2090, `sleep 30`).
    First host attempt failed A and F on the precondition "never caught the TERM": bash blocks signals across its fork, so the TERM sat pending instead of interrupting the hold. The precondition now accepts "hold interrupted OR TERM in SigPnd/ShdPnd"; both are "TERM generated inside the window".
    cargo fmt --check and cargo clippy -p devflow-core --all-targets -D warnings: clean.
  implication: RED is deterministic where it should be (A on dash, F on both); the control proves the shim harness does not by itself keep an agent alive.

- timestamp: 2026-09-22 (session 3, green)
  checked: operator approved ("Approve as proposed"). RED tests committed alone as 4fbe23c (409 insertions, 0 deletions, only monitor.rs tests module). Pre-commit host re-run of the RED set: A ok, control ok, existing ok, F FAILED on the orphan assertion (PPid=2090, `sleep 30`) — same as session 2.
  found: green_plan steps 1-6 applied: `agent_result::stop_marker_path` (`.devflow/phase-NN-monitor-stop`); Legacy arm removes a stale marker in Rust (NotFound ignored, other errors returned) before building the script; cleanup = `echo > STOP; [ -n "${apid:-$!}" ] && kill "${apid:-$!}"`; launch = `{ [ -e STOP ] && exit 143; exec "$@"; } > OUT 2>ERR &`; comments updated; new test `monitor::tests::a_stale_stop_marker_does_not_stop_the_next_agent`; parity: root .gitignore, gitignore_coverage.rs RUNTIME_PATHS, doc_check gitignore_covers_all_devflow_paths, OPERATIONS.md inventory row.
  implication: host (bash) after fix: 5 targeted tests (4 sigterm + stale-marker) 10/10 runs `5 passed`; doc_check:: (6), spawn_monitor_* (4), gitignore_coverage (2), devflow_dir_gitignore + monitor_e2e integration tests pass. fmt + clippy -D warnings clean.

- timestamp: 2026-09-22 (session 3, green)
  checked: container x10 of the 5 targeted tests after the fix (`.../sigterm-exp/green-container/tally.txt`, diff sha in diff-sha.txt).
  found: 8/10 runs all 5 pass. Runs 8 and 9: window-A test FAILED on its precondition `the held agent child never received the monitor's TERM, so the kill did not land inside the window` (monitor.rs:3106 at that point). The preceding assertion (monitor exited 0 through its trap) passed, so cleanup ran. Those two runs say nothing about the fix either way: the test stopped before release.
  implication: the harness's "TERM landed" detection under dash relied on the TERM interrupting the shim's nanosleep (EINTR). A handler that runs while the child is descheduled after a tick expired, or outside nanosleep, leaves no trace (handled, so not pending). Changed the shim to block all signals for the hold and record a pending TERM each tick and at release (sigpending), restoring the mask at the end of the shim's fork() so the TERM still reaches the inherited handler before the shell's post-fork code. This converts an H1 miss into a kernel-visible pending signal, but does NOT mask H2: an undelivered kill still fails the precondition. Host 3/3 green after the harness change; container x30 running (`.../sigterm-exp/green-container-2`).

- timestamp: 2026-09-22 (session 3, green)
  checked: container x30 of the 5 targeted tests with the blocking shim (`.../sigterm-exp/green-container-2/tally.txt`; worktree diff sha e6c7d767f6083e40 before and after the loop).
  found: 30/30 runs `5 passed; 0 failed`; each of the 5 test names appears `ok` in all 30 runs (counted per name). No precondition failure.
  implication: in every run the TERM was kernel-visibly pending in the held child, so H2 (kill not delivered) was not observed (at a 20% H2 rate, 0/30 has p~0.001). The earlier 2/10 is consistent with H1 (harness observation race); H1's mechanism itself was not directly observed. The fix held in all 30 runs where the precondition was met.

- timestamp: 2026-09-22 (session 3, green)
  checked: revert-mutations (script `.../sigterm-exp/mutate.py`, sources restored from `.../sigterm-exp/mut-backup/` after each; diff sha back to e6c7d767f6083e40).
  found: |
    M1 (drop the child's `[ -e STOP ] && exit 143`): container 3/3 FAILED window A on the orphan assertion `agent (pid N) survived a TERM that reached it before its trap reset, orphaned: ALIVE Name=sh State=S (sleeping) PPid=1 cmdline=[sh -c sleep 30]` — the CI signature, past every precondition; other 3 sigterm tests ok. Host (bash) A still passes under M1 (bash keeps the TERM), as expected.
    M2 (cleanup kills "$apid" not "${apid:-$!}"): host FAILED F `...survived a TERM that reached the monitor before it recorded the agent pid, orphaned: ...PPid=2090 cmdline=[sleep 30]`; container 2/2 FAILED F, same text, PPid=1 `sh -c sleep 30`.
    M3 (drop the Rust-side stale-marker remove): host FAILED stale-marker test `a stale stop marker stopped the next agent: its exit code is "143", not the agent's own 7`.
    M4 (drop root .gitignore `.devflow/phase-*-monitor-stop`): host FAILED doc_check::gitignore_covers_all_devflow_paths `agent_result::stop_marker_path produced uncovered runtime path .devflow/phase-16-monitor-stop`.
  implication: each guard is load-bearing and each test fails for the intended reason; A's RED depends on dash (container only).

- timestamp: 2026-09-22 (session 3)
  checked: PipeOwning arm (blind spot from session 2). Code: no signal-handling crate in either Cargo.toml, no sigaction/handler in monitor.rs or the CLI; the child is spawned with `.process_group(0)`. Probe: host-built `target/debug/devflow __monitor --project <tmp> --phase 4 --stage code --workdir <tmp> --prompt-file <tmp>/prompt --idle-timeout-secs 120 --agent claude -- sh -c 'sleep 47'`, then `kill -TERM <monitor>`. Negative control: same launch, `kill -TERM <agent>`.
  found: monitor SigCgt 0000000100000440 (TERM not caught); monitor wait status 143 (killed by the TERM); 3 s later the agent was ALIVE, State=S, PPid=2090 (user service manager), cmdline `sleep 47`. Control: agent GONE 3 s after a direct TERM, so the probe can see a dead agent. Leftover agent SIGKILLed; tmp dirs removed; `pgrep` confirms no leftovers.
  implication: a TERM to the PipeOwning `__monitor` always orphans its agent. This is not window A/F but the absence of any WR-08 handling on that arm. Reach: DevFlow's own senders do not TERM it (`stop` signals the advance lock holder; stray-reap matches only `sh -c ... trap cleanup TERM INT` wrappers and `devflow advance`, and has a 2 s age floor; idle-timeout signals the child group), so only an external TERM (operator `kill`, a supervisor) hits it. 1 probe run; deterministic by construction (no handler), not a race. Operator decision required per the approval note — not acted on.

- timestamp: 2026-09-22 (session 3)
  checked: reasoning only (not tested): interaction of the new marker with the Legacy advance tail. The old monitor shell stays alive while its foreground `devflow advance` spawns the next stage's monitor for the SAME phase; dash/bash defer a TERM's trap until that foreground command returns.
  found: a TERM sent to a monitor during its advance tail now runs cleanup after advance returns and writes the phase's marker. If the next stage's Legacy agent child has not reached its check yet, it exits 143 without starting; otherwise the marker is left stale and removed by the next Legacy spawn. Before the fix the same TERM did nothing to the next stage (and `kill $apid` targeted an already-reaped pid, a pre-existing pid-reuse hazard this fix does not change).
  implication: narrow, requires an external TERM timed to the advance tail; outcome (next agent stopped before start) matches the TERM's stop intent. Recorded for awareness, not acted on.

## Eliminated

- hypothesis: the whole fork→exec window is unsafe (any TERM before exec is lost).
  evidence: case B — child held after fork-reset (blocking FIFO redirect), TERM kills it 5/5; SigCgt bit 15 clear at TERM time.
  timestamp: 2026-09-22

## Resolution

root_cause: "The Legacy monitor script assumes one `kill \"$apid\"` from its TERM trap reaches the agent. Two windows break that. (A, dash only) The forked agent child keeps the monitor's caught TERM handler until dash's forkreset(); a TERM landing first is recorded by onsig and then discarded by dotrap because trap[TERM] is NULL after the reset, so the child execs an agent that never saw a TERM and is orphaned (PPid=1) when the monitor exits. That the CI flake is this window is inferred, not directly observed: the CI snapshot shows the child still pre-exec (State=R, monitor cmdline) when the test signalled, case B shows only a pre-reset TERM can be lost, and case A reproduces the exact after-state; the scheduling delay that widens the window under the 2-CPU loaded container was not itself measured. (F, dash and bash) A TERM reaching the monitor between its fork and `apid=$!` runs cleanup with apid='' and kills nothing."
fix: "Legacy monitor script (crates/devflow-core/src/monitor.rs spawn_monitor_inner): cleanup writes `.devflow/phase-NN-monitor-stop` (agent_result::stop_marker_path) with `echo` BEFORE `kill \"${apid:-$!}\"`; the agent is launched as `{ [ -e STOP ] && exit 143; exec \"$@\"; } > OUT 2>ERR &`, so the check runs after the child's trap reset and a lost TERM is always followed by a check that sees the marker (window A); `${apid:-$!}` covers a TERM before `apid=$!` (window F). The stale marker is removed in Rust (NotFound ignored, other errors returned) before each Legacy spawn. PipeOwning arm untouched. Commits: 4fbe23c (RED tests), 745ad9b (harness: fork shim blocks signals for the hold so TERM receipt is kernel-visible), 3ca90b6 (fix + stale-marker test + parity)."
verification: |
  - Container (dash), fixed tree, blocking shim: 5 targeted tests x30 runs, 30/30 `5 passed; 0 failed`, each name ok 30/30. (First x10 with the old EINTR-based shim: 8/10, 2 runs failed window A's PRECONDITION, not the fix assertion; led to 745ad9b.)
  - Host (bash): 5 targeted tests x10 (old shim) + x3 (new shim), all `5 passed`.
  - Revert-mutations, each failing on the intended assertion text: M1 drop child marker check -> container A red 3/3 (`...orphaned: ... PPid=1 cmdline=[sh -c sleep 30]`, the CI signature); host A stays green under M1 (bash; host cannot catch this). M2 `$apid` instead of `${apid:-$!}` -> F red on host 1/1 and container 2/2. M3 drop Rust stale-marker remove -> stale-marker test red (`exit code is "143", not the agent's own 7`). M4 drop root .gitignore line -> doc_check::gitignore_covers_all_devflow_paths red naming stop_marker_path.
  - RED-in-history check: 745ad9b content in container: A and F red 2/2 on orphan assertions; control + existing sigterm test green.
  - Full gate: scripts/check-in-container.sh all -> exit 0, `==> check.sh: all OK`; devflow-core lib `829 passed; 0 failed`; 34 `test result: ok` lines, 0 FAILED lines. Committed content == verified content (`git diff 4fbe23c HEAD` sha256 prefix e6c7d767f6083e40, same as the tree the loop/mutations/gate ran on).
  - Does NOT establish: that the CI flake was window A (inferred, see root_cause); anything about the PipeOwning arm (see pipe_owning below); a reliability bound for the original racy test beyond 30 green runs; behaviour under shells other than dash 0.5.12 and bash 5.2.15/host bash.
  - Blind gate, by construction: the window-A test can only go RED where /bin/sh is dash. On the operator's Fedora host (/bin/sh = bash) it passes with or without the fix (M1 confirmed this), so a host-only run can NEVER catch a regression of the window-A guard. Only the pinned-container path (scripts/check-in-container.sh, pre-push hook, CI) can. Window F and the stale-marker guard are RED on both shells.
human_verification:
  date: 2026-09-22
  verdict: "Confirmed fixed (operator, explicit answer via the /gsd-debug checkpoint)."
  independent_checks_reported_by_the_orchestrator_before_asking: "4fbe23c/745ad9b/3ca90b6 present on feature/phase-48; the two test commits contain no marker or `${apid:-$!}` strings (so RED was real, not pre-fixed); all five monitor SIGTERM tests pass on the host by exact module-qualified name with `1 passed` each; scripts/check-in-container.sh all at 3ca90b6 exited 0 (`==> check.sh: all OK`), 1411 passed / 0 failed over 34 suites, all four new tests present and passing under dash."
pipe_owning:
  disposition: "BACKLOG — operator chose option A on 2026-09-22: do not fix in this session."
  tracked_as: "ROADMAP 999.134 'PipeOwning Monitor Orphans Its Agent on TERM' (filed by the orchestrator in commit c830edb, with the probe evidence, the confirmed absence of any signal handler, the `.process_group(0)` at monitor.rs:961, the reachability note — only a TERM from outside DevFlow reaches it — and a fix shape)."
  note: "This debug session deliberately left the PipeOwning arm untouched. Do not re-file; see 999.134."
prevention:
  why_not_caught_sooner: "A gate did exist — monitor::tests::sigterm_to_monitor_also_kills_the_agent has been failing in the pinned container since 2026-07-26 — but it was flaky (2/30 runs) rather than deterministic, and was annotated 'Leave this red' instead of being diagnosed. Flaky-red is indistinguishable from noise at the gate, so the defect survived ~2 months."
  guard_added: "Two deterministic fault-injection tests (window A, window F) plus an opposite-result control and a stale-marker regression test in crates/devflow-core/src/monitor.rs, driven by an LD_PRELOAD fork shim that blocks signals so 'the TERM landed in the window' is a kernel-visible precondition rather than a race. Each is mutation-proven to fail on its intended assertion (M1-M4)."
files_changed:
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/doc_check.rs
  - crates/devflow-cli/tests/gitignore_coverage.rs
  - .gitignore
  - OPERATIONS.md
oracle_type: "specified (agent must not be alive with PPid!=monitor after the monitor's trap ran; stale-marker: agent's own exit code 7 recorded)"
