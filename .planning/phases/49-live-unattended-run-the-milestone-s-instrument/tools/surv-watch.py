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

import json
import shutil
import subprocess
import sys
import tempfile
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

WATCH_LOG = "watch.jsonl"
ANALYSIS = "analysis.txt"
SELF = Path(__file__).resolve()
SELFTEST_WAIT_S = 5.0


def watch(project_dir: Path, out_dir: Path) -> int:
    """Stub: write an empty log (RED)."""
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / WATCH_LOG).write_text("", encoding="utf-8")
    return 0


def analyze(out_dir: Path) -> int:
    """Stub: write an empty analysis (RED)."""
    (out_dir / ANALYSIS).write_text("", encoding="utf-8")
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


def selftest() -> int:
    """Run every case; print ok/FAIL lines and the SELFTEST summary line."""
    scratch = Path(tempfile.mkdtemp(prefix="surv-watch-selftest-"))
    results: list[CaseResult] = []
    try:
        (scratch / "live").mkdir()
        results.extend(_live_run(scratch / "live"))
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
