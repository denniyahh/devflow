"""sanitize-evidence — the only path by which attempt evidence enters the repository (Phase 49, plan 49-05, D-26).

The throwaway PR is private; the evidence committed here is not. Home paths, email
addresses and session identifiers are redacted, and anything that looks like a
credential stops the publish.

SUBCOMMANDS
    sanitize <src_dir> <dest_dir> [--home PATH] [--user NAME]
        Copies ``<src_dir>`` to ``<dest_dir>`` with every home spelling replaced by
        ``~``, every Claude-projects escaped home replaced by ``-HOME-``, every email
        address replaced by ``<email>`` and every UUID replaced by a stable ``uuid-<n>``
        pseudonym (one map across all file contents and relative path names).
    scan <path>... [--home PATH] [--user NAME]
        Exits non-zero if any file holds a secret-shaped string or an unredacted
        home spelling. Prints file and pattern NAME only, never a matched value.
    --selftest
        Runs the cases below against planted values (``/var/home/tester``,
        ``tester``), independent of the host.

``--home`` and ``--user`` default to ``Path.home()`` and the login name.

Safe to import — nothing runs at import time.
"""

from __future__ import annotations

import argparse
import getpass
import shutil
import subprocess
import sys
import tempfile
import uuid
from collections.abc import Callable
from pathlib import Path

SELF = Path(__file__).resolve()
TEST_HOME = "/var/home/tester"
TEST_USER = "tester"


# --------------------------------------------------------------------------- sanitize / scan


def sanitize(src: Path, dest: Path, home: Path, user: str) -> int:
    """Stub: copies verbatim."""
    shutil.copytree(src, dest)
    return 0


def scan(paths: list[Path], home: Path, user: str) -> int:
    """Stub: reports nothing."""
    return 0


# --------------------------------------------------------------------------- selftest


def _run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SELF), *args],
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )


def _sanitize_planted(src: Path, dest: Path) -> subprocess.CompletedProcess[str]:
    return _run(
        "sanitize", str(src), str(dest), "--home", TEST_HOME, "--user", TEST_USER
    )


def _scan_planted(*paths: Path) -> subprocess.CompletedProcess[str]:
    return _run("scan", *map(str, paths), "--home", TEST_HOME, "--user", TEST_USER)


def _plant(root: Path, files: dict[str, str | bytes]) -> Path:
    for rel, content in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, bytes):
            path.write_bytes(content)
        else:
            path.write_text(content, encoding="utf-8")
    return root


# Each case returns "" on success or the reason it failed. A reason never
# carries a value read from a real capture.


def _case_home_paths(scratch: Path) -> str:
    src = _plant(
        scratch / "src", {"a.txt": f"p={TEST_HOME}/x\nq=/home/{TEST_USER}/y\n"}
    )
    result = _sanitize_planted(src, scratch / "dest")
    if result.returncode != 0:
        return f"sanitize exited {result.returncode}"
    got = (scratch / "dest" / "a.txt").read_text(encoding="utf-8")
    if got != "p=~/x\nq=~/y\n":
        return f"output {got!r}, want 'p=~/x\\nq=~/y\\n'"
    return ""


ESCAPED_TAIL = "-cache-devflow-phase49-attempt-1-project--worktrees-phase-01"


def _case_escaped_paths(scratch: Path) -> str:
    var_form = f"-var-home-{TEST_USER}-{ESCAPED_TAIL}"
    bare_form = f"-home-{TEST_USER}-{ESCAPED_TAIL}"
    src = _plant(
        scratch / "src",
        {"a.txt": f"{var_form}\n{bare_form}\n", f"projects/{var_form}/f.txt": "x\n"},
    )
    result = _sanitize_planted(src, scratch / "dest")
    if result.returncode != 0:
        return f"sanitize exited {result.returncode}"
    got = (scratch / "dest" / "a.txt").read_text(encoding="utf-8")
    want = f"-HOME-{ESCAPED_TAIL}\n-HOME-{ESCAPED_TAIL}\n"
    if got != want:
        return f"content {got!r}, want {want!r}"
    renamed = scratch / "dest" / "projects" / f"-HOME-{ESCAPED_TAIL}" / "f.txt"
    if not renamed.is_file():
        return "escaped directory name not redacted"
    return ""


def _case_session_ids(scratch: Path) -> str:
    first = str(uuid.UUID(int=0x1234_5678_9ABC_4DEF_8123_4567_89AB_CDEF))
    second = str(uuid.UUID(int=0x0FED_CBA9_8765_4321_9FED_CBA9_8765_4321))
    src = _plant(
        scratch / "src",
        {
            "a.jsonl": f'{{"session_id": "{first}"}}\n',
            "b.jsonl": (
                f'{{"session_id": "{first.upper()}"}}\n{{"session_id": "{second}"}}\n'
            ),
        },
    )
    result = _sanitize_planted(src, scratch / "dest")
    if result.returncode != 0:
        return f"sanitize exited {result.returncode}"
    got_a = (scratch / "dest" / "a.jsonl").read_text(encoding="utf-8")
    got_b = (scratch / "dest" / "b.jsonl").read_text(encoding="utf-8")
    want_a = '{"session_id": "uuid-1"}\n'
    want_b = '{"session_id": "uuid-1"}\n{"session_id": "uuid-2"}\n'
    if (got_a, got_b) != (want_a, want_b):
        return f"outputs {got_a!r} {got_b!r}"
    return ""


def _case_scan_output(scratch: Path) -> str:
    src = _plant(scratch / "src", {"a.txt": f"cwd={TEST_HOME}/work\n"})
    result = _sanitize_planted(src, scratch / "dest")
    if result.returncode != 0:
        return f"sanitize exited {result.returncode}"
    clean = _scan_planted(scratch / "dest")
    if clean.returncode != 0:
        return f"scan <dest> exited {clean.returncode}"
    dirty = _scan_planted(src)
    if dirty.returncode == 0:
        return "scan <src> exited 0"
    if "home_path" not in dirty.stdout + dirty.stderr:
        return "scan <src> did not name home_path"
    return ""


Case = Callable[[Path], str]
CASES: tuple[tuple[str, Case], ...] = (
    ("sanitize: home paths in both spellings redacted", _case_home_paths),
    ("sanitize: escaped claude project paths redacted", _case_escaped_paths),
    ("sanitize: session ids pseudonymized consistently", _case_session_ids),
    ("scan: sanitized output passes the secret scan", _case_scan_output),
)


def _run_case(case: Case, scratch: Path) -> str:
    """A crashing case is a failing case, never a crashed selftest."""
    try:
        return case(scratch)
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        return f"case raised {type(exc).__name__}"


def selftest() -> int:
    """Run every case in its own scratch dir; print ok/FAIL lines and the SELFTEST line."""
    reasons: list[tuple[str, str]] = []
    root = Path(tempfile.mkdtemp(prefix="sanitize-evidence-selftest-"))
    try:
        for index, (name, case) in enumerate(CASES):
            scratch = root / f"case-{index:02d}"
            scratch.mkdir()
            reasons.append((name, _run_case(case, scratch)))
    finally:
        shutil.rmtree(root, ignore_errors=True)
    for name, why in reasons:
        print(f"  FAIL {name} — {why}" if why else f"  ok   {name}")
    failed = sum(bool(why) for _, why in reasons)
    passed = len(reasons) - failed
    print(f"SELFTEST passed={passed} failed={failed}")
    return 0 if failed == 0 and passed > 0 else 1


# --------------------------------------------------------------------------- CLI


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="sanitize-evidence.py")
    parser.add_argument("--selftest", action="store_true")
    commands = parser.add_subparsers(dest="command")
    for command in ("sanitize", "scan"):
        sub = commands.add_parser(command)
        if command == "sanitize":
            sub.add_argument("src", type=Path)
            sub.add_argument("dest", type=Path)
        else:
            sub.add_argument("paths", type=Path, nargs="+")
        sub.add_argument("--home", type=Path, default=None)
        sub.add_argument("--user", default=None)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = _parser()
    args = parser.parse_args(sys.argv[1:] if argv is None else argv)
    if args.selftest:
        if args.command is not None:
            parser.error("--selftest takes no subcommand")
        return selftest()
    if args.command is None:
        parser.error("a subcommand or --selftest is required")
    home = args.home or Path.home()
    user = args.user or getpass.getuser()
    if args.command == "sanitize":
        return sanitize(args.src, args.dest, home, user)
    return scan(args.paths, home, user)


if __name__ == "__main__":
    sys.exit(main())
