# Phase 50 PLAN set — adversarial review, 2026-10-14

**Target:** `.planning/phases/50-addressable-monitor-liveness/50-01…50-24-PLAN.md` at `81cbf90`, reviewed on a `git archive`
snapshot. Grounding: CONTEXT, RESEARCH, VALIDATION, PATTERNS, ROADMAP, source. The lanes were also given the three
plan-checker issue lists (21 items, already fixed) so that they would not re-report them.
**Lens:** adversarial. Prompt: `prompt.txt`.

| Lane | Model | Result | Bytes | file:line citations |
|---|---|---|---|---|
| codex | `gpt-6-sol`, high effort, read-only sandbox | delivered, rc 0 | 8,026 | 13 |
| agy | `gemini-3.8-flash-high`, high effort | delivered, rc 0 | 12,427 | 5 |

No lane dropped. Claude checked every finding below against the snapshot's plan text and source.

## Confirmed (verified)

### Blockers
1. **The 107-byte socket test cannot pass** (both lanes). 50-05 refuses a path when the final path *or* its
   `part_path()` exceeds 107 bytes (`50-05-PLAN.md:129-130`). `part_path()` is `.<stem>.part`, one byte longer than
   `<stem>.sock` (`:132`). So a 107-byte final path always fails construction, and V-06 requires it to bind (`:209`).
   Fix: give the temporary name the same length or shorter (for example `<stem>.part`, without the leading dot), or
   re-derive the boundary.
2. **The live-attempt gate asks for sidecars the runtime has already deleted** (codex). `finish()` unlinks each
   launch's socket and sidecar (`50-07-PLAN.md:124-127`). `collect-sidecars` runs after the attempt and `--attempt`
   requires a sidecar for every `stage_launched` launch id (`50-18-PLAN.md:148-152`). After a run to Ship none are left,
   so the attempt gate fails even on a passing live attempt. That costs one of the 3 capped attempts. Fix: capture the
   sidecar fields at launch time (from the event or a copy), not after the run.

### Major
3. **A SIGKILL of `__monitor` between agent spawn and publish orphans the agent group** (codex). The early-exit branch
   goes straight to the ending step (`50-24-PLAN.md:110-111`). The `/proc` child snapshot exists only on the timeout
   path (`:115-120`). `AgentGroupGuard::Drop` cannot run after SIGKILL (`:163-169`). The agent leads its own group, so
   it survives and its launch files are removed. Fix: give the launcher a durable handle on the agent, for example an
   identity-checked pid file, or make the launcher a child subreaper. Test it with a pre-publish SIGKILL fixture.
4. **A TERM/INT to stage N between safe point 3 and the publish save is lost** (codex). Safe point 3 sits before the
   save, and N is then marked `HandedOff` (`50-13-PLAN.md:88-90, 124-127`; `50-23-PLAN.md:159`). A socket caller gets
   `handed_off` and retries against N+1. A signal has no caller, so the stop is dropped and N+1 runs on. Fix: after
   `HandedOff`, forward a pending signal-source stop to N+1's socket. Test it with a signal held in that window.
5. **A corrupt sidecar plus a corrupt state hides a live run from the D-09 guard** (codex). A sidecar that fails to
   read becomes `sidecar: None` (`50-06-PLAN.md:100-101`), and `sweep_for_in` filters only by the sidecar's root and
   phase (`:105-106`). The corrupt-state guard relies on that sweep (`50-15-PLAN.md:109`). Fix: when the sidecar will
   not parse, attribute the socket by its filename (`<digest>-<phase>-<launch>.sock`) and fail closed. Add a
   negative test.
6. **50-17's doc rewrite breaks `doc_check`** (agy). `doc_check` requires every `Command` variant, including
   `devflow monitor` (`main.rs:138`), to appear in the scoped docs or the allowlist (`doc_check.rs:424-431`). The only
   occurrence is `OPERATIONS.md:54`, in the `__monitor` paragraph that 50-17 replaces (`50-17-PLAN.md:103`). Neither
   the plan nor `doc-check-allowlist.toml` keeps it. Task 2's `doc_check::` gate then fails. Fix: keep the phrase in
   the new paragraph, or add the allowlist file and entry.

### Minor
7. **50-18 Task 3's gate can pass on an older fault run** (codex). It judges the newest existing `fault-*` directory
   (`50-18-PLAN.md:198`) without proving this task produced it on the current binary.
8. **A failed publish save followed by a failed shutdown loses the operator's handle** (codex). `record_failed_launch`
   runs "best effort" whatever the shutdown outcome (`50-23-PLAN.md:149-151`). The sidecar still lets `doctor` find
   the orphan, which is why this is minor. Fix: when the shutdown is not confirmed, print the `stop --launch <id>` line.
9. **The fault arm scores a pointer as an "ok" case** (codex). The `publish-unlink-window` record is `live: false`
   (`50-18-PLAN.md:146`) and counts toward the ≥ 6 ok cases (`:198`). R-10 allows fixture coverage ("the fixtures and
   the fault arm also cover"), so this is a labelling defect, not a coverage gap. Fix: count live cases separately.
10. **The executable-bit step can fail depending on order** (agy). Probed: `git update-index --chmod=+x` on an
    untracked file exits 128, and succeeds after `git add`. 50-01 says only "`git update-index --chmod=+x`"
    (`50-01-PLAN.md:134`). Fix: spell out `git add` first, or use `--add --chmod=+x`.
11. **The semver gate is credited with checking `spawn_monitor`'s return type** (agy). `50-11-PLAN.md:33` says the
    staged run checks it, but no cargo-semver-checks lint covers a non-unit return-type change, and 50-01's table has
    no row for it. Fix: correct the claim and name the break as one no lint detects (compile-checked instead).

## Disputed
- **agy (SPECULATION): N+1 SIGKILLed during its kill grace after N's re-probe reads "gone while active".** For a
  supervisor SIGKILLed mid-stop, that is a truthful report of a crash, not a misreport. The repair (`resume` or `stop`)
  is correct. No change.

## Checked and found sound (lanes' own lists, not re-verified unless stated)
- Package names, the 50-11 `devflow_dir_gitignore` count (2 tests), 50-24's start-time field index, and the existence
  of `spawn_monitor` / `run_pipe_owning_monitor` / `State::monitor_pid` (codex, with citations).
- agy's list is broad (20 lines) but mostly carries no citations. Treat it as weak evidence that those areas are clean.

## What this review does not establish
Both lanes did static review only. Neither built or ran any plan gate, and none of the code exists yet.
