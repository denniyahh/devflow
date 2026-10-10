---
phase: 49
reviewers: [codex, antigravity]
reviewed_at: 2026-09-26T21:59:29Z
reviewed_tree: 30435ee0ecc3577e8a40963a40d7c9437d35f5b4
reviewed_base_commit: 051175e
plans_reviewed: [49-01-PLAN.md, 49-02-PLAN.md, 49-03-PLAN.md, 49-04-PLAN.md, 49-05-PLAN.md, 49-06-PLAN.md, 49-07-PLAN.md, 49-08-PLAN.md, 49-09-PLAN.md, 49-10-PLAN.md, 49-11-PLAN.md, 49-12-PLAN.md, 49-13-PLAN.md, 49-14-PLAN.md, 49-15-PLAN.md, 49-16-PLAN.md, 49-17-PLAN.md, 49-18-PLAN.md]
models:
  codex: "gpt-5.6-terra (reasoning=high)"
  antigravity: "gemini-3.8-flash-high (effort=high)"
model_sources:
  codex: "cli-flag"
  antigravity: "cli-flag"
---

# Cross-AI Plan Review — Phase 49 (re-review of the 18-plan revision)

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

**What was reviewed:** the uncommitted 18-plan revision, pinned as Git tree `30435ee` (`feature/phase-49` at
`051175e` plus 18 dirty `.planning/` paths), in a disposable shared clone with push disabled. Lanes were run by hand
per the `external-review` skill, not by `/gsd-review` (its roster includes dead lanes on this host). The
`gsd-plan-checker` ran in reviews mode against the same bytes; its report is included below as a third section but it
is not a cross-AI lane. The real worktree's tree hash was re-measured after every lane and still equals `30435ee`.

**Lane outcomes:** codex completed in 5 min (211k tokens, ~40 `file:line` citations). antigravity completed in
10 min (31 KB, 43 file links). Neither lane dropped. The antigravity section below has its absolute `file://` link
prefixes shortened to repo-relative paths; the text is otherwise verbatim.

## Consensus Summary

Every finding below was checked against the source by the orchestrator before being listed as CONFIRMED. The prior
2026-09-25 findings are largely closed: codex judges 16/19 closed and 3 partial, antigravity 19/19 closed, and the
checker finds all incorporated except codex #1, which is present but defective.

### Agreed Strengths

- Archive staging, manifest-before-ref ordering and the verified state move (prior #2/#3) are specified with
  interruption controls (all three).
- Ledger serialisation, SURV void-window handling, the C1-C5 token/evidence binding, and the backlog numbering rule
  close their prior findings (all three).

### Agreed Concerns (CONFIRMED against source)

1. **Retry path is attempt-1-only** (codex Critical, checker W4). 49-10…49-14 and 49-18 hard-code
   `…/attempt-1`, `p49-a1-*` and `gate.sh 1`; 49-10:94-99 routes a failed attempt to a future gap-closure plan and
   `launch-attempt.sh <N+1>`, but nothing obliges that plan to re-issue the live waves for attempt N, and
   49-18:120 skips on "no merge in attempt 1", so an attempt-2 success would skip the close-out.
2. **Abort after 49-12 `apply` leaves repository-wide state behind** (antigravity Critical #2; related antigravity
   High #3). `extensions.worktreeConfig=true` and the worktree `hooksPath` are restored only by `revert`
   (49-06:191-198), which runs only on the success path (49-13); neither `archive-attempt.sh` (49-08:256-285) nor
   49-10's stop routing restores them, and a later attempt's `apply` would record `prior=true`, so 49-13's gate
   (`extensions.worktreeConfig` must be unset) fails. A PR and a pushed `feature/phase-49.1` left by a stopped Ship would
   also collide with a later attempt. D-11 routes post-Ship stops to escalation rather than retry, which narrows but does
   not remove this.
3. **PR credential scan is post-publication with no hit containment** (codex High, checker W1). 49-12 Task 2 approval
   leads `/gsd-ship` to push and open the public PR; the scan is Task 3 (49-12:167-178). No step says what to do on a hit;
   editing the body leaves GitHub edit history, so containment means rotation. Nothing is scanned at the Ship-preflight
   approval, the last human stop point.
4. **Direct container gate tests the wrong tree** (checker Blocker 1). Verified: `scripts/check-in-container.sh:29-30`
   derives `REPO_ROOT` from the caller's cwd; 49-12:103 runs it by absolute path with no `cd`; the exit is
   hand-appended "on success" (49-12:105).
5. **Non-JSON line in `accommodations.jsonl`** (checker Blocker 2). Verified: 49-12:105 appends `container_check_exit=0`
   to the file 49-06 defines as one JSON line per change (49-06:124).
6. **`dirty` encoding** (checker W2). Verified: `crates/devflow-cli/src/commands.rs:729` emits it as a string;
   `:6729` asserts `is_string()`; 49-08:159 says it "must be `false`".
7. **Launch refusal leaks watcher units** (checker W3). Verified: 49-08:160-163 starts the watchers at step 7 and can
   refuse at step 8; the precondition (49-08:134-135) checks only `p49-a*-start`.
8. **30-minute container run in an executor's foreground** (checker W5). Verified: 49-12:103 `timeout 1800` with no
   backgrounding.
9. **Orphaned sentence** (antigravity Low #5). Verified: 49-12:71 "Record it and route it as cause (c)" lost its
   conditional in `8760677` (`72e3f29` read "If Ship fails on that, it is a Ship-stage failure gate.").
10. **Evidence gate accepts directories** (codex Low). Verified: 49-15:174 counts `! -type f ! -type d`, while 49-15:32
    requires only regular files.

### Divergent Views — rejected after checking source

- **antigravity #1 (49-17:103 unqualified `workspace/denniyahh`) — false positive.** It reproduced in the review clone,
  where that branch exists only as `origin/workspace/denniyahh`. In the real repository `refs/heads/workspace/denniyahh`
  is shared by every linked worktree and resolves from the phase-49 worktree (measured, with a non-existent-ref control).
- **antigravity #4 (49-18 pushes the workspace before Phase 49 planning exists) — by design.** The ROADMAP
  post-completion gate performs the separate `feature/phase-49` merge-back and push; codex confirms the ordering.
- **antigravity #6 (DECN-03 `Not followed` counted MET) — not a defect.** `47-PHASE49-OBSERVATION.md:15-16` defines
  Not followed narrowly (the session ran and halted instead of resolving); criterion 4 accepts either answer.
- **antigravity #7 (49-09 tracer checks absence) — intended.** The 49.1 gate requires `1 passed`; 0 today is exactly
  "its gates fail on today's code".

### Unverified by the orchestrator

The checker's seven info advisories were not individually re-checked; they are recorded as the checker reported them.

---

## Codex Review

Reviewed the staged revision at index tree `30435ee…`; no prohibited live/network commands ran. `bash -n` passes for the three referenced workspace/container scripts. That only establishes shell syntax, not live container, systemd, GitHub, or agent behavior.

## Part 1: Prior-finding closure

| # | finding | verdict | executable closure / gap |
|---|---|---|---|
| 1 | Linked-worktree container gate fails | CLOSED | `49-17-PLAN.md:121-142` verifies the HYG-02 mount fix; `49-12-PLAN.md:102-110` requires the direct pinned gate to pass before accommodations. |
| 2 | Retry leaves live state, blocking relaunch | PARTIAL | `49-08-PLAN.md:278-284` moves state only after a verified archive; `:250-254` tests attempt-2 check-only. But live plans remain hard-coded to attempt 1. |
| 3 | Archive ref first poisons retry | CLOSED | `49-08-PLAN.md:275-283` publishes manifest/archive before state move and writes ref last; interruption/re-run control is `:250-252`. |
| 4 | `KillMode=process` leaves descendants | CLOSED | Failed-attempt path census and cgroup-wide TERM/KILL are specified at `49-08-PLAN.md:259-268`; success-path non-retry deferral is explicit at `49-13-PLAN.md:173-178`. |
| 5 | No final base-SHA assertion before Ship | CLOSED | `49-12-PLAN.md:94-105` checks clean base before apply; `gate.sh` ship approval re-check is required by `49-08-PLAN.md:33`. |
| 6 | PR credential scan omits `ANTHROPIC_TOKEN` | PARTIAL | Scanner covers it at `49-16-PLAN.md:96-109`, but Ship creates/pushes the PR before the scan at `49-12-PLAN.md:126-145,167-178`. |
| 7 | `worktreeConfig` falsely called worktree-only | CLOSED | Repo-wide mutation is acknowledged and recorded at `49-06-PLAN.md:115-124`; restore is specified at `:191-198`. |
| 8 | `record-secure` can revert unrelated planning commits | CLOSED | Exact-path-only recording at `49-06-PLAN.md:155-162`; SHA-specific reverts at `:191-198`. |
| 9 | Five-second polling can miss gates | CLOSED | Gate persistence/event-log rationale and content-keyed watcher are at `49-03-PLAN.md:118-132`. |
| 10 | Concurrent ledger writers are unserialized | CLOSED | Shared `flock` append is specified at `49-03-PLAN.md:129-132` and tested at `:192,205`. |
| 11 | C5 can overclaim after overflow/unclean stop | CLOSED | Void/empty controls are at `49-04-PLAN.md:123-142`; verdict gate forces C5 `NOT_EXERCISED` on those states at `49-15-PLAN.md:176-177`. |
| 12 | Sanitization lacks provenance/redaction/verdict checks | PARTIAL | Sanitizer controls exist at `49-16-PLAN.md:139-153`; verdict/evidence mapping is checked at `49-15-PLAN.md:176-177`. Its regular-file gate still accepts directories. |
| 13 | Pinned binary not tied to fork SHA | CLOSED | Build commit is checked against `base_sha` before start at `49-08-PLAN.md:147-163`. |
| 14 | Mutable phase tools judge immutable-attempt evidence | CLOSED | Attempt tools are copied and hashed at `49-08-PLAN.md:154-159`; final judgment uses each attempt’s copies at `49-15-PLAN.md:123-129`. |
| A1 | Retry collides with leaked monitors | CLOSED | Retry cleanup targets the failed attempt’s cgroup, including descendants, at `49-08-PLAN.md:262-268`. |
| A2 | Three-pattern secret whitelist | CLOSED | Shared scanner has named credential patterns plus held named/suffixed environment values at `49-16-PLAN.md:96-111`. |
| A3 | Ship hook bypass invalidates real-hook measurement | CLOSED | Direct container gate is required before the copied-hook accommodation at `49-12-PLAN.md:102-106`; the run record must state the remaining limitation at `49-15-PLAN.md:161-165`. |
| A4 | Evidence verdicts are prose-only | CLOSED | C1–C5 tokens are mechanically bound to evidence outcomes at `49-15-PLAN.md:176-177`. |
| A5 | Backlog number collision | CLOSED | Computed-number action and duplicate check are at `49-17-PLAN.md:133-143`. |

## Part 2: New findings

## CONFIRMED

- **Critical — retry cannot execute past relaunch.** `49-08-PLAN.md:134-141` allows attempt 2 after archive, but every live-wave instruction and verification is fixed to attempt 1: `49-10-PLAN.md:65-86,110-111,154,169`; `49-11-PLAN.md:64,117`; `49-12-PLAN.md:98,126,181`; and close-out is also attempt-1-only at `49-18-PLAN.md:76`.  
  Failure: attempt 1 stops at Define/Code/Ship-preflight, gets correctly archived, and attempt 2 launches. The next planned task reads archived attempt-1 evidence, queries `p49-a1-*`, and calls `gate.sh 1`; it cannot inspect or approve attempt 2.  
  Evidence read: `rg` found the N-parameterized archive/relaunch design in 49-08 and the fixed attempt-1 references above.

- **High — public PR secret scanning is post-exposure, with no immediate cleanup path.** `49-12-PLAN.md:126-145` approves Ship and states that `/gsd-ship` pushes and opens a PR; only later does Task 3 retrieve and scan its title/body at `:167-178`.  
  Failure: a credential in the generated PR body is publicly created before `secret-guard` runs. A failing Task 3 blocks subsequent 49-13 PR closure/revert work, so the plan does not even require immediate containment.  
  Evidence read: ordered Task 2/Task 3 actions in 49-12.

- **Low — the committed-evidence gate passes a directory despite requiring only regular files.** `49-15-PLAN.md:32` requires only regular files, but `:174` counts `find "$E" ! -type f ! -type d`, explicitly accepting directories. `secret-guard` only promises to walk regular files (`49-16-PLAN.md:102-103`).  
  Failure: an empty `49-evidence/unexpected/` directory passes the regular-file assertion and does not cause the secret scan to fail.  
  Evidence read: the verifier predicate excludes directories from its failure count.

## SUSPECTED

None beyond the live behaviors the hard limits prevented checking.

The workspace ordering itself is correctly specified, but only as a post-completion gate: 49-18 first merges 49.1 code into `develop` and syncs/pushes `workspace/denniyahh`; then the ROADMAP requires a final `feature/phase-49` merge-back after 49-15, goal verification, and completion artifacts, followed by `sync-workspace.sh`, divergence verification, remote ancestry/blob comparison, and push. [`ROADMAP.md:332-346`](</tmp/claude-1000/-var-home-denniyahh-Github-devflow--worktrees-phase-49/76a0763e-8c10-4b3f-9ef5-71a1b365d445/scratchpad/review49/repo/.planning/ROADMAP.md:332>) `sync-workspace.sh` only merges `develop`; it does not import Phase 49 planning commits, exactly as the gate states. This is static plan analysis, not proof that the post-completion procedure will be run.



---

## Antigravity Review


---

## Part 1: Closure Table for Prior Findings

Baseline: `.planning/reviews/49-adversarial-2026-09-25/SUMMARY.md` (Findings 1–14) and `antigravity.md` (Findings AG-1–AG-5).

| # | Finding (Short) | Verdict | Plan Citation (`file:line`) / Closure Evidence |
|---|-----------------|---------|------------------------------------------------|
| **1** | ROADMAP 999.58: container gate fails in linked worktrees | **CLOSED** | [49-17-PLAN.md:22-24, 73-76, 124-131](49-17-PLAN.md#L22-L24): marks 999.58 resolved by HYG-02 (`5264a18`), citing code in [scripts/check-in-container.sh:108-140](scripts/check-in-container.sh#L108-L140) which bind-mounts gitdir and commondir. |
| **2** | Retry blocked by state file: `start` refuses if `state-49.1.json` exists | **CLOSED** | [49-08-PLAN.md:278-280](49-08-PLAN.md#L278-L280): Step 6 transactionally moves live `state-49.1.json` into archive only after `cmp` byte-equality with `archive/devflow/state-49.1.json` passes. Tested by selftest case `live state moves into the archive only after the manifest verifies`. |
| **3** | Archive failure blocks retry: partial archive leaves corrupt state | **CLOSED** | [49-08-PLAN.md:269-284](49-08-PLAN.md#L269-L284): Stages to `archive.partial/`, verifies `MANIFEST.sha256`, atomically renames to `archive/`, moves state, and writes archive ref last. Re-running after partial fault is tested by `--fault-after manifest`. |
| **4** | `KillMode=process` vs cleanup: orphaned child processes block retry | **CLOSED** | [49-08-PLAN.md:262-268](49-08-PLAN.md#L262-L268): `archive-attempt.sh --stop-unit` drains cgroup with `systemctl kill --kill-whom=all --signal=SIGTERM/SIGKILL` on `p49-a<N>-start` unit, terminating test monitors leaked by Validate. |
| **5** | Final base-head invariant before Ship: base moved before merge hook | **CLOSED** | [49-08-PLAN.md:212-217](49-08-PLAN.md#L212-L217): `gate.sh approve ship` asserts branch is `workspace/denniyahh`, `HEAD == base_sha`, and working tree is clean immediately before writing the approval response. |
| **6** | Public PR secret scan omits `ANTHROPIC_TOKEN` | **CLOSED** | [49-16-PLAN.md:20-22, 100-109](49-16-PLAN.md#L20-L22): `secret-guard.sh scan` explicitly scans `ANTHROPIC_API_KEY=` and `ANTHROPIC_TOKEN=` assignment prefixes and loads values of `ANTHROPIC_TOKEN`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN` from the host environment. |
| **7** | `extensions.worktreeConfig` repo-wide mutation | **CLOSED** | [49-06-PLAN.md:20, 110, 121, 185](49-06-PLAN.md#L20): Records prior value before setting `true`, tests that second linked worktree is isolated, and restores prior value in `ship-accommodations.sh revert` ([49-13-PLAN.md:185](49-13-PLAN.md#L185)). *(Note: A new gap in retry-path restoration is identified in Part 2 CONFIRMED 2).* |
| **8** | `record-secure` reverts unrelated planning commits | **CLOSED** | [49-06-PLAN.md:21-22, 144, 185](49-06-PLAN.md#L21-L22): Refuses if any commit since `apply` touches paths other than `49.1-SECURITY.md`; records exact commit SHAs; `revert` reverses only those SHAs. |
| **9** | 5-second polling misses gate lifecycle | **CLOSED** | [49-03-PLAN.md:115-119](49-03-PLAN.md#L115-L119): Reasoned deferral: human-blocking gates in DevFlow never auto-respond, run halts indefinitely until answered, and `events.jsonl` contains the authoritative record. |
| **10** | Concurrent `ledger.jsonl` appends | **CLOSED** | [49-03-PLAN.md:27, 127-130](49-03-PLAN.md#L27) and [49-08-PLAN.md:219-221](49-08-PLAN.md#L219-L221): All writers (`gate-watch.sh` and `gate.sh`) serialize appends with `flock -w 30 "$EV/ledger.lock"`. Verified by selftest. |
| **11** | SURV-01 valid despite invalid window | **CLOSED** | [49-15-PLAN.md:23-25, 176](49-15-PLAN.md#L23-L25): Gate asserts C5 is `NOT_EXERCISED` if `void_windows > 0`, `watcher_stopped_cleanly != true`, or `instrument_saw_nothing != false`. `MET` requires `void_windows == 0`, `stopped_cleanly == true`, and `saw_nothing == false`. |
| **12** | Evidence sanitization manual/under-verified | **CLOSED** | [49-16-PLAN.md:23, 146-154](49-16-PLAN.md#L23): `secret-guard.sh sanitize` replaces home paths and UUIDs, refuses symlinks/overwrites, and verifies no surviving secrets via `scan`. Gate at [49-15-PLAN.md:174-177](49-15-PLAN.md#L174-L177) validates clean output. |
| **13** | Binary build bound to fork SHA | **CLOSED** | [49-08-PLAN.md:148, 157-159](49-08-PLAN.md#L148): `launch-attempt.sh` reads `base_sha` before `cargo build`, builds to `$P49/bin/devflow-<sha12>`, and requires `workflow_started.commit == base_sha` and `dirty == false`. |
| **14** | Mutable tools vs immutable watcher copies | **CLOSED** | [49-08-PLAN.md:154-155](49-08-PLAN.md#L154-L155), [49-10-PLAN.md:71-75](49-10-PLAN.md#L71-L75): Tool directory is copied to `$EV/tools` at launch; `SHA256SUMS` is verified by every executor gate before executing any tool. |
| **AG-1** | Orphaned test monitors survive attempt abort | **CLOSED** | [49-08-PLAN.md:263-267](49-08-PLAN.md#L263-L267): `archive-attempt.sh --stop-unit` kills unit cgroup with `systemctl kill --kill-whom=all --signal=SIGTERM/SIGKILL`, ensuring descendant test monitors are reaped before retries. |
| **AG-2** | Secret whitelist coverage | **CLOSED** | [49-16-PLAN.md:20-22, 95-111](49-16-PLAN.md#L20-L22): Scanner loads fixed tokens (`ANTHROPIC_*`, `GH_*`, `OPENAI_*`, `AWS_*`, `CONTEXT7_*`), plus all environment vars ending in `_TOKEN`, `_SECRET`, `_API_KEY`, `_PASSWORD`, plus `gh auth token`. Matches against temporary mode-0600 files via `rg -F -c -f`. |
| **AG-3** | Container bypass in live Ship stage | **CLOSED** | [49-15-PLAN.md:32, 163-165](49-15-PLAN.md#L32): Explicitly disclosed in `49-LIVE-RUN.md` "What this run does not establish"; verified mechanically by automated gate at [49-15-PLAN.md:174-175](49-15-PLAN.md#L174-L175). |
| **AG-4** | Non-falsifiable verification gates for success criteria | **CLOSED** | [49-15-PLAN.md:176-177](49-15-PLAN.md#L176-L177): Fixed tokens C1–C5 mapped directly to evidence files (`check-arm-positive-a<N>.txt`, `check-ship-a<N>.txt`, `check-pair-a<N>.txt`, `decn03-status.txt`, `surv-report.json`) with required positive and negative checks. |
| **AG-5** | Fragile ROADMAP backlog insertion | **CLOSED** | [49-15-PLAN.md:104, 204](49-15-PLAN.md#L104) and [49-17-PLAN.md:81, 133](49-17-PLAN.md#L81): Computes next backlog number dynamically as `max(Phase 999.x) + 1`, inserts above first entry, and asserts no duplicate numbers exist. |

---

## Part 2: New Findings

## CONFIRMED

### Finding 1: Unqualified branch ref in 49-17 automated verify gate causes gate failure in any detached or worktree checkout
- **Severity**: High
- **Citation**: [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-17-PLAN.md:103](49-17-PLAN.md#L103)
- **Concrete Failure**:
  Task 1 `<verify>` command executes:
  ```bash
  a=$(git show "workspace/denniyahh:$C" | jq -S -c "del(.workflow.code_review_depth_overrides)")
  ```
  `workspace/denniyahh` is not qualified with a repository directory (unlike [49-09-PLAN.md:134](49-09-PLAN.md#L134) which uses `git -C "$R"` where `$R` is the primary checkout). In the phase worktree `feature/phase-49` or any disposable clone or CI environment, `workspace/denniyahh` is not a local branch ref (it is `refs/remotes/origin/workspace/denniyahh` or only in the primary worktree).
  When this command runs, `git show "workspace/denniyahh:$C"` prints `fatal: invalid object name 'workspace/denniyahh'` to stderr, `$a` evaluates to empty string, `[ -n "$a" ]` fails, and `gate_rc` is forced to `1`. The automated gate fails on valid edits.
- **Proof (Executed in clone)**:
  ```bash
  $ bash -c 'cd /tmp/claude-1000/-var-home-denniyahh-Github-devflow--worktrees-phase-49/76a0763e-8c10-4b3f-9ef5-71a1b365d445/scratchpad/review49/repo; C=.planning/config.json; git show "workspace/denniyahh:$C"'
  fatal: invalid object name 'workspace/denniyahh'.
  (exit code 128)
  ```

---

### Finding 2: Retry Path Leak: Attempt 1 failure at 49-12 permanently leaks `extensions.worktreeConfig true` into `.git/config`, causing Attempt 2 to fail its 49-13 gate
- **Severity**: Critical (Priority 1: Retry & Archive Idempotence)
- **Citations**:
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-06-PLAN.md:121, 185](49-06-PLAN.md#L121)
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:129-146, 256-285](49-08-PLAN.md#L129-L146)
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-PLAN.md:98](49-12-PLAN.md#L98)
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-13-PLAN.md:167, 185-186](49-13-PLAN.md#L167)
- **Concrete Failure**:
  1. Attempt 1 reaches 49-12 Task 1: `ship-accommodations.sh apply` executes. It runs:
     `git -C $ROOT config extensions.worktreeConfig true`
     writing `extensions.worktreeConfig = true` to `.git/config` of the repository. It records `prior_extensions_worktreeConfig=unset` in `$EV/ship/accommodations.jsonl`.
  2. Attempt 1 halts or fails during 49-12 (e.g. operator rejects the Ship preflight gate at Task 2, direct container check fails, PR secret check fails, or the agent times out).
  3. The stop routing or operator runs `bash $TOOLS/archive-attempt.sh 1 --stop-unit`.
  4. Inspection of `archive-attempt.sh` (49-08 Task 3, lines 256–285) shows it executes steps: Census, Quiesce, Stage, Manifest, Publish, Move live state, Ref last. It does **not** call `ship-accommodations.sh revert`, and does not unset `extensions.worktreeConfig`.
  5. Attempt 2 launches (`launch-attempt.sh 2`). Preconditions in 49-08 Task 2 (lines 129–146) do **not** verify whether `extensions.worktreeConfig` is set or unset.
  6. Attempt 2 progresses to 49-12 Task 1. `ship-accommodations.sh apply` queries the repository config for `extensions.worktreeConfig`. Because Attempt 1 left it `true`, Attempt 2 records `prior_extensions_worktreeConfig=true` in Attempt 2's `accommodations.jsonl`.
  7. Attempt 2 reaches 49-13 Task 1. `ship-accommodations.sh revert` restores `extensions.worktreeConfig` to its recorded prior value (`true`).
  8. Task 3 automated verification gate in [49-13-PLAN.md:185](49-13-PLAN.md#L185):
     ```bash
     wc_=$(git -C "$R" config --get extensions.worktreeConfig || true)
     echo "worktree_hooksPath=${wh:-unset} extensions_worktreeConfig=${wc_:-unset}"
     [ -z "$wh" ] && [ -z "$wc_" ] || rc=1
     ```
     `wc_` is `true`. The gate exits 1 with `extensions_worktreeConfig=true`.
  9. Precondition of Attempt 2 clean restoration is violated by Attempt 1's abort.
- **Proof (Read from clone source)**:
  `archive-attempt.sh` specification in `49-08-PLAN.md:256-285` contains zero invocations of `revert` or config resets. `ship-accommodations.sh revert` is referenced exclusively in `49-13-PLAN.md:167`.

---

### Finding 3: Retry Path Leak: Unclosed GitHub PR from Attempt 1 blocks Attempt 2 `/gsd-ship` PR creation
- **Severity**: High (Priority 1: Retry & Archive Idempotence)
- **Citations**:
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-08-PLAN.md:256-285](49-08-PLAN.md#L256-L285)
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-10-PLAN.md:82-100](49-10-PLAN.md#L82-L100)
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-PLAN.md:72-75](49-12-PLAN.md#L72-L75)
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-13-PLAN.md:168-170](49-13-PLAN.md#L168-L170)
- **Concrete Failure**:
  If Attempt 1 passes 49-12 Task 2, DevFlow's Ship stage invokes `/gsd-ship`. `/gsd-ship` pushes `feature/phase-49.1` to GitHub and opens a pull request targeting `main`.
  If the run subsequently stops before 49-13 Task 3 (e.g., failure in 49-12 Task 3 verification, operator rejecting the final Ship merge gate, or failure during 49-13 Task 2 undo), the PR remains `OPEN` on GitHub.
  Neither `archive-attempt.sh` nor the stop routing in `49-10-PLAN.md` closes open pull requests on abort.
  When Attempt 2 is launched, DevFlow recreates the branch and runs through Ship. When `/gsd-ship` executes `gh pr create --head feature/phase-49.1 --base main`, GitHub rejects the call because a PR for branch `feature/phase-49.1` is already open. Attempt 2 crashes during unattended Ship.
- **Proof (Read from clone source)**:
  `gh pr close` is called solely in [49-13-PLAN.md:168](49-13-PLAN.md#L168). `archive-attempt.sh` has no PR handling; `launch-attempt.sh` does not check for pre-existing remote PRs.

---

### Finding 4: Premature push of `workspace/denniyahh` in 49-18 publishes before Phase 49's `.planning/` changes exist on the branch
- **Severity**: Medium (Priority 7: Operator Instruction on `.planning/` Sync)
- **Citations**:
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-18-PLAN.md:207, 248-250](49-18-PLAN.md#L207)
  - [scripts/cut-pr-branch.sh:119](scripts/cut-pr-branch.sh#L119)
  - [scripts/sync-workspace.sh:37](scripts/sync-workspace.sh#L37)
  - [.planning/ROADMAP.md:332-345](.planning/ROADMAP.md#L332-L345)
- **Concrete Failure**:
  `scripts/cut-pr-branch.sh:119` strips `.planning` from `feature/phase-49.1-pr`. Merging that PR into `develop` lands only the test code (`phase7_cli.rs`).
  In 49-18 Task 3, `scripts/sync-workspace.sh` merges `origin/develop` into `workspace/denniyahh`. This brings zero `.planning/` files from Phase 49 or 49.1.
  At line 207, 49-18 pushes `workspace/denniyahh` to origin, declaring in success criteria: "`workspace/denniyahh` is pushable and pushed."
  At this moment:
  1. Wave 11 (`49-15-PLAN.md`) has not run yet.
  2. `49-LIVE-RUN.md` does not exist.
  3. None of Phase 49's planning artifacts exist on `workspace/denniyahh`.
  The operator requirement that all Phase 49 `.planning/` changes end up on `workspace/denniyahh` is **not** achieved by 49-18. It is achieved only when the separate post-completion integration gate runs *after* Phase 49 completes, requiring a redundant second merge and second push of `workspace/denniyahh`.

---

### Finding 5: Truncated syntax and missing condition in 49-12-PLAN.md context
- **Severity**: Low / Editorial
- **Citation**: [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-12-PLAN.md:70-71](49-12-PLAN.md#L70-L71)
- **Concrete Failure**:
  Text reads:
  ```markdown
  The same pinned container gate runs directly in the 49.1 worktree before Ship approval, outside the Ship agent's timeout budget;
  its exit and final marker are required evidence.
  Record it and route it as cause (c): escalation. It reached Ship, so it does not count as an attempt (D-11).
  ```
  Missing the conditional clause `If it fails,`. Read literally, it instructs an executor to route a successful container check as a cause (c) escalation.
- **Proof (Read from clone source)**:
  Lines 70–71 directly follow line 69 without `If this check fails,`.

---

## SUSPECTED

### Finding 6: Success Criterion 4 (DECN-03) mechanically accepts `Not followed` as `MET` without asserting prompt causality
- **Severity**: Medium (Priority 8: Claims Beyond Evidence)
- **Citations**:
  - [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-15-PLAN.md:145-153, 176-177](49-15-PLAN.md#L145-L153)
  - [.planning/ROADMAP.md:270-274](.planning/ROADMAP.md#L270-L274)
- **Concrete Failure**:
  In `49-15-PLAN.md:176`, the verification gate verifies:
  ```bash
  case "$O" in "Followed"|"Not followed") [ "$S" = EXERCISED ] && [ "${tk[4]}" = MET ] || rc=1 ;;
  ```
  Criterion 4 asks which instruction the agent follows when `CODE_STAGE_POLICY` and `checkpoint_auto_decide_prompt` conflict. If the agent acts erratically, fails to follow either prompt, or follows an unrelated third instruction, the outcome can be classified `Not followed` with `decn03-status.txt = EXERCISED`.
  The gate then marks C4 as `MET`. While the text notes that "either is a result; silence is not", mechanically certifying C4 as `MET` under `Not followed` without checking whether the prompt collision directly induced the failure claims resolution of the behavioural question from evidence that does not pin it.

---

### Finding 7: Tracer in 49-09 checks test non-existence rather than test failure
- **Severity**: Low / Editorial (Priority 6: Verify Gate Traps)
- **Citation**: [.planning/phases/49-live-unattended-run-the-milestone-s-instrument/49-09-PLAN.md:98-99](49-09-PLAN.md#L98-L99)
- **Concrete Failure**:
  Task 1 is titled: `Tracer — the pre-authored 49.1 plan is what DevFlow and GSD will see, and its gates fail on today's code`.
  The verify command runs:
  ```bash
  timeout 1800 cargo test -p devflow --test phase7_cli wait_budget_tests::override_wins -- --exact > "$L/ct" 2>&1
  ce=$?
  one=$(rg --count-matches "^test result: ok\\. 1 passed" "$L/ct" || true)
  one=${one:-0}
  [ "$ce" -eq 0 ] && [ "$one" -eq 0 ] || rc=1
  ```
  Because `wait_budget_tests::override_wins` is not in `phase7_cli.rs`, Cargo matches 0 tests, prints `test result: ok. 0 passed`, and exits 0.
  The check passes because `$ce == 0` and `$one == 0`. This proves the test does not exist in today's code; it does not execute a test that fails against today's implementation.

---

## Detailed Responses to Stated Priority Areas

### 1. Retry and Archive Idempotence
- **Walk across live gates (49-10 to 49-13)**:
  - **49-10 (Define Gate)**: If stopped, `archive-attempt.sh 1 --stop-unit` drains cgroups, moves `state-49.1.json` to archive, and creates archive ref. Preconditions for `launch-attempt.sh 2` hold.
  - **49-11 (Code / DECN-03 Gate)**: If stopped, live state is moved and cgroups reaped. Preconditions hold.
  - **49-12 (Ship Preflight Gate)**: If stopped *after* Task 1 (`ship-accommodations.sh apply`), `extensions.worktreeConfig true` is leaked in `.git/config` (**CONFIRMED 2**).
  - **49-12 (Ship PR Opened)**: If stopped *after* `/gsd-ship` opens the PR to `main`, the PR remains open on GitHub, causing Attempt 2 to fail on `gh pr create` (**CONFIRMED 3**).

### 2. Irreversible or Outward-Facing Operations
- Every outward mutation in the revised plans is preceded by a strict guard on current state:
  - Base fast-forward in `49-09-PLAN.md:128-130`: guarded by clean tree, unpushed remote check, exact SHA comparisons.
  - Major bump approval in `49-12-PLAN.md:137-140`: `gate.sh approve ship` asserts `HEAD == base_sha` and porcelain status is clean immediately before approval.
  - Version bump/tag undo in `49-13-PLAN.md:142`: `undo-ship-hooks.sh` verifies parentage, base ref, and version `2.12.0` before stripping commits and deleting tag.
  - Closeout PR merge in `49-18-PLAN.md:172-174`: guarded by `--match-head-commit <head sha>` and direct `gh pr checks` assertions.

### 3. Current-Head PR Checks
- Fully compliant: `gh run list --branch` is eliminated from PR verification.
- [49-18-PLAN.md:25, 158-164, 172-175](49-18-PLAN.md#L25) explicitly enforces `gh pr checks <N> --json name,state,bucket,link` against the exact `headRefOid`. `--match-head-commit` guarantees that only the verified SHA merges.

### 4. Secrets and Evidence Handling
- In [49-16-PLAN.md:95-115](49-16-PLAN.md#L95-L115), `secret-guard.sh` loads all specified token names from `49-RESEARCH.md`, plus dynamic environment suffixes (`_TOKEN`, `_SECRET`, `_API_KEY`, `_PASSWORD`) and `gh auth token`.
- Matches are performed using temporary mode-0600 files with `rg -F -c -f`.
- `secret-guard.sh sanitize` redacts home paths and UUIDs, rejecting symlinks and surviving secrets.

### 5. Cross-Plan Contradictions
- Wave order and `depends_on` graphs match across all 18 plans and `ROADMAP.md`.
- Tool paths correctly reference `$EV/tools` with `SHA256SUMS` verification across all live plans.

### 6. Gates That Fail Correct Work or Pass Broken Work
- Automated verify commands properly use `rg --count-matches ... || true` with `var=${var:-0}` fallbacks to avoid silent nonzero exits.
- All cargo invocations correctly target the binary package `-p devflow`.
- Finding 1 above is the only automated gate confirmed to fail correct work.

### 7. Operator Instruction: Sync of `.planning/` to `workspace/denniyahh`
- As established in **CONFIRMED 4**, `49-18` does **not** bring Phase 49's `.planning/` changes to `workspace/denniyahh`.
- `scripts/cut-pr-branch.sh:119` strips `.planning/` entirely.
- Merging to `develop` and running `scripts/sync-workspace.sh` in 49-18 Task 3 syncs code only (`phase7_cli.rs`).
- The ROADMAP post-completion integration gate ([ROADMAP.md:332-345](.planning/ROADMAP.md#L332-L345)) correctly prescribes the eventual merge of `feature/phase-49` into `workspace/denniyahh`. However, pushing `workspace/denniyahh` in 49-18 pushes the branch before Phase 49's planning records exist on it.

### 8. Claims the Evidence Cannot Carry
- Success Criterion 4 (DECN-03) scoring allows `Not followed` to count as `MET` without proving the prompt conflict caused the deviation (**SUSPECTED 6**).
- Success Criterion 5 (SURV-01) is strictly bound: [49-15-PLAN.md:176](49-15-PLAN.md#L176) mechanically blocks `MET` and enforces `NOT_EXERCISED` if any void window, missing stop marker, or zero interleaving occurred.

---

## GSD Plan Checker (gsd-plan-checker, reviews mode — not a cross-AI lane)

Verdict: **ISSUES FOUND** — 2 blockers, 5 warnings, 7 info. Read the uncommitted working-tree files in the
phase-49 worktree on `feature/phase-49` (verified). Mode: reviews (2026-09-25 findings).

### Blockers

1. **49-12 Task 1 (lines 102-106, gate 109) — the direct container gate tests the wrong tree and self-reports its pass.**
   `scripts/check-in-container.sh:29-30` sets `REPO_ROOT="$(git rev-parse --show-toplevel)"` from the caller's cwd and
   `cd`s there. 49-12 invokes the 49.1 copy by absolute path with no `cd`, so from the executor's phase-49 cwd it tests
   `feature/phase-49`, and `==> check.sh: all OK` still appears. The exit is recorded by hand-appending
   `container_check_exit=0` "on success". The hooks copy then omits the container stanza (49-06), so 49.1's pushed code
   is never container-verified; codex #1 is closed only nominally.
   Fix hint: `(cd "$WT" && timeout 1800 scripts/check-in-container.sh all; echo "exit=$?") > log`, record
   `git -C "$WT" rev-parse HEAD` and branch, assert the recorded head equals `feature/phase-49.1`'s tip and `exit=0`.
2. **49-12 Task 1 vs 49-06 — `$EV/ship/accommodations.jsonl` must stay one JSON value per line.** 49-12 appends the bare
   line `container_check_exit=0` (the file's first line, before `apply`) and reads it with `sed`; 49-06's `record-secure`
   and `revert` read recorded SHAs and prior values from the same file, so a `jq` read under `set -euo pipefail` fails.
   49-06's selftest never plants a foreign line. Fix hint: a separate file or a JSON record, plus a foreign-line selftest case.

### Warnings

1. **49-12 Task 3 (lines 173-177, 181) — codex #6 timing:** `/gsd-ship` publishes the PR before the scan; no step says what to
   do on a hit (edit, close, rotate). The last human stop point, the Ship-preflight approval, scans nothing.
2. **49-08 (truth line 25, step 6 lines 157-159) — `dirty` encoding:** `commands.rs:729` emits
   `"dirty": env!("DEVFLOW_BUILD_DIRTY")` and the test at `:6729` asserts `payload["dirty"].is_string()`; the plan says
   `dirty` "must be `false`", so a boolean fixture can pass the selftest while the real launch refuses `build commit`.
3. **49-08 Task 1 (steps 7-9) — launch refusal leaks watcher units:** steps 8/9 can refuse after step 7 started
   `p49-a<N>-gatewatch`/`survwatch`; the precondition checks only `p49-a*-start`; 49-10's rename-and-rerun recovery
   collides with the still-active units.
4. **49-10 … 49-14, 49-18 — live plans work only for attempt 1** although D-11 allows three: `EV=…/attempt-1`,
   `p49-a1-*`, `gate.sh 1` are literals; only 49-08 and 49-15 are parameterised.
5. **49-12 Task 1 — `timeout 1800` container run in an executor's foreground** exceeds the 10-minute tool-call cap;
   49-18 backgrounds the same class of command.

### Info

1. 49-06 T-49-06-02 (line 229) still promises the container call survives in the hooks copy.
2. 49-02 layout contract (line 74) lists a `run_end` ledger kind 49-03 never writes and omits `prompt_generation`.
3. `49-RESEARCH.md:70` still lists the 49.1-to-develop route as undecided; plans record `closeout-pr-now`.
4. The ROADMAP post-completion integration gate has no executable owner (it runs after `phase.complete`); name one.
5. Copied `negctl.sh` step 12 (49-07) still calls the phase-worktree `check-evidence.sh` via `--repo-root`; negligible.
6. Sanitiser redacts home paths and UUIDs but not the bare username (ledger `"user": $USER`).
7. 49-15 Task 2's two traceability predicates (token count vs any "unmet" text in `## Verdict`) can disagree.

### Per-finding incorporation (2026-09-25 review)

codex 2, 3, 5, 8, 10, 11 OK; codex 4 OK (success path explicitly deferred to 999.141); codex 9 OK (explicit rejection);
codex 7 OK (stale threat line, info 1); codex 12 OK (username residual, info 6); codex 14 OK (negctl residual, info 5);
codex 6 incorporated, timing gap (warning 1); codex 13 incorporated, encoding gap (warning 2);
**codex 1 present but defective (blocker 1)**; agy 1-5 OK.

### Passed dimensions

VERIFY-01 claimed by all 18 plans; `verify.plan-structure` 18/18 valid; DAG acyclic and waves equal DAG level (1-11
re-derived); no undeclared same-wave coupling; ≤3 tasks per plan, largest estimate 22.5k/100k; D-01..D-16 each
implemented; research open questions RESOLVED; no bare `--exact`, no `-p devflow --lib`, no `gh run list`.
Live-measured assumptions that matched: 49-09's six counted loops, the single 49.1 `blocking-human` checkpoint,
highest backlog number 146, `config.json` identical to `workspace/denniyahh`'s, `.devflow/` gitignored, verbatim
preflight/gate strings, `ShipEvidence.shipped` serialized, `==> check.sh: all OK` at `check.sh:105`.

### What this check does not establish

No plan gates were run. The path probe resolved no targets, so `<automated>` path resolvability is unverified. RESEARCH
claims about DevFlow runtime behaviour (gate timing, resume routing, soft-failing BranchCleanup) were not re-verified.

---

# Round 2 — Cross-AI re-review of the revision (2026-09-27)

**Reviewed tree:** `9b2718bebf7abd4b19c6249add8d0c07cd2b7b2a` (the revision since `30435ee`: revision rounds 1 and 2,
after the gsd-plan-checker re-check found 0 blockers / 4 warnings on round 1). **Lanes:** codex (`gpt-5.6-terra`, high;
5 min) and antigravity (`gemini-3.8-flash-high`, high; 12 min, 26 citation lines), both completed, in a sandboxed
shared clone with push disabled. **Raw lane output was lost:** the host rebooted at 2026-09-27 14:10 and `/tmp` (the
session scratchpad holding both reports) was wiped. The findings below are the orchestrator's record from the session
transcript, each verified against source before being accepted; they are not the lanes' verbatim text.

## Verified findings

| # | Finding | Lane | Verdict | Evidence checked |
|---|---------|------|---------|------------------|
| 1 | Record-and-continue's "skip" is written as a `<precondition>`, but GSD treats a precondition as an assertion: unmet → STOP with a blocking-human checkpoint, never a branch | antigravity | CONFIRMED (High) | `gsd-core references/planner-preconditions.md:115-124`; pattern present in HEAD 49-14 (2×) and spread by round 2 to 49-11/12/13/18 |
| 2 | A stopped plan cannot complete: task acceptance criteria still require a `gate.sh` approval; GSD blocks the next task until they pass | antigravity | CONFIRMED (High) | 49-10:322, 49-11:139, 49-13:115; `execute-plan.md:229-238` |
| 3 | `/gsd-plan-phase 49 --gaps` reads only `VERIFICATION.md` gaps, so the seven wrappers depend on two AI steps (verifier, gap planner) that need not read 49-LIVE-RUN's disposition | codex, antigravity | CONFIRMED (Critical as written) | `references/planner-gap-closure.md` step 1; `plan-phase.md:743-744`; only `--prd` extra input exists and was disabled |
| 4 | A fabricated stop record passes every stop-aware gate (values of `plan`/`stage` unchecked, extra lines accepted) | codex | CONFIRMED (High) | codex executed the 49-10 Task 2 gate in a `mktemp` HOME with `plan: invented-plan`, `stage: banana` → `gate_rc=0`; its controls (duplicate `stage:`, `attempt: 2`) → `gate_rc=1` |
| 5 | The resume record (`$EV/resume/`, `resume_at_task`) has no GSD reader; a re-presented plan starts at Task 1 | codex, antigravity | CONFIRMED, severity lowered to Medium | if ignored, 49-10 Task 1's own precondition refuses — a confusing stop, not a duplicate launch |
| 6 | A credential hit on the public PR is not enforced closed on the stop path; 49-13's close is skipped | codex, antigravity | CONFIRMED (High) — main path | 49-12:232-238, 49-13:97 |
| 7 | 49-15 accepts `C5: NOT_MET` when the SURV report is absent (must be `NOT_EXERCISED`) | antigravity | CONFIRMED (Medium, stop path) | 49-15:202 vs truth :24 |
| — | Stopping plan fails plan-level `<verification>` | codex (suspected), antigravity (suspected) | REJECTED | 49-11:205-207 plan-level verification already accepts a stop record |
| — | Task 2 gate fails on a first-launch refusal with no `current-attempt` | antigravity | REJECTED | failing is the refused-launch route's intended outcome |
| — | 49-18 uses the phase-worktree `secret-guard.sh` | antigravity | REJECTED | by design: 49-18 judges no attempt evidence |
| — | Remote `feature/phase-49.1` blocks relaunch with no scripted cleanup | antigravity (suspected) | Not a defect | the refusal is contract item 4; a post-Ship stop is an escalation and branch keep/delete is the operator's 49-14 decision |

## Disposition

Findings 1-5 share one cause: record-and-continue tried to make GSD repeat or skip plans, which its run-once
execution model does not support. **Operator decision (2026-09-27): resume-forward replaces record-and-continue** — a
failed attempt stops like an interrupted session (no SUMMARY), counted retries follow a Define or Code stop only (D-11),
and GSD's own resume continues at the failed plan; 49-11 Task 1 gets a post-hoc catch-up for a Code-stop retry. This
removes the machinery behind findings 1-5. Findings 6 and 7 are fixed directly (revision round 3).
