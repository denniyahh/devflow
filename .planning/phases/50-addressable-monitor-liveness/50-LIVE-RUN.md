# Phase 50 Live Run — Outcome Record

**Written:** 2026-10-20, plan 50-20, from the sanitized evidence committed under `50-evidence/` and the checker output
saved with it. Every verdict line below is quoted from a saved `verdict-*.txt` file. Where a statement rests on
anything else (the raw evidence still on disk, a test run made while writing this record, or an operator note), it
says so.

## Verdict

**SUPV-01 live record (V-28, V-29): the attempt gate says `shipped`.**

From `50-evidence/attempt-1/verdict-attempt-gate.txt`, saved from `check-supervisor-evidence.sh --attempt-gate` run on
the raw evidence home before sanitizing:

```
ATTEMPT_GATE fault=fault-5 bin_sha256=9b0ec70c7190301067b6e8438e27ed7fb8ae2f00de90638de3608b1391c098f6
ATTEMPT_GATE verdict=shipped attempt=1
```

This rests on one live attempt and one fault-arm run on the same binary. It shows that the supervisor path *can* carry
a full auto run to Ship and that the five fault cases behave as specified on that binary. It is one sample: it does not
bound how often either succeeds. The limits are listed under "What this evidence does not establish" below.

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

### Attempts

| Attempt | Outcome | Binary `bin_sha256` | Build commit | Claude | GSD |
|---------|---------|---------------------|--------------|--------|-----|
| 1 | shipped (`verdict.json`: `"outcome":"shipped"`, `"workflow_shipped":true`) | `9b0ec70c7190301067b6e8438e27ed7fb8ae2f00de90638de3608b1391c098f6` | `1deaa4d` (`build_dirty=false`) | 2.1.304 | 1.18.0 |

From `50-evidence/attempt-1/identity.txt` and `verdict.json`. The run started 2026-10-20T18:51:41Z and finished
19:26:30Z on the private throwaway repository `denniyahh/p50-live-attempt-1` (PR #1, `feature/phase-01` into `main`).

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

`50-evidence/fault-5/verdict-fault.txt` (`--fault fault-5 --require-all`, run on the raw directory):

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

The six `stage_launched` events in `50-evidence/attempt-1/collected/devflow/events.jsonl` (sanitized; `~` is the home
directory):

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

| Criterion | What it requires | Tests (V-ids) | Live record lines |
|-----------|------------------|---------------|-------------------|
| 1 | GONE / STALE / ALIVE from state plus socket, no PID; each fixture produces only its state | V-01, V-02, V-03, V-04 (discrimination; mutants in 50-05), V-05 | Fault arm: every live case `probe_before` `ALIVE`, `probe_after` `GONE`; both kill cases `probe_after_kill` `STALE` |
| 2 | `PipeOwning` and `Legacy` answer liveness identically | V-08 (non-Claude adapter, both arms), V-09, V-10 | Attempt 1: `ATTEMPT launches=6 stream=5 capture=1`, `ATTEMPT check=both launch shapes ok`; the `stage_launched` table above (`stream` and `capture`). Fault arm: `alive-stop-stream` and `alive-stop-capture` both `ok live` |
| 3 | Socket path persisted, under the cache dir, `sun_path` overflow refused by name | V-06, V-07 | Six recorded `socket_path`s under `$HOME/.cache/devflow/sock`, each 77 bytes raw (`ATTEMPT check=every launch names its socket, label and id ok`) |
| 4 | `stop` over the socket: group killed, 143, no advance, socket removed; lock-free run; stop racing completion never launches the next stage | V-14, V-15, V-17 | Fault arm: `alive-stop-stream`, `alive-stop-capture`, `stop-races-completion` `ok live`, each `next_stage_launched: false`, `members_after: []`, `probe_after` `GONE` |
| 5 | SIGTERM / SIGINT behave like a socket `shutdown` | V-16 | None live: no signal was sent to a supervisor in the attempt or in the fault arm |
| 6 | STALE backstop signals only a validated group; previous boot removed; leader gone refused | V-18, V-19, V-20, V-21 | Fault arm: `kill9-stale-stop` and `kill9-at-gate` `ok live` (leader-present path only; the refusal paths are fixture-only) |
| 7 | Sockets listed and probed with no state; `doctor` reports orphans; read-only commands never kill or remove; `start`/`resume` refuse over a live sidecar when state will not load | V-22, V-23, V-36, V-37, V-49 | `ATTEMPT sidecars_left=0`: no sidecar left after a run to Ship. The refusal paths did not occur live |
| 8 | Recycled-pid lock reclaim by start time; e2e tests isolate `HOME`, `XDG_CACHE_HOME`, `DEVFLOW_CACHE_DIR` and reap supervisors | V-24, V-25, V-26 | None live (test-suite property) |

V-28 (the live rig) is closed by attempt 1 and its saved verdicts. V-29 (the fault arm on the attempt's binary) is
closed by `fault-5` and its saved verdicts.

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
- **Only the happy path ran live.** No stop, kill or signal was sent during attempt 1. Criteria 4-6 rest on the fault
  arm's synthetic cases and on tests; criterion 5 (signals) and criteria 7-8 have no live record at all.
- **The race case was won by the stop.** In the fault arm, `stop-races-completion` is recorded with the stop winning
  (`first_agent_exit: "143"`, `advance_ran: false` in all five fault-arm runs, `fault-1` to `fault-5`). The branch where the agent completes first and the stop reaches the advance rests on
  the 50-13 and 50-23 fixtures.
- **The run used seeded gates.** Define and Code fired on the disposable project's planted D-07 preflight refusals and
  the seeded `blocking-human` checkpoint was auto-decided by design. That shows nothing about an unplanned gate.
- **The checks on the published tree are about content, not meaning.** `--published` and the sanitizer scan look for
  home-path spellings and credential shapes; they do not check file names for a bare user name, and they do not judge
  whether a capture says something sensitive in prose.

## Defects

**No non-blocking defect found** by 50-18 or 50-19, so this plan adds no backlog entry.

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
