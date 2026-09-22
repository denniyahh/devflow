I've completed the analysis: static trace of the fix plus a build in a scratch copy, with the new tests run and revert-mutations performed. Findings below (all paths relative to `snapshot/`).

---

## CONFIRMED

### C-1. Partial gate removal is dropped on the error path, so `recover --clean --phase N` can print "nothing was removed" after actually deleting a gate file
**File:** `crates/devflow-core/src/recover.rs:255`, `crates/devflow-core/src/gates.rs:331-360`, `crates/devflow-core/src/recover.rs:320-328`, `crates/devflow-cli/src/commands.rs:2576-2585`
**Mechanism:**
1. `remove_gate_files` (recover.rs:252-258) loops `for stage in STAGES { removed |= Gates::cleanup(...)?; }`. `removed` accumulates per-stage results.
2. `Gates::cleanup` (gates.rs:325-360) sets `removed = true` at :335 (gate/response/ack) and :357 (temps) but propagates via `?` at :334/:356. If a *later* path fails (e.g. `remove_file` on a directory → EISDIR), it returns `Err`, discarding its own `removed` flag.
3. The `?` in `remove_gate_files` (recover.rs:255) then returns `Err` with the accumulated `removed` (already `true` from an earlier stage) discarded.
4. `clean_phase_files`'s gate-error arm (recover.rs:323-328) calls `report.fail(...)` and `return`s **without setting `removed_anything`**.
5. `recover_cmd` (commands.rs:2577-2585) picks the `"nothing was removed"` fallback because `removed_anything` is false.

**Reproduction:** scratch test in a copy of the snapshot — phase 61 with a real `61-define.json` gate and a directory at `61-plan.json`:
```
removed_anything=false removal_failed=true
warnings=["kept phase 61's state — could not remove all of its gate files: gate I/O failed: Is a directory (os error 21)"]
define gate still exists: false
```
The Define gate was deleted, yet the report (and the CLI message it drives) states nothing was removed. Same shape exists in `clear_state` (workflow.rs:340-351: per-phase state removed, then legacy removal `?` at :350 fails → `removed` lost) and `delete_cron_instructions` (ship.rs:157-168: per-phase record removed, legacy `?` at :167 fails → `removed` lost); those need a per-file permission asymmetry so are harder to reach than the EISDIR case above.
**Severity:** low — misleading message on a partial-failure path; the non-zero exit and "could not remove everything" warning still fire. But it is the exact WR-03 class ("report only what was actually removed") on the failure branch.

### C-2. A lock file the sweep fails to remove does not set `removal_failed`, so `recover --clean` still exits 0
**File:** `crates/devflow-core/src/recover.rs:147-148` and `:315-316`, `crates/devflow-core/src/lock.rs:492-493`, `OPERATIONS.md:157-158`
**Mechanism:** The fix wired gate/state/cron/legacy-state removal failures into `report.fail(...)` (which sets `removal_failed`). `remove_stale_locks` is the one removal step that is instead appended bare:
```rust
report.warnings.append(&mut crate::lock::remove_stale_locks(project_root));   // recover.rs:148 (sweep) and :316 (--phase)
```
`remove_stale_locks` pushes `"could not remove {path}: {err}"` when `fs::remove_file` fails (lock.rs:492-493) but returns only `Vec<String>`. Nothing downstream sets `removal_failed`, so `recover_cmd` (commands.rs:2624-2628) never returns non-zero for it — contradicting the promise the fix itself added at OPERATIONS.md:157-158 ("exits non-zero if anything could not be removed").
**Reproduction:** not run end-to-end (needs a stale `lock-NN` whose `remove_file` fails, e.g. read-only `.devflow` with a pre-existing `.lock-NN.coord` or an immutable file); the wiring gap is unambiguous in the code. The pre-existing `remove_stale_locks` doc comment (lock.rs:441-448) even states its warnings exist so a callers "report[s] … a clean sweep that left wedging locks behind", which the exit code currently does not do.
**Severity:** low — a lingering stale lock is reclaimed by the next `acquire`, but the exit-code/`removal_failed` gap is a sibling of the failure-reporting class WR-03/WR-04 fixed.

### C-3. The sweep silently leaves corrupt per-phase (and legacy) cron records — the same "unparsable file skipped without a word" that the fix just closed for state files
**File:** `crates/devflow-core/src/ship.rs:129-146`, `crates/devflow-core/src/recover.rs:151-160`, `crates/devflow-core/src/ship.rs:102`
**Mechanism:** `list_cron_instructions` (ship.rs:129-146) only `found.push(...)`s records that parse (`if let Ok(instructions) = serde_json::from_str` at :138-141); an unparsable `cron-instructions-NN.json` is skipped with **no warning**. The sweep's cron loop (recover.rs:151-160) therefore never iterates that phase, never deletes the record, and never reports it; `removal_failed` stays false and `clean()` returns `warnings=[]`. The CLI then prints "no stale workflow state was cleaned" and exits 0 while the record remains. This is reachable because `write_cron_instructions` writes non-atomically (`std::fs::write` at ship.rs:102), so a crash mid-write leaves exactly such a file. Contrast: the same fix added `state_file_phases` + "kept phase N — its state file cannot be parsed" for corrupt **state** files (recover.rs:131-137), and `--phase N` *does* remove the corrupt cron record (delete_cron_instructions removes by path regardless of parse, ship.rs:155-158).
**Reproduction:** scratch test — write `.devflow/cron-instructions-77.json` = `{corrupt`, then `clean(root)` → returned `warnings=[]`, `removal_failed=false`, and the file still existed.
**Severity:** low — inert leftover (the cron poller also skips it), but a sweep that silently leaves an artifact it is documented to reset (and reports nothing) is the WR-02/WR-03 class.

---

## SUSPECTED

### S-1. `clear_state`'s orphan-state-temp removal failures are only `warn!`-logged, not surfaced — unlike `Gates::cleanup`, which propagates the same failure
**File:** `crates/devflow-core/src/workflow.rs:359-371` vs `crates/devflow-core/src/gates.rs:352-357`
**Mechanism:** `clear_state` sweeps orphan state temps with `Err(error) => warn!(...)` (workflow.rs:365-369) — a failed temp removal is a tracing log only, not a returned warning and not `removal_failed`. `Gates::cleanup` does the same job with `remove_file(entry.path())?` (gates.rs:356), so a failed gate-temp removal fails the clean and is surfaced. A state-temp that cannot be removed (EACCES) would let `recover --clean` report success with no operator-visible trace of the failure.
**Evidence to confirm/refute:** make an orphan `.state-NN.json.<pid>.<seq>.tmp` unremovable (read-only dir after creating the temp) and run `clean_phase_report`; expect "cleaned up"/"nothing to clean" with the failure only in the trace log. If the project intends best-effort temp cleanup to stay best-effort, this is by design and should be stated rather than left asymmetric with `Gates::cleanup`.
**Severity:** low.

---

## CHECKED AND CLEAN

- Monitor: TERM before/at fork (Windows A/F) still writes the marker and kills `$!`/`$apid` because `reaped` is empty then — `monitor.rs:525-527`; unchanged behavior.
- Monitor: TERM during `wait` with the agent alive is byte-for-byte the pre-fix behavior (guard is trivially true, `reaped` only set after `wait` returns) — `monitor.rs:531`; no WR-08 regression.
- Monitor: TERM after reap, while the `advance` tail runs, is a no-op (marker + kill both inside `if [ -z "$reaped" ]`) — `monitor.rs:525-526`.
- Monitor: the one remaining marker-write window (`wait` returns → `reaped=1`) exits via `cleanup`'s `exit 0` *before* `advance`, so the next `spawn_monitor`'s Rust-side `remove_file(stop_path)` clears it as stale — `monitor.rs:506-512` + `513-518`; claim holds on both shells (assignments/builtins are not "foreground commands" that defer traps).
- Monitor: `MonitorTail::Advance` reproduces the old `run_advance=true` string exactly; `Script` is `#[cfg(test)]`, so no production path can construct it — `monitor.rs:303`, `monitor.rs:524-530`.
- Monitor: PipeOwning arm ignores the tail the same way it ignored `run_advance` (`let _ = tail`) — `monitor.rs:377`.
- Monitor test is non-vacuous: revert-mutation (unconditional `cleanup`) made `a_term_after_the_agent_exits_leaves_no_stop_marker` fail on bash with the marker assertion; the marker check is shell-independent so it also guards dash.
- Sweep reaches per-phase state (`clear_state`, workflow.rs:335-341), legacy state naming the phase (workflow.rs:345-351), state temps (workflow.rs:359-371), gate request/response/ack + their temps (gates.rs:325-360), per-phase + legacy cron (ship.rs:153-170), corrupt legacy state (recover.rs:139-145), stale locks (lock.rs:449-496).
- Orphan sweep re-checks `state_path` after taking the lock, closing the listing→lock race — recover.rs:212-215.
- No live window has gates-but-no-state: `start` saves state before `launch_stage` (commands.rs:695) and gates are only written later, so `sweep_orphan_gates`'s `state_path(...).exists()` guard (recover.rs:208) is sufficient; `abort`/`finish_workflow` (pipeline_gate.rs:465-466, 273-275) only clear state when the phase is dead.
- `Gates::phases_on_disk` matching a non-gate file (e.g. `7-notes.txt`) only wastes a lock and `remove_gate_files` returns `Ok(false)` — `Gates::cleanup` removes only the five real stage paths and their `.tmp` prefixes (gates.rs:326-360), so no non-gate file is deleted; it cannot miss a real gate (names are always `padded-stage...`, first `-` is the phase/stage separator).
- Callers of the three return-type-changed helpers (`clear_state` at pipeline_gate.rs:275,466; `Gates::cleanup` at pipeline_launch.rs:1167, pipeline_gate.rs:178/240/260/273/274/465, preflight.rs:1408/1441, pipeline_outcomes.rs:917/926; `delete_cron_instructions` in recover) all use `?` or `let _`, dropping the `bool` — no behavior change.
- `clean_report` can no longer return `Err` (every step is downgraded), so `recover_cmd`'s `?` at commands.rs:2600 is vestigial but harmless.
- Per-phase gate failure no longer aborts the sweep: `sweep_phase` records and continues (recover.rs:187-192), pinned by `a_failed_gate_removal_does_not_abort_the_sweep`.
- "cleaned up"/"nothing to clean"/"no stale workflow state was cleaned" are keyed on `removed_anything`/`cleared`/`orphan_gates_cleared` (commands.rs:2576-2627) and the revert-mutations I ran (orphan-sweep disabled → `clean_removes_the_gate_files_of_a_phase_with_no_state` fails; monitor guard removed → marker test fails) confirm the new tests are non-vacuous.
