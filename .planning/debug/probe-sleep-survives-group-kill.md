---
status: investigating
trigger: "agents::opencode::tests::spawn_with_timeout_kills_a_hung_child failed once in the pinned CI container: the stub's sleep descendant was still alive 2s after spawn_with_timeout SIGKILLed the probe's process group."
created: 2026-09-22
updated: 2026-09-22
---

# Debug: Probe Sleep Survives the Group Kill

## Symptoms

- Expected behavior: when `spawn_with_timeout` times out, it SIGKILLs the probe's process group, so the stub's backgrounded `sleep` is gone well within the test's 2-second poll.
- Actual behavior: once, the sleep was still alive after 2 seconds: `the timed-out probe's sleep descendant must be dead: pid 11529` (`crates/devflow-core/src/agents/opencode.rs`, the assertion after the reap poll).
- Error messages: only that line. **The test captures nothing about the surviving process** — no state, PPid, pgid or cmdline — so it is not knowable from the failure whether the pid was still the sleep, a zombie, or a reused pid. Log: `/home/denniyahh/.claude/jobs/28cb39f7/tmp/container-gate5.clean.log` (job-local, not durable).
- Timeline: first and only observation 2026-09-22, in a full `scripts/check-in-container.sh all` at `1d93895`. Not seen in 30 subsequent `cargo test -p devflow-core --lib` container runs at the same commit, nor in 4 earlier full container runs that day — about 1 in 35 core-library executions. Phase 48 rewrote this probe's containment (`681af07`, `2f596ab`, `3c90755`) and added the 2-second reap poll this assertion sits behind.
- Reproduction: not reproduced on demand. Container loop: `/home/denniyahh/.claude/jobs/28cb39f7/tmp/flake-loop2.sh <N> <repo_root> <out_dir>` (pinned image, `taskset -c 0,1`, same mounts and per-checkout cache volumes as `scripts/check-in-container.sh`), ~20 s per run.

## Current Focus

- status: **NOT ROOT-CAUSED, session left open.** Diagnostics landed (`e683f89`); the bounded
  reproduction campaign is finished and did not reproduce. No fix made, none proposed.
- hypothesis (leading, UNCONFIRMED — nothing below has been demonstrated): the SIGKILL reached the
  process group and the containment code is correct, but the dying `sleep` had not been scheduled
  to process it within the test's two seconds. `kill` makes an interruptible sleeper runnable; it
  must then be scheduled before it can die, and until then `/proc/<pid>/status` reads `State: R`,
  which `agent_running` correctly reports as alive. Under `taskset -c 0,1`, a full
  `check-in-container.sh all` (fmt + clippy + build, then 830 parallel tests on two CPUs) is a far
  heavier load than the bare test loop. Falsifiable: the next dump must show `State=R` (or `D`),
  `pgid == probe leader pid`, and `kill path: ... reached the group=true`. Anything else refutes it.
- supporting asymmetry (the strongest empirical clue, and it argues the campaign just run was the
  wrong experiment): the single observed failure came from a full `check-in-container.sh all` run —
  1 of 5 full-gate runs that day — while 0 of 70 bare `cargo test -p devflow-core --lib` container
  runs have reproduced it. If load is the trigger, the per-run rate under the full gate is
  roughly an order of magnitude higher, and further bare-suite runs are low-yield.
- next_action (for whoever picks this up): do NOT run more bare-suite loops. Either wait for the
  next natural full-gate failure — it is now self-explaining — or, to force the issue, run
  `scripts/check-in-container.sh all` repeatedly (~15 runs, ~1 h) and read any dump against the
  signature table below. A deterministic alternative that needs an operator decision first: pin a
  stub `sleep` to the same CPU as a busy-spin load and SIGSTOP/SIGCONT it around the group kill to
  hold a dying task off the run queue on purpose.
- constraints: do not widen the 2-second poll to make the symptom disappear; module-qualified
  exact test names must show `1 passed`; any fix needs an opposite-result control; warm host runs
  have hidden this class before, so container evidence is what counts. Any change to production
  containment behaviour needs a failing test first AND an operator checkpoint.
- superseded note (kept for the record): the original candidate list was (a) ESRCH fallback to
  `child.kill()`, (b) the surviving pid was not the sleep, (c) the sleep was in a different
  process group, (d) SIGKILL delivery delayed past 2 s. All four survive as candidates and are
  now individually distinguishable from a single failure — see the signature table.

## Evidence

- timestamp: 2026-09-22 (source audit, reasoning only — not observation)
  checked: the deadline branch of `spawn_with_timeout`, `kill_probe_group`, and Linux's group-kill
  semantics.
  found: in the deadline branch the leader is always alive or an unreaped zombie, because Rust's
  `Child` holds it until `child.wait()` runs after the kill. `kill(-pgid)` returns `ESRCH` only
  when no task in the group is signalable, and an unreaped zombie still is.
  implication: the `child.kill()` leader-only fallback should be unreachable in this test, so
  candidate (a) is a priori unlikely. This is inference, not evidence, which is exactly why the
  kill path is now recorded rather than argued.

- timestamp: 2026-09-22
  checked: `agent_running` / `is_zombie` in `crates/devflow-core/src/agent.rs` against the
  container's process topology.
  found: `agent_running` returns true when `kill(pid, 0)` succeeds and `/proc/<pid>/status` is
  *unreadable*, because `is_zombie` deliberately answers "not a zombie" when it cannot tell. That
  is a real false-positive path. But docker runs `taskset -c 0,1 cargo test` as PID 1, and cargo
  reaps only its own child by pid — it never calls `wait(-1)` — so an orphaned descendant that
  dies in this container becomes a zombie that is never reaped, and its `/proc/<pid>/status`
  therefore stays readable and reads `Z` indefinitely.
  implication: the false-positive path needs the pid to be *fully* reaped, which this container
  does not do for orphans. Candidate (c) drops to a low prior — and the `State` field in the new
  dump settles it either way.

- timestamp: 2026-09-22
  checked: whether the new dump actually produces discriminating values, by forcing the assertion
  to fail against a live process deliberately placed outside the probe's group (temporary
  fault-injected test, run once on the host and then removed — not committed).
  found: the dump fired and every field was populated and correct — `kill path: deadline branch:
  kill(-pgid, SIGKILL) reached the group=true, child.kill() leader-only fallback used=false`,
  `probe leader pid: 3789333`, `stub pidfile: 3789336`, survivor `pgid=3789351` (≠ leader, as
  planted), `Name=sleep`, `State=R` then `S`, identical `starttime` across before/after, and
  `leader group now: <no members left>`.
  implication: the instrumentation is verified by firing, not by inspection. It also incidentally
  demonstrated correct containment: the real probe group was empty while the planted foreign-group
  process survived — the opposite result the measurement needed to be able to produce.

- timestamp: 2026-09-22
  checked: `cargo test -p devflow-core --lib` on the host at `e683f89` after the change.
  found: 830 passed, 0 failed (829 before; the added one is the diagnostics' negative control).
  `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean.
  implication: the instrumentation is green, but a green host run says nothing about the flake —
  the host has never reproduced this class.

- timestamp: 2026-09-22 (reproduction campaign)
  checked: 40 runs of `cargo test -p devflow-core --lib` in the pinned image at `e683f89`
  (`flake-loop2.sh`, same mounts, per-checkout cache volumes and `taskset -c 0,1` as
  `scripts/check-in-container.sh`), ~16 s per run.
  found: 0 failures. Tallied four independent ways so a silently-broken run could not read as a
  pass: 40 `run=` lines, 0 non-zero exit codes, 0 runs with a non-empty `failed=[...]` list, and
  0 runs missing the expected `test result: ok. 830 passed` line.
  implication: not reproduced. Combined with the earlier 30 runs, that is 0/70 bare-suite
  container runs against 1/5 full-gate runs. 0/70 bounds the bare-suite per-run rate at roughly
  under 4% with 95% confidence — it does **not** show the defect is absent, and it does not
  contradict a load-gated mechanism, because the bare suite is the low-load case.

- timestamp: 2026-09-22 (full gate, and one more full-gate repro sample)
  checked: `scripts/check-in-container.sh all` at `e683f89`.
  found: exit 0 with the final `==> check.sh: all OK` line, 0 `FAILED` occurrences after
  stripping `\x1b[..m` and `\x1b(B` (the strip mattered — 1499 lines of the raw log contained
  ESC), `830 passed` for the core library, and
  `agents::opencode::tests::probe_proc_snapshot_separates_a_live_process_from_a_reaped_one`
  present by name in the output, so the new negative control actually ran rather than being
  filtered out.
  implication: the instrumentation passes the canonical gate. As a repro sample it is one clean
  full-gate run — nowhere near enough to move the estimate given the base rate is ~1 in 5.

### Candidate causes and their predicted dump signatures

The next occurrence is meant to be read straight off this table.

| # | Candidate (Ishikawa category) | Signature in the dump | Prior |
|---|---|---|---|
| a | Group kill returned ESRCH, leader-only `child.kill()` fallback ran (code) | `kill path: ... reached the group=false, ... fallback used=true` | low — see source-audit evidence |
| b | The polled pid was never the sleep: `$!` named something else, or the pidfile was misread (code/data) | `stub pidfile` ≠ polled pid, or `Name=sh`, or `cmdline` is not `sleep N` | low |
| c | `agent_running` false positive on an unreadable `/proc/<pid>/status` (code) | `survivor after: GONE (...)` despite the poll reporting it alive, or `State=Z` | low — orphans are never reaped in this container |
| d | **SIGKILL delivered but the dying task was not scheduled (or was in uninterruptible sleep) inside the 2 s** (environment/load) | `State=R` or `State=D`, `pgid == leader pid`, `reached the group=true`, survivor still listed in `leader group now` | **leading** |
| e | Fork/group-kill race: the sleep joined the pgrp after `kill(-pgid)` enumerated it (kernel) | as (d) but `starttime` is *later* than the kill | low — `copy_process` aborts a fork under `tasklist_lock` when `fatal_signal_pending`, and `__kill_pgrp_info` iterates under the same lock |
| f | Descendant left the probe's process group despite the pre-exec seccomp guard (code/config) | `pgid` ≠ probe leader pid | very low for this stub (it never calls setsid/setpgid), but it would invalidate the containment claim, so it must be visible |
| g | pid reuse — the pid named an unrelated process by assert time (data) | `starttime` differs between `before` and `after`, or `Name`/`cmdline` changed | very low — needs a full pid wraparound inside 2 s |

AND-gate: a two-independent-cause failure is not indicated. The leading candidate (d) is one cause
with a precondition (container-wide CPU/IO load), which is consistent with the full-gate-only
observation rather than requiring a second contributing defect.

**If (d) is what the next dump shows, the correct response is still not to widen the window.** It
would mean the test's two-second bound is measuring scheduler latency under a 2-CPU pin rather
than containment, and the decision about what to assert instead is the operator's, not the
debugger's.

## Eliminated

## Resolution

root_cause: **NOT FOUND.** Not reproduced in 40 pinned-container runs of the core library at
`e683f89` (0/70 including the earlier 30), nor in one full `check-in-container.sh all`. Source
reading still does not explain the one observation, and no fix has been made or proposed for the
probe's containment: inventing one for a mechanism that has not been demonstrated would be worse
than leaving this open.

fix: none. Diagnostics only (`e683f89`) — the three probe-containment tests now dump the
survivor's `/proc` identity and state, the remains of the probe's process group, and which kill
path `spawn_with_timeout` took. The two-second poll was deliberately left at two seconds.

verification: `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -d
warnings` clean; 830/830 core library tests pass on the host; `scripts/check-in-container.sh all`
exits 0 with `==> check.sh: all OK`. The dump itself was verified by *firing* it under injected
fault, not by reading it — see the third Evidence entry.

files_changed: [crates/devflow-core/src/agents/opencode.rs]

status: open, NOT ROOT-CAUSED. Leave this session active. The next occurrence is expected to be
self-explaining: read its dump against the signature table above.
