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
    --selftest [--real-captures DIR]
        Runs the cases below against planted values (``/var/home/tester``,
        ``tester``), independent of the host. With ``--real-captures DIR`` it also
        proves the publish on every ``*-stdout`` DevFlow stage capture directly under
        DIR (copied into a temp dir; content is never printed).

``--home`` and ``--user`` default to ``Path.home()`` and the login name.

Safe to import — nothing runs at import time.
"""

from __future__ import annotations

import argparse
import getpass
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import uuid
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from pathlib import Path

SELF = Path(__file__).resolve()
TEST_HOME = "/var/home/tester"
TEST_USER = "tester"

HOME_TOKEN = "~"
ESCAPED_HOME_TOKEN = "-HOME"
EMAIL_TOKEN = "<email>"

# Token shapes copied unchanged from 30c-monitor-env-harness.py:584-588.
PREFIX_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    ("openai_key_prefix", re.compile(r"\bsk-[A-Za-z0-9_\-]{8,}")),
    ("github_token_prefix", re.compile(r"\bgh[pousr]_[A-Za-z0-9]{8,}")),
    ("bearer_token", re.compile(r"\bBearer\s+[A-Za-z0-9._~+/\-]{8,}={0,2}")),
)

# D-26: a named credential is a name holding one of these words bound to a
# credential-SHAPED value: 20+ characters of [A-Za-z0-9_+/=.-] with at least one
# letter and one digit. A usage counter, a placeholder or a short word is not.
CREDENTIAL_WORDS = re.compile(r"KEY|TOKEN|SECRET|PASSWORD", re.IGNORECASE)
_VALUE_SHAPE = (
    r"(?=[A-Za-z0-9_+/=.-]*[A-Za-z])(?=[A-Za-z0-9_+/=.-]*[0-9])[A-Za-z0-9_+/=.-]{20,}"
)
CREDENTIAL_VALUE = re.compile(_VALUE_SHAPE)
NAMED_ASSIGNMENT = re.compile(
    # 30c's name part, anchored at the start of the identifier (linear time)
    r"(?<![A-Za-z0-9_])(?=[A-Za-z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD))[A-Za-z0-9_]+"
    r"""["']?\s*[:=]\s*["']?""" + _VALUE_SHAPE,
    re.IGNORECASE,
)
STRING_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    *PREFIX_PATTERNS,
    ("credential_named_assignment", NAMED_ASSIGNMENT),
)

# The lookbehinds anchor each match at the start of its character run, so a
# long run (a base64 blob in a capture) costs linear time, not quadratic.
UUID_PATTERN = re.compile(
    r"(?<![0-9A-Fa-f])[0-9A-Fa-f]{8}(?:-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12}(?![0-9A-Fa-f])"
)
EMAIL_PATTERN = re.compile(
    r"(?<![A-Za-z0-9._%+-])[A-Za-z0-9._%+-]+@(?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,}"
    r"(?![A-Za-z0-9-])"
)


# --------------------------------------------------------------------------- redaction


def home_spellings(home: Path) -> tuple[str, ...]:
    """The home path, its realpath and the /home <-> /var/home twin of each (30c:607-630)."""
    direct = {str(home), os.path.realpath(home)}
    candidates = direct | {_twin(spelling) for spelling in direct}
    candidates -= {"", "/"}
    return tuple(sorted(candidates, key=len, reverse=True))


def _twin(spelling: str) -> str:
    """Fedora Atomic exposes one home under both /home and /var/home."""
    if spelling.startswith("/var/home/"):
        return spelling.replace("/var/home/", "/home/", 1)
    if spelling.startswith("/home/"):
        return spelling.replace("/home/", "/var/home/", 1)
    return spelling


def escaped_spellings(spellings: tuple[str, ...], user: str) -> tuple[str, ...]:
    """Claude-projects escaped forms: every non-alphanumeric becomes ``-``."""
    escaped = {re.sub(r"[^A-Za-z0-9]", "-", spelling) for spelling in spellings}
    if user:
        escaped |= {f"-var-home-{user}", f"-home-{user}"}
    return tuple(sorted(escaped, key=len, reverse=True))


def _alternation(literals: tuple[str, ...], boundary: str) -> re.Pattern[str]:
    return re.compile(
        "(?:" + "|".join(map(re.escape, literals)) + ")" + f"(?![{boundary}])"
    )


@dataclass
class Redactor:
    """Everything the sanitizer replaces, resolved once per run."""

    home_pattern: re.Pattern[str]
    escaped_pattern: re.Pattern[str]
    pseudonyms: dict[str, str] = field(default_factory=dict)

    @classmethod
    def build(cls, home: Path, user: str) -> Redactor:
        spellings = home_spellings(home)
        if not spellings:
            raise ValueError("home resolves to no redactable spelling")
        return cls(
            home_pattern=_alternation(spellings, "A-Za-z0-9_-"),
            escaped_pattern=_alternation(
                escaped_spellings(spellings, user), "A-Za-z0-9_"
            ),
        )

    def _pseudonym(self, match: re.Match[str]) -> str:
        """``uuid-<n>`` by first appearance; equal ids (any case) map to equal pseudonyms."""
        key = match.group().lower()
        if key not in self.pseudonyms:
            self.pseudonyms[key] = f"uuid-{len(self.pseudonyms) + 1}"
        return self.pseudonyms[key]

    def redact(self, text: str) -> str:
        text = self.home_pattern.sub(HOME_TOKEN, text)
        text = self.escaped_pattern.sub(ESCAPED_HOME_TOKEN, text)
        text = EMAIL_PATTERN.sub(EMAIL_TOKEN, text)
        return UUID_PATTERN.sub(self._pseudonym, text)

    def findings(self, text: str) -> list[str]:
        """Pattern NAMES that match ``text``. Never returns a matched value (30c:694-710).

        Leftover home spellings and emails are looked for in the raw text and in every
        JSON-decoded string (an escaped spelling is still a spelling); credentials in the
        decoded strings only (``credential_findings``).
        """
        decoded = decode(text)
        found = [
            name
            for name, pattern in (
                ("home_path", self.home_pattern),
                ("home_path_escaped", self.escaped_pattern),
                ("email_address", EMAIL_PATTERN),
            )
            if pattern.search(text) or any(map(pattern.search, decoded.strings))
        ]
        return found + credential_findings(decoded)


# --------------------------------------------------------------------------- JSON-decoded scan


@dataclass
class Decoded:
    """Every string a file holds once JSON is decoded, plus every string-valued field."""

    strings: list[str] = field(default_factory=list)
    fields: list[tuple[str, str]] = field(default_factory=list)


def decode(text: str) -> Decoded:
    """Parse the whole file as JSON; failing that, each line; a line that is not JSON
    is kept as plain text. Credential patterns never see raw JSON, where an escaped
    quote or newline would hide an assignment or fake one."""
    decoded = Decoded()
    documents: list[object] = []
    try:
        documents.append(json.loads(text))
    except (ValueError, RecursionError):
        for line in text.splitlines():
            try:
                documents.append(json.loads(line))
            except (ValueError, RecursionError):
                decoded.strings.append(line)
    stack = documents[::-1]
    while stack:
        node = stack.pop()
        if isinstance(node, str):
            decoded.strings.append(node)
        elif isinstance(node, dict):
            for key, value in node.items():
                decoded.strings.append(key)
                if isinstance(value, str):
                    decoded.fields.append((key, value))
            stack.extend(reversed(list(node.values())))
        elif isinstance(node, list):
            stack.extend(reversed(node))
    return decoded


def credential_findings(decoded: Decoded) -> list[str]:
    """Credential pattern NAMES found in the decoded strings and fields."""
    found = [
        name
        for name, pattern in STRING_PATTERNS
        if any(map(pattern.search, decoded.strings))
    ]
    if any(
        CREDENTIAL_WORDS.search(key) and CREDENTIAL_VALUE.fullmatch(value)
        for key, value in decoded.fields
    ):
        found.append("credential_named_field")
    return found


# --------------------------------------------------------------------------- walk


def _walk(root: Path) -> Iterator[Path]:
    """Every entry under ``root`` in sorted path order, never following a symlink."""
    for entry in sorted(root.iterdir()):
        yield entry
        if entry.is_dir() and not entry.is_symlink():
            yield from _walk(entry)


def _refuse(lines: list[str]) -> int:
    for line in lines:
        print(f"refused: {line}", file=sys.stderr)
    return 1


# --------------------------------------------------------------------------- sanitize / scan


@dataclass
class Staged:
    """One output entry: redacted relative path, source text (None = directory)."""

    out_rel: Path
    text: str | None


def _read_source(src: Path, redactor: Redactor) -> tuple[list[Staged], list[str]]:
    """Walk ``src``; stage every entry or name why it cannot be vouched for."""
    staged: list[Staged] = []
    problems: list[str] = []
    claimed: set[Path] = set()
    for entry in _walk(src):
        rel = entry.relative_to(src)
        out_rel = Path(*(redactor.redact(part) for part in rel.parts))
        if out_rel in claimed:
            problems.append(f"{rel}: name_collision")
        claimed.add(out_rel)
        if entry.is_symlink():
            problems.append(f"{rel}: symlink")
        elif entry.is_dir():
            staged.append(Staged(out_rel, None))
        elif not entry.is_file():
            problems.append(f"{rel}: not_regular_file")
        else:
            try:
                text = entry.read_bytes().decode("utf-8")
            except UnicodeDecodeError:
                problems.append(f"{rel}: not_utf8")
                continue
            # A credential is never redacted and published: it is an incident.
            problems += [f"{rel}: {name}" for name in credential_findings(decode(text))]
            staged.append(Staged(out_rel, text))
    return staged, problems


def sanitize(src: Path, dest: Path, redactor: Redactor) -> int:
    """Publish a redacted copy of ``src`` at ``dest``, or publish nothing."""
    if dest.exists() or dest.is_symlink():
        return _refuse([f"{dest.name}: destination_exists"])
    if src.is_symlink() or not src.is_dir():
        return _refuse([f"{src.name}: source_not_a_directory"])
    staged, problems = _read_source(src, redactor)
    if problems:
        return _refuse(problems)
    dest.parent.mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix=f".{dest.name}.publish-", dir=dest.parent))
    try:
        for item in staged:
            target = tmp / item.out_rel
            if item.text is None:
                target.mkdir(parents=True, exist_ok=True)
                continue
            redacted = redactor.redact(item.text)
            with target.open("xb") as handle:
                handle.write(redacted.encode("utf-8"))
            problems += [f"{item.out_rel}: {n}" for n in redactor.findings(redacted)]
        if problems:
            return _refuse(problems)
        try:
            tmp.rename(dest)
        except OSError:
            return _refuse([f"{dest.name}: destination_exists"])
    finally:
        if tmp.exists():
            shutil.rmtree(tmp)
    return 0


def _scan_file(file: Path, redactor: Redactor) -> list[str]:
    if file.is_symlink():
        return ["symlink"]
    try:
        text = file.read_bytes().decode("utf-8")
    except UnicodeDecodeError:
        return ["not_utf8"]
    return redactor.findings(text)


def scan(paths: list[Path], redactor: Redactor) -> int:
    """Print ``<file>: <pattern>`` per finding; exit 1 if there is any."""
    problems: list[str] = []
    for path in paths:
        if path.is_symlink() or path.is_file():
            files = [path]
        elif path.is_dir():
            files = [p for p in _walk(path) if p.is_symlink() or not p.is_dir()]
        else:
            problems.append(f"{path}: missing")
            continue
        for file in files:
            problems += [f"{file}: {name}" for name in _scan_file(file, redactor)]
    for line in problems:
        print(line)
    return 1 if problems else 0


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


# Secret-shaped test values are assembled at runtime from fragments so that no
# credential-shaped literal ever exists in this file (the gitleaks hook scans it).


def _planted_github_token() -> str:
    return "gh" + "p_" + ("A1b2" + "C3d4") * 4 + "E5f6"


def _planted_credential_value() -> str:
    """24 letters and digits: the credential value shape D-26 names."""
    return ("q7" + "Zx" + "4m") * 4


def _refusal_problem(
    result: subprocess.CompletedProcess[str], dest: Path, *needles: str
) -> str:
    """ "" when ``result`` is a deliberate refusal naming every needle and nothing was published."""
    if result.returncode == 0:
        return "exited 0"
    if "Traceback" in result.stderr:
        return "crashed instead of refusing"
    refusals = [
        line for line in result.stderr.splitlines() if line.startswith("refused: ")
    ]
    missing = [n for n in needles if not any(n in line for line in refusals)]
    if missing:
        return f"no refusal line names {missing}"
    if dest.exists():
        return "destination exists after a refusal"
    return ""


def _case_symlink(scratch: Path) -> str:
    src = _plant(scratch / "src", {"a.txt": "x\n"})
    (src / "link").symlink_to("a.txt")
    dest = scratch / "dest"
    return _refusal_problem(_sanitize_planted(src, dest), dest, "link", "symlink")


def _case_surviving_secret(scratch: Path) -> str:
    token = _planted_github_token()
    src = _plant(scratch / "src", {"a.txt": f"token {token}\n"})
    dest = scratch / "dest"
    result = _sanitize_planted(src, dest)
    if token in result.stdout + result.stderr:
        return "output contains the planted value"
    return _refusal_problem(result, dest, "a.txt", "github_token_prefix")


def _case_existing_dest(scratch: Path) -> str:
    src = _plant(scratch / "src", {"a.txt": "x\n"})
    dest = _plant(scratch / "dest", {"marker.txt": "keep\n"})
    result = _sanitize_planted(src, dest)
    if result.returncode == 0:
        return "exited 0"
    if "Traceback" in result.stderr or "refused: " not in result.stderr:
        return "no deliberate refusal"
    if sorted(p.name for p in dest.iterdir()) != ["marker.txt"]:
        return "destination contents changed"
    if (dest / "marker.txt").read_text(encoding="utf-8") != "keep\n":
        return "marker changed"
    return ""


def _case_non_utf8(scratch: Path) -> str:
    src = _plant(scratch / "src", {"bad.bin": b"ok\xff\n"})
    dest = scratch / "dest"
    return _refusal_problem(_sanitize_planted(src, dest), dest, "bad.bin")


def _case_bare_username(scratch: Path) -> str:
    text = f"owner={TEST_USER} repo={TEST_USER}/p49-live-attempt-1 by {TEST_USER}\n"
    src = _plant(scratch / "src", {"a.txt": text})
    result = _sanitize_planted(src, scratch / "dest")
    if result.returncode != 0:
        return f"sanitize exited {result.returncode}"
    got = (scratch / "dest" / "a.txt").read_text(encoding="utf-8")
    return "" if got == text else f"output {got!r}, want {text!r}"


def _case_email(scratch: Path) -> str:
    src = _plant(scratch / "src", {"a.txt": "contact someone@example.com now\n"})
    result = _sanitize_planted(src, scratch / "dest")
    if result.returncode != 0:
        return f"sanitize exited {result.returncode}"
    got = (scratch / "dest" / "a.txt").read_text(encoding="utf-8")
    want = f"contact {EMAIL_TOKEN} now\n"
    return "" if got == want else f"output {got!r}, want {want!r}"


# 30c's raw-text named-assignment pattern (30c-monitor-env-harness.py:588-600),
# kept ONLY as a selftest reference: the cases that must prove their sample
# exercises the false-positive class this tool's decoded scan avoids.
LEGACY_30C_NAMED_ASSIGNMENT = re.compile(
    r"""["']?[A-Za-z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD)[A-Za-z0-9_]*["']?\s*[:=]\s*"""
    r"""(?!["']?<redacted:)"""
    r"""(?!["']?[0-9.]+(?:["'\s,}\]]|$))"""
    r"""["']?[^\s"',}\]]{6,}""",
    re.IGNORECASE,
)


def _stream_json(*events: object) -> str:
    return "".join(json.dumps(event) + "\n" for event in events)


def _usage() -> dict[str, int]:
    return {
        "input_tokens": 1234,
        "output_tokens": 567,
        "cache_read_input_tokens": 1234567,
        "cache_creation_input_tokens": 89012,
    }


def _case_usage_counters(scratch: Path) -> str:
    plan_text = (
        "estimate:\n  raw_tokens: 30000\n  tokens: 60000\n  tasks: 3\n"
        "key_links:\n  - from: tools/x.py\nkey: value_token\nsecrets: none\n"
    )
    json_text = '{"raw_tokens": 30000,\n "tokens": 60000}\nnext line\n'
    capture = _stream_json(
        {
            "type": "assistant",
            "message": {
                "content": [{"type": "text", "text": plan_text}],
                "usage": _usage(),
            },
        },
        {
            "type": "user",
            "message": {"content": [{"type": "tool_result", "content": json_text}]},
        },
        {"type": "result", "usage": _usage(), "total_cost_usd": 0.5},
    )
    if not LEGACY_30C_NAMED_ASSIGNMENT.search(capture):
        return "30c's raw pattern does not hit the sample; it exercises nothing"
    src = _plant(scratch / "src", {"0-stdout": capture})
    result = _sanitize_planted(src, scratch / "dest")
    if result.returncode != 0:
        return f"sanitize exited {result.returncode}"
    clean = _scan_planted(scratch / "dest")
    return "" if clean.returncode == 0 else f"scan <dest> exited {clean.returncode}"


def _credential_line(value: str) -> str:
    return _stream_json(
        {
            "type": "user",
            "message": {
                "content": [
                    {
                        "type": "tool_result",
                        "content": f'export API_KEY="{value}"\nnext\n',
                    }
                ]
            },
        }
    )


def _case_escaped_credential(scratch: Path) -> str:
    value = _planted_credential_value()
    cases = (
        ("a", {"a.jsonl": _credential_line(value)}, "credential_named_assignment"),
        ("b", {"b.json": json.dumps({"api_key": value})}, "credential_named_field"),
    )
    for label, files, pattern in cases:
        src = _plant(scratch / f"src-{label}", files)
        dest = scratch / f"dest-{label}"
        result = _sanitize_planted(src, dest)
        if value in result.stdout + result.stderr:
            return f"{pattern}: output contains the planted value"
        problem = _refusal_problem(result, dest, next(iter(files)), pattern)
        if problem:
            return f"{pattern}: {problem}"
    return ""


Case = Callable[[Path], str]
CASES: tuple[tuple[str, Case], ...] = (
    ("sanitize: home paths in both spellings redacted", _case_home_paths),
    ("sanitize: escaped claude project paths redacted", _case_escaped_paths),
    ("sanitize: session ids pseudonymized consistently", _case_session_ids),
    ("scan: sanitized output passes the secret scan", _case_scan_output),
    ("control: symlink in source refused", _case_symlink),
    ("control: surviving secret refused and nothing published", _case_surviving_secret),
    ("control: existing destination never overwritten", _case_existing_dest),
    ("control: non-UTF-8 file refused", _case_non_utf8),
    ("sanitize: bare username kept", _case_bare_username),
    ("sanitize: email address redacted", _case_email),
    (
        "sanitize: stream-json usage counters and key names publish",
        _case_usage_counters,
    ),
    (
        "control: escaped credential inside a JSON string refused",
        _case_escaped_credential,
    ),
)


# Real-capture cases (--real-captures DIR): run only when asked, so the plain
# selftest stays host-independent. Each gets the capture directory and a scratch
# directory; none ever prints capture content.
RealCase = Callable[[Path, Path], str]


def _case_real_publish(captures: Path, scratch: Path) -> str:
    return "ABSENT: not implemented"


def _case_real_legacy_trips(captures: Path, scratch: Path) -> str:
    return "ABSENT: not implemented"


def _case_real_planted_credential(captures: Path, scratch: Path) -> str:
    return "ABSENT: not implemented"


REAL_CASES: tuple[tuple[str, RealCase], ...] = (
    ("real: every capture in the sample publishes and scans clean", _case_real_publish),
    (
        "control: the real sample trips the legacy 30c raw named-assignment pattern",
        _case_real_legacy_trips,
    ),
    (
        "control: a credential planted in a real capture is refused",
        _case_real_planted_credential,
    ),
)


def _run_case(case: Case, scratch: Path) -> str:
    """A crashing case is a failing case, never a crashed selftest."""
    try:
        return case(scratch)
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        return f"case raised {type(exc).__name__}"


def selftest(real_captures: Path | None = None) -> int:
    """Run every case in its own scratch dir; print ok/FAIL lines and the SELFTEST line."""
    cases: list[tuple[str, Case]] = list(CASES)
    if real_captures is not None:
        cases += [
            (name, lambda scratch, case=case: case(real_captures, scratch))
            for name, case in REAL_CASES
        ]
    reasons: list[tuple[str, str]] = []
    root = Path(tempfile.mkdtemp(prefix="sanitize-evidence-selftest-"))
    try:
        for index, (name, case) in enumerate(cases):
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
    parser.add_argument("--real-captures", type=Path, default=None)
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
        return selftest(args.real_captures)
    if args.real_captures is not None:
        parser.error("--real-captures is a --selftest option")
    if args.command is None:
        parser.error("a subcommand or --selftest is required")
    redactor = Redactor.build(args.home or Path.home(), args.user or getpass.getuser())
    if args.command == "sanitize":
        return sanitize(args.src, args.dest, redactor)
    return scan(args.paths, redactor)


if __name__ == "__main__":
    sys.exit(main())
