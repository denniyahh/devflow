# Phase 50 Live Run — Outcome Record

**Written:** 2026-10-20, plan 50-20, from the sanitized evidence committed under `50-evidence/` and the checker output
saved with it. Every verdict line below is quoted from a saved `verdict-*.txt` file. Where a statement rests on
anything else (the raw evidence still on disk, a test run made while writing this record, or an operator note), it
says so.

**Updated:** 2026-10-21, plan 50-29, around live attempt 2 (run by 50-28 on the fixed binary) and its fault record
`fault-7`, both published under `50-evidence/`. Attempt 1 and `fault-5` stay below as the history of the pre-fix binary.

## Verdict

**SUPV-01 live record (V-28, V-29): the attempt gate says `shipped` for live attempt 2, on the fixed binary.**

From `50-evidence/attempt-2/verdict-attempt-gate.txt`, saved from
`check-supervisor-evidence.sh --attempt-gate <home> --current-source` run on the raw evidence home before sanitizing:

```
ATTEMPT_GATE current_source=yes
ATTEMPT_GATE fault=fault-7 bin_sha256=182ab8b546a8f93c3e62b33c6516448446593bdb767e8f2ff250ec3393f5045d
ATTEMPT_GATE verdict=shipped attempt=2
```

**Why attempt 2 exists.** D-18 asks for the live record on the binary the phase ships. After attempt 1, the end-of-phase
review found CR-01 (critical: a stop accepted during a `--yes-ship` Ship advance did not stop the merge) and the phase
verifier reproduced WR-03 (a completed stream run whose agent left a process holding its stdout never returned). Both
were treated as blocking and fixed in-phase (CR-01: `8946d7d`, `373f1e6`; WR-03: `07e237b`, `493156b`, `5b1c417`), so
attempt 1's binary no longer is the phase's code. Attempt 2's binary was built clean from `bca7c35`, which contains both
fixes. `current_source=yes` says the fault directory paired with it by `bin_sha256` (`fault-7`) also records the
checkout's committed crate tree.

This rests on one live attempt and one fault-arm run on the same binary. It shows that the supervisor path *can* carry
a full auto run to Ship and that the five fault cases behave as specified on that binary. It is one sample: it does not
bound how often either succeeds. The limits are listed under "What this evidence does not establish" below.

**Attempt 1, history on the pre-fix binary.** From `50-evidence/attempt-1/verdict-attempt-gate.txt`, saved by 50-20
from the plain `--attempt-gate` (`--current-source` did not exist yet):

```
ATTEMPT_GATE fault=fault-5 bin_sha256=9b0ec70c7190301067b6e8438e27ed7fb8ae2f00de90638de3608b1391c098f6
ATTEMPT_GATE verdict=shipped attempt=1
```

Attempt 1 ran on `9b0ec70c…`, built from `1deaa4d`, before the CR-01 and WR-03 fixes. It is kept as history and is not
the phase's verdict: under `--current-source` it is refused, because its paired fault record is not from the current
crate tree (50-27's real-data control).

## The record

What is committed, and how it got there (D-18, after Phase 49's D-26):

- The checkers ran on the **raw** evidence under `$HOME/.cache/devflow-phase50/`, and each output was saved inside the
  raw directory as `verdict-<check>.txt` before anything was sanitized (redaction changes path lengths, which the
  attempt checker measures):
  - `fault-<M>/verdict-fault.txt`: `check-supervisor-evidence.sh --fault <dir> --require-all`
  - `attempt-1/verdict-attempt.txt`: `check-supervisor-evidence.sh --attempt <dir>`
  - `verdict-attempt-gate.txt` in every directory: `check-supervisor-evidence.sh --attempt-gate <home>` (one run, one
    copy per directory)
  - `fault-<M>/verdict-require-all.txt` was already there: the operator saved it per the runbook. It was not rewritten.
- `attempt-1/` and `fault-5/` were copied into a scratch staging directory and passed through Phase 49's
  `tools/sanitize-evidence.py sanitize` (home paths to `~`, email addresses to `<email>`, every UUID to a stable
  `uuid-<n>` pseudonym, and a refusal on any credential shape, symlink or non-UTF-8 file). Both sanitized without a
  refusal and were moved into `50-evidence/`.
- **Curated as Phase 49 did (D-26, operator decision 2026-09-29).** Not committed: `attempt-1/project/` (the throwaway
  project checkout, which holds binary git objects), `attempt-1/collected/sessions/` (full Claude transcripts, 9.3 MB),
  `attempt-1/prelaunch/agent-env-names.txt` (the auth-probe unit's variable names) and the empty `ledger.lock`. Also
  dropped: `collected/devflow/.gitignore`, DevFlow's own runtime file holding `*`, which would have made git ignore the
  published event log. All of these stay in the raw tree until the operator's cleanup.
- **Which fault directories are published.** `fault-1` to `fault-4` are earlier passing fault-arm runs on older
  binaries (sha256 prefixes `a22575f0`, `45e9ec04`, `3dad1d76`, `5f09eeba`). The attempt gate pairs the attempt with the
  fault directory whose `bin_sha256` matches the attempt's (D-17), and that is `fault-5` only. So only `fault-5` is
  published. Each of `fault-1` to `fault-4` also printed `FAULT verdict=pass` under `--require-all` in this plan's run;
  those outputs are saved in the raw tree, not committed.
- The collector's `MANIFEST.sha256` files describe pre-sanitization bytes; they do not verify the sanitized copies.
- Checks on the published tree: `check-supervisor-evidence.sh --published 50-evidence` printed
  `PUBLISHED verdict=pass`, and the Phase 49 sanitizer's own `scan` over `50-evidence` printed nothing and exited 0.
  Negative controls, on scratch copies: a planted `/var/home/<user>/…` line and a planted hidden file naming the literal
  `$HOME` each made both `--published` (`PUBLISHED verdict=fail`) and the sanitizer scan (`home_path`) fail.
- The GitHub user name still appears in the published tree (the throwaway repository's `owner/name`, `responded_by`,
  `ls -l` owner columns). The sanitizer does not redact a bare user name by design, and Phase 49's committed evidence
  carries it the same way. It is not a home path or a credential.
- **One redaction by hand after sanitizing (operator decision 2026-10-20).** The live agent's own leak search, echoed in
  `attempt-1/collected/devflow/phase-01-stdout` and its copy `attempt-1/gates/003-ship/devflow/phase-01-stdout`, spelled
  out fragments of the operator's account email (never the full address, so the sanitizer's email rule did not match).
  Those fragments were replaced with `REDACTED-EMAIL-LOCAL` and `REDACTED-EMAIL-DOMAIN` in the published copies only
  (3 occurrences each); the raw tree is unchanged. `--published` and the sanitizer scan were re-run afterwards.

### Attempt 2's publication (50-29)

The bullets above describe 50-20's publication of attempt 1 and `fault-5`. Attempt 2 and `fault-7` went through the same
steps, on 2026-10-21:

- **Judged on the raw tree first, into new file names only.** `attempt-2/verdict-attempt.txt` (`--attempt`), one
  `--attempt-gate <home> --current-source` run copied to `attempt-2/verdict-attempt-gate.txt` and
  `fault-7/verdict-attempt-gate.txt`, and `fault-7/verdict-fault.txt` (`--fault fault-7 --require-all`). No file already
  in the raw tree was overwritten. `fault-7/verdict-require-all.txt` was already there from 50-28's run and was not
  rewritten. There is no stopped attempt between 2 and N-1 (N is 2).
- **Which fault directory is published.** Only `fault-7`, the one the gate pairs with attempt 2 by `bin_sha256`.
  `fault-6` (`bc862759…`, the CR-01-fixed binary before the WR-03 fix) is not paired with any attempt and stays in the
  raw tree.
- **Curated as for attempt 1.** Not committed: `attempt-2/project/`, `attempt-2/collected/sessions/`,
  `attempt-2/prelaunch/agent-env-names.txt`, the empty `ledger.lock` and `collected/devflow/.gitignore`. Published:
  82 files in `attempt-2/`, 12 in `fault-7/`.
- **The sanitizer refused attempt 2 once, and the refusal was correct.** It printed `github_token_prefix` and
  `credential_named_assignment` for line 171 of `collected/devflow/phase-01-stdout` and for the same line of its copy
  `gates/003-ship/devflow/phase-01-stdout`. The throwaway project's own security review, run by the live agent, had
  planted three made-up credentials (an AWS access key id, an AWS secret access key and a `ghp_` GitHub token) in a
  scratch `ctl/leak.txt` as a gitleaks negative control, and the command that wrote them is in the stream transcript.
- **Redacted by hand in the published copy (operator decision 2026-10-21, "Redact in published copy").** In the staged
  copy only, each of the three values was replaced with `REDACTED-PLANTED-CONTROL-SECRET` in both files: one occurrence
  of each value per file, so 3 replacements per file and 6 in all. A count of each value over every staged file found
  them in those two files only. The raw tree is unchanged. The `ghp_` value is not the host's `gh auth token` (compared
  by equality in a script that printed neither); whether it is valid anywhere else is not known. The values were never
  printed. Both ways: the sanitizer refused the unredacted staged copy (exit 1, four `refused:` lines, no destination
  left) and accepted the redacted one; gitleaks found 2 findings (`aws-access-token`, `github-pat`) in the raw file and
  none in the published tree.
- **No email fragment needed redacting.** `--identity-scan` (the host's Claude account address and its git
  `user.email`, 9 tokens) printed `IDENTITY tokens=9` and `IDENTITY verdict=pass` on both sanitized copies and on the
  whole published tree, so no `REDACTED-EMAIL-*` replacement was made for attempt 2. The raw attempt-2 tree does hold
  the full account address and its fragments, in 16 files, all under `collected/sessions/`, which is not published. The
  scan's real-data control: the raw attempt-1 capture fails it with two `source=account` hit lines (a `local` and a
  `domain` run).
- **Checks on the published tree** (after the move, before the commit): `--published 50-evidence` printed
  `PUBLISHED verdict=pass`; `--published 50-evidence --attempt 2` printed `PUBLISHED attempt=2 verdict_line=shipped`,
  `PUBLISHED attempt_current_source=yes`, `PUBLISHED attempt_fault=fault-7` and `PUBLISHED verdict=pass`; the Phase 49
  sanitizer's `scan` printed nothing and exited 0; `gitleaks dir` reported no finding.

### Attempts

| Attempt | Outcome | Binary `bin_sha256` | Build commit | Claude | GSD |
|---------|---------|---------------------|--------------|--------|-----|
| 1 (history: pre-fix binary) | shipped (`verdict.json`: `"outcome":"shipped"`, `"workflow_shipped":true`) | `9b0ec70c7190301067b6e8438e27ed7fb8ae2f00de90638de3608b1391c098f6` | `1deaa4d` (`build_dirty=false`) | 2.1.304 | 1.18.0 |
| 2 (fixed binary; the verdict) | shipped (`verdict.json`: `"outcome":"shipped"`, `"workflow_shipped":true`, `"stop_note":null`) | `182ab8b546a8f93c3e62b33c6516448446593bdb767e8f2ff250ec3393f5045d` | `bca7c35` (`build_dirty=false`) | 2.1.306 | 1.18.0 |

**Attempt 2.** From `50-evidence/attempt-2/identity.txt`, `verdict.json` and `collected/devflow/events.jsonl`. The run
started 2026-10-21T20:28:05Z and finished 20:51:15Z on the private throwaway repository `denniyahh/p50-live-attempt-2`
(PR #1, `feature/phase-01` into `main`, open). **It was performed by the orchestrating Claude session under the
operator's delegation of 2026-10-21 ("Yes, but you do the run")**: preparing, the fault arm, creating and pushing the
throwaway repository, launching, answering the Define and Code gates (the seeded D-07 preflight refusals, approved at
20:30:08Z and 20:31:18Z) and collecting. `--yes-ship` answered Ship. Deleting the throwaway repository stays with the
operator. No attempt was stopped between attempt 1 and attempt 2.

`50-evidence/attempt-2/verdict-attempt.txt`:

```
CHECK passed=11 failed=0
ATTEMPT p49_positive=pass
ATTEMPT check=events readable ok
ATTEMPT check=stage_launched present ok
ATTEMPT check=every launch names its socket, label and id ok
ATTEMPT check=both launch shapes ok
ATTEMPT check=no monitor_pid key ok
ATTEMPT launches=6 stream=5 capture=1
ATTEMPT sidecars_left=0
ATTEMPT verdict=pass
```

It also carries `NOTE gate_census count=3 stages=define,code,ship` and `NOTE gate <stage> unexpected=false` for each
gate, with the same two meanings of "unexpected" as attempt 1 (below).

**Attempt 1.** From `50-evidence/attempt-1/identity.txt` and `verdict.json`. The run started 2026-10-20T18:51:41Z and
finished 19:26:30Z on the private throwaway repository `denniyahh/p50-live-attempt-1` (PR #1, `feature/phase-01` into
`main`).

`50-evidence/attempt-1/verdict-attempt.txt` (the Phase 49 positive arm, then the Phase 50 launch checks):

```
CHECK passed=11 failed=0
ATTEMPT p49_positive=pass
ATTEMPT check=events readable ok
ATTEMPT check=stage_launched present ok
ATTEMPT check=every launch names its socket, label and id ok
ATTEMPT check=both launch shapes ok
ATTEMPT check=no monitor_pid key ok
ATTEMPT launches=6 stream=5 capture=1
ATTEMPT sidecars_left=0
ATTEMPT verdict=pass
```

The same file carries `NOTE gate_census count=3 stages=define,code,ship` and one `NOTE gate <stage> unexpected=false`
per gate. **Two meanings of "unexpected":** the collector's `unexpected=false` means the gate is in the runbook's
expected set. DevFlow's own `gate_fired` events carry `"unexpected":true` for Define and Code, meaning a never-silent
gate that auto mode would not normally fire: here the seeded D-07 preflight refusals the disposable project plants on
purpose. Both values are correct under their own definitions; one is not to be read as the other.

### Fault arm (V-29)

**On attempt 2's binary.** `50-evidence/fault-7/verdict-fault.txt` (`--fault fault-7 --require-all`, run by 50-29 on
the raw directory):

```
FAULT case=alive-stop-capture ok live
FAULT case=alive-stop-stream ok live
FAULT case=kill9-at-gate ok live
FAULT case=kill9-stale-stop ok live
FAULT case=publish-unlink-window ok pointer
FAULT case=stop-races-completion ok live
FAULT bin_sha256=182ab8b546a8f93c3e62b33c6516448446593bdb767e8f2ff250ec3393f5045d
FAULT complete=yes
FAULT verdict=pass
```

`fault-7/src_matches_checkout` is `true`, and `fault-7/verdict-require-all.txt` (50-28's run) holds the same lines. Per
case (from the committed `case-*.json`), as for `fault-5` below: every live case probed `ALIVE` before the stop and
`GONE` after it, the stop exited 0, `next_stage_launched` is `false` and `members_after` is empty; both kill cases probed
`STALE` after the SIGKILL; `kill9-at-gate` recorded `lock_holder_after: "none"` and `gate_response_written: false`;
`stop-races-completion` recorded `first_agent_exit: "143"` and `advance_ran: false`.

**On attempt 1's binary (history).** `50-evidence/fault-5/verdict-fault.txt` (`--fault fault-5 --require-all`, run on
the raw directory):

```
FAULT case=alive-stop-capture ok live
FAULT case=alive-stop-stream ok live
FAULT case=kill9-at-gate ok live
FAULT case=kill9-stale-stop ok live
FAULT case=publish-unlink-window ok pointer
FAULT case=stop-races-completion ok live
FAULT bin_sha256=9b0ec70c7190301067b6e8438e27ed7fb8ae2f00de90638de3608b1391c098f6
FAULT complete=yes
FAULT verdict=pass
```

`fault-5/src_matches_checkout` is `true`. The operator's own `fault-5/verdict-require-all.txt` carries the same
`FAULT verdict=pass` line. The fault arm runs a built `devflow` with fake agents and a private cache; the capture case
reaches the capture shape through `--legacy-claude-launch`, with the fake agent standing in for `claude`.

Per case (from the committed `case-*.json`): every live case probed `ALIVE` before the stop and `GONE` after it, the
stop exited 0, `next_stage_launched` is `false` and `members_after` is empty. The two kill cases probed `STALE` after
the SIGKILL of the supervisor. `kill9-at-gate` recorded `lock_holder_after: "none"` and
`gate_response_written: false`. `publish-unlink-window` is a pointer to the fixture test
`pipeline_launch::tests::a_kill_between_the_next_publish_and_the_old_unlink_leaves_state_on_the_live_next_supervisor`,
not a live case.

### Live launch records (V-28)

**Attempt 2.** The six `stage_launched` events in `50-evidence/attempt-2/collected/devflow/events.jsonl` (sanitized;
`~` is the home directory):

| Stage | `launch` | `launch_id` | `socket_path` |
|-------|----------|-------------|---------------|
| define | `stream` | `18e0a580d5f5a82e` | `~/.cache/devflow/sock/14dce5a1c1366f6d-01-18e0a580d5f5a82e.sock` |
| plan | `stream` | `18e0a5840b0e4e26` | `~/.cache/devflow/sock/14dce5a1c1366f6d-01-18e0a5840b0e4e26.sock` |
| code | `stream` | `18e0a58d55c6b722` | `~/.cache/devflow/sock/14dce5a1c1366f6d-01-18e0a58d55c6b722.sock` |
| code (resume) | `capture` | `18e0a5b5a11e0224` | `~/.cache/devflow/sock/14dce5a1c1366f6d-01-18e0a5b5a11e0224.sock` |
| validate | `stream` | `18e0a61347900216` | `~/.cache/devflow/sock/14dce5a1c1366f6d-01-18e0a61347900216.sock` |
| ship | `stream` | `18e0a61fcc71eb82` | `~/.cache/devflow/sock/14dce5a1c1366f6d-01-18e0a61fcc71eb82.sock` |

As in attempt 1, the capture launch follows the `checkpoint_auto_decided` event for Code. Measured by 50-29 on the
**raw** events: every socket path is 77 bytes and its directory is `$HOME/.cache/devflow/sock`, under the 107-byte bound.
`supervisor/status.txt` shows `stage: idle` and no supervisor line. The raw `events.jsonl` holds 43 events and **0
`stream_wait_bounded` events** (counted two ways: parsing every event's name, and a fixed-string search that found no
line).

**Attempt 1 (history).** The six `stage_launched` events in `50-evidence/attempt-1/collected/devflow/events.jsonl`
(sanitized; `~` is the home directory):

| Stage | `launch` | `launch_id` | `socket_path` |
|-------|----------|-------------|---------------|
| define | `stream` | `18e051ef6de593d6` | `~/.cache/devflow/sock/b42190c734d0b40c-01-18e051ef6de593d6.sock` |
| plan | `stream` | `18e051f1ae4a918c` | `~/.cache/devflow/sock/b42190c734d0b40c-01-18e051f1ae4a918c.sock` |
| code | `stream` | `18e051fc65e607fb` | `~/.cache/devflow/sock/b42190c734d0b40c-01-18e051fc65e607fb.sock` |
| code (resume) | `capture` | `18e05236188c40a0` | `~/.cache/devflow/sock/b42190c734d0b40c-01-18e05236188c40a0.sock` |
| validate | `stream` | `18e052f3f672869b` | `~/.cache/devflow/sock/b42190c734d0b40c-01-18e052f3f672869b.sock` |
| ship | `stream` | `18e053094cb68223` | `~/.cache/devflow/sock/b42190c734d0b40c-01-18e053094cb68223.sock` |

The capture launch follows the `checkpoint_auto_decided` event for Code: the seeded `blocking-human` checkpoint was
auto-decided and the resumed turn ran on the capture-shape supervisor. Measured on the **raw** events (the sanitized
`~` is shorter): every socket path is 77 bytes and its directory is `$HOME/.cache/devflow/sock`, the default
`registry::cache_dir()` with no override: not the project tree and not `$XDG_RUNTIME_DIR`. 77 bytes is under the
107-byte bound the attempt checker enforces. `supervisor/status.txt` (collected after the run) shows `stage: idle` and
no supervisor line.

## Criterion mapping

Tests are named by V-id (`50-VALIDATION.md`). Each named test below was run again by exact name while writing this
record (one `cargo nextest run --no-tests=fail -E 'test(=<name>)'` per test, 26 runs) on a tree whose `crates/`,
`Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml` are unchanged since `1deaa4d`, the attempt binary's build commit:
26 passed. A misspelled-name control in the same run failed, so a pass is not an empty filter.

**Re-run for attempt 2 (50-29, 2026-10-21).** Every test row 4 cites that was added after `1deaa4d` was run again by
exact name, one `cargo nextest run --no-tests=fail -E 'test(=<path>)'` per test (14 runs: the WR-03 tests A, B, C, D, F,
G, E1, E2, E3, the doc check, and CR-01's four), at `2096b85`, whose `crates/`, `Cargo.toml`, `Cargo.lock` and
`rust-toolchain.toml` are unchanged since `bca7c35`, attempt 2's build commit: 14 passed (`1 test run: 1 passed` each).
A misspelled-name control in the same run exited 4 (`0 tests run`, `error: no tests to run`). No crate test changed
after `1deaa4d` other than these additions (the 50-26 and CR-01 diffs append tests and change production code only), so
the 26 tests above were not re-run.

| Criterion | What it requires | Tests (V-ids) | Live record lines |
|-----------|------------------|---------------|-------------------|
| 1 | GONE / STALE / ALIVE from state plus socket, no PID; each fixture produces only its state | V-01, V-02, V-03, V-04 (discrimination; mutants in 50-05), V-05 | Fault arm: every live case `probe_before` `ALIVE`, `probe_after` `GONE`; both kill cases `probe_after_kill` `STALE` |
| 2 | `PipeOwning` and `Legacy` answer liveness identically | V-08 (non-Claude adapter, both arms), V-09, V-10 | Attempt 2 (and attempt 1 before it): `ATTEMPT launches=6 stream=5 capture=1`, `ATTEMPT check=both launch shapes ok`; the `stage_launched` tables above (`stream` and `capture`). Fault arm (`fault-7`, and `fault-5` before it): `alive-stop-stream` and `alive-stop-capture` both `ok live` |
| 3 | Socket path persisted, under the cache dir, `sun_path` overflow refused by name | V-06, V-07 | Attempt 2: six recorded `socket_path`s under `$HOME/.cache/devflow/sock`, each 77 bytes raw (`ATTEMPT check=every launch names its socket, label and id ok`); the same for attempt 1 |
| 4 | `stop` over the socket: group killed, 143, no advance, socket removed; lock-free run; stop racing completion never launches the next stage | V-14, V-15, V-17. **CR-01** (a stop during a `--yes-ship` Ship advance): `pipeline_launch::tests::a_stop_after_safe_point_1_is_honoured_before_the_yes_ship_approval` and `pipeline_launch::tests::a_stop_before_finalization_is_honoured_before_the_merge`, with their no-stop controls `pipeline_launch::tests::without_a_stop_after_safe_point_1_the_yes_ship_run_ships` and `pipeline_launch::tests::without_a_stop_before_finalization_the_run_ships`. **WR-03** (a completed stream run whose stdout a leftover process holds): A `monitor::tests::a_completed_stream_run_returns_while_a_leftover_process_holds_the_agents_stdout`, C `monitor::tests::no_line_reaches_the_capture_after_the_stdout_wait_expires`, D `monitor::tests::a_stopped_stream_run_returns_while_a_process_outside_the_agent_group_holds_its_stdout`, F `monitor::tests::a_stream_run_returns_while_a_leftover_process_keeps_writing_to_the_agents_stdout`, E1 `a_completed_stream_run_reaches_its_advance_while_a_leftover_process_holds_stdout`, E3 `a_stop_sent_while_a_completed_run_waits_on_a_held_stdout_ends_it_at_safe_point_1`; controls B `monitor::tests::a_completed_stream_run_returns_once_its_leftover_process_released_the_agents_stdout` and E2 `a_completed_stream_run_reaches_its_advance_once_its_leftover_process_released_stdout`; the revert check (`wr03-revert-check.sh run`, 50-26) printed `REVERT verdict=pass`, with A, C, D, F, E1 and E3 failing (`rc=100 marker=yes`) and B and E2 passing on the reverted fix. On the WR-03 path these clauses hold (E3): the stop sent while the supervisor is `advancing` is accepted and honoured at safe point 1 (state `stopped`, one `advance_stopped` with `safe_point` 1, no next stage), the socket is removed and `devflow stop` exits 0; D: a stop while the agent runs, its stdout also held outside its group, ends with exit file `143`, advance suppressed and the socket GONE. Two clauses do **not** hold on the completed-run path and are not claimed: no agent group is killed, because the agent had already exited and an in-group leftover keeps running (999.164 item 5); and the exit file keeps the agent's own code `0` while the `stopped` line reports 143 (999.166 item 6) | Fault arm (`fault-7`, and `fault-5` before it): `alive-stop-stream`, `alive-stop-capture`, `stop-races-completion` `ok live`, each `next_stage_launched: false`, `members_after: []`, `probe_after` `GONE`. No stop was sent in either live attempt, and attempt 2's log has 0 `stream_wait_bounded` events, so neither fix's path ran live |
| 5 | SIGTERM / SIGINT behave like a socket `shutdown` | V-16 | None live: no signal was sent to a supervisor in the attempt or in the fault arm |
| 6 | STALE backstop signals only a validated group; previous boot removed; leader gone refused | V-18, V-19, V-20, V-21 | Fault arm: `kill9-stale-stop` and `kill9-at-gate` `ok live` (leader-present path only; the refusal paths are fixture-only) |
| 7 | Sockets listed and probed with no state; `doctor` reports orphans; read-only commands never kill or remove; `start`/`resume` refuse over a live sidecar when state will not load | V-22, V-23, V-36, V-37, V-49 | `ATTEMPT sidecars_left=0` (attempt 2, and attempt 1): no sidecar left after a run to Ship. The refusal paths did not occur live |
| 8 | Recycled-pid lock reclaim by start time; e2e tests isolate `HOME`, `XDG_CACHE_HOME`, `DEVFLOW_CACHE_DIR` and reap supervisors | V-24, V-25, V-26 | None live (test-suite property) |

V-28 (the live rig) is closed by attempt 2 and its saved verdicts (`ATTEMPT_GATE verdict=shipped attempt=2`,
`ATTEMPT_GATE current_source=yes`). V-29 (the fault arm on the attempt's binary) is closed by `fault-7` and its saved
verdicts (`FAULT verdict=pass`). Attempt 1 and `fault-5` closed them on the pre-fix binary and are kept as history.

## What this evidence does not establish

- **One live run is a weak bound.** One shipped attempt shows the path can work on this binary. A run that failed 1
  time in 5 would still pass a single attempt about 80% of the time. Nothing here measures a rate.
- **The capture shape was reached live only through the Claude checkpoint resume.** No non-Claude agent ran live, and
  the fault arm's capture case uses a fake agent standing in for `claude`. Parity for non-Claude agents rests on V-08
  (`monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter`), a unit test.
- **STALE frequency in the field is unmeasured.** STALE was produced only by the fault arm's deliberate SIGKILLs. How
  often a real run leaves a STALE socket is not known.
- **macOS is unverified beyond compiling.** The phase-end macOS gate (V-56) is `cargo check` only, so nothing was
  linked or run on macOS. The macOS peer uid check (`getpeereid`) and the off-Linux backstop refusal are unverified at
  run time. Socket supervision is unverified on macOS: the 103-byte macOS `sun_path` limit is enforced by a constant
  only, and no macOS host or CI has run it.
- **The publish/unlink window is fixture-only.** The fault arm records it as a pointer to its fixture test, never as a
  live case.
- **Only the happy path ran live.** No stop, kill or signal was sent during attempt 1 or attempt 2. Criteria 4-6 rest on
  the fault arm's synthetic cases and on tests; criterion 5 (signals) and criteria 7-8 have no live record at all. In
  particular, the WR-03 and CR-01 stop paths were not exercised live in attempt 2: they rest on their tests (row 4), on
  50-26's revert check and on the fault arm.
- **The race case was won by the stop.** In the fault arm, `stop-races-completion` is recorded with the stop winning
  (`first_agent_exit: "143"`, `advance_ran: false` in `fault-1` to `fault-5`, and in `fault-7`). The branch where the agent completes first and the stop reaches the advance rests on
  the 50-13 and 50-23 fixtures.
- **Attempt 2's gates were answered by the orchestrating Claude session**, under the operator's delegation of
  2026-10-21. `responded_by` records the same user name whoever typed the answer, so the event log cannot tell the
  session's answers from the operator's. The gates answered were the seeded D-07 refusals, not an unplanned gate.
- **A stop honoured at a safe point after a completed run leaves the exit file with the agent's own code** (`0`) while
  the parked caller's `stopped` line says 143. E3 asserts the `0`; it is backlog 999.166 item 6, which names this limit.
- **WR-03's 3 s stream-wait bound is not a measured need.** It rests on reading the code and on 50-26's tests. After
  the supervisor exits, a leftover process that writes to the agent's old stdout gets SIGPIPE or EPIPE: DevFlow sends
  nothing, that is the pipe closing.
- **A stream now ends at the agent leader's exit, not at stdout EOF.** A launcher whose leader exits before the children
  it started finish writing would lose what they write more than 3 s later (50-26's limit). Checked while planning
  50-26: `claude` is a native ELF binary with no wrapper process, and the `agy` wrapper `exec`s
  `mise exec … -- antigravity-cli`, so it does not stay behind as a separate leader; whether `mise exec` and
  `antigravity-cli` exec or wait on a child was not checked.
- **WR-03's condition did not occur live.** Attempt 2's raw `events.jsonl` holds **0** `stream_wait_bounded` events, so
  no stream wait was cut short by a held stdout in that run. That says nothing about how often the condition occurs.
- **`--identity-scan` covers two identifiers only:** the host's Claude account email and its git `user.email` values.
  The GitHub user name still appears in the published tree by design (see "The record"), and any other personal
  identifier is not scanned.
- **The run used seeded gates.** Define and Code fired on the disposable project's planted D-07 preflight refusals and
  the seeded `blocking-human` checkpoint was auto-decided by design. That shows nothing about an unplanned gate.
- **The checks on the published tree are about content, not meaning.** `--published` and the sanitizer scan look for
  home-path spellings and credential shapes; they do not check file names for a bare user name, and they do not judge
  whether a capture says something sensitive in prose.

## Defects

**No non-blocking defect found** by 50-18, 50-19 or 50-28 (live attempt 2), so no backlog entry was added.

Fixed in-phase:

- **The backstop's leader-gone refusal suggested `ps -g <pgid>`**, which on procps-ng selects a session, so it would
  list no member of a group that still had members (found by 50-18). Fixed by `ce6e718`
  (`fix(50-09): point the backstop's group hint at pgrep -g, not ps -g`); `deferred-items.md` records it resolved.
- **50-18's planned member recipe used the same `ps -g`**, so a fault record would have shown `members_after: []`
  whether or not the stop worked. 50-18 fixed it in `fault-arm.sh` before committing the tool (members are read from
  `/proc/<pid>/stat` field 5), with a selftest in both directions.
- **The operator's guided attempt script did not say that the Define gate's D-07 refusal text was the expected seeded
  gate** (found by 50-19), so the operator first declined it at the script's prompt. The script, which is outside this
  repository, was fixed during the attempt and resumed at the gate step. No gate response was written in between: the
  events hold exactly one `gate_resolved` for Define. No repository commit.

Not defects: the script's watcher being stopped once during Code (no stop or abort event in `events.jsonl`), and a
cosmetic `systemctl --user reset-failed` "Unit not loaded" message (50-19 deviations 2 and 3).

Already filed this phase by the 2026-10-19 external review (`reviews/2026-10-19-TRIAGE.md`), not re-filed here:
backlog 999.164 (`doctor`'s orphan list, probe timeouts in a stop's grace, `.part` leaks, connection flooding, members
left after a normal completion) and 999.165 (`gate_notify_cmd` through `sh -c`).

**Found after this record was written, by the end-of-phase code review (`50-REVIEW.md`, 2026-10-20):**
- **CR-01 (critical): a stop accepted during a `--yes-ship` Ship advance did not stop the merge and release.** The
  pre-written approval was read before the stop flag, and no safe point stood between safe point 1 and `finish_workflow`.
  Fixed in-phase by operator decision (2026-10-21): `8946d7d` (tests that failed on the old code: the run shipped) and
  `373f1e6` (safe point 5 before an auto-answered gate, safe point 6 at the start of finalization), each with a no-stop
  control that ships. **Live attempt 1 ran on `9b0ec70c…`, which predates this fix**, so the live run does not exercise
  the fixed path. The fault arm was re-run on the fixed binary: `fault-6` (`bin_sha256=bc862759f27c…`) prints
  `FAULT verdict=pass` under `--require-all`, and `--fault-current` passes (raw tree only, not published). A copy of the
  record holding only `fault-5` fails `--fault-current` (`dir=ABSENT`), the control.
- **WR-03 (warning) was reproduced by the phase verifier on 2026-10-21** with a `/dev/null` control: a completed stream
  run whose agent left a process holding its stdout waited one idle timeout and then forever, and `devflow stop` could
  not end it. Fixed in-phase by operator decision (50-26): `07e237b` and `5b1c417` (tests, revert-checked by
  `60b1538`'s `wr03-revert-check.sh`) and `493156b` (the loop ends at the agent's exit, the thread wait is bounded at
  3 s, nothing is signalled, the capture is closed at the bound, and a `stream_wait_bounded` event records it).
  Attempt 2 ran on the fixed binary.
- The review's other two warnings and three infos stay deferred to backlog 999.166 (operator decision 2026-10-21),
  whose item 6 now also names the exit-file limit (the exit file keeps the agent's code while `stopped` says 143).

**Live attempt 2 (50-28).** It found no defect: no unexpected gate, no deviation from the runbook, no `deviations.txt`
(50-28's SUMMARY), and its judges all passed. Not defects: the three made-up credentials the live agent's own security
review planted as a gitleaks negative control in the throwaway project (the sanitizer refused them, as designed, and
they were redacted in the published copy by operator decision, see "The record"); and `surv-watch.py analyze`
printing nothing to the orchestrator's output tail, which 50-28 left unjudged: its report is in
`attempt-2/surv/analysis.txt` (`writers=7`, `interleaved_windows=0`, `overflow_windows=0`, `stop_marker=present`, one
observation of one run, as the file itself says).

## Phase-end macOS compile gate (V-56, MAC-3)

**Re-run 2026-10-21 by 50-29 on the final tree** (`ae5c853` plus this plan's uncommitted record edits; crate sources
unchanged since `bca7c35`, which holds the CR-01 and WR-03 fixes), rustc 1.97.1: the same two commands exited 0, with
no `warning` or `error` line. Compile only, as below.

Run 2026-10-20 on `97b5def` (crate sources unchanged since `1deaa4d`), rustc 1.97.1:
`cargo check --target aarch64-apple-darwin -p devflow-core --all-targets && cargo check --target aarch64-apple-darwin -p devflow --all-targets`
exited 0, with no `warning` or `error` line in its output. So no plan after 50-05 and 50-09 left an ungated
Linux-only `libc` item (`E0425`) in either package. This is a compile check only: nothing was linked or run on macOS
(see "What this evidence does not establish").

## Release handoff (3.0.0)

**1. Merge the phase PR into `develop` with a merge commit, never squash it and never use auto-merge.**

```
gh pr merge <PR> --merge --match-head-commit <sha>
```

Auto-merge squashes on this repository (CONTRIBUTING.md "Cutting a Release" step 6); the `develop` ruleset allows both
methods, so the choice is the operator's. Why it matters: the 3.0.0 entry is drafted at the release cut from the
subjects of the non-merge commits in the release range on `develop`, by `devflow ship`'s VersionBump hook
(`version::changelog_sections`) or by `scripts/cut-release.sh branch`'s suggested entry for a hand cut. 50-17's commit
`b629cd0` `docs(50-17): state socket supervision is unverified on macOS` is the one that carries the macOS note into
that draft. A squash replaces the phase's commit subjects with the PR title, and a section written by hand can drop
the sentence even after a merge commit.

**2. Check the 3.0.0 section.** The check, verbatim from 50-20-PLAN.md:

```
bash -c 'R="${1:-origin/main}"; cd "$(git rev-parse --show-toplevel)" || exit 2; case "$R" in origin/*) git fetch -q origin "${R#origin/}" || exit 2;; esac; L=$(mktemp -d); trap "rm -rf \"$L\"" EXIT; git show "${R}:CHANGELOG.md" > "$L/c.md" || exit 2; awk "/^## /{p=(\$2==\"3.0.0\")} p" "$L/c.md" > "$L/s.md"; n=$(wc -l < "$L/s.md"); m=no; rg -q -i -F "unverified on macOS" "$L/s.md" && m=yes; echo "ref=$R changelog_3_0_0_lines=$n macos_note=$m"; [ "$n" -ge 1 ] && [ "$m" = yes ]' _ origin/main
```

When to run it:

- **Before the release PR:** on the release branch, with `HEAD` in place of the final `origin/main`, before
  `scripts/cut-release.sh pr-develop`.
- **After the release:** as written, once the release PR has merged to `main`.

It prints `ref=<ref> changelog_3_0_0_lines=<N> macos_note=yes|no`. Exit 0: the `## 3.0.0` section exists and says
"unverified on macOS" (any case). Exit 1: the section is missing or does not say it. Exit 2: `CHANGELOG.md` cannot be
read at that ref (or the fetch failed).

**3. Repair when it exits 1.**

- **Before the release PR:** add this sentence to the 3.0.0 section on the release branch, then re-run the check:
  "Socket supervision is unverified on macOS: the 103-byte macOS `sun_path` limit is enforced by a constant only, and
  no macOS host or CI has run it."
- **After the release:** add the same sentence to the 3.0.0 section through a `docs` PR into `develop`, so the next
  release carries it to `main`, and update the published notes now with
  `gh release edit v3.0.0 --notes-file <file>` (the GitHub Release is how the section reaches users, CONTRIBUTING.md
  step 7).

This phase does not write `CHANGELOG.md`: both release paths write that section.

## Cleanup pending (operator)

- The private throwaway repository `denniyahh/p50-live-attempt-1` and its open PR #1 remain, per the runbook: delete it
  only after Phase 50 verification.
- The raw evidence under `$HOME/.cache/devflow-phase50/` (including the transcripts, the project checkout and
  `fault-1` to `fault-4`) stays until the operator's cleanup. This plan added only `verdict-*.txt` files there.
