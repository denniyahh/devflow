"""surv-watch — out-of-band observer of DevFlow state-file writes (Phase 49, plan 49-04).

Answers ONE question: during one live run, did two processes ever hold an open
write of phase 01's state file at the same time?

DevFlow's ``save_state`` writes ``.state-01.json.<pid>.<seq>.tmp`` beside the
state file and renames it onto ``state-01.json``
(``crates/devflow-core/src/workflow.rs`` ``unique_temp_path_with`` /
``write_state_atomic``). A temp file that has been created but not yet renamed
or deleted is an open write. An interleaving is one pid creating a state temp
file while another pid's temp file is still open.

SUBCOMMANDS
    watch <project_dir> <out_dir>
        Watches ``<project_dir>`` with inotify so a ``.devflow/`` created after
        start is noticed, then watches ``<project_dir>/.devflow/`` for creates,
        renames and deletes. Every record is one JSON line appended and flushed
        to ``<out_dir>/watch.jsonl``. Stops on SIGTERM, SIGINT or when
        ``<out_dir>/stop`` exists. Reads events only; never writes under
        ``<project_dir>``.
    analyze <out_dir>
        Reads ``<out_dir>/watch.jsonl`` and publishes ``<out_dir>/analysis.txt``.
    --selftest
        Runs the cases below against a real inotify watch in a scratch directory.

This module reports OBSERVATIONS ONLY. An unflagged run is one observation of
one run: it establishes nothing about SURV-01's guarantee either way
(REQUIREMENTS.md SURV-01 cross-phase note), so the analysis never phrases an
absence as a confirmation.

Safe to import — nothing runs at import time.
"""

from __future__ import annotations

import ctypes
import json
import os
import re
import select
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from types import FrameType
from typing import Any, TextIO

WATCH_LOG = "watch.jsonl"
ANALYSIS = "analysis.txt"
STOP_FILE = "stop"
DEVFLOW = ".devflow"
SELF = Path(__file__).resolve()
SELFTEST_WAIT_S = 5.0
POLL_S = 0.5
READ_BYTES = 64 * 1024

# <sys/inotify.h>
IN_NONBLOCK = 0o4000
IN_CLOEXEC = 0o2000000
IN_MOVED_FROM = 0x00000040
IN_MOVED_TO = 0x00000080
IN_CREATE = 0x00000100
IN_DELETE = 0x00000200
IN_Q_OVERFLOW = 0x00004000
IN_IGNORED = 0x00008000
PROJECT_MASK = IN_CREATE | IN_MOVED_TO
DEVFLOW_MASK = IN_CREATE | IN_MOVED_FROM | IN_MOVED_TO | IN_DELETE
KIND_BY_BIT = (
    (IN_CREATE, "create"),
    (IN_MOVED_FROM, "moved_from"),
    (IN_MOVED_TO, "moved_to"),
    (IN_DELETE, "delete"),
)
# struct inotify_event { int wd; uint32_t mask; uint32_t cookie; uint32_t len; char name[]; }
EVENT_HEADER = struct.Struct("iIII")

STATE_TEMP = re.compile(r"^\.state-01\.json\.(\d+)\.(\d+)\.tmp$")
CLOSING_SENTENCE = (
    "This is one observation of one run; "
    "it establishes nothing about SURV-01's guarantee either way."
)


# --------------------------------------------------------------------------
# watch
# --------------------------------------------------------------------------


@dataclass(frozen=True)
class RawEvent:
    wd: int
    mask: int
    cookie: int
    name: str


class Inotify:
    """One non-blocking inotify descriptor, bound through ctypes (no inotifywait on the host)."""

    def __init__(self) -> None:
        self._libc = ctypes.CDLL(None, use_errno=True)
        self._libc.inotify_init1.argtypes = [ctypes.c_int]
        self._libc.inotify_add_watch.argtypes = [
            ctypes.c_int,
            ctypes.c_char_p,
            ctypes.c_uint32,
        ]
        fd = self._libc.inotify_init1(IN_NONBLOCK | IN_CLOEXEC)
        if fd < 0:
            errno = ctypes.get_errno()
            raise OSError(errno, f"inotify_init1: {os.strerror(errno)}")
        self.fd: int = fd

    def add_watch(self, path: Path, mask: int) -> int:
        wd = self._libc.inotify_add_watch(self.fd, os.fsencode(path), mask)
        if wd < 0:
            errno = ctypes.get_errno()
            raise OSError(errno, os.strerror(errno), str(path))
        return int(wd)

    def read_events(self) -> list[RawEvent]:
        """Drain every queued event without blocking."""
        events: list[RawEvent] = []
        while True:
            try:
                buf = os.read(self.fd, READ_BYTES)
            except BlockingIOError:
                return events
            offset = 0
            while offset < len(buf):
                wd, mask, cookie, length = EVENT_HEADER.unpack_from(buf, offset)
                start = offset + EVENT_HEADER.size
                name = os.fsdecode(buf[start : start + length].split(b"\0", 1)[0])
                events.append(RawEvent(wd, mask, cookie, name))
                offset = start + length

    def close(self) -> None:
        os.close(self.fd)


@dataclass
class Watcher:
    project_dir: Path
    log: TextIO
    inotify: Inotify
    project_wd: int
    devflow_wd: int | None = None

    @property
    def devflow(self) -> Path:
        return self.project_dir / DEVFLOW

    def emit(
        self, kind: str, directory: str | None, name: str, cookie: int = 0
    ) -> None:
        record = {
            "mono_ns": time.monotonic_ns(),
            "wall": datetime.now(UTC).isoformat(),
            "kind": kind,
            "dir": directory,
            "name": name,
            "cookie": cookie,
        }
        self.log.write(json.dumps(record) + "\n")
        self.log.flush()

    def attach_devflow(self) -> None:
        """Watch .devflow/ if it exists, then log what it already held."""
        if self.devflow_wd is not None or not self.devflow.is_dir():
            return
        try:
            self.devflow_wd = self.inotify.add_watch(self.devflow, DEVFLOW_MASK)
        except FileNotFoundError:
            return
        self.emit("watch_added", str(self.devflow), "")
        for entry in sorted(self.devflow.iterdir()):
            self.emit("initial", str(self.devflow), entry.name)

    def handle(self, event: RawEvent) -> None:
        if event.mask & IN_Q_OVERFLOW:
            self.emit("overflow", None, "")
            self.attach_devflow()
            return
        if event.wd == self.devflow_wd:
            if event.mask & IN_IGNORED:
                self.devflow_wd = None
                return
            for bit, kind in KIND_BY_BIT:
                if event.mask & bit:
                    self.emit(kind, str(self.devflow), event.name, event.cookie)
        elif event.wd == self.project_wd and event.name == DEVFLOW:
            self.attach_devflow()


def _run_until_stopped(watcher: Watcher, stop_file: Path) -> str:
    received: list[str] = []

    def on_signal(signum: int, _frame: FrameType | None) -> None:
        received.append(signal.Signals(signum).name)

    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGINT, on_signal)
    while True:
        if received:
            return received[0]
        if stop_file.exists():
            return "stop file"
        ready, _, _ = select.select([watcher.inotify.fd], [], [], POLL_S)
        if ready:
            for event in watcher.inotify.read_events():
                watcher.handle(event)


def watch(project_dir: Path, out_dir: Path) -> int:
    """Log state-file writes under project_dir/.devflow/ until stopped."""
    project_dir = project_dir.resolve()
    out_dir = out_dir.resolve()
    if not project_dir.is_dir():
        print(f"project dir ABSENT: {project_dir}", file=sys.stderr)
        return 2
    if out_dir.is_relative_to(project_dir):
        print(
            f"refusing: {out_dir} is inside the watched {project_dir}", file=sys.stderr
        )
        return 2
    log_path = out_dir / WATCH_LOG
    if log_path.exists():
        print(f"refusing: {log_path} already exists (one log per run)", file=sys.stderr)
        return 2
    inotify = Inotify()
    try:
        # The project watch is in place before watch.jsonl exists, so a
        # .devflow/ created after the log appears is always noticed.
        project_wd = inotify.add_watch(project_dir, PROJECT_MASK)
        out_dir.mkdir(parents=True, exist_ok=True)
        with log_path.open("x", encoding="utf-8") as log:
            watcher = Watcher(project_dir, log, inotify, project_wd)
            watcher.attach_devflow()
            reason = _run_until_stopped(watcher, out_dir / STOP_FILE)
            for event in inotify.read_events():
                watcher.handle(event)
            watcher.emit("stopped", None, reason)
    finally:
        inotify.close()
    return 0


# --------------------------------------------------------------------------
# analyze
# --------------------------------------------------------------------------


class AnalysisError(Exception):
    pass


@dataclass(frozen=True)
class Overlap:
    holder_pid: int
    holder_seq: int
    creator_pid: int
    creator_seq: int
    mono_ns: int


@dataclass
class Observation:
    writers: set[int] = field(default_factory=set)
    temp_writes: int = 0
    overlaps: list[Overlap] = field(default_factory=list)


def _load_records(log: Path) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for number, line in enumerate(log.read_text(encoding="utf-8").splitlines(), 1):
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise AnalysisError(f"{log} line {number} is not JSON: {error}") from error
        missing = {"mono_ns", "kind", "name"} - set(record)
        if missing:
            raise AnalysisError(f"{log} line {number} lacks {sorted(missing)}")
        records.append(record)
    return records


def observe(records: list[dict[str, Any]]) -> Observation:
    """Walk records in queue order; a window is open from a temp's create to its rename or delete."""
    seen = Observation()
    open_windows: set[tuple[int, int]] = set()
    for record in records:
        kind = record["kind"]
        match = STATE_TEMP.match(record["name"])
        if match is None or kind not in ("create", "initial", "moved_from", "delete"):
            continue
        pid, seq = int(match[1]), int(match[2])
        if kind in ("moved_from", "delete"):
            open_windows.discard((pid, seq))
            continue
        if kind == "create":
            seen.temp_writes += 1
            seen.writers.add(pid)
            seen.overlaps.extend(
                Overlap(held_pid, held_seq, pid, seq, int(record["mono_ns"]))
                for held_pid, held_seq in sorted(open_windows)
                if held_pid != pid
            )
        open_windows.add((pid, seq))
    return seen


def render(seen: Observation) -> str:
    lines = [
        f"surv-watch analysis of {WATCH_LOG}",
        f"writers={len(seen.writers)}",
        f"temp_writes={seen.temp_writes}",
        f"interleaved_windows={len(seen.overlaps)}",
    ]
    lines.extend(
        f"overlap pid={o.holder_pid} seq={o.holder_seq} with pid={o.creator_pid} "
        f"seq={o.creator_seq} at mono_ns={o.mono_ns}"
        for o in seen.overlaps
    )
    lines.append(CLOSING_SENTENCE)
    return "\n".join(lines) + "\n"


def _atomic_publish(text: str, dest: Path) -> None:
    handle, tmp_name = tempfile.mkstemp(dir=dest.parent, prefix=".publish-")
    tmp = Path(tmp_name)
    try:
        with os.fdopen(handle, "w", encoding="utf-8") as out:
            out.write(text)
        tmp.replace(dest)
    except BaseException:
        tmp.unlink(missing_ok=True)
        raise


def analyze(out_dir: Path) -> int:
    """Publish out_dir/analysis.txt from out_dir/watch.jsonl."""
    try:
        records = _load_records(out_dir / WATCH_LOG)
    except AnalysisError as error:
        print(error, file=sys.stderr)
        return 2
    _atomic_publish(render(observe(records)), out_dir / ANALYSIS)
    return 0


# --------------------------------------------------------------------------
# selftest
# --------------------------------------------------------------------------


@dataclass
class CaseResult:
    name: str
    ok: bool
    why: str = ""


def _read_records(log: Path) -> list[dict[str, object]]:
    if not log.is_file():
        return []
    return [
        json.loads(line)
        for line in log.read_text(encoding="utf-8").splitlines()
        if line
    ]


def _wait_for(predicate: Callable[[], bool], timeout_s: float) -> bool:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


def _run_analyze(out_dir: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SELF), "analyze", str(out_dir)],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )


def _analysis_lines(out_dir: Path) -> list[str]:
    path = out_dir / ANALYSIS
    return path.read_text(encoding="utf-8").splitlines() if path.is_file() else []


def _verdict_words(lines: list[str]) -> list[str]:
    text = "\n".join(lines).lower()
    return [word for word in ("confirmed", "no interleaving") if word in text]


def _live_run(scratch: Path) -> list[CaseResult]:
    """Drive a real watcher through a two-writer overlap; return both cases."""
    proj = scratch / "proj"
    out = scratch / "out"
    proj.mkdir()
    created: set[str] = set()
    live = "watch: live two-writer overlap is logged and flagged"
    unchanged = "watch: watched directory left unchanged"
    stderr_path = scratch / "watch.stderr"
    with stderr_path.open("w", encoding="utf-8") as stderr:
        proc = subprocess.Popen(
            [sys.executable, str(SELF), "watch", str(proj), str(out)],
            stdout=subprocess.DEVNULL,
            stderr=stderr,
        )
    try:
        log = out / WATCH_LOG
        if not _wait_for(log.exists, SELFTEST_WAIT_S):
            fail = CaseResult(live, False, "watcher never opened watch.jsonl")
            return [fail, CaseResult(unchanged, False, "no completed watch to judge")]
        devflow = proj / ".devflow"
        devflow.mkdir()
        created.add(".devflow")

        def watch_added() -> bool:
            return any(rec["kind"] == "watch_added" for rec in _read_records(log))

        if not _wait_for(watch_added, SELFTEST_WAIT_S):
            fail = CaseResult(
                live, False, "no watch_added record within 5 s of creating .devflow"
            )
            return [fail, CaseResult(unchanged, False, "no completed watch to judge")]
        first = devflow / ".state-01.json.100.1.tmp"
        second = devflow / ".state-01.json.200.1.tmp"
        first.write_text("{}", encoding="utf-8")
        second.write_text("{}", encoding="utf-8")
        first.rename(devflow / "state-01.json")
        second.rename(devflow / "state-01.json")
        created.add(".devflow/state-01.json")
        (out / "stop").write_text("", encoding="utf-8")
        try:
            code = proc.wait(timeout=SELFTEST_WAIT_S)
        except subprocess.TimeoutExpired:
            fail = CaseResult(
                live, False, "watcher did not exit within 5 s of the stop file"
            )
            return [fail, CaseResult(unchanged, False, "no completed watch to judge")]
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    records = _read_records(out / WATCH_LOG)
    stopped = bool(records) and records[-1]["kind"] == "stopped"
    return [
        _judge_live(live, code, records, out),
        _judge_unchanged(unchanged, proj, created, stopped),
    ]


def _judge_live(
    name: str, code: int, records: list[dict[str, object]], out: Path
) -> CaseResult:
    if code != 0:
        stderr = (out.parent / "watch.stderr").read_text(encoding="utf-8").strip()
        return CaseResult(name, False, f"watcher exit {code}: {stderr}")
    seen = {(rec["kind"], rec["name"]) for rec in records}
    for temp in (".state-01.json.100.1.tmp", ".state-01.json.200.1.tmp"):
        for kind in ("create", "moved_from"):
            if (kind, temp) not in seen:
                return CaseResult(name, False, f"no {kind} record for {temp}")
    if not records or records[-1]["kind"] != "stopped":
        return CaseResult(name, False, "last record is not stopped")
    result = _run_analyze(out)
    if result.returncode != 0:
        return CaseResult(
            name, False, f"analyze exit {result.returncode}: {result.stderr.strip()}"
        )
    lines = _analysis_lines(out)
    for want in ("writers=2", "temp_writes=2", "interleaved_windows=1"):
        if lines.count(want) != 1:
            return CaseResult(
                name, False, f"analysis.txt lacks exactly one line {want!r}"
            )
    overlaps = [line for line in lines if line.startswith("overlap pid=100")]
    if len(overlaps) != 1 or "pid=200" not in overlaps[0]:
        return CaseResult(name, False, f"overlap lines {overlaps!r}")
    words = _verdict_words(lines)
    if words:
        return CaseResult(name, False, f"verdict words {words}")
    return CaseResult(name, True)


def _judge_unchanged(
    name: str, proj: Path, created: set[str], stopped: bool
) -> CaseResult:
    if not stopped:
        return CaseResult(name, False, "no completed watch to judge")
    found = {str(path.relative_to(proj)) for path in proj.rglob("*")}
    if found != created:
        return CaseResult(
            name,
            False,
            f"extra={sorted(found - created)} missing={sorted(created - found)}",
        )
    return CaseResult(name, True)


TEMP_100 = ".state-01.json.100.1.tmp"
TEMP_200 = ".state-01.json.200.1.tmp"


def _rec(mono_ns: int, kind: str, name: str = "") -> dict[str, object]:
    return {
        "mono_ns": mono_ns,
        "wall": "",
        "kind": kind,
        "dir": "/synthetic/.devflow",
        "name": name,
        "cookie": 0,
    }


def _synthetic_analysis(
    out_dir: Path, records: list[dict[str, object]]
) -> tuple[subprocess.CompletedProcess[str], list[str]]:
    out_dir.mkdir()
    text = "".join(json.dumps(record) + "\n" for record in records)
    (out_dir / WATCH_LOG).write_text(text, encoding="utf-8")
    return _run_analyze(out_dir), _analysis_lines(out_dir)


def _fields_wrong(lines: list[str], expected: dict[str, str]) -> str:
    """Each field must sit alone on exactly one line with the expected value."""
    for key, value in expected.items():
        keyed = [line for line in lines if line.startswith(f"{key}=")]
        if keyed != [f"{key}={value}"]:
            return f"{key} lines {keyed!r}, want [{key}={value!r}]"
    return ""


def _case_sequential(scratch: Path) -> CaseResult:
    name = "analyze: sequential writer not flagged"
    result, lines = _synthetic_analysis(
        scratch / "sequential",
        [
            _rec(1, "watch_added"),
            _rec(10, "create", TEMP_100),
            _rec(11, "moved_from", TEMP_100),
            _rec(11, "moved_to", "state-01.json"),
            _rec(20, "create", TEMP_200),
            _rec(21, "moved_from", TEMP_200),
            _rec(21, "moved_to", "state-01.json"),
            _rec(30, "stopped", "stop file"),
        ],
    )
    if result.returncode != 0:
        return CaseResult(name, False, f"analyze exit {result.returncode}")
    wrong = _fields_wrong(
        lines, {"writers": "2", "temp_writes": "2", "interleaved_windows": "0"}
    )
    words = _verdict_words(lines)
    if wrong or words:
        return CaseResult(name, False, wrong or f"verdict words {words}")
    return CaseResult(name, True)


def _case_overflow(scratch: Path) -> CaseResult:
    name = "analyze: overflow recorded as a void window"
    result, lines = _synthetic_analysis(
        scratch / "overflow",
        [
            _rec(1, "watch_added"),
            _rec(10, "create", TEMP_100),
            _rec(11, "moved_from", TEMP_100),
            _rec(500, "overflow"),
            _rec(600, "create", TEMP_200),
            _rec(601, "moved_from", TEMP_200),
            _rec(700, "stopped", "SIGTERM"),
        ],
    )
    if result.returncode != 0:
        return CaseResult(name, False, f"analyze exit {result.returncode}")
    wrong = _fields_wrong(lines, {"overflow_windows": "1"})
    voids = [line for line in lines if line.startswith("void window")]
    if wrong or len(voids) != 1:
        return CaseResult(name, False, wrong or f"void window lines {voids!r}")
    return CaseResult(name, True)


def _case_missing_stop(scratch: Path) -> CaseResult:
    """Paired control: the same log with a stopped record must say present."""
    name = "analyze: missing stop marker recorded"
    writes = [
        _rec(1, "watch_added"),
        _rec(10, "create", TEMP_100),
        _rec(11, "moved_from", TEMP_100),
    ]
    _, lines = _synthetic_analysis(scratch / "no-stop", writes)
    wrong = _fields_wrong(lines, {"stop_marker": "absent"})
    if wrong:
        return CaseResult(name, False, wrong)
    _, lines = _synthetic_analysis(
        scratch / "with-stop", [*writes, _rec(20, "stopped", "SIGTERM")]
    )
    wrong = _fields_wrong(lines, {"stop_marker": "present"})
    if wrong:
        return CaseResult(name, False, f"control with a stopped record: {wrong}")
    return CaseResult(name, True)


def _case_empty(scratch: Path) -> CaseResult:
    name = "analyze: empty log reports saw nothing without a verdict"
    result, lines = _synthetic_analysis(scratch / "empty", [])
    if result.returncode != 0:
        return CaseResult(name, False, f"analyze exit {result.returncode}")
    saw_nothing = [line for line in lines if line.startswith("saw nothing")]
    if len(saw_nothing) != 1:
        return CaseResult(name, False, f"saw nothing lines {saw_nothing!r}")
    wrong = _fields_wrong(
        lines,
        {
            "writers": "0",
            "temp_writes": "0",
            "interleaved_windows": "0",
            "overflow_windows": "0",
            "stop_marker": "absent",
        },
    )
    words = _verdict_words(lines)
    if wrong or words:
        return CaseResult(name, False, wrong or f"verdict words {words}")
    return CaseResult(name, True)


def _case_missing_log(scratch: Path) -> CaseResult:
    name = "control: missing watch log refused"
    out_dir = scratch / "no-log"
    out_dir.mkdir()
    result = _run_analyze(out_dir)
    if result.returncode == 0:
        return CaseResult(name, False, "analyze exited 0 without watch.jsonl")
    if "watch.jsonl ABSENT" not in result.stderr:
        return CaseResult(name, False, f"stderr {result.stderr.strip()!r}")
    if (out_dir / ANALYSIS).exists():
        return CaseResult(name, False, "analysis.txt was written")
    return CaseResult(name, True)


def selftest() -> int:
    """Run every case; print ok/FAIL lines and the SELFTEST summary line."""
    scratch = Path(tempfile.mkdtemp(prefix="surv-watch-selftest-"))
    results: list[CaseResult] = []
    try:
        (scratch / "live").mkdir()
        results.extend(_live_run(scratch / "live"))
        for case in (
            _case_sequential,
            _case_overflow,
            _case_missing_stop,
            _case_empty,
            _case_missing_log,
        ):
            results.append(case(scratch))
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    for result in results:
        print(
            f"  ok   {result.name}"
            if result.ok
            else f"  FAIL {result.name} — {result.why}"
        )
    passed = sum(result.ok for result in results)
    failed = len(results) - passed
    print(f"SELFTEST passed={passed} failed={failed}")
    return 0 if failed == 0 and passed > 0 else 1


USAGE = "usage: surv-watch.py watch <project_dir> <out_dir> | analyze <out_dir> | --selftest"


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if argv == ["--selftest"]:
        return selftest()
    if len(argv) == 3 and argv[0] == "watch":
        return watch(Path(argv[1]), Path(argv[2]))
    if len(argv) == 2 and argv[0] == "analyze":
        return analyze(Path(argv[1]))
    print(USAGE, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
