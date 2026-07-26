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

