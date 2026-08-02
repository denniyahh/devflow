#!/usr/bin/env python3
"""30d — exit-timing and close-with-pending-tasks harness (Phase 30, plan 30-04).

Answers TWO questions that 30a left open:

  **Mode A — drained close (review finding M1).** How long does the CLI take to
  exit after its stdin is closed, once the background task set has drained and
  the final ``result`` has arrived? The figure the ROADMAP and
  ``30a-evidence/README.md`` both cite — 0.38s — exists only in a vanished
  harness's console output. It is a single sample, unarchived and
  unreproducible. This mode re-measures it as a distribution.

  **Mode B — close with pending tasks (the undefined case).** What happens when
  stdin is closed while ``background_tasks_changed`` still lists outstanding
  ``local_agent`` tasks? Review constraint 4 requires Phase 31's monitor to
  close only on marker AND drained task set, and states plainly that
  close-with-pending-tasks "is untested and must be treated as undefined".
  This mode has NO expected outcome. Hang, immediate exit, truncated result and
  silently discarded child work are all legitimate findings.

WHY THIS IMPORTS 30c RATHER THAN EXTENDING v3
---------------------------------------------
``30a-evidence/run_experiment_v3.py`` builds a bare ``["claude", "-p", ...]``
argv and calls ``subprocess.Popen(cmd, ..., stderr=subprocess.STDOUT)`` — direct
exec, merged stderr, unscrubbed environment, no detachment. That is the
interactive shape ``30c-monitor-env-harness.py`` exists to move away from.
Measuring exit timing in it would produce numbers that cannot be read alongside
30c's delivery verdict, because they would describe a different process
environment.

So the launch path, the publish pipeline and the secret scanner are IMPORTED
from 30c. Only the experiment core — v3's two-concurrent-children prompt and its
never-block read loop — is inherited from v3, and the prompt itself is taken
from 30c's verbatim copy of it. If 30c's module cannot be loaded this harness
ABORTS. It never falls back to a direct launch: a silent fallback would publish
evidence labelled "production replica" that is not one.

WHAT THIS HARNESS ADDS OVER 30c
-------------------------------
1. **Sub-second exit timing.** 30c polls at 0.25s, so its eight recorded
   ``exit_delay_after_stdin_close`` values are 0.25 / 0.5 / 0.51 — quantised to
   its own poll interval, not a distribution. This harness polls the post-close
   exit at 2ms and measures the CLI process itself (via the pid the ``sh``
   wrapper records), not only the ``sh`` wrapper that waits on it.
2. **A monotonic clock everywhere.** Wall-clock is subject to NTP adjustment
   mid-measurement, and sub-second intervals are exactly where that shows up.
3. **A verified process-group reap.** 30c signals the group; this harness first
   takes a running census of every pid that has ever belonged to the trial
   (group members plus descendants), then signals, then VERIFIES the census is
   empty and records the result — including the case where survivors remain.
   Mode B deliberately closes stdin while children run, which is precisely the
   shape that orphans grandchildren. Backlog 999.46 already tracks leaked
   fixture processes in this repository.
4. **An observation window with a FLOOR, not just a ceiling.** A window shorter
   than the slowest child's deadline manufactures "child work lost" out of an
   early stopwatch. See :data:`MODE_B_MIN_WINDOW_S`.

This module reports OBSERVATIONS ONLY and prints no verdict, for the reason
30a's README records: the harness-printed verdict was wrong in v1 and unreliable
in v2. The published capture is the evidence of record.

Safe to import — nothing runs at import time.
"""

import argparse
import importlib.util
import json
import os
import signal
import statistics
import sys
import time
from typing import Any, Optional

HARNESS_DIR = os.path.dirname(os.path.abspath(__file__))

#: The 30c harness. Imported, never reimplemented. A dashed filename cannot be
#: imported by name, hence the explicit spec-from-file-location load below.
HARNESS_30C_PATH = os.path.join(HARNESS_DIR, "30c-monitor-env-harness.py")

EVIDENCE_DIR = os.path.join(HARNESS_DIR, "30d-evidence")

#: Every name this harness calls on the 30c module. Checked at load time so a
#: rename in 30c surfaces as an explicit abort rather than an AttributeError
#: halfway through a measured trial.
REQUIRED_30C_ATTRS = (
    "launch_in_monitor_env",   # the production-replica launcher
    "make_stage_dir",          # staging outside .planning/
    "publish_jsonl",           # validate -> redact -> scan -> atomic replace
    "publish_text",            # same gate for non-JSON artifacts
    "scan_for_secrets",        # pattern-name-only scanner
    "RedactionContext",
    "StagedTail",
    "PROMPT_TEMPLATE",         # v3's two-child prompt, verbatim
    "discover_agent_session_markers",
    "parse_git_scrub_vars",       # the scrub list, parsed from live git.rs
    "RELIABILITY_DECOY_GIT_ENV",  # 30c's decoys, so the scrub does real work
    "resolve_cli_version",
    "EXPECTED_CLI_VERSION",
    "REPO_ROOT",
)

# ---------------------------------------------------------------------------
# Timing constants — every one of these is an assumption, so each is named
# ---------------------------------------------------------------------------

#: The two children's sleep durations, from the reused v3 prompt.
CHILD_A_DEADLINE_S = 10.0
CHILD_B_DEADLINE_S = 22.0

#: Added on top of the slowest child's deadline before a missing signal file may
#: be called a real absence. Sized well above the 11.51s longest quiet gap
#: 30c measured in a healthy run (binding constraint 8): a buffer at the same
#: order as the interval it must outlast is not a buffer.
DELIVERY_BUFFER_S = 30.0

#: The FLOOR for Mode B's observation window. Below this, a `child_work_lost`
#: finding is a stopwatch artifact rather than a CLI behaviour — and that
#: finding is the empirical input to Phase 31's constraint-4 close rule, so it
#: has to be real.
MODE_B_MIN_WINDOW_S = CHILD_B_DEADLINE_S + DELIVERY_BUFFER_S  # 52.0

#: Mode B's default total observation window, measured from launch. Child B is
#: dispatched around t+8s, so this leaves ~60s past its deadline.
MODE_B_WINDOW_S = 90.0

#: Mode A closes stdin only after the task set has drained AND the stream has
#: been quiet this long. 30c measured the longest quiet gap inside a HEALTHY run
#: at 10.52-11.51s and the drain-to-final-result lag at 4.54-11.51s, so a
#: shorter settle would close on a live run mid-turn and measure the exit
#: latency of a truncated session instead of a finished one.
MODE_A_QUIET_SETTLE_S = 15.0

#: Ceiling on Mode A's pre-close phase. A trial that hits it closes anyway and
#: records `drained_before_close: false`; such a trial is excluded from the
#: drained-close distribution with its reason stated.
MODE_A_PRE_CLOSE_CAP_S = 120.0

#: How long Mode A waits for exit after the close before giving up and killing.
MODE_A_EXIT_WINDOW_S = 30.0

#: Mode A keeps draining this long after the process exits, so a post-exit event
#: is captured rather than missed.
MODE_A_POST_EXIT_DRAIN_S = 5.0

#: Ceiling on Mode B's pre-close phase (waiting for the first result while at
#: least one local_agent task is still outstanding).
MODE_B_PRE_CLOSE_CAP_S = 60.0

#: Post-close exit poll interval. 30c polls at 0.25s, which quantises a
#: sub-second measurement into three distinct values.
EXIT_POLL_S = 0.002

#: How long the 2ms resolution is held after the close before dropping back to
#: :data:`STREAM_POLL_S`. Every 30c trial exited within 0.51s, so this covers
#: the measurement by ~20x while keeping a Mode B hang from spinning at 500Hz.
EXIT_HIRES_S = 10.0

#: Stream/signal poll interval during observation.
STREAM_POLL_S = 0.05

#: How often the running process census is refreshed. Cheap enough at 1Hz.
CENSUS_INTERVAL_S = 1.0

#: Reap escalation grace periods.
REAP_GRACE = ((signal.SIGTERM, 3.0), (signal.SIGKILL, 2.0))


class Abort(RuntimeError):
    """A guarantee this harness cannot honour. Never recovered from silently."""


# ---------------------------------------------------------------------------
# 30c module loading — abort, never fall back
# ---------------------------------------------------------------------------


def load_30c(path: str = HARNESS_30C_PATH) -> Any:
    """Import ``30c-monitor-env-harness.py`` by path, or abort.

    There is deliberately no fallback. Review finding M-30-04-1 is that this
    harness must measure inside 30c's production-replica environment; a
    direct-launch fallback would keep the harness running while quietly
    invalidating every number it produces.
    """
    if not os.path.exists(path):
        raise Abort(
            f"30c harness not found at {path!r}. This harness launches ONLY "
            "through 30c's production-replica launcher and has no direct-launch "
            "fallback by design — a fallback would publish evidence labelled a "
            "production replica that is not one."
        )
    if not os.access(path, os.R_OK):
        raise Abort(
            f"30c harness at {path!r} is not readable. Aborting rather than "
            "falling back to a direct launch."
        )
    spec = importlib.util.spec_from_file_location("harness_30c", path)
    if spec is None or spec.loader is None:
        raise Abort(f"cannot build an import spec for {path!r}.")
    module = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(module)
    except Exception as exc:  # noqa: BLE001 — any import failure is fatal here
        raise Abort(f"30c harness at {path!r} failed to import: {exc!r}") from exc

    missing = [name for name in REQUIRED_30C_ATTRS if not hasattr(module, name)]
    if missing:
        raise Abort(
            f"30c harness at {path!r} is missing {missing}. It was renamed or "
            "changed shape; this harness will not improvise a replacement."
        )
    return module


# ---------------------------------------------------------------------------
# Process census and verified group reaping (threat T-30-17)
# ---------------------------------------------------------------------------


def read_proc_stat(pid: int) -> Optional[dict[str, Any]]:
    """``comm``/``state``/``ppid``/``pgrp`` for ``pid``, or None if it is gone.

    ``comm`` can contain spaces and parentheses, so the fixed fields are parsed
    from after the LAST ``)`` rather than by splitting the whole line.
    """
    try:
        with open(f"/proc/{pid}/stat", encoding="utf-8", errors="replace") as handle:
            data = handle.read()
    except OSError:
        return None
    close = data.rfind(")")
    open_paren = data.find("(")
    if close == -1 or open_paren == -1 or close < open_paren:
        return None
    fields = data[close + 1 :].split()
    if len(fields) < 3:
        return None
    try:
        return {
            "pid": pid,
            "comm": data[open_paren + 1 : close],
            "state": fields[0],
            "ppid": int(fields[1]),
            "pgrp": int(fields[2]),
        }
    except ValueError:
        return None


def proc_table() -> dict[int, dict[str, Any]]:
    """Every live process, keyed by pid. Names only — never argv."""
    table: dict[int, dict[str, Any]] = {}
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        stat = read_proc_stat(int(entry))
        if stat is not None:
            table[stat["pid"]] = stat
    return table


def trial_pids(root_pid: int, pgid: int) -> dict[int, str]:
    """Every live pid belonging to this trial, as ``{pid: comm}``.

    Two independent memberships, unioned, because neither alone is sufficient:

    * **process group** catches grandchildren after their parent has exited and
      they have been reparented to init (ancestry is then untraceable), and
    * **descendant walk** catches a child that called ``setpgid`` and left the
      group.

    Zombies are excluded: a process in state ``Z`` has already exited and is
    only awaiting a reap by its parent. Counting it as a survivor would report
    a leak that does not exist.
    """
    table = proc_table()
    found: dict[int, str] = {}

    for stat in table.values():
        if stat["pgrp"] == pgid and stat["state"] != "Z":
            found[stat["pid"]] = stat["comm"]

    children: dict[int, list[int]] = {}
    for stat in table.values():
        children.setdefault(stat["ppid"], []).append(stat["pid"])
    queue = list(children.get(root_pid, []))
    seen = set()
    while queue:
        pid = queue.pop()
        if pid in seen:
            continue
        seen.add(pid)
        stat = table.get(pid)
        if stat is not None and stat["state"] != "Z":
            found[pid] = stat["comm"]
        queue.extend(children.get(pid, []))

    root = table.get(root_pid)
    if root is not None and root["state"] != "Z":
        found[root_pid] = root["comm"]
    return found


def survivors_of(census: dict[int, str], pgid: int, root_pid: int) -> dict[int, str]:
    """Which census members — plus any current group member — are still alive."""
    alive = dict(trial_pids(root_pid, pgid))
    for pid, comm in census.items():
        stat = read_proc_stat(pid)
        if stat is not None and stat["state"] != "Z" and stat["comm"] == comm:
            alive[pid] = comm
    return alive


def reap_process_group(
    proc: Any, pgid: int, census: dict[int, str]
) -> dict[str, Any]:
    """Terminate the trial's process group and VERIFY it is empty afterwards.

    Returns a record for the trial log. "We called kill on ``proc``" is not
    evidence that the group is gone, so this records the pre-kill census, the
    escalation steps actually taken, and the post-kill survivor check —
    including the case where survivors remain, which is a finding rather than
    something to hide.
    """
    record: dict[str, Any] = {
        "pgid": pgid,
        "census_pids_seen": len(census),
        "census_comms": sorted(set(census.values())),
        "steps": [],
        "pre_kill_alive": [],
        "survivors_after_reap": [],
        "survivor_check_completed": False,
    }

    own_pgid = os.getpgid(0)
    if pgid in (0, 1, own_pgid):
        record["steps"].append(
            f"REFUSED: pgid {pgid} is 0, 1 or this harness's own group ({own_pgid})"
        )
        record["survivor_check_completed"] = True
        return record

    alive = survivors_of(census, pgid, proc.pid)
    record["pre_kill_alive"] = [{"pid": p, "comm": c} for p, c in sorted(alive.items())]

    for signum, grace in REAP_GRACE:
        alive = survivors_of(census, pgid, proc.pid)
        if not alive:
            record["steps"].append(f"{signum.name}: skipped, group already empty")
            break
        try:
            os.killpg(pgid, signum)
            record["steps"].append(f"killpg({pgid}, {signum.name})")
        except ProcessLookupError:
            record["steps"].append(f"killpg({pgid}, {signum.name}) -> no such group")
        except PermissionError:
            record["steps"].append(f"killpg({pgid}, {signum.name}) -> permission denied")
        # A member that drifted out of the group is signalled individually.
        for pid in sorted(alive):
            stat = read_proc_stat(pid)
            if stat is not None and stat["pgrp"] != pgid:
                try:
                    os.kill(pid, signum)
                    record["steps"].append(f"kill({pid}, {signum.name}) [out of group]")
                except OSError:
                    pass
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            if not survivors_of(census, pgid, proc.pid):
                break
            time.sleep(0.05)

    try:
        proc.wait(timeout=2.0)
    except Exception:  # noqa: BLE001 — a wait failure must not mask the check
        record["steps"].append("proc.wait() did not complete within 2s")

    remaining = survivors_of(census, pgid, proc.pid)
    record["survivors_after_reap"] = [
        {"pid": p, "comm": c} for p, c in sorted(remaining.items())
    ]
    record["survivor_check_completed"] = True
    record["clean"] = not remaining
    return record


# ---------------------------------------------------------------------------
# Observation — the experiment core, inherited from run_experiment_v3.py
# ---------------------------------------------------------------------------


class Watcher:
    """Reads the staged capture and records facts. No interpretation.

    ``local_bash``-typed task events are RECORDED, never filtered out, while the
    drain gate considers only ``local_agent`` entries. RESEARCH.md's Pitfall 6
    records an extra ``local_bash`` ``task_started``/``task_notification`` pair
    per child that never appears in ``background_tasks_changed`` at all, and
    assumption A2 is that it is informational. Recording it gives that
    assumption evidence instead of another restatement.
    """

    def __init__(self, module: Any, stdout_path: str, signal_paths: dict[str, str], t0: float) -> None:
        self._tail = module.StagedTail(stdout_path)
        self._signal_paths = signal_paths
        self._t0 = t0
        self.events: list[dict[str, Any]] = []
        self.results: list[dict[str, Any]] = []
        self.notifications: list[dict[str, Any]] = []
        self.btc: list[dict[str, Any]] = []
        self.local_bash_events: list[dict[str, Any]] = []
        self.outstanding: list[str] = []
        self.untyped_task_entries = 0
        self.drained_at: Optional[float] = None
        self.first_dispatch_at: Optional[float] = None
        self.last_dispatch_at: Optional[float] = None
        self.signals: dict[str, Optional[float]] = {label: None for label in signal_paths}
        self.last_event_at: Optional[float] = None
        self.unparseable_lines = 0
        self.assistant_events = 0

    # -- timing ---------------------------------------------------------
    def now(self) -> float:
        return time.monotonic() - self._t0

    # -- polling --------------------------------------------------------
    def poll(self) -> int:
        """Read whatever is available. Returns the number of new events."""
        lines = self._tail.poll_lines()
        for line in lines:
            self._record(line, self.now())
        self._poll_signals()
        return len(lines)

    def close(self) -> None:
        self._tail.close()

    def _poll_signals(self) -> None:
        stamp = self.now()
        for label, path in self._signal_paths.items():
            if self.signals[label] is None and os.path.exists(path):
                self.signals[label] = round(stamp, 3)
                print(f"[30d] t+{stamp:6.2f} signal {label} written", flush=True)

    def _record(self, raw_line: str, at: float) -> None:
        try:
            event = json.loads(raw_line)
        except json.JSONDecodeError:
            self.unparseable_lines += 1
            return
        if not isinstance(event, dict):
            self.unparseable_lines += 1
            return

        kind = event.get("type")
        subtype = event.get("subtype")
        self.last_event_at = at
        compact = {"at": round(at, 3), "type": kind, "subtype": subtype}

        if kind == "assistant":
            self.assistant_events += 1
        elif kind == "result":
            entry = {
                "at": round(at, 3),
                "origin_kind": (event.get("origin") or {}).get("kind"),
                "num_turns": event.get("num_turns"),
                "is_error": event.get("is_error"),
                "stop_reason": event.get("stop_reason"),
                "parent_tool_use_id": event.get("parent_tool_use_id"),
                "result_chars": len(event.get("result") or ""),
            }
            self.results.append(entry)
            compact["result_index"] = len(self.results)
            print(
                f"[30d] t+{at:6.2f} result #{len(self.results)} "
                f"origin={entry['origin_kind']!r} turns={entry['num_turns']}",
                flush=True,
            )
        elif subtype == "task_started":
            compact["task_id"] = event.get("task_id")
            compact["task_type"] = event.get("task_type")
            if event.get("task_type") == "local_bash":
                self.local_bash_events.append(
                    {
                        "at": round(at, 3),
                        "subtype": "task_started",
                        "task_id": event.get("task_id"),
                        "description": event.get("description"),
                    }
                )
        elif subtype == "task_notification":
            self.notifications.append(
                {
                    "at": round(at, 3),
                    "task_id": event.get("task_id"),
                    "status": event.get("status"),
                    "task_type": event.get("task_type"),
                }
            )
            compact["task_id"] = event.get("task_id")
            if event.get("task_type") == "local_bash":
                self.local_bash_events.append(
                    {
                        "at": round(at, 3),
                        "subtype": "task_notification",
                        "task_id": event.get("task_id"),
                        "status": event.get("status"),
                    }
                )
        elif subtype == "background_tasks_changed":
            tasks = event.get("tasks") or []
            agent_ids, untyped = [], 0
            for task in tasks:
                if not isinstance(task, dict):
                    continue
                task_type = task.get("task_type")
                if task_type == "local_agent":
                    agent_ids.append(task.get("task_id"))
                elif task_type is None:
                    # Conservative: an entry we cannot type is treated as still
                    # outstanding, so a missing field can never fake a drain.
                    untyped += 1
                    agent_ids.append(task.get("task_id"))
            self.untyped_task_entries += untyped
            self.outstanding = [tid for tid in agent_ids if tid]
            self.btc.append(
                {
                    "at": round(at, 3),
                    "total_entries": len(tasks),
                    "local_agent_count": len(agent_ids),
                    "task_ids": list(self.outstanding),
                }
            )
            compact["local_agent_count"] = len(agent_ids)
            if agent_ids:
                if self.first_dispatch_at is None:
                    self.first_dispatch_at = at
                self.last_dispatch_at = at
            elif self.drained_at is None and self.first_dispatch_at is not None:
                self.drained_at = at
                print(f"[30d] t+{at:6.2f} task set DRAINED to []", flush=True)

        self.events.append(compact)

    # -- derived facts ---------------------------------------------------
    @staticmethod
    def _is_milestone(event: dict[str, Any]) -> bool:
        return event["type"] == "result" or event["subtype"] in (
            "task_notification",
            "background_tasks_changed",
        )

    def longest_quiet_gap(self, milestones_only: bool = False) -> Optional[float]:
        """Longest interval between consecutive stream events, in seconds.

        TWO definitions are recorded per trial, because they are not the same
        number and constraint 8 depends on which one is meant:

        * ``milestones_only=True`` counts only ``result`` /
          ``task_notification`` / ``background_tasks_changed`` events. This is
          30c's definition — its published ``run.log`` carries no per-line
          timestamps, so a milestone set was all it could measure. Verified to
          reproduce 30c's 10.52-11.51s band exactly across its seven trials, so
          this field is directly comparable to binding constraint 8.
        * ``milestones_only=False`` counts EVERY parsed line. That is what a
          monitor's idle timer actually sees on the wire, and it is necessarily
          smaller. Timestamps are read-times at this harness's poll resolution,
          not the CLI's own write times.
        """
        stamps = [
            event["at"]
            for event in self.events
            if not milestones_only or self._is_milestone(event)
        ]
        if len(stamps) < 2:
            return None
        return round(max(b - a for a, b in zip(stamps, stamps[1:])), 3)

    def results_after(self, at: float) -> list[dict[str, Any]]:
        return [r for r in self.results if r["at"] > at]

    def events_after(self, at: float) -> list[dict[str, Any]]:
        return [e for e in self.events if e["at"] > at]


def observe_until(watcher: Watcher, predicate, cap_s: float, label: str) -> bool:
    """Poll until ``predicate()`` is true or ``cap_s`` elapses. Never blocks."""
    while watcher.now() < cap_s:
        watcher.poll()
        if predicate():
            return True
        time.sleep(STREAM_POLL_S)
    watcher.poll()
    print(f"[30d] t+{watcher.now():6.2f} {label} cap of {cap_s}s reached", flush=True)
    return predicate()


# ---------------------------------------------------------------------------
# Signal-file classification (review finding M-30-04-3)
# ---------------------------------------------------------------------------


def classify_signal(
    observed_at: Optional[float],
    path: str,
    window_s: float,
    dispatch_at: Optional[float],
    deadline_s: float,
) -> str:
    """``present`` / ``absent`` / ``absent_at_window_close``.

    ``absent`` is the STRONG claim and is licensed only when observation ran
    past the child's own deadline plus :data:`DELIVERY_BUFFER_S`, measured from
    the child's dispatch rather than from launch. Otherwise the weaker
    ``absent_at_window_close`` is recorded, so a missing file is never conflated
    with a file that simply had not been written yet when the clock stopped.
    """
    if observed_at is not None or os.path.exists(path):
        return "present"
    required = deadline_s + DELIVERY_BUFFER_S
    observed_past_dispatch = window_s - (dispatch_at or 0.0)
    if observed_past_dispatch >= required:
        return "absent"
    return "absent_at_window_close"


# ---------------------------------------------------------------------------
# Publication — 30c's pipeline, not a second one
# ---------------------------------------------------------------------------


def publish_trial(
    module: Any,
    run: Any,
    record: dict[str, Any],
    log_lines: list[str],
    dest_dir: str,
) -> list[dict[str, Any]]:
    """Publish raw capture, stderr, run log and timings through 30c's gate.

    Order matters: :func:`publish_jsonl` harvests the capture's session ids into
    the shared :class:`RedactionContext`, so it must run before the text
    artifacts that are scrubbed with that same context.
    """
    ctx = module.RedactionContext.build()
    published = [
        module.publish_jsonl(run.stdout_path, os.path.join(dest_dir, "raw_output.jsonl"), ctx),
        module.publish_text(run.stderr_path, os.path.join(dest_dir, "stderr.log"), ctx),
    ]
    published.append(
        module.publish_text(
            "\n".join(log_lines) + "\n", os.path.join(dest_dir, "run.log"), ctx, is_text=True
        )
    )
    published.append(
        module.publish_text(
            json.dumps(record, indent=2, sort_keys=True) + "\n",
            os.path.join(dest_dir, "timings.json"),
            ctx,
            is_text=True,
        )
    )
    return published


def run_log_lines(run: Any, record: dict[str, Any], version: str) -> list[str]:
    """A human-readable mirror of the machine-readable timings file."""
    residue = run.residue
    lines = [
        f"# 30d run log — {record['mode']} trial {record['trial']}",
        f"claude_cli_version: {version}",
        f"harness: 30d-exit-timing-harness.py (launcher imported from {os.path.basename(HARNESS_30C_PATH)})",
        f"launcher_source: {record['launcher']['module_path']}",
        f"launcher_function: {record['launcher']['function']}",
        f"invocation: {record['invocation']}",
        f"staged_output_dir: {run.stage_dir}",
        f"clock: {record['clock']}",
        "",
        "## Replicated from spawn_monitor (via 30c's launcher)",
        "launch_via_sh_c: true",
        "detached_start_new_session: true",
        "stderr_separate_file: true",
        f"stdout_file: {os.path.basename(run.stdout_path)}",
        f"stderr_file: {os.path.basename(run.stderr_path)}",
        f"stdout_and_stderr_are_distinct_paths: {run.stdout_path != run.stderr_path}",
        "harness_holds_child_stdin_open: true (the deliberate deviation, closed on purpose below)",
        "",
        "## Environment scrub (names only)",
        "scrub_list_source: crates/devflow-core/src/git.rs, parsed at runtime by 30c's parse_git_scrub_vars",
        f"git_scrub_list_parsed: {', '.join(record['launcher']['git_scrub_list_parsed'])}",
        f"removed_variables: {', '.join(run.scrubbed_vars) or '(none were set)'}",
        f"removed_agent_session_markers: {', '.join(run.scrubbed_markers) or '(none — production replica arm)'}",
        f"parent_process: {residue['parent_process']}",
        f"launched_from_agent_session: {residue['launched_from_agent_session']}",
        f"claude_session_markers: {', '.join(residue['claude_session_markers']) or '(none)'}",
        f"credential_named_var_count: {residue['credential_named_var_count']} (names withheld per T-30-07)",
        f"total_env_vars_in_child: {residue['total_env_vars']}",
        "",
        "## Observation window",
        f"observation_window_s: {record['observation_window_s']}",
        f"observation_window_floor_s: {record['observation_window_floor_s']}",
        f"window_after_last_dispatch_s: {record['window_after_last_dispatch_s']}",
        f"slowest_child_deadline_s: {CHILD_B_DEADLINE_S}",
        f"delivery_buffer_s: {DELIVERY_BUFFER_S}",
        "",
        "## Close and exit",
        f"stdin_closed_at_s: {record['stdin_closed_at_s']}",
        f"close_precondition_met: {record['close_precondition_met']}",
        f"outstanding_local_agent_tasks_at_close: {record['outstanding_local_agent_tasks_at_close']}",
        f"process_exited: {record['process_exited']}",
        f"exit_latency_ms: {record['exit_latency_ms']} (the CLI process itself)",
        f"wrapper_exit_latency_ms: {record['wrapper_exit_latency_ms']} (the sh wrapper that waits on it)",
        f"exit_code: {record['exit_code']}",
        "",
        "## Stream facts",
        f"result_events: {len(record['results'])}",
        f"results_after_close: {record['results_after_close']}",
        f"final_result_truncated: {record['final_result_truncated']}",
        f"final_result_truncation_basis: {record['final_result_truncation_basis']}",
        f"drained_event_observed: {record['drained_event_observed']}",
        f"drained_at_s: {record['drained_at_s']}",
        f"longest_quiet_gap_milestone_s: {record['longest_quiet_gap_milestone_s']} (30c's definition — comparable to constraint 8)",
        f"longest_quiet_gap_all_events_s: {record['longest_quiet_gap_all_events_s']} (every parsed line — what an idle timer sees)",
        f"unparseable_lines: {record['unparseable_lines']}",
        f"untyped_task_entries: {record['untyped_task_entries']}",
        f"local_bash_events_recorded: {len(record['local_bash_events'])}",
        f"stderr_nonempty: {record['stderr_nonempty']}",
        "",
        "## Children",
        f"child_a_signal_file: {record['child_a_signal_file']}",
        f"child_b_signal_file: {record['child_b_signal_file']}",
        f"child_a_signal_at_s: {record['child_a_signal_at_s']}",
        f"child_b_signal_at_s: {record['child_b_signal_at_s']}",
        f"child_a_signal_contents: {record['child_a_signal_contents']!r}",
        f"child_b_signal_contents: {record['child_b_signal_contents']!r}",
        "",
        "## Cleanup (T-30-17)",
        f"cleanup_action: {record['cleanup_action']}",
        f"cleanup_pgid: {record['cleanup']['pgid']}",
        f"cleanup_census_pids_seen: {record['cleanup']['census_pids_seen']}",
        f"cleanup_census_comms: {', '.join(record['cleanup']['census_comms']) or '(none)'}",
        f"cleanup_steps: {'; '.join(record['cleanup']['steps']) or '(none needed)'}",
        f"cleanup_survivors_after_reap: {record['cleanup']['survivors_after_reap']}",
        f"cleanup_survivor_check_completed: {record['cleanup']['survivor_check_completed']}",
        "",
        "## Result events (origin verbatim)",
    ]
    for index, result in enumerate(record["results"], 1):
        lines.append(
            f"result[{index}]: at=t+{result['at']:.3f} origin={result['origin_kind']!r} "
            f"turns={result['num_turns']} is_error={result['is_error']} "
            f"stop_reason={result['stop_reason']!r} chars={result['result_chars']}"
        )
    lines += ["", "## background_tasks_changed (local_agent entries only)"]
    for change in record["background_tasks_changed"]:
        lines.append(
            f"btc: at=t+{change['at']:.3f} local_agent={change['local_agent_count']} "
            f"ids={change['task_ids']}"
        )
    lines += ["", "## local_bash events (recorded, never used for the drain gate — Pitfall 6)"]
    for event in record["local_bash_events"]:
        lines.append(f"local_bash: at=t+{event['at']:.3f} {event}")
    lines += [
        "",
        "# No verdict is printed here by design: 30a records that the",
        "# harness-printed verdict was wrong in v1 and unreliable in v2.",
    ]
    return lines


# ---------------------------------------------------------------------------
# The trial
# ---------------------------------------------------------------------------


def run_trial(
    module: Any,
    mode: str,
    trial_index: int,
    version: str,
    evidence_root: str,
    window_s: float,
    scrub_markers: bool,
    invocation: str,
) -> dict[str, Any]:
    """Launch, observe, close stdin, measure the exit, and reap the group.

    The reap runs in a ``finally`` covering all three exit paths — normal
    completion, cap/timeout and exception (including KeyboardInterrupt) — and
    only AFTER the observation window has closed. Reaping earlier would kill the
    children this experiment exists to observe and manufacture the very
    "child work lost" finding it is measuring.
    """
    dest_dir = os.path.join(evidence_root, mode, f"trial-{trial_index}")
    stage_dir = module.make_stage_dir()
    signal_paths = {
        "A": os.path.join(stage_dir, "signalA_30d.txt"),
        "B": os.path.join(stage_dir, "signalB_30d.txt"),
    }
    prompt = module.PROMPT_TEMPLATE.format(signal_a=signal_paths["A"], signal_b=signal_paths["B"])
    markers = module.discover_agent_session_markers() if scrub_markers else []

    record: dict[str, Any] = {
        "mode": mode,
        "trial": trial_index,
        "claude_code_version": version,
        "invocation": invocation,
        "clock": "time.monotonic (not wall-clock: NTP adjustment shows up exactly at sub-second scale)",
        "launcher": {
            "module_path": os.path.basename(HARNESS_30C_PATH),
            "function": "launch_in_monitor_env",
            "reimplemented": False,
            "git_scrub_list_parsed": module.parse_git_scrub_vars(),
        },
        "agent_session_markers_scrubbed": bool(markers),
        "observation_window_s": None,
        "observation_window_floor_s": MODE_B_MIN_WINDOW_S if mode == "mode-b" else None,
        "window_after_last_dispatch_s": None,
        "slowest_child_deadline_s": CHILD_B_DEADLINE_S,
        "delivery_buffer_s": DELIVERY_BUFFER_S,
        "stdin_closed_at_s": None,
        "close_precondition_met": False,
        "outstanding_local_agent_tasks_at_close": [],
        "process_exited": False,
        "exit_latency_ms": None,
        "wrapper_exit_latency_ms": None,
        "exit_code": None,
        "results": [],
        "results_after_close": 0,
        "final_result_truncated": None,
        "final_result_truncation_basis": "not evaluated",
        "drained_event_observed": False,
        "drained_at_s": None,
        "drained_before_close": False,
        "longest_quiet_gap_all_events_s": None,
        "longest_quiet_gap_milestone_s": None,
        "unparseable_lines": 0,
        "untyped_task_entries": 0,
        "local_bash_events": [],
        "background_tasks_changed": [],
        "notifications": [],
        "event_count": 0,
        "stderr_nonempty": None,
        "child_a_signal_file": "absent_at_window_close",
        "child_b_signal_file": "absent_at_window_close",
        "child_a_signal_at_s": None,
        "child_b_signal_at_s": None,
        "child_a_signal_contents": None,
        "child_b_signal_contents": None,
        "cleanup_action": "not reached",
        "cleanup": {"pgid": None, "census_pids_seen": 0, "census_comms": [], "steps": [],
                     "survivors_after_reap": [], "survivor_check_completed": False},
        "aborted_with": None,
    }

    run = None
    pgid = None
    census: dict[int, str] = {}
    watcher = None
    print(f"\n{'=' * 72}\n[30d] {mode} trial {trial_index} — staging in {stage_dir}\n{'=' * 72}", flush=True)

    try:
        t0 = time.monotonic()
        run = module.launch_in_monitor_env(
            module.REPO_ROOT, prompt, stage_dir=stage_dir, extra_scrub_names=markers
        )
        try:
            pgid = os.getpgid(run.proc.pid)
        except ProcessLookupError:
            pgid = run.proc.pid
        record["cleanup"]["pgid"] = pgid
        print(
            f"[30d] launched via 30c's launch_in_monitor_env; sh pid={run.proc.pid} pgid={pgid}; "
            f"scrubbed {len(run.scrubbed_vars)} git vars + {len(run.scrubbed_markers)} markers",
            flush=True,
        )

        watcher = Watcher(module, run.stdout_path, signal_paths, t0)
        next_census = 0.0

        def tick() -> None:
            nonlocal next_census
            watcher.poll()
            if watcher.now() >= next_census:
                next_census = watcher.now() + CENSUS_INTERVAL_S
                census.update(trial_pids(run.proc.pid, pgid))

        # -- phase 1: wait for this mode's close precondition ------------
        if mode == "mode-a":
            def ready() -> bool:
                tick()
                if watcher.drained_at is None or not watcher.results:
                    return False
                quiet = watcher.now() - (watcher.last_event_at or 0.0)
                return quiet >= MODE_A_QUIET_SETTLE_S

            met = observe_until(watcher, ready, MODE_A_PRE_CLOSE_CAP_S, "mode-a pre-close")
            record["close_precondition_met"] = bool(met and watcher.drained_at is not None)
        else:
            def ready() -> bool:
                tick()
                return bool(watcher.results) and bool(watcher.outstanding)

            met = observe_until(watcher, ready, MODE_B_PRE_CLOSE_CAP_S, "mode-b pre-close")
            record["close_precondition_met"] = bool(met)

        record["drained_before_close"] = watcher.drained_at is not None
        record["outstanding_local_agent_tasks_at_close"] = list(watcher.outstanding)

        # -- phase 2: close stdin and time the exit ----------------------
        close_at = watcher.now()
        print(
            f"[30d] t+{close_at:6.2f} closing stdin "
            f"(outstanding local_agent tasks: {len(watcher.outstanding)})",
            flush=True,
        )
        try:
            run.proc.stdin.close()
        except OSError:
            pass
        closed_mono = time.monotonic()
        record["stdin_closed_at_s"] = round(close_at, 3)

        cli_pid = _read_agent_pid(run.pid_path)
        record["cli_pid_recorded"] = cli_pid is not None

        exit_deadline = closed_mono + (
            MODE_A_EXIT_WINDOW_S if mode == "mode-a" else max(window_s - close_at, 1.0)
        )
        cli_exit_at = None
        wrapper_exit_at = None
        while time.monotonic() < exit_deadline:
            tick()
            if cli_exit_at is None and cli_pid is not None and not _alive(cli_pid):
                cli_exit_at = time.monotonic()
            if wrapper_exit_at is None and run.proc.poll() is not None:
                wrapper_exit_at = time.monotonic()
            if (cli_exit_at is not None or cli_pid is None) and wrapper_exit_at is not None:
                break
            # 2ms resolution only while the sub-second measurement is in play.
            # A hang is a legitimate Mode B outcome, and spinning at 500Hz for
            # eighty seconds would add load that perturbs what is being measured.
            since_close = time.monotonic() - closed_mono
            time.sleep(EXIT_POLL_S if since_close < EXIT_HIRES_S else STREAM_POLL_S)

        if cli_exit_at is not None:
            record["exit_latency_ms"] = round((cli_exit_at - closed_mono) * 1000.0, 1)
        if wrapper_exit_at is not None:
            record["wrapper_exit_latency_ms"] = round((wrapper_exit_at - closed_mono) * 1000.0, 1)
        record["process_exited"] = wrapper_exit_at is not None
        record["exit_code"] = run.proc.returncode
        print(
            f"[30d] t+{watcher.now():6.2f} exit: cli={record['exit_latency_ms']}ms "
            f"wrapper={record['wrapper_exit_latency_ms']}ms code={record['exit_code']}",
            flush=True,
        )

        # -- phase 3: keep observing for the rest of the window ----------
        if mode == "mode-a":
            tail_until = watcher.now() + MODE_A_POST_EXIT_DRAIN_S
        else:
            tail_until = window_s
        print(f"[30d] t+{watcher.now():6.2f} observing until t+{tail_until:.1f}", flush=True)
        while watcher.now() < tail_until:
            tick()
            time.sleep(STREAM_POLL_S)
        watcher.poll()

        record["observation_window_s"] = round(watcher.now(), 3)

        # -- derived facts ----------------------------------------------
        _finalise(record, watcher, run, signal_paths, close_at)

    except BaseException as exc:  # noqa: BLE001 — recorded, re-raised below
        record["aborted_with"] = f"{type(exc).__name__}: {exc}"
        print(f"[30d] trial aborted: {record['aborted_with']}", flush=True)
        raise
    finally:
        if run is not None and pgid is not None:
            census.update(trial_pids(run.proc.pid, pgid))
            cleanup = reap_process_group(run.proc, pgid, census)
            record["cleanup"] = cleanup
            survivors = cleanup["survivors_after_reap"]
            record["cleanup_action"] = (
                f"process group {pgid} signalled ({'; '.join(cleanup['steps']) or 'already empty'}); "
                f"survivor check completed: {cleanup['survivor_check_completed']}; "
                + (f"SURVIVORS REMAIN: {survivors}" if survivors else "no survivors")
            )
            print(f"[30d] cleanup: {record['cleanup_action']}", flush=True)
        if watcher is not None:
            watcher.close()
        if run is not None:
            try:
                published = publish_trial(module, run, record, run_log_lines(run, record, version), dest_dir)
                print(f"[30d] published to {dest_dir}:", flush=True)
                for entry in published:
                    print(f"       {entry}", flush=True)
            except Exception as exc:  # noqa: BLE001 — must not mask the trial's own failure
                print(f"[30d] PUBLISH FAILED for {dest_dir}: {exc!r}", flush=True)
        print(f"[30d] staged raw output retained at {stage_dir} (outside .planning/)", flush=True)

    return record


def _finalise(
    record: dict[str, Any],
    watcher: Watcher,
    run: Any,
    signal_paths: dict[str, str],
    close_at: float,
) -> None:
    """Fill the derived observation fields. Independent facts stay independent."""
    window = record["observation_window_s"] or watcher.now()

    record["results"] = watcher.results
    record["background_tasks_changed"] = watcher.btc
    record["notifications"] = watcher.notifications
    record["local_bash_events"] = watcher.local_bash_events
    record["event_count"] = len(watcher.events)
    record["unparseable_lines"] = watcher.unparseable_lines
    record["untyped_task_entries"] = watcher.untyped_task_entries
    record["drained_event_observed"] = watcher.drained_at is not None
    record["drained_at_s"] = watcher.drained_at
    record["longest_quiet_gap_all_events_s"] = watcher.longest_quiet_gap()
    record["longest_quiet_gap_milestone_s"] = watcher.longest_quiet_gap(milestones_only=True)
    record["first_dispatch_at_s"] = watcher.first_dispatch_at
    record["last_dispatch_at_s"] = watcher.last_dispatch_at
    record["window_after_last_dispatch_s"] = (
        round(window - watcher.last_dispatch_at, 3) if watcher.last_dispatch_at is not None else None
    )

    after = watcher.results_after(close_at)
    record["results_after_close"] = len(after)

    # Truncation basis, stated rather than asserted: assistant/stream events
    # continuing past the last result means a turn was in flight when the
    # capture stopped.
    last_result_at = watcher.results[-1]["at"] if watcher.results else None
    trailing = (
        [e for e in watcher.events_after(last_result_at) if e["type"] == "assistant"]
        if last_result_at is not None
        else []
    )
    last_is_error = bool(watcher.results[-1]["is_error"]) if watcher.results else None
    if not watcher.results:
        record["final_result_truncated"] = True
        record["final_result_truncation_basis"] = "no result event was observed at all"
    elif trailing:
        record["final_result_truncated"] = True
        record["final_result_truncation_basis"] = (
            f"{len(trailing)} assistant event(s) follow the last result event — a turn was in flight"
        )
    elif last_is_error:
        record["final_result_truncated"] = True
        record["final_result_truncation_basis"] = "the final result event carries is_error: true"
    else:
        record["final_result_truncated"] = False
        record["final_result_truncation_basis"] = (
            f"the capture ends on a complete result event (is_error false, "
            f"{watcher.results[-1]['result_chars']} chars) with no trailing assistant events"
        )

    try:
        record["stderr_nonempty"] = os.path.getsize(run.stderr_path) > 0
    except OSError:
        record["stderr_nonempty"] = None

    dispatch = {"A": watcher.first_dispatch_at, "B": watcher.last_dispatch_at}
    deadlines = {"A": CHILD_A_DEADLINE_S, "B": CHILD_B_DEADLINE_S}
    for label in ("A", "B"):
        key = f"child_{label.lower()}_signal_file"
        record[key] = classify_signal(
            watcher.signals[label], signal_paths[label], window, dispatch[label], deadlines[label]
        )
        record[f"child_{label.lower()}_signal_at_s"] = watcher.signals[label]
        contents = None
        if os.path.exists(signal_paths[label]):
            try:
                with open(signal_paths[label], encoding="utf-8", errors="replace") as handle:
                    contents = handle.read().strip()
            except OSError:
                contents = None
        record[f"child_{label.lower()}_signal_contents"] = contents


def _read_agent_pid(pid_path: str) -> Optional[int]:
    """The CLI's own pid, written by the sh wrapper's ``echo $! > pidfile``."""
    for _ in range(40):  # the wrapper writes it within milliseconds of launch
        try:
            with open(pid_path, encoding="utf-8") as handle:
                text = handle.read().strip()
            if text.isdigit():
                return int(text)
        except OSError:
            pass
        time.sleep(0.05)
    return None


def _alive(pid: int) -> bool:
    """True while ``pid`` exists and is not a zombie.

    A zombie has already exited; only its exit status is still pending a reap by
    the ``sh`` wrapper. Treating ``Z`` as alive would add the wrapper's reap
    latency to the CLI's measured exit latency.
    """
    stat = read_proc_stat(pid)
    return stat is not None and stat["state"] != "Z"


# ---------------------------------------------------------------------------
# Aggregation
# ---------------------------------------------------------------------------


def recompute(evidence_root: str = EVIDENCE_DIR, mode: str = "mode-a") -> dict[str, Any]:
    """Recompute the reported aggregates from the PUBLISHED per-trial timings.

    The 0.38s figure this plan replaces is unreproducible precisely because it
    was never archived (T-30-18). This entry point exists so the numbers in
    ``30d-MEASUREMENTS.md`` can be re-derived by anyone holding the evidence
    directory, without rerunning a single CLI trial.
    """
    mode_dir = os.path.join(evidence_root, mode)
    trials = []
    if os.path.isdir(mode_dir):
        for name in sorted(os.listdir(mode_dir)):
            path = os.path.join(mode_dir, name, "timings.json")
            if os.path.exists(path):
                with open(path, encoding="utf-8") as handle:
                    trials.append(json.load(handle))
    latencies = [
        t["exit_latency_ms"]
        for t in trials
        if t.get("exit_latency_ms") is not None and t.get("drained_before_close", True)
    ]
    summary: dict[str, Any] = {
        "mode": mode,
        "trials_found": len(trials),
        "trials_counted": len(latencies),
        "per_trial": [
            {
                "trial": t["trial"],
                "exit_latency_ms": t.get("exit_latency_ms"),
                "wrapper_exit_latency_ms": t.get("wrapper_exit_latency_ms"),
                "exit_code": t.get("exit_code"),
                "drained_before_close": t.get("drained_before_close"),
                "observation_window_s": t.get("observation_window_s"),
                "longest_quiet_gap_milestone_s": t.get("longest_quiet_gap_milestone_s"),
                "longest_quiet_gap_all_events_s": t.get("longest_quiet_gap_all_events_s"),
            }
            for t in trials
        ],
    }
    if latencies:
        summary["exit_latency_ms_min"] = min(latencies)
        summary["exit_latency_ms_median"] = statistics.median(latencies)
        summary["exit_latency_ms_max"] = max(latencies)
    for field_name in ("longest_quiet_gap_milestone_s", "longest_quiet_gap_all_events_s"):
        gaps = [t[field_name] for t in trials if t.get(field_name) is not None]
        if gaps:
            summary[f"{field_name}_min"] = min(gaps)
            summary[f"{field_name}_max"] = max(gaps)
    return summary


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="30d-exit-timing-harness.py",
        description="Exit-timing (mode-a) and close-with-pending-tasks (mode-b) measurement.",
    )
    parser.add_argument("mode", choices=("mode-a", "mode-b", "recompute"))
    parser.add_argument("--iterations", type=int, default=5, help="trials to run (default: 5)")
    parser.add_argument("--start-index", type=int, default=1, help="first trial number (default: 1)")
    parser.add_argument(
        "--window",
        type=float,
        default=MODE_B_WINDOW_S,
        help=f"mode-b observation window in seconds (default: {MODE_B_WINDOW_S}, floor {MODE_B_MIN_WINDOW_S})",
    )
    parser.add_argument("--evidence-dir", default=EVIDENCE_DIR)
    parser.add_argument(
        "--scrub-agent-markers",
        action="store_true",
        help="also remove CLAUDE*/ANTHROPIC*/AI_AGENT* from the child environment. "
             "OFF by default: 30-02's finding F-2 showed that scrub diverges from "
             "production, which carries ANTHROPIC_API_KEY.",
    )
    parser.add_argument(
        "--no-git-decoys",
        action="store_true",
        help="do not plant 30c's GIT_DIR/GIT_WORK_TREE decoys. They are planted by "
             "default so production's git scrub does real work DURING each measured "
             "run rather than removing nothing, exactly as 30c's replication set did.",
    )
    parser.add_argument("--harness-30c", default=HARNESS_30C_PATH)
    return parser


def main(argv: Optional[list[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    invocation = f"30d-exit-timing-harness.py {' '.join(argv if argv is not None else sys.argv[1:])}"

    if args.mode == "recompute":
        print(json.dumps(recompute(args.evidence_dir, "mode-a"), indent=2))
        print(json.dumps(recompute(args.evidence_dir, "mode-b"), indent=2))
        return 0

    module = load_30c(args.harness_30c)

    version = module.resolve_cli_version()
    if version != module.EXPECTED_CLI_VERSION:
        raise Abort(
            f"claude CLI is {version!r}, expected {module.EXPECTED_CLI_VERSION!r} — the version "
            "30c recorded. Measuring across a version boundary makes 30c's and 30d's evidence "
            "non-comparable."
        )

    if args.mode == "mode-b" and args.window < MODE_B_MIN_WINDOW_S:
        raise Abort(
            f"--window {args.window}s is below the floor of {MODE_B_MIN_WINDOW_S}s "
            f"({CHILD_B_DEADLINE_S}s slowest-child deadline + {DELIVERY_BUFFER_S}s buffer). "
            "A shorter window manufactures 'child work lost' out of an early stopwatch."
        )

    if not args.no_git_decoys:
        os.environ.update(module.RELIABILITY_DECOY_GIT_ENV)
        print(
            f"[30d] planted 30c's git decoys {sorted(module.RELIABILITY_DECOY_GIT_ENV)} — "
            "both point at paths that do not exist, so a scrub failure surfaces as a "
            "loud git error inside the child rather than a silent redirect",
            flush=True,
        )

    print(f"[30d] cli={version} mode={args.mode} iterations={args.iterations}", flush=True)
    print(f"[30d] launcher imported from {args.harness_30c}", flush=True)

    records = []
    for offset in range(args.iterations):
        index = args.start_index + offset
        records.append(
            run_trial(
                module,
                args.mode,
                index,
                version,
                args.evidence_dir,
                args.window,
                args.scrub_agent_markers,
                invocation,
            )
        )

    print(f"\n[30d] {len(records)} {args.mode} trial(s) complete.", flush=True)
    print(json.dumps(recompute(args.evidence_dir, args.mode), indent=2))
    print("[30d] observations only; derive conclusions from the PUBLISHED files.", flush=True)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Abort as exc:
        print(f"[30d] ABORT: {exc}", file=sys.stderr, flush=True)
        sys.exit(2)
    except KeyboardInterrupt:
        print("[30d] interrupted — per-trial cleanup ran in its finally block", file=sys.stderr, flush=True)
        sys.exit(130)
