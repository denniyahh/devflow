outcome: run-incomplete

# Phase 23 Plan 15 — Acceptance Run Record (Round 2)

Target: Phase 24 (`release --check` signing-key inline classification).
Authorization: operator `PROCEED` recorded verbatim in
`23-ACCEPTANCE-SETUP-2.md` Task 3, against `origin/develop` SHA `0dad20d`,
with predicted version `1.8.2` (operator's own prediction, given knowingly
against the orchestrator's `~1.11.339` counter-finding — "the bad version IS
the finding").

**Verdict, stated up front and expanded below:** the run was blocked before
the Define stage ever launched — `devflow start` created the phase-24
worktree and branch, then the self-dogfood staleness **hard block**
(D-18, `StalenessOutcome::Block`) fired synchronously inside the `start`
command itself, before any monitor process or Claude agent was ever spawned.
This is a **VALID RECORD** of an **INCOMPLETE (failed) acceptance
attempt** — judged separately from the record's own validity, per this
plan's design. **This is also the first time in this project's real-run
history that the self-dogfood staleness hard block has stopped an
acceptance attempt** — closing, in the most direct way possible, the
coverage gap `23-FINDINGS.md` §B3 named against the previous attempt
(`23-ACCEPTANCE-RUN.md` §9: "only the warn branch was exercised").

---

## 1. Launch

**Precondition re-verified before anything else, per the plan's own
instruction (not trusted from the record):**

```
$ git rev-parse --git-dir
.git
$ git rev-parse --git-common-dir
.git
```

Both resolve to the same path — this is the primary checkout, not a linked
worktree. Confirmed independently of the orchestrator's pre-dispatch check.

```
$ git rev-parse --abbrev-ref HEAD
feature/phase-23
$ git status --porcelain
(empty)
```

**Launch command, verbatim, exactly as run:**

```
devflow start --phase 24 --agent claude --mode auto --yes-ship
```

Run from the primary checkout (`<repo-root>`, branch `feature/phase-23`,
HEAD `00b6f10993da35144282b3d9dbf00cc748288057`) at `2026-07-26T22:07:42Z`.

**Launch-time freshness re-check — all four results, re-measured at launch
time, not copied from `23-ACCEPTANCE-SETUP-2.md`:**

```
$ date -u +"%Y-%m-%dT%H:%M:%SZ"
2026-07-26T22:07:12Z
$ git fetch origin
$ git rev-parse origin/develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
```

1. **`origin/develop` SHA vs. authorized SHA:** `0dad20d3e85d82d60235b8f91cb944e4cbed433c`
   — **matches** the SHA the operator's Task 3 authorization in
   `23-ACCEPTANCE-SETUP-2.md` was given against. Unchanged since 23-14.

2. **Binary hash vs. the 23-13-recorded hash:**

   ```
   $ sha256sum ./target/release/devflow
   b5db079ad7c76a9e33d7f6b1bffa0b1caeedf208789f7f38353602628e26dc98  ./target/release/devflow
   $ sha256sum "$(command -v devflow)"
   b5db079ad7c76a9e33d7f6b1bffa0b1caeedf208789f7f38353602628e26dc98  <homebrew-prefix>/bin/devflow
   ```

   Both **match** `23-GUARD-SHIP-RECORD.md` Task 3's recorded hash
   (`b5db079a…6dc98`) exactly. PATH-resolved binary is byte-identical to the
   locally built one.

3. **`crates/`/`Cargo.toml`/`Cargo.lock` commit range vs. `origin/develop`:**

   ```
   $ git diff origin/develop HEAD -- crates/ Cargo.toml Cargo.lock | wc -l
   0
   ```

   **Zero lines.** No build-affecting commit has landed since the 23-13
   rebuild.

4. **Rebuild performed:** **No.** All three checks above passed — nothing had
   moved that our own pre-launch check is designed to catch. (Section 9 below
   records a *different* staleness signal — ancestry against the target
   worktree's own `HEAD`, not against `origin/develop` — that our pre-launch
   check does not and structurally cannot cover; see that section for why a
   rebuild from this branch would not have helped.)

**Pre-launch state, recorded immediately before the real launch:**

```
$ devflow start --phase 24 --agent claude --mode auto --yes-ship --dry-run
dry run — phase 24 | agent claude | mode auto

stage pipeline:
  define /gsd-discuss-phase 24
  plan /gsd-plan-phase 24
  code /gsd-execute-phase 24
  validate /gsd-validate-phase 24 [GATE after 3 failures]
            ↳ hooks: [DocsUpdate]
  ship /gsd-ship 24 [GATE]

after ship: [Merge, VersionBump, ChangelogAppend, BranchCleanup]
EXIT=0

$ git status --porcelain
(empty)
$ git ls-remote origin refs/heads/recovery/pre-23-15-acceptance-0dad20d
0dad20d3e85d82d60235b8f91cb944e4cbed433c	refs/heads/recovery/pre-23-15-acceptance-0dad20d
$ devflow gate list --all-roots
(5 rows, all phase 12, all /tmp/.tmp* roots — pre-existing noise, see §7)
$ devflow status .
stage: idle
project_root: <repo-root>
open branches:
  feature/phase-23 — 9 ahead (2 behind develop)
$ test -f .devflow/state-24.json && echo EXISTS || echo absent
absent
$ ps aux | grep -c 'phase.24\|phase-24'
2   (both self-referential: this shell command's own text, and the grep
     process matching its own argv — zero real phase-24 processes)
$ devflow evidence --phase 24 --require-shipped
... error: phase 24 has not shipped — DevFlow has no record of a completed Ship
EXIT=1
```

**Pre-run baseline for the shipped oracle: `EXIT=1`**, matching the value
`23-ACCEPTANCE-SETUP-2.md` recorded at `2026-07-26T21:28:11Z` — unchanged in
the ~40 minutes between setup and this launch.

**What the 23f reachability guard did at launch: it allowed the launch.**
`devflow start` proceeded past the guard, created the worktree
(`<repo-root>/.worktrees/phase-24`) and the branch `feature/phase-24` from
`develop`'s tip, and produced no reachability-refusal message of any kind.
This is the guard's first exercise against the exact condition that killed
the 2026-07-26 attempt (`23-ACCEPTANCE-RUN.md` — Phase 24's ROADMAP entry
unreachable from `develop`) — and, with 23-14's fast-forward and the
now-merged guard in place, phase 24 is reachable and the guard said nothing,
as designed.

**Verbatim launch output (both worktree-creation and the blocking error came
back on the same invocation):**

```
$ devflow start --phase 24 --agent claude --mode auto --yes-ship
created worktree: <repo-root>/.worktrees/phase-24 (branch feature/phase-24)
error: self-dogfood stale build blocked for stage define: a build-relevant file (.rs/Cargo.toml/Cargo.lock/build.rs/rust-toolchain.toml) changed in <repo-root>/.worktrees/phase-24's tracked source since this devflow binary was built, or its embedded commit is not an ancestor of current HEAD at all — rebuild devflow before driving its own workspace (D-18; the Phase 16 false-evidence incident) — evaluated against this phase's WORKTREE HEAD, not the main checkout; rebuild and reinstall the binary before resuming
EXIT=1
```

**This is not the 23f guard refusing — it is a different, later guard in the
same `start` invocation.** The 23f reachability guard (`ensure_phase_reachable_on_base`)
ran first and passed (the worktree was created). The self-dogfood build-staleness
guard (D-18, `enforce_build_staleness`) ran immediately after, evaluated
against the freshly created worktree's own `HEAD`, and blocked. Both are
recorded as separate, distinguishable findings — this section states plainly
what each one did.

---

## 2. What happened — event excerpts, verbatim, in stage order

Every `"phase":24` line ever written to `.devflow/events.jsonl` across *both*
acceptance attempts (23-11's and this one) exists in the log; only the lines
from **this** attempt (`ts` starting at `1785103662`, corresponding to
`2026-07-26T22:07:42Z`) are this task's own evidence:

```json
{"agent":"claude","commit":"0c9dcfecb9c15cf39a07c766e91f805df67f56ab","dirty":"false","event":"workflow_started","exe_path":"devflow","mode":"auto","phase":24,"ts":1785103662,"v":1,"version":"1.8.1","worktree":"<repo-root>/.worktrees/phase-24"}
{"event":"self_dogfood_stale_blocked","phase":24,"reason":"stale_build_blocked","stage":"define","ts":1785103662,"v":1,"worktree":true}
```

**Both lines share the same `ts` (`1785103662`)** — the block fired within
the same second as launch, before any stage-launch event, before any monitor
process, before any Claude invocation.

**`workflow_shipped` — absent.** Never emitted for phase 24 across either
attempt.

**`workflow_finished` — absent.** Never emitted for phase 24 across either
attempt.

**`stage_launched` — absent for this attempt.** No stage of any kind ever
launched; the block fires inside `launch_stage`, before `monitor::spawn_monitor`
is called (source: `crates/devflow-cli/src/staleness.rs` doc comment on
`enforce_build_staleness`, confirmed by reading the function).

**No agent turn ran.** The `.devflow/phase-24-stdout` / `-stderr.log` /
`-exit` / `-agent-pid` files present on disk at the time of this run are
**stale leftovers from the previous (23-11) attempt**, not new output from
this one — confirmed directly: this attempt's block fired before any process
that could have written them was ever spawned, and their content (quoted in
full in `23-ACCEPTANCE-RUN.md` §2) is the earlier attempt's Define-stage
agent turn, unchanged. No `.devflow/phase-24-*` capture file was produced by
this attempt, because no agent process ever ran for it to capture. **This is
itself recorded as evidence** — its absence is exactly as informative as a
capture file would have been (per the plan's own instruction that an absent
event, or in this case an absent capture, is evidence).

---

## 3. Stop cause and cleanup state

**Stop cause:** launch itself, stage `define` (never actually entered), the
self-dogfood staleness hard block (`self_dogfood_stale_blocked`,
`reason: "stale_build_blocked"`). No gate was ever written for this attempt —
the block is `enforce_build_staleness`'s own hard-block path, which the
source (`staleness.rs:286-294`) documents as "deliberately NOT an approvable
gate" — so there was nothing pending to poll toward a 10-minute or 30-minute
timeout. The run terminated synchronously, inside the foreground `devflow
start` process, with exit code `1`, before any detached monitor was ever
created.

**No `devflow stop` was needed or run for this outcome** — there was no lock,
no monitor, no persisted `state-24.json` for a stop to act on. Confirmed
directly:

```
$ test -f .devflow/state-24.json && echo EXISTS || echo absent
absent
$ ls .devflow/lock-24 2>&1
ls: cannot access '.devflow/lock-24': No such file or directory
```

**Cleanup state at the moment observation ended** (immediately, since the
outcome was already terminal at launch): `devflow status .` reports `stage:
idle`; `devflow gate list --all-roots` shows zero rows for phase 24; the
worktree `<repo-root>/.worktrees/phase-24` and branch `feature/phase-24`
existed (created before the block fired) until Task 2's post-run hygiene
removed them (§7).

**Acceptance criterion NOT met by this run.** Recorded per the plan's
instruction: "record that the acceptance criterion was not met by that run,
along with everything observed."

---

## 4. Absent events (explicit list)

| Event | Status | Why it matters |
|---|---|---|
| `workflow_shipped` | **absent** | The sole arbiter of ACCEPTANCE PASSED (23-06). Its absence alone is sufficient to fix `outcome: run-incomplete`. |
| `workflow_finished` | **absent** | Not even the ambiguous older event fired. |
| `stage_launched` (any stage) | **absent** | The block fires before `launch_stage` reaches `monitor::spawn_monitor` — no stage, including `define`, was ever actually launched. |
| `advance_evaluated` | **absent** | No `devflow advance` process was ever spawned; there was nothing to evaluate. |
| `transition` (any) | **absent** | The pipeline never advanced between stages. |
| `capture_archived` | **absent** | Only fires on a successful stage-to-stage transition; never reached. |
| `gate_fired` / `notify_fired` / `gate_resolved` | **absent** | The staleness hard block is explicitly not an approvable gate (`staleness.rs`'s own doc comment); it emits its own named event (`self_dogfood_stale_blocked`) instead of a `gate_fired`/`notify_fired` pair. |
| `workflow_aborted` | **absent** | No workflow ever reached a state from which it could be aborted; it never started running a stage. |

---

## 5. Poll series (timestamped)

Unlike the previous attempt, this run's terminal condition was established
**at launch**, not after a period of polling a live, detached process — there
was no monitor to poll. The poll series below documents that this was
confirmed, not assumed, immediately and again ~2 minutes later to rule out
any delayed/asynchronous continuation:

```
[2026-07-26T22:07:12Z] pre-launch: devflow status → stage: idle; gate list → 5 rows, all phase 12, /tmp/.tmp* roots; evidence --require-shipped → EXIT=1
[2026-07-26T22:07:42Z] launch: devflow start --phase 24 ... → worktree created, then self_dogfood_stale_blocked, EXIT=1 (foreground process terminated synchronously)
[2026-07-26T22:08:03Z] poll: events.jsonl tail for phase 24 → workflow_started + self_dogfood_stale_blocked, both ts=1785103662; devflow status → stage: idle; gate list --all-roots → zero rows for phase 24; state-24.json → absent; ps aux → zero real phase-24 matches (2 self-referential only)
[2026-07-26T22:09:58Z] poll: devflow status → stage: idle (unchanged); gate list --all-roots → zero rows for phase 24 (unchanged) — confirms no delayed/asynchronous continuation occurred after the synchronous block
```

Polling tested for **event names** across the whole recent tail of
`.devflow/events.jsonl` (`workflow_started`, `stage_launched`,
`advance_evaluated`, `transition`, `capture_archived`, `gate_fired`,
`gate_resolved`, `notify_fired`, `workflow_shipped`, `workflow_finished`,
`self_dogfood_stale_blocked`), never by substring-matching a single last
line — explicitly avoiding the probe's own documented instrument defect
(`23-PROBE-FINDINGS.md`).

No 30–60s sustained-cadence polling window was required, because the run
never reached an in-flight, detached state to observe — the terminal
condition was established with certainty (agreement across `events.jsonl`,
`devflow status`, `devflow gate list --all-roots`, and the process
inventory) within seconds of launch, and reconfirmed unchanged ~2 minutes
later.

**Turn budget: not a factor in this observation.** The run was not stopped
to fit a turn — it had already terminated, on its own, before this executor
took any polling action at all.

---

## 6. Manual intervention — explicit assertion

**Manual intervention: none.** `devflow start` was invoked exactly once, with
the exact flags the plan specifies, and it terminated on its own. No `devflow
advance`, `devflow stop`, `devflow gate approve/reject`, no signal, no hand
edit of any state or event file, no touching of the worktree beyond what
`devflow start` itself created. The one non-observational action taken after
the block was `devflow cleanup` (Task 2, §7), run only after the phase-24
attempt had already fully and synchronously terminated — disclosed there in
full, matching the previous attempt's disclosure standard.

---

## 7. The shipped oracle — pre-run/post-run delta, and nothing else substituted

**Pre-run baseline** (quoted verbatim from `23-ACCEPTANCE-SETUP-2.md`,
recorded `2026-07-26T21:28:11Z`): `devflow evidence --phase 24
--require-shipped` exited **1**, with `shipped: false`.

**Post-run:**

```
$ devflow evidence --phase 24 --json
{
  "phase": 24,
  "shipped": false,
  "workflow_finished_seen": false,
  "finished_reason": null,
  "stage": null,
  "state_present": false,
  "feature_branch_exists": true,
  "merged_into_develop": true,
  "has_remote": true
}

$ devflow evidence --phase 24 --require-shipped
phase: 24
shipped: false
workflow_finished_seen: false
finished_reason: none
stage: none
state_present: false
feature_branch_exists: true
merged_into_develop: true
has_remote: true
error: phase 24 has not shipped — DevFlow has no record of a completed Ship
EXIT=1
```

**Exit code, adjacent to the declaration: pre-run `EXIT=1` → post-run
`EXIT=1`. No change. `outcome: run-incomplete` (line 1 of this document)
agrees with both** — they never disagreed at any point, no correction to
Task 1's declaration was needed.

**The acceptance verdict rests on exactly the two facts the plan specifies,
and nothing else:**
1. A `workflow_shipped` event for phase 24 in `.devflow/events.jsonl` —
   **absent** (§4). No such line exists to quote.
2. `devflow evidence --phase 24 --require-shipped` exiting 0 — **it exits 1**,
   quoted above in full.

Both facts say the same thing: **ACCEPTANCE FAILED.**
`workflow_finished` is also absent (not merely present-but-disqualified), so
there is no ambiguous line to explain away here — this run does not even
reach the false-green class 23-06 was built to close; it falls short of it
entirely, at an earlier point than the previous attempt did.

**`feature_branch_exists: true` and `merged_into_develop: true` are both
artifacts of the branch's own zero-commit history, not evidence of a real
merge** — identical in shape to the same finding in `23-ACCEPTANCE-RUN.md`
§7. `feature/phase-24` was created at, and never diverged from, `develop`'s
own tip (`0dad20d3e85d82d60235b8f91cb944e4cbed433c`) before the block ended
the attempt: `git rev-list --count 0dad20d3e85d82d60235b8f91cb944e4cbed433c..feature/phase-24`
was `0` before the branch was removed in §8's cleanup — zero commits ahead
reads as "merged" by the evidence oracle's own trivial-ancestry definition,
not because any real merge occurred.

---

## 8. Post-run hygiene — proving the run left nothing behind

### `devflow gate list --all-roots`

```
$ devflow gate list --all-roots
PHASE  STAGE     AGE       ROOT / CONTEXT
12     code      2h!       /tmp/.tmp8YPpPz
12     plan      48m!      /tmp/.tmpBV38S7
12     plan      7h!       /tmp/.tmpNuFaCh
12     code      7h!       /tmp/.tmpSAHPzj
12     code      5h!       /tmp/.tmpqZmoON
```

**Zero rows for phase 24.** All five rows are phase 12, all rooted under
`/tmp/.tmp*` scratch directories — the known `23-FINDINGS.md` §A1/§A3 noise
class (leaked `devflow advance` registrations from this project's own
`gate_sweep_e2e.rs`/`stop_e2e.rs` test suites and older phase-12 fixtures),
named as such rather than mistaken for this run's residue. **One row beyond
the orchestrator's pre-dispatch baseline of four** (`.tmp8YPpPz`,
`.tmpBV38S7`, `.tmpNuFaCh`, `.tmpSAHPzj`) appeared during the session:
`/tmp/.tmpqZmoON`, also phase 12. This is the same noise class accruing
further between the orchestrator's snapshot and this measurement — a fifth
instance of the identical pre-existing pattern, not a new class of finding,
and not attributable to the phase-24 attempt (which never wrote any gate at
all — its block path is explicitly not an approvable gate, §3).

### `devflow gate sweep --dry-run`

```
$ devflow gate sweep --dry-run
would reap phase 12 plan (age 25535s) at /tmp/.tmpNuFaCh
would reap phase 12 code (age 25819s) at /tmp/.tmpSAHPzj
sweep complete (dry run): 2 would be reaped, 0 skipped, 3 left alone
EXIT=0
```

Nothing mutated (dry run). Zero phase-24 lines — consistent with zero rows
above. No §A2 duplicate-count over-report observed in this measurement (2
would-reap + 3 left-alone accounts for all 5 registered rows exactly, no
double-count).

### Process inventory (read-only)

```
$ ps aux | command grep -c 'phase.24\|phase-24'
2
```

Both matches are self-referential — the shell command's own text containing
the literal string `phase.24\|phase-24`, and the `grep` process itself
matching its own argv (confirmed by inspecting the raw matched lines: one is
this session's own `zsh -c ... eval '...phase.24...'` wrapper, the other is
literally `grep phase.24\|phase-24`). **Zero real processes reference phase
24.** Matches the empty `.devflow/state-24.json` / absent
`.devflow/lock-24` already recorded in §3.

### `devflow status`

```
$ devflow status .
stage: idle
project_root: <repo-root>

open branches:
  feature/phase-23 — 9 ahead (2 behind develop)
```

No active phases, no pending gate, no worktree listed (worktree/branch
cleanup, below, has already run by the time this was captured for this
section — see the ordering note under §9).

### Worktree and branch cleanup, with the ref hazard checked

**Before `devflow cleanup`:**

```
$ git branch --list 'recovery/*'
  recovery/pre-23-11-acceptance-e0f87c2
$ git ls-remote origin 'refs/heads/recovery/*'
e0f87c2c2230257f7aa8092a836225626941d09a	refs/heads/recovery/pre-23-11-acceptance-e0f87c2
0dad20d3e85d82d60235b8f91cb944e4cbed433c	refs/heads/recovery/pre-23-15-acceptance-0dad20d
```

This run's own recovery ref (`recovery/pre-23-15-acceptance-0dad20d`) has
**no local copy** — by design (23-14 Task 2), it was never fetched or
branched locally. The *older* `recovery/pre-23-11-acceptance-e0f87c2` local
branch is present — restored earlier in this phase's history per
`23-FINDINGS.md` §B2a, which explicitly says not to keep re-restoring it.

**Cleanup:**

```
$ devflow cleanup
deleting branch: feature/phase-24
removed worktree <repo-root>/.worktrees/phase-24 (branch feature/phase-24 kept: git command failed: error: the branch 'feature/phase-24' is not fully merged ...)
cleaning up merged branch: feature/phase-24
cleaning up merged branch: recovery/pre-23-11-acceptance-e0f87c2
deleted merged branch feature/phase-24
deleted merged branch recovery/pre-23-11-acceptance-e0f87c2
EXIT=0
```

**After `devflow cleanup`:**

```
$ git branch --list 'recovery/*'
(empty)
$ git ls-remote origin 'refs/heads/recovery/*'
e0f87c2c2230257f7aa8092a836225626941d09a	refs/heads/recovery/pre-23-11-acceptance-e0f87c2
0dad20d3e85d82d60235b8f91cb944e4cbed433c	refs/heads/recovery/pre-23-15-acceptance-0dad20d
$ git worktree list
<repo-root> 00b6f10 [feature/phase-23]
$ git branch --list feature/phase-24
(empty)
```

**Disclosed exactly as `23-FINDINGS.md` §B2 predicted it would recur:**
`devflow cleanup` deleted the local `recovery/pre-23-11-acceptance-e0f87c2`
branch again — the **third** recorded occurrence of this exact behavior in
this phase's history (the first two are in `23-ACCEPTANCE-RUN.md` §6/§7 and
`23-FINDINGS.md` §B2). **Both `origin` copies survived untouched** — the
read-back above shows both `recovery/pre-23-11-acceptance-e0f87c2` and
**this run's own** `recovery/pre-23-15-acceptance-0dad20d` unchanged on
`origin` at their recorded SHAs. **Per `23-FINDINGS.md` §B2a's own explicit
instruction ("do not restore the local copy — `devflow cleanup` will keep
deleting it"), the local branch was deliberately NOT restored this time.**
The `origin` ref remains the authoritative copy and is unaffected. This
run's own recovery ref never had a local copy to lose in the first place, so
it was never at risk from this exact hazard during this session — it
survives on `origin`, unused, because the run never reached a stage that
could have needed it (§9).

`feature/phase-24` was also correctly deleted (both the worktree and the
branch) as part of the same cleanup pass — expected and desired, since the
branch carried zero commits and the acceptance attempt using it had already
fully terminated.

---

## 9. Post-run git evidence, against the operator's prediction

| | Predicted (`23-ACCEPTANCE-SETUP-2.md` Task 3) | Actual (post-run) |
|---|---|---|
| Merge into `develop` | Expected, as part of a successful Ship | **None.** `develop`/`origin/develop` unchanged at `0dad20d3e85d82d60235b8f91cb944e4cbed433c`, identical to the pre-run tip. |
| Resulting version | Operator's stated prediction: **`1.8.2`** (given knowingly against the orchestrator's own `~1.11.339` counter-finding — "the bad version IS the finding") | **No version bump occurred.** `Cargo.toml` still reads `version = "1.8.1"`. Neither `1.8.2` nor `~1.11.339` was produced, because `VersionBump` never ran — the run never reached Ship, or even Define. |
| Changelog commit | Expected, as part of `hooks_after_ship` | **None.** `CHANGELOG.md`'s `## 2.0.0` heading is the same pre-existing, already-committed entry from plan 23-08's landed breaking removal — this run added nothing to it. |

```
$ git fetch origin
$ git rev-parse develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
$ git rev-parse origin/develop
0dad20d3e85d82d60235b8f91cb944e4cbed433c
$ command grep '^version' Cargo.toml
version = "1.8.1"
$ head -3 CHANGELOG.md
# Changelog

## 2.0.0 — 2026-07-26
```

**Neither prediction was tested, because `VersionBump` never ran.** The
operator's `1.8.2` and the orchestrator's `~1.11.339` `compute_version`
finding both remain untested predictions — this run stopped before the point
where either could have been produced or falsified. This is stated plainly
rather than read either way: it is not a match, and it is not a mismatch —
it is a non-event, because the mechanism that would have produced a version
number never executed.

---

## 10. Self-dogfood staleness path — the hard block, exercised for the first time in a real run

**This closes `23-FINDINGS.md` §B3, the exact coverage gap the previous
attempt left open**, and it does so directly rather than by inference: the
block did not merely become theoretically reachable — it actually fired and
stopped this attempt.

**Source-grounded account of what happened, read directly against
`crates/devflow-cli/src/staleness.rs`, not inferred from the error message
alone:**

The running binary's embedded commit is `0c9dcfecb9c15cf39a07c766e91f805df67f56ab`
(`docs(23-13): record operator merge of PR #32 and stale-local-develop
finding` — a `.planning/`-only commit on `feature/phase-23`, made when
`23-GUARD-SHIP-RECORD.md` Task 2 was written, i.e. from *before* this
session's own binary rebuild-freshness re-check, since no rebuild was needed
between 23-13 and now). The phase-24 worktree's `HEAD` is `develop`'s tip,
`0dad20d3e85d82d60235b8f91cb944e4cbed433c` (a later, and diverged, commit).

```
$ git merge-base --is-ancestor 0c9dcfecb9c15cf39a07c766e91f805df67f56ab 0dad20d3e85d82d60235b8f91cb944e4cbed433c; echo "exit: $?"
exit: 1
$ git merge-base --is-ancestor 0dad20d3e85d82d60235b8f91cb944e4cbed433c 0c9dcfecb9c15cf39a07c766e91f805df67f56ab; echo "exit: $?"
exit: 1
```

**Neither commit is an ancestor of the other — the two have genuinely
diverged**, not merely fallen behind linearly. `0c9dcfe` sits on
`feature/phase-23` after the guard PR's own head (`2f8686e`, which the PR
merged into `develop`); `develop`'s tip is the merge commit itself, built
from `2f8686e` plus `develop`'s own prior history. `feature/phase-23`
continued past `2f8686e` with further doc-only commits (`0c9dcfe` among
them) that were never part of the merged PR, so the fork point is
`2f8686e`, and neither branch's later commits are reachable from the other.

Per `embedded_commit_is_stale` (`staleness.rs:45-82`): the first
`merge-base --is-ancestor <embedded> HEAD` call exits `1` ("not an
ancestor"), which is ambiguous between "older/divergent" and "descendant" —
so the function probes the reverse direction. That reverse probe **also**
exits `1` here (confirmed above), which the function's own match arm
(`staleness.rs:76`) classifies as `Staleness::Stale` — genuinely divergent,
not descendant, so not `Ahead`. Combined with `is_self_dogfood_workspace`
correctly returning `true` for this workspace (`staleness_outcome`,
`staleness.rs:276-284`: `(true, Stale) => Block`), the hard block fired.

**A discovered nuance, worth naming precisely for Task 3's judgment:** the
content-aware exemption this project built specifically so that
"DevFlow's own primary workflow commits docs constantly" would not
needlessly re-arm the block (`ancestry_range_affects_build`,
`staleness.rs:84-101`, cited in that function's own doc comment as the 21d /
999.29 fix) is invoked **only** from inside the strict-ancestor branch of
`embedded_commit_is_stale` (`staleness.rs:54-63`) — the case where the
embedded commit **is** an ancestor of `HEAD` and `HEAD` has simply moved
forward. It is **never consulted** on the divergent-lineage path this run
actually took (`staleness.rs:69-79`), which goes straight to
`Staleness::Stale` on a second `exit 1` with no content-diff check at all.
**Concretely: the one commit that made this binary's embedded commit
"divergent" instead of "an ancestor" was itself docs-only** (`0c9dcfe` touches
only `23-GUARD-SHIP-RECORD.md`) — the exact class of commit the content-aware
exemption exists to look past — but because the relationship is divergence
rather than linear staleness, that exemption's own logic is never reached to
look past it. Whether this is the guard behaving correctly (a divergent
embedded commit is a stronger and more legitimate staleness signal than
linear staleness, regardless of content) or an unintended gap in 21d/999.29's
coverage is a judgment call, not resolved here — recorded as a precise,
source-grounded finding for Task 3 rather than a fixed conclusion, per this
plan's own no-source-changes boundary.

**A structural consequence worth naming for any future attempt on this
project's own working style:** rebuilding again from this session's current
`feature/phase-23` `HEAD` would **not** have resolved this. `feature/phase-23`
has continued to accumulate commits past the guard PR's merged head
(`2f8686e`) — including this very plan's own commits — so any binary built
from `feature/phase-23` will have an embedded commit that is, by
construction, **not** an ancestor of `develop`'s tip, for exactly the same
divergent-lineage reason demonstrated above. The only way to obtain a binary
whose embedded commit is provably an ancestor of `develop`'s tip is to build
from a checkout of `develop` itself (or an ancestor of it) — not from the
working branch this phase has been executed on throughout. This is recorded
as a finding about how this project's own acceptance-run precondition
(binary freshness re-checked against `origin/develop`'s tree content, §1)
interacts with the staleness guard's ancestry check (which is stricter — it
requires an actual commit relationship, not merely identical tree content) —
not as something this plan attempted to work around.

Confirmed, per `23-FINDINGS.md` §B3's own naming: **the `Stale`/hard-block
branch has now fired in a real run.** D-02 chose the self-hosted target
specifically because this branch is structurally unreachable in a scratch
repository — this attempt demonstrates it is very much reachable in this
project's own normal working style (a long-lived feature branch that keeps
committing docs past the point its own PR was merged).

---

## 11. Full gate chain (post-run tree)

Run as a direct `&&` status chain, not a piped/grep shape:

```
$ cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check
```

**Per-binary test result lines, summed:** 608 passed, 0 failed, across 17
binaries (identical composition to `23-ACCEPTANCE-SETUP-2.md`'s pre-run
baseline: 184 + 3 + 7 + 4 + 1 + 1 + 1 + 3 + 17 + 8 + 2 + 9 + 1 + 363 + 2 + 2
+ 0 = 608).

```
$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.78s
CLIPPY_EXIT=0
$ cargo fmt --check
FMT_EXIT=0
```

**CHAIN_EXIT=0.**

**Delta against the pre-run baseline** (`23-ACCEPTANCE-SETUP-2.md`: 608
passed / 0 failed / clean clippy / clean fmt): **zero.** Exactly expected —
this run never touched `crates/`, `Cargo.toml`, or `Cargo.lock` (it never
got past the staleness block, and the block fires before any source-writing
stage runs), so a byte-identical test count is the correct, unremarkable
outcome.

---

## 12. Mechanisms this run left unexercised

- **Everything past launch.** Define, Plan, Code, Validate, Ship, and every
  stage-to-stage `transition` were never reached — this run stopped one
  layer earlier than the previous attempt (`23-ACCEPTANCE-RUN.md`, which at
  least reached and gated inside Define).
- **`--yes-ship` Ship-gate pre-authorization (D-04/D-05/D-06, 23-09).** Not
  exercised at all this time — not even the state-persistence half the
  previous attempt confirmed, since `state-24.json` was never written
  (confirmed absent, §3).
- **The `hooks_after_ship` batch** (`Merge` → `VersionBump` →
  `ChangelogAppend` → `BranchCleanup`). Never reached.
- **`devflow resume`** (the rate-limit/infra-pause resume path). Not
  triggered.
- **The never-silent gate mechanism / `devflow stop`'s clean-unwind path.**
  Not exercised this time — there was no gate to fire and nothing to stop
  (contrast with the previous attempt, which exercised both).
- **The 23f reachability guard's *refusal* path.** Only its *allow* path was
  exercised this time (§1) — it was never seen refusing a genuinely
  unreachable phase in this session (that refusal was proven separately, in
  a throwaway clone, by `23-GUARD-SHIP-RECORD.md` Task 3).

**Exercised, for contrast:** the 23f reachability guard's allow path (a
genuine, real-run first); the self-dogfood staleness hard block's `Stale`
classification and `Block` outcome, in a real self-hosted run, for the first
time (§10); `devflow gate list --all-roots` and `--dry-run` sweep correctly
reporting zero phase-24 rows against a clean, blocked-before-registering
attempt; the evidence oracle correctly reading a never-shipped, never-even-
started phase as not-shipped both before and after, with no disagreement;
`devflow cleanup`'s worktree/branch removal, including its now-thrice-
recorded recovery-ref side effect (§8).

---

## 13. What this run does and does not prove

**Does prove:** the 23f reachability guard, merged and in place for the
first time in a real acceptance attempt, correctly allowed a launch against
a genuinely reachable phase — the exact condition that killed the previous
attempt does not recur here. The self-dogfood staleness hard block (D-18)
is not merely theoretically present in this codebase; it fires, for real,
against this project's own actual working pattern (a long-lived feature
branch whose docs commits keep landing after its own guard PR was merged),
and it did so here before any agent turn, before any cost was incurred, and
before any write reached `develop`. The evidence oracle
(`devflow evidence --require-shipped`) correctly reads a blocked-at-launch
run as not-shipped, in agreement with the absence of any `workflow_shipped`
or even `workflow_finished` event. `devflow gate list`/`sweep --dry-run`/
`cleanup` all behaved correctly against this run's own footprint.

**Does not prove:** that DevFlow can drive a phase all the way to a
completed, shipped Ship stage unattended — this run went **less far** than
the previous attempt (blocked before Define even launched, vs. the previous
attempt reaching and gating inside Define). The phase's own acceptance
criterion — "one phase has actually been driven start-to-finish by devflow,
unattended, reaching a completed Ship stage" — remains **unproven**, for the
second consecutive attempt, and for a different reason each time (target
unreachable from `develop`, then a genuinely stale binary relative to that
same target once reachability was fixed). One run under one specific set of
conditions is one sample; nothing here should be read as a reliability claim
about repeated runs, only as a record of what this specific attempt, under
these specific conditions, actually did.

**On the phase's own acceptance criterion:** not met by this attempt either.
What this attempt establishes, beyond what the previous one did, is that the
staleness guard's hard-block path — this phase's own most safety-critical
untested mechanism, per `23-FINDINGS.md` §B3 — genuinely works end to end in
a real self-hosted run, and that this project's actual working style
(a feature branch accumulating docs commits well past its own merged PR)
makes that block considerably easier to trigger than a synthetic test would
suggest. Whether that is grounds for `accepted with gaps` or `failed`, and
what a further attempt would need (very likely: a binary built from a
`develop` checkout rather than from the working branch), is Task 3's
judgment, not this document's.

---

## 14. Redaction

Checklist: OS username, home-directory basename, absolute home paths,
temporary-directory paths, remote URLs. This document contains no GitHub
URLs (no PR was opened, no merge occurred, nothing shipped), so the
username-vs-account-name interpretation question that applied to
`23-GUARD-SHIP-RECORD.md` and `23-ACCEPTANCE-SETUP-2.md` does not arise
here — the grep below is a plain, unqualified pass.

```
$ rg -n '/home/denniyahh|/var/home/denniyahh' .planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-RUN-2.md
(no match)
$ rg -c 'denniyahh' .planning/phases/23-end-to-end-dogfood/23-ACCEPTANCE-RUN-2.md
0
```

Grep run against the file as committed, at the point this section was
written. `/tmp/.tmpXXXXXX`-style scratch-root names (§8's gate-list rows)
are left as-is per this phase's established convention — they carry no home
directory or username, only opaque `mktemp`-style random suffixes.

