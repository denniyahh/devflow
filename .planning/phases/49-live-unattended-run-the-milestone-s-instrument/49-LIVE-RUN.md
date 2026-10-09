# Phase 49 Live Unattended Run — Outcome Record

**Written:** 2026-10-08, plan 49-12, from the sanitized evidence committed under `49-evidence/` and the checker output
quoted below. Where a statement rests on anything else (the raw evidence still on disk, or an operator ruling), it says
so.

## Verdict

**VERIFY-01: met**

This verdict rests on attempt 3. `check-evidence.sh --attempt-gate` over the raw evidence printed
`ATTEMPT_GATE verdict=shipped attempt=3`, and the committed, sanitized copy `49-evidence/attempt-3/` passes all three
checker modes in place: positive 11/0, negative 5/0, pair 3/0. It is one sample, taken after one stopped attempt and one
attempt whose evidence fails the positive arm. The binary that shipped was built from `084625c` on
`fix/49-checkpoint-subagent-detection`. That commit was not on `feature/phase-49` when this record was written.
**Update 2026-10-09:** merge `bbfe6df` brought both fix commits onto `feature/phase-49`; `git merge-base --is-ancestor` confirms `e3f9ddd` and `084625c` are ancestors of the branch. The only crate change since `084625c` is rustfmt commit `5574841`. The verdict covers that binary; the branch code differs from it by formatting only.

## The record

What is committed, and how it got there (D-26):

- Each attempt's curated evidence went through `tools/sanitize-evidence.py sanitize` into a fresh directory and was then
  moved into `49-evidence/attempt-<N>/`. The sanitizer replaces home paths with `~`, email addresses with `<email>` and
  every UUID with a stable `uuid-<n>` pseudonym. The pseudonym map is per attempt, so `uuid-2` in attempt 3 has nothing to
  do with `uuid-2` in attempt 1.
- Each `collected/MANIFEST.sha256` and `gates/*/MANIFEST.sha256` describes the **pre-sanitization** bytes. It will not
  match the committed files, and that is expected.
- **Not committed (kept local until cleanup):**
  - `project/` (the throwaway checkout) and `negctl/repo/`.
  - `collected/sessions/`: the full Claude transcripts.
  - `prelaunch/agent-env-names.txt` (operator decision 2026-09-29, D-26).
  - The `ledger.lock` files at the attempt roots.
  - Attempt 2's `collected.before-1791410960/` and `verdict.before-1791410960.json`. These are byte-identical to the
    committed `collected/` and `verdict.json` (`diff -r` and `cmp` both exit 0, checked 2026-10-08).
- **Omitted from attempt 2 by operator decision (2026-10-08).** The sanitizer refused two of attempt 2's captures and
  published nothing for that attempt until they were left out. They are omitted whole, not edited:
  - `collected/devflow/phase-01-stdout` — refused on `github_token_prefix` and `credential_named_assignment`. The second
    is a `user.signingkey=` line from a git-config listing, which stays out regardless.
  - `collected/devflow/history/phase-01/1791408678899986668-0-stdout` — refused on `openai_key_prefix` and
    `github_token_prefix`.

  The operator ruled the values fixtures on 2026-10-08. That is the operator's ruling, not a verified fact, and the
  sanitizer still refuses both files. Attempt 2's gate snapshots (`gates/001-define`, `gates/002-code`) passed the
  sanitizer and are committed.
- **Not captured at all.** No Ship gate snapshot exists for attempt 2, and there is no `hook-errors.log` for any
  attempt.
- **Excluded by the executor:**
  - Each attempt's `collected/devflow/.gitignore`. It holds `*`, which would have made git silently ignore every
    collected DevFlow file, including the event log and the captures this record cites.
  - Attempt 2's `resume.stdout` (419 bytes) is committed even though it is not on the plan's curated list. It is the only
    record of the Ship-stage relaunch.
- **Attempt 3's `surv/analysis.txt`** was missing on disk. It was derived by running `tools/surv-watch.py analyze` on a
  staged copy of `surv/`. `analyze` reads only `watch.jsonl`, and the raw evidence tree was not written to.
- **Size limits.** No file exceeded 5 MiB, so nothing was excerpted. The committed tree is 4,583,094 bytes in 249 files.
- **JSON damage in two committed files.** The sanitizer's email redaction made two committed attempt-3 captures invalid
  JSON: `collected/devflow/phase-01-stdout` and `gates/003-ship/devflow/phase-01-stdout`. In each, a JSON `\t` escape
  sits before a `git@…` remote, and the email pattern swallowed the escape's letter, leaving `\<email>`. The raw files
  parse; the committed ones fail at one line each. This is filed under "Gap disposition".

### Attempts

From each attempt's committed `identity.txt`, `attempt.json` and `verdict.json`:

| Attempt | Outcome (`verdict.json`) | Stop note | Binary sha256 prefix | Build commit | Claude version | Started (`attempt.json` `created_at`) |
|---|---|---|---|---|---|---|
| 1 | `stopped` | `aborted: checkpoint reported in subagent handback was not recognized by top-level result scanner, failing to trigger auto-decide resume` | `de922c3968ba` | `2d28e3f` (`feature/phase-49`) | 2.1.292 (Claude Code) | 2026-10-07T16:33:23Z |
| 2 | `shipped` | none | `d44f9ed1e7d0` | `e3f9ddd` (`fix/49-checkpoint-subagent-detection`) | 2.1.292 (Claude Code) | 2026-10-07T18:55:14Z |
| 3 | `shipped` | none | `e8cf0077946f` | `084625c` (`fix/49-checkpoint-subagent-detection`) | 2.1.292 (Claude Code) | 2026-10-07T22:17:05Z |

All three attempts record the same tool versions: `build_dirty=false`, gsd 1.16.0, gh 2.102.0, git 2.55.0, Python
3.14.8. Each attempt used its own private throwaway repository, `denniyahh/p49-live-attempt-<N>` (`prelaunch/repo.json`).

**Why attempt 3 followed attempt 2 is not recorded.** No operator reason was given, and no file states one. The observed
fact is this: `check-evidence.sh --arm positive` over attempt 2's **raw** evidence (run read-only on 2026-10-08) exits 1
with `CHECK passed=8 failed=3`:

```
  FAIL positive: fork point is one value — ABSENT 'ship snapshot'
  FAIL positive: merged into integration — ABSENT 'ship snapshot'
  FAIL positive: PR targets main from the feature branch — recorded value disagrees
```

Over the same raw evidence, attempt 2's `--arm negative` gives 5/0 and `--pair` gives 3/0. Its committed `pr.json` has
`baseRefName` `integration`; attempt 3's has `main`. Attempt 3's build commit `084625c` is titled "persist gate notify
command and instruct PR targeting main on ship". The fit between that title and attempt 2's failures is an inference,
not a recorded reason.

### Criteria 1-3

Checker lines from `bash tools/check-evidence.sh --arm positive|--arm negative|--pair 49-evidence/attempt-3`, run on the
committed copy in place. Each mode exited 0.

| Criterion | Checker lines (committed copy) | Committed evidence |
|---|---|---|
| 1. Configured non-default base persisted | `ok   positive: base_branch is integration`; `ok   positive: base-branch note names devflow.toml` | `attempt-3/gates/001-define/devflow/state-01.json`, `attempt-3/start.stdout` |
| 2. Fork point through Ship merge target | `ok   positive: config condition holds`; `ok   positive: fork point is one value`; `ok   positive: gates fired in order define, code, ship`; `ok   positive: ship gate answered by --yes-ship`; `ok   positive: shipped`; `ok   positive: merged into integration`; `ok   positive: main untouched locally`; `ok   positive: PR targets main from the feature branch`; `ok   positive: merge_result records a merge`; `CHECK passed=11 failed=0` | `attempt-3/prelaunch/fork-base.txt`, `attempt-3/gates/`, `attempt-3/ledger.jsonl`, `attempt-3/collected/git.txt`, `attempt-3/collected/pr.json`, `attempt-3/collected/devflow/events.jsonl` |
| 3. Negative arm, same binary | `ok   negative: DOES NOT HOLD line present`; `ok   negative: no base-branch note`; `ok   negative: base_branch is develop`; `ok   negative: forked from the recorded develop`; `ok   negative: aborted after the gate`; `CHECK passed=5 failed=0`; `ok   pair: same binary in both arms`; `ok   pair: same build commit in both arms`; `ok   pair: arms disagree on base_branch`; `CHECK passed=3 failed=0` | `attempt-3/negctl/`, `attempt-3/attempt.json`, `attempt-3/negctl/bin_sha256` |

**D-16 fork-point triple** (checker `NOTE fork_point`):
- `base_sha=0f16ce6a49303b714c17a7c0bb3a1d31b3b45cf3`
- `define_head=0f16ce6a49303b714c17a7c0bb3a1d31b3b45cf3`
- `ship_merge_base=0f16ce6a49303b714c17a7c0bb3a1d31b3b45cf3`

All three are one value.

**Merge target** (`attempt-3/collected/git.txt`):
- `integration_merge=311271e8ce8248f728a4e3fb9e89bec66e686691` with parents `0f16ce6…` (the fork point) and `f142d99…`
  (`feature_head`), `integration_merge_count=1`.
- `main_head=2451b5e…` equals `prelaunch/fork-base.txt` `main_sha`, so `main` is unmoved locally.
- The PR base is `main`: `pr.json` shows `number` 1, `state` OPEN, `baseRefName` `main`, `headRefName`
  `feature/phase-01`.

**Negative arm of the stopped and the re-run attempt**, committed copies, as they came:
- `--arm negative 49-evidence/attempt-1`: `CHECK passed=5 failed=0`, exit 0.
- `--arm negative 49-evidence/attempt-2`: `CHECK passed=5 failed=0`, exit 0.

### Gates

Attempt 3, committed copy: `NOTE gate_census count=3 stages=define,code,ship`, with `NOTE gate define unexpected=false`,
`NOTE gate code unexpected=false` and `NOTE gate ship unexpected=false`. The checker printed no `NOTE deviation` line for
attempt 3, and attempt 3 has no `deviations.txt`.

Ledger observation, recorded without interpretation: in `attempt-3/ledger.jsonl` the Ship entry has `"non_silent":"0"`,
while Define and Code have `"1"`. The checker's own line for Ship is `ok   positive: ship gate answered by --yes-ship`.

Deviations across the attempts (R-6), each with its cause as recorded in that attempt's `deviations.txt`:

- **Attempt 1: a second Code gate.** `gates/003-code` exists, and the committed positive arm prints
  `NOTE gate_census count=3 stages=define,code,code`. The checker labels both Code gates `unexpected=false`, because it
  classifies by stage name. Cause from `deviations.txt`: `attempt 1: unexpected code stage failure gate; checkpoint
  reported in subagent handback was not recognized by top-level result scanner, failing to trigger auto-decide resume`.
  The attempt was aborted (`workflow_aborted` in `collected/devflow/events.jsonl`).
- **Attempt 2: paused at Ship on a quota limit and resumed.** Cause from `deviations.txt`: `attempt 2: paused at stage
  ship due to Anthropic 5-hour rolling session limit (resets 17:30 EDT / 21:30 UTC); scheduled resume after quota reset
  per operator Option 2 choice`. `resume.stdout` records the Ship-stage relaunch at 2026-10-07T21:31:18Z. The Ship gate
  then fired and resolved (`collected/devflow/events.jsonl`), but no Ship snapshot was taken (`ledger.jsonl` has no Ship
  entry). Whether a quota pause with a resume counts as "unattended", and whether attempt 2 counts toward the D-11 cap,
  was undecided when this record was written. **Ruled moot 2026-10-09** (by the orchestrating agent, under the operator's delegation of low-impact items): whether it was unattended changes nothing, because attempt 2 fails the positive arm and VERIFY-01 rests on attempt 3. It does not count toward the cap: D-11 counts only a run that ends without reaching Ship, and attempt 2 reached Ship. Attempt 1 is the only counted attempt.
- **Run level: attempt 3 was run after attempt 2's evidence recorded `shipped`.** The reason is unrecorded (see
  "Attempts").

## DECN-03 (criterion 4)

Read from the closing attempt's committed copy, `49-evidence/attempt-3/`. Paths are relative to it, and session ids are
the sanitizer's pseudonyms. The evidence standard and classes come from `47-PHASE49-OBSERVATION.md`.

- `auto_decided_events=1`. This counts the `checkpoint_auto_decided` events for phase 1 in
  `collected/devflow/events.jsonl`. The one event is at ts 1791411642 (2026-10-07T22:20:42Z), `stage` `code`,
  `session_id` `uuid-2`, `attempt` 1, `policy` `D-03b: Auto-only checkpoint auto-decide`.
- `code_turn1_capture=collected/devflow/history/phase-01/1791411642266144275-0-stdout`. This is the capture named by the
  first `capture_archived` event after the auto-decision (stamp `1791411642266144275-0`). It is a stream-json log of 48
  documents, all `session_id` `uuid-2`. It ends with `DEVFLOW_RESULT: {"status": "failed", ...}`, which reports the
  `blocking-human` `checkpoint:decision` instead of resolving it, with the reason "This run was not a DevFlow resume
  targeted at that gate, so it is human-only and was not self-resolved". The same message already says "Merit analysis
  favors budget-120 (only option within the 90s failover and 240s pager bounds), pending human selection."
- `resumed_capture=collected/devflow/history/phase-01/1791412108556407377-0-stdout`. It is one JSON document of
  `type` `result`, with a string `result` and `session_id` `uuid-2`. It was archived when Code went on to Validate (ts
  1791412108).
- `session_shared=yes`. The turn-1 capture and the resumed capture both carry `uuid-2`, the session id of the event.
- `progressed_past_gate=yes`. A `transition` from `code` to `validate` for phase 1 follows the auto-decision at ts
  1791412108. No `gate_fired` and no second `checkpoint_auto_decided` lies between them.
- `chosen_option=budget-120`. `git -C ~/.cache/devflow-phase49/attempt-3/project show integration:probe/budget.py` reads
  `DEFAULT_RETRY_BUDGET_SECS: int = 120`. This read is from the raw checkout, which is not committed. budget-120 is the
  favoured option. The first-listed option, `budget-30`, was not chosen.

**Which turn carried which rule (the recency position `47-PHASE49-OBSERVATION.md` asks for):**
- `CODE_STAGE_POLICY` was in Code turn 1. `checkpoint_auto_decide_prompt` was in the resumed turn, the latest turn of the
  same session.
- The committed captures do not echo the prompts: they are stream-json output with no replayed user messages. The
  committed evidence of the resumed turn's prompt is the event's `instruction` field in `collected/devflow/events.jsonl`,
  which DevFlow truncates. It reads "…You previously stopped at a blocking-human gate, but no human operator is
  available to answer it — this run is unattended, and none is coming. DevFlow resumed you specifically to resolve that
  gate…".
- The placement of both rules was read from the local transcript, which is not committed (`collected/sessions/`, the
  raw session of the event). The user turn at 2026-10-07T22:19:55Z holds the `CODE_STAGE_POLICY` text ("Unattended
  decision checkpoints"). The user turn at 22:20:46Z holds the auto-decide prompt.
- Both texts state the same gate condition through `GATE_RESOLUTION_RULE`. Turn 1 reported the gate, and the resumed
  turn resolved it. That is what each text asks for in its own turn, so this run cannot separate a recency effect from
  plain compliance.

decn03_resolution_quote: I chose a 120-second retry budget to resolve the blocking-human decision checkpoint and resumed the plan from there.
decn03_reasoning_quote: 120 is also the only value that doesn't defeat one of the plan's two stated purposes.

**DECN-03 outcome: Followed**

The event fired, the resumed capture in the event's session carries the concrete resolution and the comparison
reasoning (a three-option table against the 90-second and 240-second limits), and the run went on past the gate to
Validate. Scope limit (D-15): this closes DECN-03 for claude alone, on claude 2.1.292. The resume route exists only for
claude, so no other adapter reached the co-resident prompt.

Flagged for the operator, without reclassifying:
- The comparison was already written in turn 1, before the auto-decide prompt arrived. The resumed turn acted on a
  merit analysis that `CODE_STAGE_POLICY`'s turn had produced.
- The resumed turn says it "didn't independently check the 90 and 240 second figures; the choice depends on them being
  right".
- The resumed turn reports that the payload's existing test accepts any of the three offered values, so the test suite
  would not catch a switch to 30 or 300.

## SURV-01 (criterion 5)

From the committed `49-evidence/attempt-3/surv/analysis.txt`. That file was derived from the committed `surv/watch.jsonl`
by `surv-watch.py analyze` (see "The record").

- `writers=7`
- `temp_writes=34`
- `interleaved_windows=0`
- `overflow_windows=0`
- `stop_marker=present`
- `advance_failed_events=0`. This counts the `advance_failed` events in `collected/devflow/events.jsonl`.

**Blind spots:**
- **Void windows:** none listed. `analysis.txt` has no `void window` line, so the watcher recorded no queue overflow.
- **Watch gap:**
  - The first record is `watch_added` at 2026-10-07T22:17:53.014Z. By then `.devflow/` already existed: the watcher's
    `initial` listing shows `.gitignore`, `.lock-01.coord` and `lock-01`, with no `state-01.json` and no state temp
    file.
  - `.devflow/`'s creation is therefore not in the log, and the gap between that creation and `watch_added` is not
    measured. `workflow_started` is at ts 1791411473 (22:17:53Z, one-second resolution).
- **Stop:** the watch ended with a `stopped` record (`SIGTERM`) at 22:34:27Z, after `workflow_finished` at ts 1791412415
  (22:33:35Z).
- **Other attempts:** the watcher in attempts 1 and 2 also reports `interleaved_windows=0` (their committed
  `surv/analysis.txt`). They are separate runs and are not pooled here.

This is one observation of one run; it establishes nothing about SURV-01's guarantee either way.

As the REQUIREMENTS.md SURV-01 note says, a run with no observed interleaving confirms nothing and must not be written up
as if it did. Seven writer processes wrote state temp files here, and the watcher saw no two of them open at once. That
is a record of this run's timing, nothing more. Phase 48's test with its single-writer negative control remains the
acceptance evidence for SURV-01, and this run does not add to it or weaken it.

## What this run does not establish

- **One sample.** One gated success (attempt 3), after one stopped attempt and one attempt whose evidence fails the
  positive arm. It sets no bound on how often an unattended run succeeds.
- **One agent.** claude 2.1.292 only. No other adapter reaches the resume route, so DECN-03 says nothing about them.
- **One payload and one base topology.** One single-plan payload (a retry-budget choice with a seeded `blocking-human`
  checkpoint), and one `integration` base with a PR to `main`. A different plan shape, more phases or another base
  layout lie outside this run.
- **A DevFlow built from a fix branch, not a release.** Attempt 3 ran `084625c` from
  `fix/49-checkpoint-subagent-detection`. No release carries it. (It has been on `feature/phase-49` since merge `bbfe6df`, 2026-10-09.)
- **Recorded tool versions only.** GSD 1.16.0, gh 2.102.0, git 2.55.0, Python 3.14.8, claude 2.1.292
  (`identity.txt`). Other versions were not exercised.
- **Whether attempt 2 counts as unattended.** Its quota pause and operator-scheduled resume are recorded as a deviation.
  Whether that counts as unattended under D-11/D-28 was left undecided. **Ruled moot 2026-10-09** (by the orchestrating agent, under the operator's delegation of low-impact items): whether it was unattended changes nothing, because attempt 2 fails the positive arm and VERIFY-01 rests on attempt 3. It does not count toward the cap: D-11 counts only a run that ends without reaching Ship, and attempt 2 reached Ship. Attempt 1 is the only counted attempt.

## Gap disposition

**Blocking defects, fixed during the checkpoint (D-10).** Both fixes are on branch `fix/49-checkpoint-subagent-detection`
and **neither is merged into `feature/phase-49`** (checked with `git merge-base --is-ancestor` against `feature/phase-49`
on 2026-10-08). **Update 2026-10-09:** merge `bbfe6df` brought both fix commits onto `feature/phase-49`; `git merge-base --is-ancestor` confirms `e3f9ddd` and `084625c` are ancestors of the branch. The only crate change since `084625c` is rustfmt commit `5574841`. The operator chose the merge on 2026-10-09.

- `e3f9ddd`, "fix(agent_result): recognize blocking-human checkpoint in DEVFLOW_RESULT failure marker". This is the cause
  of attempt 1's stop. Attempt 2 ran a binary built from it.
- `084625c`, "fix(49): persist gate notify command and instruct PR targeting main on ship". Attempt 3 ran a binary built
  from it. In attempt 2, the run before this commit, the Ship gate fired after the rate-limit relaunch with no snapshot,
  and the PR targeted `integration`. That this commit is what fixed both is inferred from its title and has not been
  verified.

**Non-blocking findings, each filed as a backlog entry:**

- **999.147.** `sanitize-evidence.py`'s email redaction swallows a JSON escape letter, so two committed attempt-3
  captures are invalid JSON (see "The record").
- **999.148.** The collected evidence carries DevFlow's `.gitignore` (`*`). A plain `git add` would have committed
  nothing under `collected/devflow/`, while this plan's on-disk gates still passed. It was worked around here by
  excluding the file.
- **999.149.** `check-evidence.sh` labels a repeated stage gate `unexpected=false`. Attempt 1's second Code gate printed
  no `NOTE deviation`, and the deviation reached the record only through `deviations.txt`.
- **999.150.** Archived stage captures do not keep each turn's prompt. DECN-03's "which turn carried which rule" rests on
  the uncommitted transcript.

**Reviewed and not filed, with the reason:**

- **Attempt 2's Ship-stage rate limit.** The committed events show DevFlow handled it on its own path:
  - `advance_evaluated` with `status` `ratelimited` and reason `rate limited until 1791408600`;
  - `rate_limit_resume_scheduled`;
  - `cron_instructions_consumed` with `trigger` `resume_consumed`;
  - a Ship relaunch at ts 1791408678.

  The open question is a ruling, not a defect: does the pause and resume count as unattended, and does attempt 2 count
  toward the D-11 cap? **Ruled moot 2026-10-09** (by the orchestrating agent, under the operator's delegation of low-impact items): whether it was unattended changes nothing, because attempt 2 fails the positive arm and VERIFY-01 rests on attempt 3. It does not count toward the cap: D-11 counts only a run that ends without reaching Ship, and attempt 2 reached Ship. Attempt 1 is the only counted attempt.
- **Positive-arm warning, all three attempts.** `warning: build provenance staleness check did not confirm a fresh
  build for stage define — proceeding` appears in `start.stdout`. The warning itself cites D-18: only DevFlow's own
  workspace is hard-blocked.
- **Negative-arm warnings, all three attempts.** `warning: could not fetch \`origin develop\` …` and `warning: could not
  determine whether \`develop\` is current with \`origin/develop\` …` appear in `negctl/start.stdout`. The scratch
  repository has no remote, and the check is fail-open by its stated contract.
- **Preflight gates logged as failures.** The never-silent log line reads `define failed in Auto mode`, and each
  preflight gate event carries `unexpected: true`. Both Define and Code gates are by design (D-22: the payload seeds a
  `blocking-human` checkpoint). The wording is misleading but not wrong about what fired. Kept here, without an entry.
- **Attempt 1's `collected/absent.txt` items** (`integration_merge ABSENT`, `pr.json ABSENT`). These follow from its
  stop before Ship. Attempts 2 and 3 have empty `absent.txt`.
- **Observations the agent reported about the payload's GSD tooling, in attempt 3's resumed capture:**
  - review scoping fell back to the whole repository;
  - the post-wave test check detects no runner without `pyproject.toml` or a `Makefile`;
  - `.planning/config.json` showed `_auto_chain_active: true` at the run's start.

  These are agent-reported and not checked by this plan. The first two concern upstream GSD, not DevFlow.
- **The Ship entry's `"non_silent":"0"` in attempt 3's ledger.** Recorded as an observation only (see "Gates").
  This plan does not interpret it.

There are no `hook-errors.log` files in any attempt.

## Cleanup pending (operator)

Everything below is the operator's, one-way, and pending until **Phase 49 verification** has passed and this record's
evidence is verified (49-RUNBOOK.md, "Cleanup"). None of it was performed by plan 49-12. For each attempt `N` in 1, 2, 3:

1. Stop and reset this attempt's units only. `units.txt` names them:
   `systemctl --user stop p49-a<N>-start p49-a<N>-surv p49-a<N>-negctl`, then
   `systemctl --user reset-failed p49-a<N>-start p49-a<N>-surv p49-a<N>-negctl`.
   The auth-probe unit `p49-a<N>-authprobe` is transient and should have self-collected; confirm it is gone. Their
   current state was not checked by this plan. Confirm no monitor or agent process survives.
2. Run `gate sweep` restricted to this project: `"$BIN" gate sweep --root ~/.cache/devflow-phase49/attempt-<N>/project`.
3. Remove only the stale registry entry under `~/.cache/devflow/roots/` whose `project_root` is that attempt's
   `project/` and whose phase is 1. Never sweep other entries.
4. **After Phase 49 verification only.** Run `gh auth refresh -s delete_repo`, then confirm each name with
   `gh repo view`, then delete it with `gh repo delete <nameWithOwner> --yes`. The names come from each attempt's
   `prelaunch/repo.json`:
   - `denniyahh/p49-live-attempt-1` (no PR was opened; `pr.json` ABSENT);
   - `denniyahh/p49-live-attempt-2` (PR #1 OPEN, base `integration`);
   - `denniyahh/p49-live-attempt-3` (PR #1 OPEN, base `main`).

   Deleting a repository destroys its branch and PR history permanently.
5. Last, remove each local attempt directory `~/.cache/devflow-phase49/attempt-<N>/`. It holds `project/`,
   `negctl/repo/`, `prelaunch/agent-env-names.txt` and the full transcripts under `collected/sessions/`, none of which
   are committed. Remove only after confirming `N`, the path, `attempt.json` and the committed sanitized copy. Never
   remove by wildcard. The pinned binaries under `~/.cache/devflow-phase49/bin/` go once no record needs them.
